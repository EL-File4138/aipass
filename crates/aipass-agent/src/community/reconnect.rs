//! Explicit local authorization keeps the selected logical account and its routes.
use super::*;
use crate::session::SessionState;

pub(super) struct Target {
    id: Uuid,
    session: Uuid,
    descriptor: SensitiveString,
    binding: Option<Uuid>,
}
impl Target {
    pub(super) fn capture(
        state: &Arc<AgentState>,
        id: Uuid,
        provider: &str,
    ) -> ServiceResult<Self> {
        let guard = state
            .session
            .lock()
            .map_err(|_| invalid("session unavailable"))?;
        let SessionState::Unlocked(info) = &*guard else {
            return Err(invalid("vault is locked"));
        };
        crate::control_panel::check_session(info)?;
        info.vault.ensure_sync_ready().map_err(map_vault_error)?;
        let entry = info
            .vault
            .get_provider_summary(id)
            .map_err(map_vault_error)?;
        if entry.provider_id.as_deref() != Some(provider)
            || entry.archived_at.is_some()
            || entry.deleted_at.is_some()
        {
            return Err(invalid("subscription unavailable"));
        }
        let raw = stored(&info.vault, id)?.ok_or_else(|| invalid("subscription unavailable"))?;
        let account = decode(raw.expose())?;
        let local = account::storage_key(&info.vault, id)? == device::KEY;
        if !local && !device::requires_local(&account)? {
            return Err(invalid(
                "this subscription does not use managed local authorization",
            ));
        }
        let binding = if local {
            device::local(&info.vault, &account)
                .ok()
                .flatten()
                .and_then(|a| a.local_binding)
        } else {
            None
        };
        Ok(Self {
            id,
            session: info.id,
            descriptor: SensitiveString::new(raw.expose()),
            binding,
        })
    }
    pub(super) fn id(&self) -> Uuid {
        self.id
    }
    pub(super) fn commit(&self, vault: &Vault, account: Account) -> ServiceResult<Uuid> {
        // with_session holds the original unlocked session through persistence.
        // Recheck the selected descriptor and local bind under that same lock.
        let raw = stored(vault, self.id)?.ok_or_else(|| invalid("subscription unavailable"))?;
        if raw.expose() != self.descriptor.expose() {
            return Err(invalid("subscription changed during sign-in; retry"));
        }
        let before = decode(raw.expose())?;
        let local = account::storage_key(vault, self.id)? == device::KEY;
        let binding = if local {
            device::local(vault, &before)
                .ok()
                .flatten()
                .and_then(|a| a.local_binding)
        } else {
            None
        };
        if binding != self.binding {
            return Err(invalid("local authorization changed during sign-in; retry"));
        }
        let entry = vault
            .get_provider_summary(self.id)
            .map_err(map_vault_error)?;
        if entry.archived_at.is_some() || entry.deleted_at.is_some() {
            return Err(invalid("subscription unavailable"));
        }
        device::bind(vault, self.id, account)?;
        Ok(self.id)
    }
}

pub(super) fn with_session<T>(
    state: &Arc<AgentState>,
    target: Option<&Target>,
    f: impl FnOnce(&Vault) -> ServiceResult<T>,
) -> ServiceResult<T> {
    let Some(target) = target else {
        return with_vault(state, false, f);
    };
    let guard = state
        .session
        .lock()
        .map_err(|_| invalid("session unavailable"))?;
    let SessionState::Unlocked(info) = &*guard else {
        return Err(invalid("vault is locked"));
    };
    if info.id != target.session {
        return Err(invalid("session changed during sign-in"));
    }
    crate::control_panel::check_session(info)?;
    info.vault.ensure_sync_ready().map_err(map_vault_error)?;
    f(&info.vault)
}

#[cfg(test)]
mod tests;
