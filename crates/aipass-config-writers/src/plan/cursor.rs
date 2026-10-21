//! Cursor's separately distributed Local runtime, not the regular cloud-backed CLI.
use super::*;

pub fn plan_cursor_local(home: &Path, entry: &ToolEntry) -> Result<(ConfigPlan, String)> {
    ensure_direct_entry(entry)?;
    plan_with_key(home, entry, None)
}

pub fn plan_cursor_local_plaintext(home: &Path, entry: &ToolEntry) -> Result<(ConfigPlan, String)> {
    ensure_direct_entry(entry)?;
    plan_cursor_local_proxy_plaintext(home, entry)
}

/// AIPass advertises the inbound protocol in `/models` through `api_types`.
/// Agent Local uses that metadata to select Chat, Responses or Messages.
/// A standard Anthropic models response does not provide this extension.
pub fn plan_cursor_local_proxy_plaintext(
    home: &Path,
    entry: &ToolEntry,
) -> Result<(ConfigPlan, String)> {
    let valid = match entry.interface_type {
        InterfaceType::OpenAiCompatible => entry.auth_scheme == AuthScheme::Bearer,
        InterfaceType::AnthropicMessages => {
            matches!(entry.auth_scheme, AuthScheme::XApiKey | AuthScheme::Bearer)
        }
        _ => false,
    };
    anyhow::ensure!(
        valid,
        "Cursor Agent Local requires an OpenAI-compatible or Anthropic Messages proxy route"
    );
    let key = entry
        .api_key
        .as_deref()
        .context("plaintext Cursor local config requires an API key")?;
    plan_with_key(home, entry, Some(key))
}

fn ensure_direct_entry(entry: &ToolEntry) -> Result<()> {
    anyhow::ensure!(
        entry.interface_type == InterfaceType::OpenAiCompatible && entry.auth_scheme == AuthScheme::Bearer,
        "Cursor Agent Local direct integration requires an OpenAI-compatible provider with bearer authentication; use an AIPass local proxy route for Anthropic Messages"
    );
    Ok(())
}

fn plan_with_key(
    home: &Path,
    entry: &ToolEntry,
    api_key: Option<&str>,
) -> Result<(ConfigPlan, String)> {
    let endpoint = entry
        .endpoint
        .as_deref()
        .context("Cursor local config requires an API endpoint")?
        .trim_end_matches('/');
    // Agent Local 2026.10.19 appends `/messages` in its Anthropic SDK.
    // Passing `/v1/messages` as its base produces `/v1/messages/messages`.
    let endpoint = if entry.interface_type == InterfaceType::AnthropicMessages {
        let base = endpoint.strip_suffix("/messages").unwrap_or(endpoint);
        if base.ends_with("/v1") {
            base.to_string()
        } else {
            format!("{base}/v1")
        }
    } else {
        endpoint.to_string()
    };
    let target = crate::helper_path(home, "cursor-agent-local");
    let mut content = crate::shell_env(entry, "CURSOR_LOCAL_AGENT_API_KEY", api_key);
    content.push_str(&crate::env_literal(
        "CURSOR_LOCAL_AGENT_BASE_URL",
        &endpoint,
    ));
    if entry.interface_type == InterfaceType::AnthropicMessages {
        // Import metadata is private to AIPass, not a Cursor runtime setting.
        content.push_str("# AIPASS_CURSOR_API_TYPE=anthropic_messages\n");
    }
    content.push_str(&crate::env_literal("CURSOR_ENABLE_AUTHLESS", "1"));
    let redactions = api_key.into_iter().collect::<Vec<_>>();
    let plan = new_plan(
        ToolId::Cursor,
        target.clone(),
        format!(
            "Configure Cursor Agent Local env helper for {}",
            entry.title
        ),
        redacted_diff_preview(&diff_preview_for_path(&target, &content), &redactions),
    );
    Ok((plan, content))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn anthropic_proxy_uses_a_versioned_base_without_a_messages_suffix() {
        let home = tempfile::tempdir().unwrap();
        let mut entry = ToolEntry {
            supports_websockets: None,
            id: uuid::Uuid::new_v4(),
            secret_id: None,
            title: "Cursor proxy".into(),
            provider_id: None,
            endpoint: None,
            interface_type: InterfaceType::AnthropicMessages,
            auth_scheme: AuthScheme::XApiKey,
            env_key: "AIPASS_PROXY_TOKEN".into(),
            default_model: Some("fixture-model".into()),
            api_key: Some("cursor-proxy-fixture-key".into()),
        };
        for base in [
            "http://127.0.0.1:8787",
            "http://127.0.0.1:8787/v1/",
            "http://127.0.0.1:8787/v1/messages",
        ] {
            entry.endpoint = Some(base.into());
            let (plan, content) = plan_cursor_local_proxy_plaintext(home.path(), &entry).unwrap();
            assert!(content.contains("CURSOR_LOCAL_AGENT_BASE_URL='http://127.0.0.1:8787/v1'"));
            assert!(!plan.preview.contains("cursor-proxy-fixture-key"));
            assert!(content.contains("AIPASS_CURSOR_API_TYPE=anthropic_messages"));
            assert!(plan_cursor_local(home.path(), &entry).is_err());
            assert!(plan_cursor_local_plaintext(home.path(), &entry).is_err());
        }
    }
}
