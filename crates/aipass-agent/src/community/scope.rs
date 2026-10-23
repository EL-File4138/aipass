//! Legacy scope proof is acquired by the Agent with the saved key, outside the vault lock.
use super::*;
use crate::session::SessionState;
use crate::subscription_import::reader::PrivateAuth;

pub(crate) fn read_runtime(
    state: &Arc<AgentState>,
    id: Uuid,
    marker: &str,
) -> ServiceResult<SensitiveString> {
    let (session, raw, outbound) = {
        let guard = state
            .session
            .lock()
            .map_err(|_| invalid("session unavailable"))?;
        let SessionState::Unlocked(info) = &*guard else {
            return Err(ServiceError::new(AgentErrorCode::Locked, "vault is locked"));
        };
        crate::control_panel::check_session(info)?;
        info.vault.ensure_sync_ready().map_err(map_vault_error)?;
        let raw = read(&info.vault, id, marker)?;
        let account = decode(raw.expose())?;
        if account.provider != "devin" || account::storage_key(&info.vault, id)? == KEY_V2 {
            return Ok(raw);
        }
        let prefs = crate::provider_runtime::load(&info.vault, id)?;
        let mut proxy = state
            .proxy
            .lock()
            .map_err(|_| invalid("proxy unavailable"))?;
        let outbound = crate::provider_runtime::outbound(&prefs)
            .map_err(invalid)?
            .unwrap_or(proxy.load_config(&info.vault)?.upstream_proxy);
        (info.id, raw, outbound)
    };
    let old = decode(raw.expose())?;
    let auth = PrivateAuth(
        serde_json::from_str(old.auth.expose())
            .map_err(|_| invalid("invalid Devin credentials"))?,
    );
    let resolved = PrivateAuth(
        crate::subscriptions::devin::identity::resolve_saved(&auth.0, &outbound)
            .map_err(invalid)?,
    );
    let next = migrated(&old, &resolved.0)?;
    let bundle = SecretString::new(serde_json::to_string(&next).map_err(ServiceError::internal)?);
    let next_marker = format!("aipass:community:{}", next.generation);
    let guard = state
        .session
        .lock()
        .map_err(|_| invalid("session unavailable"))?;
    let SessionState::Unlocked(info) = &*guard else {
        return Err(invalid("session changed"));
    };
    if info.id != session {
        return Err(invalid("session changed"));
    }
    crate::control_panel::check_session(info)?;
    info.vault.ensure_sync_ready().map_err(map_vault_error)?;
    commit(&info.vault, id, marker, raw.expose(), &next, &bundle)?;
    state.sync_revision.fetch_add(1, Ordering::Relaxed);
    let mut proxy = state
        .proxy
        .lock()
        .map_err(|_| invalid("proxy unavailable"))?;
    if proxy.refresh_provider_credentials(&info.vault, id).is_err() {
        proxy.reload_if_running(&info.vault)?;
    }
    read(&info.vault, id, &next_marker)
}

fn migrated(old: &Account, auth: &Value) -> ServiceResult<Account> {
    if old.provider != "devin" || !auth["devinScope"].is_object() {
        return Err(invalid("invalid scope migration"));
    }
    let mut next = old.clone();
    next.auth = SensitiveString::new(auth.to_string());
    next.identity = identity(auth).ok_or_else(|| invalid("Devin identity missing"))?;
    next.generation = Uuid::new_v4();
    next.revision = 0;
    next.models = json!({});
    next.feedback = Default::default();
    Ok(next)
}

fn commit(
    vault: &Vault,
    id: Uuid,
    marker: &str,
    old: &str,
    next: &Account,
    bundle: &SecretString,
) -> ServiceResult<()> {
    for entry in vault.list_provider_summaries().map_err(map_vault_error)? {
        if entry.id != id
            && entry.provider_id.as_deref() == Some("devin")
            && entry.account_identity.as_deref() == Some(&next.identity)
        {
            return Err(invalid(
                "Devin organization already exists; resolve the duplicate accounts explicitly",
            ));
        }
    }
    let next_marker = format!("aipass:community:{}", next.generation);
    let result = vault.migrate_subscription_scope(
        id,
        marker,
        &SecretString::new(old),
        &next.identity,
        &next_marker,
        bundle,
    );
    if let Err(error) = result {
        if read(vault, id, &next_marker).is_ok_and(|raw| raw.expose() == bundle.expose()) {
            return Ok(());
        }
        return Err(map_vault_error(error));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
