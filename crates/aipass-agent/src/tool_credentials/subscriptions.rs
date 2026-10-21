//! Preserve other native subscriptions whose selection can be replaced by an API.
use super::*;
use aipass_config_writers::transaction::{Change, Transaction};
use serde_json::{json, Value};

fn register(
    vault: &Vault,
    state: &Arc<AgentState>,
    provider: &str,
    auth: Value,
) -> ServiceResult<()> {
    let id = crate::community::register_cli(vault, provider, auth)?;
    // Rebinding an existing native home also changes credentials cached by routes.
    state
        .proxy
        .lock()
        .map_err(|_| error("proxy unavailable"))?
        .refresh_provider_credentials(vault, id)?;
    state
        .sync_revision
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

pub(super) fn active_gemini_identity(
    home: &Path,
    tool: &ToolConfigTool,
) -> ServiceResult<Option<String>> {
    if *tool != ToolConfigTool::GeminiCli {
        return Ok(None);
    }
    let source = aipass_config_writers::gemini_config_dir(home);
    let settings = reader::document(&source.join("settings.json")).map_err(error)?;
    let selected = settings.0["security"]["auth"]["selectedType"].as_str();
    if selected.is_some_and(|s| s != "oauth-personal")
        || Resource::File(source.join("oauth_creds.json"))
            .read()
            .map_err(error)?
            .is_none()
    {
        return Ok(None);
    }
    let root = source
        .parent()
        .ok_or_else(|| error("invalid Gemini home"))?;
    let reference =
        crate::subscriptions::cli_accounts::reference("gemini-cli", root).map_err(error)?;
    Ok(reference["accountId"].as_str().map(str::to_owned))
}

pub(super) fn preserve(
    vault: &Vault,
    state: &Arc<AgentState>,
    home: &Path,
    tool: &ToolConfigTool,
) -> ServiceResult<()> {
    if *tool == ToolConfigTool::Grok {
        let root = aipass_config_writers::grok_config_dir(home);
        if Resource::File(root.join("auth.json"))
            .read()
            .map_err(error)?
            .is_none()
        {
            return Ok(());
        }
        let auth = crate::subscriptions::cli_accounts::reference("grok", &root).map_err(error)?;
        register(vault, state, "grok", auth)?;
    }
    if *tool != ToolConfigTool::GeminiCli {
        return Ok(());
    }
    let source = aipass_config_writers::gemini_config_dir(home);
    if Resource::File(source.join("oauth_creds.json"))
        .read()
        .map_err(error)?
        .is_none()
    {
        return Ok(());
    }
    let source_root = source
        .parent()
        .ok_or_else(|| error("invalid Gemini home"))?;
    let reference =
        crate::subscriptions::cli_accounts::reference("gemini-cli", source_root).map_err(error)?;
    let identity = reference["accountId"]
        .as_str()
        .ok_or_else(|| error("invalid Gemini identity"))?;
    // If a safe, independent native home already exists, keep it. A reference
    // to the active home needs relocation before settings/.env are replaced.
    for entry in vault.list_provider_summaries().map_err(map_vault_error)? {
        if entry.provider_id.as_deref() != Some("gemini-cli")
            || entry.account_identity.as_deref() != Some(identity)
            || entry.archived_at.is_some()
        {
            continue;
        }
        if let Some(bundle) = vault
            .provider_runtime_extension(entry.id, "community_account_v1")
            .map_err(map_vault_error)?
        {
            let bundle: Value = serde_json::from_str(bundle.expose()).map_err(error)?;
            let auth: Value = serde_json::from_str(
                bundle["auth"]
                    .as_str()
                    .ok_or_else(|| error("invalid native reference"))?,
            )
            .map_err(error)?;
            if let Some(saved) = auth["nativeHome"]
                .as_str()
                .filter(|s| Path::new(s) != source_root)
            {
                if crate::subscriptions::cli_accounts::reference("gemini-cli", Path::new(saved))
                    .is_ok_and(|r| r["accountId"].as_str() == Some(identity))
                {
                    return Ok(());
                }
            }
        }
    }
    let operation_id = Uuid::new_v4();
    let archived_root = source
        .join("aipass-accounts")
        .join(operation_id.to_string());
    let archived_config = archived_root.join(".gemini");
    let mut changes = Vec::new();
    for file in [
        "oauth_creds.json",
        "google_accounts.json",
        "settings.json",
        ".env",
    ] {
        let origin = Resource::File(source.join(file));
        let bytes = origin.read().map_err(error)?;
        changes.push(Change::unchanged(origin).map_err(error)?);
        changes.push(
            Change::new(
                Resource::File(archived_config.join(file)),
                bytes.as_deref().map(|b| b.to_vec()),
            )
            .map_err(error)?,
        );
    }
    let tx = Transaction {
        operation_id,
        committed: true,
        changes,
        metadata: json!([]),
    };
    let backup = tx
        .save(
            &state.vault_dir.join("agent-credential-imports"),
            &vault.config_backup_key(),
        )
        .map_err(error)?;
    let committed = tx.apply().map_err(error).and_then(|_| {
        let auth = crate::subscriptions::cli_accounts::reference("gemini-cli", &archived_root)
            .map_err(error)?;
        if auth["accountId"].as_str() != Some(identity) {
            return Err(error("native identity changed"));
        }
        register(vault, state, "gemini-cli", auth)
    });
    if let Err(failure) = committed {
        tx.restore().map_err(error)?;
        std::fs::remove_file(backup).map_err(error)?;
        return Err(failure);
    }
    Ok(())
}
