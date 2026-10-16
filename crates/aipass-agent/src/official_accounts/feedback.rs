//! Passive Claude allowance readings remain bound to the account capability.
use super::*;
use aipass_agent_protocol::{AgentErrorCode, ClaudeAllowanceObservation};
use serde::{Deserialize, Serialize};
use std::sync::{atomic::Ordering, Arc};
const KEY: &str = "claude_response_usage_v1";

#[derive(Serialize, Deserialize)]
struct Observations {
    marker: String,
    windows: Vec<ClaudeAllowanceObservation>,
    #[serde(default)]
    polled_at: Option<String>,
}
fn span(scope: &str) -> Option<u64> {
    match scope {
        "five_hour" => Some(300),
        "seven_day"
        | "seven_day_opus"
        | "seven_day_sonnet"
        | "seven_day_haiku"
        | "seven_day_fable"
        | "seven_day_overage_included" => Some(10080),
        _ => None,
    }
}
pub(crate) fn claude_observations(vault: &Vault, id: Uuid) -> Vec<ClaudeAllowanceObservation> {
    stored(vault, id).map(|s| s.windows).unwrap_or_default()
}
fn stored(vault: &Vault, id: Uuid) -> Option<Observations> {
    let raw = vault.provider_runtime_extension(id, KEY).ok()??;
    let stored: Observations = serde_json::from_str(raw.expose()).ok()?;
    (stored.marker == vault.fingerprint_secret(&vault.reveal_secret(id).ok()?)).then_some(stored)
}
pub(crate) fn claude_polled_at(vault: &Vault, id: Uuid) -> Option<String> {
    stored(vault, id)?.polled_at
}
fn merge(windows: &mut Vec<ClaudeAllowanceObservation>, heard: ClaudeAllowanceObservation) {
    if let Some(old) = windows.iter_mut().find(|old| old.scope == heard.scope) {
        if heard.observed_at >= old.observed_at {
            *old = heard;
        }
    } else {
        windows.push(heard);
    }
}
pub(crate) fn observe_claude(
    state: &Arc<AgentState>,
    id: Uuid,
    marker: &str,
    heard: Vec<ClaudeAllowanceObservation>,
) -> ServiceResult<()> {
    let invalid = || {
        ServiceError::new(
            AgentErrorCode::ValidationFailed,
            "Invalid Claude allowance observation",
        )
    };
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    if heard.is_empty()
        || heard.len() > 8
        || heard.iter().any(|w| {
            span(&w.scope).is_none()
                || !w.used_percent.is_finite()
                || !(0.0..=100.0).contains(&w.used_percent)
                || w.observed_at > now + 30
                || w.observed_at < now - 300
        })
    {
        return Err(invalid());
    }
    with_vault(state, false, |vault| {
        crate::claude_bridge::validate_account(vault, id, marker)?;
        let auth = claude_reference(vault, id)?.ok_or_else(invalid)?;
        cli_accounts::check_device(&auth).map_err(|_| invalid())?;
        let home = PathBuf::from(auth["nativeHome"].as_str().ok_or_else(invalid)?);
        let default = crate::server::home_dir(vault)?.join(".claude");
        // Passive feedback must not read OAuth grants or prompt for Keychain.
        use aipass_config_writers::native_auth::{claude_identity, parse, Resource};
        let profile = Resource::File(claude_identity::profile_path(&home, &default))
            .read()
            .map_err(|_| invalid())?
            .ok_or_else(invalid)?;
        let profile = parse(&profile).map_err(|_| invalid())?;
        let actual = claude_identity::identity(&Value::Null, &profile.0).map_err(|_| invalid())?;
        if Some(actual.as_str()) != auth["accountId"].as_str() {
            return Err(invalid());
        }
        let mut snapshot = vault
            .get_provider_summary(id)
            .map_err(map_vault_error)?
            .subscription
            .unwrap_or_default();
        let previous = stored(vault, id);
        let mut windows = previous
            .as_ref()
            .map(|s| s.windows.clone())
            .unwrap_or_default();
        let polled_at = if snapshot.source != "claude-cli:response" {
            Some(snapshot.observed_at.clone())
        } else {
            previous.and_then(|s| s.polled_at)
        };
        // Preserve scheduled scopes and their original ages before publishing
        // the response view. A five-hour event must not erase a weekly limit.
        if snapshot.source != "claude-cli:response" && !snapshot.stale && snapshot.error.is_none() {
            if let Ok(at) = time::OffsetDateTime::parse(
                &snapshot.observed_at,
                &time::format_description::well_known::Rfc3339,
            ) {
                for w in &snapshot.windows {
                    if span(&w.id).is_none() {
                        continue;
                    }
                    let Some(used) = w
                        .used_percent
                        .filter(|n| n.is_finite() && (0.0..=100.).contains(n))
                    else {
                        continue;
                    };
                    merge(
                        &mut windows,
                        ClaudeAllowanceObservation {
                            scope: w.id.clone(),
                            used_percent: used,
                            observed_at: at.unix_timestamp(),
                            resets_at: w
                                .resets_at
                                .as_deref()
                                .and_then(|s| {
                                    time::OffsetDateTime::parse(
                                        s,
                                        &time::format_description::well_known::Rfc3339,
                                    )
                                    .ok()
                                })
                                .map(|t| t.unix_timestamp()),
                        },
                    );
                }
            }
        }
        for w in heard {
            merge(&mut windows, w);
        }
        snapshot.windows = windows
            .iter()
            .filter(|w| w.observed_at >= now - 300)
            .map(display_window)
            .collect();
        snapshot.observed_at = super::now();
        snapshot.source = "claude-cli:response".into();
        snapshot.error = None;
        snapshot.stale = false;
        let raw = SecretString::new(
            serde_json::to_string(&Observations {
                marker: vault.fingerprint_secret(marker),
                windows,
                polled_at,
            })
            .map_err(ServiceError::internal)?,
        );
        vault
            .set_provider_runtime_extension(id, KEY, Some(&raw))
            .map_err(map_vault_error)?;
        vault
            .update_provider_subscription(id, Some(snapshot))
            .map_err(map_vault_error)?;
        state
            .proxy
            .lock()
            .map_err(|_| invalid())?
            .refresh_provider_credentials(vault, id)?;
        state.sync_revision.fetch_add(1, Ordering::Relaxed);
        Ok(())
    })
}

pub(crate) fn display_window(
    w: &ClaudeAllowanceObservation,
) -> aipass_provider_registry::SubscriptionWindow {
    aipass_provider_registry::SubscriptionWindow {
        id: w.scope.clone(),
        label: w.scope.replace('_', " "),
        used_percent: Some(w.used_percent),
        resets_at: w
            .resets_at
            .and_then(|t| time::OffsetDateTime::from_unix_timestamp(t).ok())
            .and_then(|t| {
                t.format(&time::format_description::well_known::Rfc3339)
                    .ok()
            }),
        window_minutes: span(&w.scope),
        source: Some("claude-cli:response".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn passive_updates_preserve_scheduled_scopes_ages_and_poll_time_without_reading_grants() {
        use crate::session::{SessionInfo, SessionState};
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let state = crate::server::tests::sync_test_state(dir.path().join("vault"));
        let vault = Vault::create(&state.vault_dir, &SecretString::new("feedback-password"))
            .unwrap()
            .vault;
        let at = time::OffsetDateTime::now_utc();
        *state.session.lock().unwrap() = SessionState::Unlocked(Box::new(SessionInfo {
            id: Uuid::new_v4(),
            vault,
            unlocked_at: at,
            last_activity_at: at,
        }));
        let cli = home.join("claude-account");
        std::fs::create_dir_all(&cli).unwrap();
        let profile = cli.join(".claude.json");
        let write_profile = |org| {
            std::fs::write(&profile, json!({"oauthAccount":{"emailAddress":"alice@example.test","organizationUuid":org}}).to_string()).unwrap()
        };
        write_profile("a");
        // No .credentials.json or Keychain grant exists. Passive observation
        // needs only the non-sensitive profile and the existing capability.
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let old = now - 120;
        let (id, marker, polled) = with_vault(&state, false, |vault| {
            let id = persist_claude_login(
                vault,
                &crate::claude_cli::NativeAccount {
                    home: cli.clone(),
                    identity: "alice@example.test::org::a".into(),
                    plan: None,
                },
            )
            .unwrap();
            let polled = time::OffsetDateTime::from_unix_timestamp(old)
                .unwrap()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap();
            let windows = ["five_hour", "seven_day", "seven_day_sonnet"]
                .into_iter()
                .map(|scope| {
                    display_window(&ClaudeAllowanceObservation {
                        scope: scope.into(),
                        used_percent: 100.,
                        resets_at: Some(now + 3600),
                        observed_at: old,
                    })
                })
                .collect();
            vault
                .update_provider_subscription(
                    id,
                    Some(SubscriptionSnapshot {
                        source: "claude-cli".into(),
                        observed_at: polled.clone(),
                        windows,
                        ..Default::default()
                    }),
                )
                .unwrap();
            Ok((id, vault.reveal_secret(id).unwrap(), polled))
        })
        .unwrap();
        let heard = || {
            vec![ClaudeAllowanceObservation {
                scope: "five_hour".into(),
                used_percent: 30.,
                resets_at: Some(now + 3600),
                observed_at: now,
            }]
        };
        observe_claude(&state, id, &marker, heard()).unwrap();
        observe_claude(&state, id, &marker, heard()).unwrap();
        with_vault(&state, false, |vault| {
            assert_eq!(
                claude_polled_at(vault, id).as_deref(),
                Some(polled.as_str())
            );
            let entry = vault.get_provider_summary(id).unwrap();
            assert_eq!(entry.subscription.as_ref().unwrap().windows.len(), 3);
            let quota = crate::proxy_service::account_quota_with_observations(vault, &entry);
            assert_eq!(quota.len(), 3);
            assert_eq!(
                quota
                    .iter()
                    .filter(|w| w.used_basis_points == 10000 && w.observed_at == old as u64)
                    .count(),
                2
            );
            assert!(quota
                .iter()
                .any(|w| w.used_basis_points == 3000 && w.observed_at == now as u64));
            Ok(())
        })
        .unwrap();
        write_profile("b");
        assert!(observe_claude(&state, id, &marker, heard()).is_err());
    }
}
