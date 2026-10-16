//! Explicit account subscription policies. Vendor grants remain CLI-owned.
use crate::session::{map_vault_error, ServiceError, ServiceResult};
use crate::session::{with_vault, AgentState};
use aipass_agent_protocol::SensitiveString;
use aipass_agent_protocol::{AgentErrorCode, SubscriptionPolicy};
use aipass_provider_registry::{CredentialKind, ProviderKind};
use aipass_vault::Vault;
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;
pub(crate) fn validate(vault: &Vault, id: Uuid, policy: &SubscriptionPolicy) -> ServiceResult<()> {
    let entry = vault.get_provider_summary(id).map_err(map_vault_error)?;
    let native = entry.provider_kind == ProviderKind::Official
        && entry.credential_kind == CredentialKind::OAuth;
    let codex = native && entry.provider_id.as_deref() == Some("codex");
    let claude = native && entry.provider_id.as_deref() == Some("anthropic");
    if ((policy.allow_paid_credits || policy.auto_reset_credits) && !codex)
        || (policy.native_switch.is_some() && !codex && !claude)
    {
        return Err(ServiceError::new(
            AgentErrorCode::ValidationFailed,
            "Subscription policy is unavailable for this provider",
        ));
    }
    if policy.native_switch.is_some() {
        for other in vault.list_provider_summaries().map_err(map_vault_error)? {
            if other.id != id
                && other.provider_id == entry.provider_id
                && other.deleted_at.is_none()
                && other.archived_at.is_none()
                && crate::provider_runtime::load(vault, other.id)?
                    .subscription
                    .native_switch
                    .is_some()
            {
                return Err(ServiceError::new(
                    AgentErrorCode::Conflict,
                    "Disable the other preferred account's automatic switching first",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_group(
    state: &Arc<AgentState>,
    vault: &Vault,
    id: Uuid,
    policy: &SubscriptionPolicy,
) -> ServiceResult<()> {
    let Some(policy) = &policy.native_switch else {
        return Ok(());
    };
    let error = || {
        ServiceError::new(AgentErrorCode::ValidationFailed, "Select an enabled group containing this account and another native account of the same provider")
    };
    let entry = vault.get_provider_summary(id).map_err(map_vault_error)?;
    let tool = if entry.provider_id.as_deref() == Some("codex") {
        aipass_agent_protocol::ToolConfigTool::Codex
    } else {
        aipass_agent_protocol::ToolConfigTool::ClaudeCode
    };
    let config = state
        .proxy
        .lock()
        .map_err(|_| error())?
        .load_config(vault)?;
    let route = config
        .routes
        .iter()
        .find(|r| r.id == policy.route_id && r.enabled)
        .ok_or_else(error)?;
    let mut candidates = std::collections::HashSet::new();
    for target in route.targets.iter().filter(|t| t.enabled) {
        let candidate = vault
            .get_provider_summary(target.provider_entry_id)
            .map_err(map_vault_error)?;
        if candidate.provider_id == entry.provider_id
            && candidate.credential_kind == CredentialKind::OAuth
            && candidate.archived_at.is_none()
            && candidate.deleted_at.is_none()
            && crate::tool_switch::validate_reference(vault, candidate.id, &tool).is_ok()
        {
            candidates.insert(candidate.id);
        }
    }
    if candidates.len() < 2 || !candidates.contains(&id) {
        return Err(error());
    }
    Ok(())
}

pub(crate) fn reset(
    state: &Arc<AgentState>,
    id: Uuid,
    request_id: Option<Uuid>,
    credit_id: Option<String>,
) -> ServiceResult<Value> {
    let error = |s| ServiceError::new(AgentErrorCode::ValidationFailed, s);
    let (marker, proxy, bridge) = with_vault(state, true, |vault| {
        let entry = vault.get_provider_summary(id).map_err(map_vault_error)?;
        if entry.provider_id.as_deref() != Some("codex") || !crate::community::is_account(vault, id)
        {
            return Err(error(
                "Reset credits require a connected Codex account".into(),
            ));
        }
        let prefs = crate::provider_runtime::load(vault, id)?;
        let mut proxy = state
            .proxy
            .lock()
            .map_err(|_| error("Proxy unavailable".into()))?;
        let global = proxy.load_config(vault)?.upstream_proxy;
        Ok((
            SensitiveString::new(vault.reveal_secret(id).map_err(map_vault_error)?),
            crate::provider_runtime::outbound(&prefs)
                .map_err(error)?
                .unwrap_or(global),
            proxy.community_bridge(),
        ))
    })?;
    let outcome = bridge
        .reset(id, marker.expose(), &proxy, request_id, credit_id)
        .map_err(error)?;
    // The reset receipt is authoritative even if quota refresh is unavailable.
    let _ = crate::community::refresh(state, id);
    Ok(outcome)
}
