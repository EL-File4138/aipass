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
    if policy
        .quota_use_limit_percent
        .is_some_and(|n| n == 0 || n > 100)
    {
        return Err(ServiceError::new(
            AgentErrorCode::ValidationFailed,
            "Quota usage limit must be 1–100 percent",
        ));
    }
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

/// Routing uses the configured headroom; the UI keeps the actual vendor reading.
pub(crate) fn capped_quota(
    policy: &SubscriptionPolicy,
    mut windows: Vec<aipass_proxy::QuotaWindow>,
) -> Vec<aipass_proxy::QuotaWindow> {
    if let Some(cap) = policy
        .quota_use_limit_percent
        .filter(|n| *n > 0 && *n < 100)
    {
        for window in &mut windows {
            window.used_basis_points =
                (u32::from(window.used_basis_points) * 100 / u32::from(cap)).min(10000) as u16;
        }
    }
    windows
}
#[cfg(test)]
mod quota_tests {
    use super::*;
    #[test]
    fn headroom_changes_only_routing_and_preserves_scope_and_freshness() {
        let window = aipass_proxy::QuotaWindow {
            model_family: None,
            models: Some(vec!["paid".into()]),
            not_models: vec!["free".into()],
            used_basis_points: 8000,
            observed_at: 100,
            resets_at: Some(200),
            span_seconds: Some(18000),
        };
        let default = SubscriptionPolicy::default();
        assert_eq!(
            capped_quota(&default, vec![window.clone()]),
            vec![window.clone()]
        );
        let policy = SubscriptionPolicy {
            quota_use_limit_percent: Some(80),
            ..default
        };
        let capped = capped_quota(&policy, vec![window.clone()]);
        assert_eq!(capped[0].used_basis_points, 10000);
        assert_eq!(capped[0].models, window.models);
        assert_eq!(capped[0].resets_at, window.resets_at);
        assert!(capped[0].current_usage(201).is_none());
        assert_eq!(window.used_basis_points, 8000);
    }
}
