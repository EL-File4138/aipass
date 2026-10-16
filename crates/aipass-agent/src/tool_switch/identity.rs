//! Legacy bindings follow an upgraded entry, never an arbitrary same-email login.
use super::*;

pub(super) fn upgrade_binding(vault: &Vault, binding: &mut Binding) -> Result<()> {
    if binding.request.tool != ToolConfigTool::ClaudeCode
        || binding.request.mode != ToolConfigMode::Official
    {
        return Ok(());
    }
    let Some(legacy) = binding.identity.as_deref() else {
        return Ok(());
    };
    if legacy.contains("::org::") {
        return Ok(());
    }
    let Ok(entry) = vault.get_provider_summary(binding.request.id) else {
        return Ok(());
    };
    let Some(scoped) = entry.account_identity.as_deref() else {
        return Ok(());
    };
    if aipass_config_writers::native_auth::claude_identity::is_legacy_of(legacy, scoped) {
        let auth = reference(vault, entry.id, &ToolConfigTool::ClaudeCode)?;
        if auth["accountId"].as_str() == Some(scoped) {
            binding.identity = Some(scoped.to_owned());
        }
    }
    Ok(())
}
