//! Opt-in native switching reuses the coordinated, encrypted-backup writer.
//! Unknown quotas, user changes and active bridge turns prevent a switch.
use super::*;
const KEY: &str = "native_auto_switch_v1";
#[derive(Serialize, Deserialize)]
struct Receipt {
    policy: String,
    selected: Uuid,
}

pub(crate) fn refresh(state: &Arc<AgentState>) {
    let preferred = with_vault(state, false, |vault| {
        vault
            .list_provider_summaries()
            .map_err(map_vault_error)?
            .into_iter()
            .filter(|e| e.archived_at.is_none() && e.deleted_at.is_none())
            .map(|e| crate::provider_runtime::load(vault, e.id).map(|o| (e.id, o.subscription)))
            .collect::<ServiceResult<Vec<_>>>()
    });
    for (id, policy) in preferred.unwrap_or_default() {
        if policy.native_switch.is_some() {
            let _ = attempt(state, id);
        }
    }
}

fn attempt(state: &Arc<AgentState>, preferred: Uuid) -> ServiceResult<()> {
    // Coordinate the observed current account and the actual mutation with
    // manual switches, including their ambiguous-write recovery.
    let _guard = SWITCH_LOCK
        .lock()
        .map_err(|_| safe_error(anyhow::anyhow!("tool coordinator unavailable")))?;
    let prepared = with_vault(state, false, |vault| {
        let options = crate::provider_runtime::load(vault, preferred)?;
        let Some(policy) = options.subscription.native_switch else {
            return Ok(None);
        };
        let entry = vault
            .get_provider_summary(preferred)
            .map_err(map_vault_error)?;
        let tool = match entry.provider_id.as_deref() {
            Some("codex") => ToolConfigTool::Codex,
            Some("anthropic") => ToolConfigTool::ClaudeCode,
            _ => return Ok(None),
        };
        let config = state
            .proxy
            .lock()
            .map_err(|_| safe_error(anyhow::anyhow!("proxy unavailable")))?
            .load_config(vault)?;
        let Some(route) = config
            .routes
            .iter()
            .find(|r| r.id == policy.route_id && r.enabled)
        else {
            return Ok(None);
        };
        let mut targets = route
            .targets
            .iter()
            .filter(|t| t.enabled)
            .collect::<Vec<_>>();
        targets.sort_by_key(|t| t.priority);
        let mut candidates = Vec::new();
        for t in targets {
            let e = vault
                .get_provider_summary(t.provider_entry_id)
                .map_err(map_vault_error)?;
            if e.provider_id == entry.provider_id
                && e.archived_at.is_none()
                && e.deleted_at.is_none()
                && validate_reference(vault, e.id, &tool).is_ok()
                && !candidates.iter().any(|(id, _)| *id == e.id)
            {
                candidates.push((e.id, usage(vault, &e)));
            }
        }
        let policy_key = serde_json::to_string(&policy).map_err(ServiceError::internal)?;
        let receipt = vault
            .provider_runtime_extension(preferred, KEY)
            .map_err(map_vault_error)?
            .and_then(|raw| serde_json::from_str::<Receipt>(raw.expose()).ok())
            .filter(|r| r.policy == policy_key)
            .map(|r| r.selected);
        Ok(Some((
            tool,
            candidates,
            receipt,
            policy.return_preferred,
            policy_key,
        )))
    })?;
    let Some((tool, candidates, receipt, return_preferred, policy_key)) = prepared else {
        return Ok(());
    };
    let status = status_snapshot(state, tool.clone())?;
    if !matches!(status.state.as_str(), "ready" | "unmanaged")
        || status.mode != Some(ToolConfigMode::Official)
    {
        return Ok(());
    }
    let Some(current) = status.entry_id else {
        return Ok(());
    };
    // A manual/external choice wins over the automatic policy.
    if !can_manage(preferred, current, receipt, &candidates) {
        return Ok(());
    }
    if receipt.is_none() {
        remember(state, preferred, current, &policy_key)?;
    }
    let Some(selected) = choose(preferred, current, return_preferred, &candidates) else {
        return Ok(());
    };
    if tool == ToolConfigTool::ClaudeCode {
        let bridge = state
            .proxy
            .lock()
            .map_err(|_| safe_error(anyhow::anyhow!("proxy unavailable")))?
            .claude_bridge();
        if candidates.iter().any(|(id, _)| bridge.active(*id)) {
            return Ok(());
        }
    }
    let response = apply_locked_guarded(
        state,
        ToolConfigRequest {
            tool,
            id: selected,
            secret_id: None,
            mode: ToolConfigMode::Official,
            codex_api_key_mode: None,
            preview_id: None,
        },
        false,
        Some((preferred, &policy_key, current)),
    )?;
    if response.outcome == ToolConfigOutcome::Applied {
        remember(state, preferred, selected, &policy_key)?;
    }
    Ok(())
}

fn remember(
    state: &Arc<AgentState>,
    preferred: Uuid,
    selected: Uuid,
    policy: &str,
) -> ServiceResult<()> {
    with_vault(state, false, |vault| {
        vault
            .set_provider_runtime_extension(
                preferred,
                KEY,
                Some(&SecretString::new(
                    serde_json::to_string(&Receipt {
                        policy: policy.into(),
                        selected,
                    })
                    .map_err(ServiceError::internal)?,
                )),
            )
            .map_err(map_vault_error)
    })
}
fn can_manage(
    preferred: Uuid,
    current: Uuid,
    receipt: Option<Uuid>,
    candidates: &[(Uuid, Option<f64>)],
) -> bool {
    candidates.iter().any(|(id, _)| *id == current)
        && (current == preferred || receipt.is_none() || receipt == Some(current))
}
pub(super) fn check_policy(
    state: &Arc<AgentState>,
    vault: &Vault,
    preferred: Uuid,
    expected: &str,
    current: Uuid,
    selected: Uuid,
) -> ServiceResult<()> {
    let invalid = || {
        ServiceError::new(
            AgentErrorCode::Conflict,
            "Automatic switch settings changed during verification",
        )
    };
    let options = crate::provider_runtime::load(vault, preferred)?;
    let policy = options
        .subscription
        .native_switch
        .as_ref()
        .ok_or_else(invalid)?;
    if serde_json::to_string(policy).map_err(ServiceError::internal)? != expected {
        return Err(invalid());
    }
    let preferred_entry = vault
        .get_provider_summary(preferred)
        .map_err(map_vault_error)?;
    if preferred_entry.archived_at.is_some() || preferred_entry.deleted_at.is_some() {
        return Err(invalid());
    }
    let mut service = state.proxy.lock().map_err(|_| invalid())?;
    let config = service.load_config(vault)?;
    let route = config
        .routes
        .iter()
        .find(|r| r.enabled && r.id == policy.route_id)
        .ok_or_else(invalid)?;
    for id in [preferred, current, selected] {
        let entry = vault.get_provider_summary(id).map_err(map_vault_error)?;
        if entry.archived_at.is_some()
            || entry.deleted_at.is_some()
            || entry.provider_id != preferred_entry.provider_id
            || !route.targets.iter().any(|t| {
                t.enabled
                    && t.provider_entry_id == id
                    && aipass_provider_registry::primary_secret_ref(&entry.secret_refs)
                        .is_some_and(|s| s.id == t.secret_id)
            })
        {
            return Err(invalid());
        }
    }
    if preferred_entry.provider_id.as_deref() == Some("anthropic")
        && route
            .targets
            .iter()
            .any(|t| t.enabled && service.claude_bridge().active(t.provider_entry_id))
    {
        return Err(invalid());
    }
    drop(service);
    let tool = if preferred_entry.provider_id.as_deref() == Some("codex") {
        ToolConfigTool::Codex
    } else {
        ToolConfigTool::ClaudeCode
    };
    let home = crate::server::home_dir(vault)?;
    let expected_identity = vault
        .get_provider_summary(current)
        .map_err(map_vault_error)?
        .account_identity;
    if expected_identity.is_none()
        || active_store(&home, &tool)
            .map_err(safe_error)?
            .identity()
            .map_err(safe_error)?
            != expected_identity
    {
        return Err(invalid());
    }
    Ok(())
}

fn usage(vault: &Vault, entry: &aipass_vault::EntrySummary) -> Option<f64> {
    let windows = if entry.provider_id.as_deref() == Some("codex") {
        crate::community::native_quota(vault, entry.id)
    } else {
        crate::proxy_service::account_quota_with_observations(vault, entry)
    };
    let now = time::OffsetDateTime::now_utc().unix_timestamp() as u64;
    windows
        .iter()
        .filter(|w| {
            w.models.is_none()
                && w.not_models.is_empty()
                && w.model_family.is_none()
                && w.observed_at <= now
                && now - w.observed_at <= 300
                && w.resets_at.is_none_or(|t| t > now)
        })
        .map(|w| w.used_basis_points as f64 / 100.)
        .reduce(f64::max)
}

fn choose(
    preferred: Uuid,
    current: Uuid,
    return_preferred: bool,
    candidates: &[(Uuid, Option<f64>)],
) -> Option<Uuid> {
    let used = |id| {
        candidates
            .iter()
            .find(|(candidate, _)| *candidate == id)
            .and_then(|(_, used)| *used)
    };
    if current != preferred && return_preferred && used(preferred).is_some_and(|v| v < 90.) {
        return Some(preferred);
    }
    if !used(current).is_some_and(|v| v >= 100.) {
        return None;
    }
    candidates
        .iter()
        .find(|(id, used)| *id != current && used.is_some_and(|v| v < 98.))
        .map(|(id, _)| *id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_opt_in_adopts_an_existing_group_account_but_later_manual_changes_win() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let outside = Uuid::new_v4();
        let candidates = [(a, Some(40.)), (b, Some(80.))];
        assert!(can_manage(a, b, None, &candidates));
        assert!(can_manage(a, b, Some(b), &candidates));
        assert!(!can_manage(a, b, Some(a), &candidates));
        assert!(!can_manage(a, outside, None, &candidates));
    }
    #[test]
    fn switching_requires_exhaustion_and_return_uses_hysteresis() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let unknown = Uuid::new_v4();
        assert_eq!(
            choose(
                a,
                a,
                true,
                &[(a, Some(100.)), (unknown, None), (b, Some(40.))]
            ),
            Some(b)
        );
        assert_eq!(choose(a, a, true, &[(a, Some(99.)), (b, Some(40.))]), None);
        assert_eq!(choose(a, b, true, &[(a, Some(91.)), (b, Some(40.))]), None);
        assert_eq!(
            choose(a, b, true, &[(a, Some(40.)), (b, Some(40.))]),
            Some(a)
        );
        assert_eq!(choose(a, b, false, &[(a, Some(40.)), (b, Some(40.))]), None);
        assert_eq!(choose(a, b, true, &[(a, None), (b, None)]), None);
    }
}
