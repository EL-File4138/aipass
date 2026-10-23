//! Release the local usage database before replacing its containing vault.
use super::*;

impl ProxyService {
    pub(super) fn usage_store(&self) -> ServiceResult<&Arc<UsageStore>> {
        self.usage.as_ref().ok_or_else(|| {
            ServiceError::new(
                aipass_agent_protocol::AgentErrorCode::ServiceUnavailable,
                "usage storage unavailable; restart the Agent",
            )
        })
    }

    pub(crate) fn suspend_for_import(&mut self) -> ServiceResult<()> {
        self.stop()?;
        if self
            .usage
            .as_ref()
            .is_some_and(|usage| Arc::strong_count(usage) != 1)
        {
            return Err(ServiceError::new(
                aipass_agent_protocol::AgentErrorCode::ServiceUnavailable,
                "proxy requests are still stopping; retry the import",
            ));
        }
        // SQLite handles prohibit directory renames on Windows. On Unix an
        // open connection would keep writing into the renamed backup instead.
        self.usage.take();
        Ok(())
    }

    pub(crate) fn resume_after_import(&mut self, replaced: bool) -> ServiceResult<()> {
        if replaced {
            self.config = ProxyConfig::default();
            self.pending_disabled_persist = false;
            self.pending_ws_events.clear();
            self.pending_ws_refresh.clear();
            self.claude_bridge = Arc::new(crate::claude_bridge::ClaudeBridge::new(&self.vault_dir));
            self.community_bridge =
                Arc::new(crate::community::CommunityBridge::new(&self.vault_dir));
        }
        if !self.vault_dir.is_dir() {
            return Err(ServiceError::new(
                aipass_agent_protocol::AgentErrorCode::ServiceUnavailable,
                "vault import rollback failed; recover the retained vault backup",
            ));
        }
        self.usage = Some(Arc::new(
            UsageStore::open(self.vault_dir.join("proxy-usage.sqlite"))
                .map_err(ServiceError::internal)?,
        ));
        Ok(())
    }
}
