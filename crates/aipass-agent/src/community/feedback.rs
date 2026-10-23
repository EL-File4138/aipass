//! Publish account observations without rotating proxy capability markers or
//! invalidating active conversations. Account writes retain their normal CAS.
use super::*;

pub(crate) fn write_runtime(
    state: &Arc<AgentState>,
    id: Uuid,
    marker: &str,
    revision: u64,
    raw: &str,
) -> ServiceResult<()> {
    with_vault(state, false, |vault| {
        let before = decode(read(vault, id, marker)?.expose())?;
        let next = decode(raw)?;
        write(vault, id, marker, revision, raw)?;
        if before.feedback != next.feedback {
            let entry = vault.get_provider_summary(id).map_err(map_vault_error)?;
            // Revocation belongs to this device's authorization, while billing
            // windows belong to the account. Do not sync a local login failure.
            let local_revoked = next.local_binding.is_some() && next.feedback.revoked.is_some();
            if let Some(snapshot) = (!local_revoked)
                .then(|| observed_snapshot(entry.subscription, &next))
                .flatten()
            {
                vault
                    .update_provider_subscription(id, Some(snapshot))
                    .map_err(map_vault_error)?;
            }
            state
                .proxy
                .lock()
                .map_err(|_| invalid("proxy unavailable"))?
                .refresh_provider_credentials(vault, id)?;
            state.sync_revision.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    })
}

fn observed_snapshot(
    previous: Option<SubscriptionSnapshot>,
    account: &Account,
) -> Option<SubscriptionSnapshot> {
    let mut snapshot = previous.unwrap_or_default();
    if account.feedback.revoked.is_some() {
        snapshot.error = Some("Subscription sign-in was revoked; sign in again".into());
        snapshot.stale = true;
        return Some(snapshot);
    }
    if account.feedback.windows.is_empty() {
        if snapshot.error.as_deref() == Some("Subscription sign-in was revoked; sign in again") {
            snapshot.error = None;
        }
        return Some(snapshot);
    }
    let mut newest = 0;
    for w in &account.feedback.windows {
        newest = newest.max(w.observed_at);
        let next = SubscriptionWindow {
            id: format!("community_account_observed_{}", w.name),
            label: w.name.clone(),
            used_percent: Some(w.used),
            resets_at: w
                .resets_at
                .and_then(|t| time::OffsetDateTime::from_unix_timestamp(t).ok())
                .and_then(|t| {
                    t.format(&time::format_description::well_known::Rfc3339)
                        .ok()
                }),
            window_minutes: Some(w.span / 60),
            source: Some(format!("community:{}:response", account.provider)),
        };
        if let Some(old) = snapshot.windows.iter_mut().find(|old| {
            old.id.starts_with("community_account_") && old.window_minutes == next.window_minutes
        }) {
            *old = next;
        } else {
            snapshot.windows.push(next);
        }
    }
    snapshot.observed_at = time::OffsetDateTime::from_unix_timestamp(newest as i64)
        .ok()?
        .format(&time::format_description::well_known::Rfc3339)
        .ok()?;
    snapshot.source = format!("community:{}:response", account.provider);
    snapshot.stale = false;
    snapshot.error = None;
    Some(snapshot)
}

pub(super) fn usage(vault: &Vault, id: Uuid, mut stored: Value) -> Value {
    let account = vault
        .reveal_secret(id)
        .ok()
        .and_then(|marker| read(vault, id, &marker).ok())
        .and_then(|raw| decode(raw.expose()).ok());
    let Some(account) = account else {
        return stored;
    };
    if account.feedback.windows.is_empty() {
        return stored;
    }
    let timestamp = stored["observedAt"]
        .as_str()
        .and_then(|s| {
            time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
        })
        .and_then(|t| u64::try_from(t.unix_timestamp()).ok())
        .unwrap_or(0);
    let failed = stored["usage"]["error"].is_string();
    let mut windows = stored["usage"]["windows"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if failed {
        windows.clear();
    }
    for w in &mut windows {
        w["observedAt"] = json!(timestamp);
    }
    for w in account.feedback.windows {
        let matches = |v: &Value| {
            v["span"].as_u64() == Some(w.span)
                && v["aside"] != true
                && v.get("models").is_none()
                && v.get("notModels").is_none()
        };
        if !failed && timestamp > w.observed_at && windows.iter().any(matches) {
            continue;
        }
        windows.retain(|v| !matches(v));
        let reset = w
            .resets_at
            .and_then(|t| time::OffsetDateTime::from_unix_timestamp(t).ok())
            .and_then(|t| {
                t.format(&time::format_description::well_known::Rfc3339)
                    .ok()
            });
        windows.push(json!({"name":w.name,"used":w.used,"span":w.span,"resetsAt":reset,"observedAt":w.observed_at}));
    }
    if !stored.is_object() {
        stored = json!({});
    }
    stored["usage"] = json!({"windows":windows});
    stored
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observations_survive_restart_and_merge_by_scope_span_and_individual_age() {
        let dir = tempfile::tempdir().unwrap();
        let password = SecretString::new("response-feedback-test");
        let vault = Vault::create(dir.path(), &password).unwrap().vault;
        let auth = json!({"type":"oauth","access":"synthetic-only","accountId":"alice"});
        let id = register_cli(&vault, "codex", auth).unwrap();
        let marker = vault.reveal_secret(id).unwrap();
        let mut account = decode(read(&vault, id, &marker).unwrap().expose()).unwrap();
        let at = time::OffsetDateTime::now_utc().unix_timestamp() as u64;
        account.feedback.windows = vec![crate::subscriptions::feedback::Window {
            name: "primary".into(),
            used: 100.,
            span: 18000,
            observed_at: at,
            resets_at: Some(at as i64 + 600),
        }];
        account.revision += 1;
        write(
            &vault,
            id,
            &marker,
            0,
            &serde_json::to_string(&account).unwrap(),
        )
        .unwrap();
        drop(vault);
        let vault = Vault::open(dir.path(), &password).unwrap();
        let older = (time::OffsetDateTime::now_utc() - time::Duration::seconds(30))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let raw = json!({"observedAt":older,"usage":{"windows":[
            {"name":"primary","span":18000,"used":50}, {"name":"secondary","span":604800,"used":50},
            {"name":"scoped","span":18000,"used":70,"models":["separate"]}]}});
        let merged = usage(&vault, id, raw);
        let windows = merged["usage"]["windows"].as_array().unwrap();
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0]["span"], 604800);
        assert!(windows[0]["observedAt"].as_u64().unwrap() < at);
        assert_eq!(windows[1]["models"], json!(["separate"]));
        assert_eq!(windows[2]["used"], 100.);
        assert_eq!(windows[2]["observedAt"], at);
        let failed = usage(
            &vault,
            id,
            json!({"observedAt":now(),"usage":{"error":"429"}}),
        );
        assert_eq!(failed["usage"]["windows"][0]["used"], 100.);
        account.revision += 1;
        assert!(write(
            &vault,
            id,
            "aipass:community:reconnected",
            1,
            &serde_json::to_string(&account).unwrap()
        )
        .is_err());
    }
}
