//! Reread the authoritative account after a vendor refuses a rotating grant.
use super::*;

impl CommunityBridge {
    pub(super) fn reread_for_worker(
        &self,
        id: Uuid,
        marker: &str,
        account: &mut Account,
        worker: &Adapter,
        frame: &Value,
    ) -> Result<(), String> {
        let next = self.load_raw(id, marker)?;
        if next.generation != account.generation
            || next.local_binding != account.local_binding
            || next.identity != account.identity
            || next.provider != account.provider
            || next.native_method != account.native_method
            || next.revision < account.revision
        {
            return Err("subscription binding changed during renewal".into());
        }
        let old = crate::subscription_import::reader::PrivateAuth(
            serde_json::from_str(account.auth.expose()).map_err(|_| "invalid account")?,
        );
        let auth = crate::subscription_import::reader::PrivateAuth(
            serde_json::from_str(next.auth.expose()).map_err(|_| "invalid account")?,
        );
        ensure_owner(&old.0, &auth.0)?;
        worker
            .process
            .send(&json!({"type":"reloaded","id":frame["id"],"value":auth.0}))?;
        *account = next;
        Ok(())
    }
}
