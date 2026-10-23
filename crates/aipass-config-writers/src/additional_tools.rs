//! Native configuration for Kimi Code, Zed, DeepSeek Harness and Antigravity CLI.
//! Schemas and limitations are documented in docs/agent-integrations.md.
use crate::env_helper::env_literal;
pub(super) use crate::env_helper::{helper_path, shell_env};
use crate::models::{ConfigPlan, PlannedWrite, ToolEntry, ToolId};
use crate::utils::{ensure_table, new_plan, read_toml};
use aipass_provider_registry::{AuthScheme, InterfaceType};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use toml_edit::value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelApi {
    ChatCompletions,
    Responses,
    Messages,
    Gemini,
}

pub fn kimi_config_dir(home: &Path) -> PathBuf {
    configured_home(home, "KIMI_CODE_HOME", ".kimi-code")
}
pub fn dsh_home(home: &Path) -> PathBuf {
    configured_home(home, "DSH_HOME", ".dsh")
}
pub(super) fn configured_home(home: &Path, key: &str, fallback: &str) -> PathBuf {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(fallback))
}
pub fn zed_config_path(home: &Path) -> PathBuf {
    #[cfg(target_os = "linux")]
    if crate::installed_apps::zed_uses_flatpak(home) {
        return home.join(".var/app/dev.zed.Zed/config/zed/settings.json");
    }
    #[cfg(windows)]
    {
        crate::utils::resolve_profile_directory(
            home,
            std::env::var_os("APPDATA").map(PathBuf::from).as_deref(),
            crate::user_home_dir().as_deref(),
            "AppData/Roaming",
        )
        .join("Zed/settings.json")
    }
    #[cfg(not(windows))]
    crate::xdg_config_home(home).join("zed/settings.json")
}

pub fn read_yaml(path: &Path, fallback: Value) -> Result<Value> {
    match std::fs::read_to_string(path) {
        Ok(raw) => {
            let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&zeroize::Zeroizing::new(raw))
                .map_err(|_| anyhow::anyhow!("invalid YAML configuration"))?;
            serde_json::to_value(yaml).map_err(|_| anyhow::anyhow!("invalid YAML configuration"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(fallback),
        Err(_) => anyhow::bail!("configuration unavailable"),
    }
}

pub(super) fn json_object(path: &Path) -> Result<Value> {
    let root = crate::utils::read_json_value(path)
        .map_err(|_| anyhow::anyhow!("invalid JSON configuration"))?;
    anyhow::ensure!(root.is_object(), "configuration must be an object");
    Ok(root)
}
pub(super) fn object<'a>(
    root: &'a mut Value,
    key: &str,
) -> Result<&'a mut serde_json::Map<String, Value>> {
    let root = root
        .as_object_mut()
        .context("configuration must be an object")?;
    root.entry(key)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("configuration section must be an object")
}
pub(super) fn extra(plan: &mut ConfigPlan, path: PathBuf, content: String) {
    plan.extra_writes.push(PlannedWrite {
        backup_path: plan.backup_path.with_file_name(format!(
            "{}-extra-{}.aipbackup",
            plan.operation_id,
            plan.extra_writes.len()
        )),
        target_path: path,
        content,
    });
}
pub(super) fn make_plan(
    home: &Path,
    tool: ToolId,
    path: PathBuf,
    content: String,
) -> (ConfigPlan, String) {
    let before = std::fs::read_to_string(&path).unwrap_or_default();
    let preview = crate::diff_preview_from(
        &crate::redact_config(&before),
        &crate::redact_config(&content),
    );
    let mut plan = new_plan(
        tool,
        path,
        "Configure the selected Agent credential".into(),
        preview,
    );
    // Group every file under the discoverable helper backup root so a single
    // rollback restores settings and credentials even across vendor directories.
    plan.backup_path = home
        .join(".aipass/tools/.aipass-backups")
        .join(format!("{}-main.aipbackup", plan.operation_id));
    (plan, content)
}
pub(super) fn model(entry: &ToolEntry) -> Result<&str> {
    entry
        .default_model
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .context("Agent integration requires a default model")
}
fn api_for_entry(entry: &ToolEntry) -> Result<ModelApi> {
    match (&entry.interface_type, &entry.auth_scheme) {
        (InterfaceType::OpenAiCompatible, AuthScheme::Bearer) => Ok(ModelApi::ChatCompletions),
        (InterfaceType::AnthropicMessages, AuthScheme::Bearer | AuthScheme::XApiKey) => {
            Ok(ModelApi::Messages)
        }
        (InterfaceType::Gemini, AuthScheme::GoogleApiKey) => Ok(ModelApi::Gemini),
        _ => anyhow::bail!("unsupported Agent credential protocol"),
    }
}

/// Helper mode keeps the selected key in the vault. Plaintext mode writes only
/// the tool's supported credential store (or an explicitly sourced shell file).
pub fn plan_additional_tool(
    home: &Path,
    tool: ToolId,
    entry: &ToolEntry,
    plaintext: bool,
    api: Option<ModelApi>,
) -> Result<(ConfigPlan, String)> {
    let supported = api_for_entry(entry)?;
    let api = api.unwrap_or(supported);
    anyhow::ensure!(
        api == supported || (supported == ModelApi::ChatCompletions && api == ModelApi::Responses),
        "incompatible Agent API protocol"
    );
    let endpoint = entry
        .endpoint
        .as_deref()
        .context("Agent integration requires an endpoint")?;
    let normalized_endpoint;
    let endpoint = if api == ModelApi::Messages
        && matches!(
            tool,
            ToolId::KimiCode | ToolId::Zed | ToolId::DeepseekHarness
        ) {
        normalized_endpoint = crate::utils::anthropic_sdk_base_url(endpoint);
        normalized_endpoint.as_str()
    } else {
        endpoint
    };
    anyhow::ensure!(
        !endpoint.chars().any(char::is_control),
        "invalid Agent endpoint"
    );
    let key = if plaintext {
        Some(
            entry
                .api_key
                .as_deref()
                .filter(|k| !k.trim().is_empty())
                .context("API key required")?,
        )
    } else {
        None
    };
    anyhow::ensure!(
        !key.is_some_and(|key| key.chars().any(char::is_control)),
        "invalid Agent API key"
    );
    if matches!(
        tool,
        ToolId::MuseCode
            | ToolId::HermesAgent
            | ToolId::CommandCode
            | ToolId::Codebuddy
            | ToolId::Workbuddy
    ) {
        // Muse's native model wire is Responses even for a generic OpenAI credential.
        let api = if tool == ToolId::MuseCode && api == ModelApi::ChatCompletions {
            ModelApi::Responses
        } else {
            api
        };
        return crate::byok_tools::plan_byok_tool(home, tool, entry, plaintext, api);
    }
    match tool {
        ToolId::KimiCode => {
            let model = model(entry)?;
            let path = kimi_config_dir(home).join("config.toml");
            let mut doc =
                read_toml(&path).map_err(|_| anyhow::anyhow!("invalid TOML configuration"))?;
            let provider = ensure_table(&mut doc, "providers")?;
            // Own one provider slot; preserve unrelated providers, OAuth and preferences.
            provider["aipass"] = toml_edit::Item::Table(toml_edit::Table::new());
            let provider = provider["aipass"]
                .as_table_mut()
                .context("invalid provider")?;
            provider["type"] = value(match api {
                ModelApi::ChatCompletions => "openai",
                ModelApi::Responses => "openai_responses",
                ModelApi::Messages => "anthropic",
                ModelApi::Gemini => "google-genai",
            });
            provider["base_url"] = value(if api == ModelApi::Gemini {
                endpoint.trim_end_matches('/').trim_end_matches("/v1beta")
            } else {
                endpoint
            });
            provider[if key.is_some() {
                "api_key"
            } else {
                "api_key_env"
            }] = value(key.unwrap_or(&entry.env_key));
            let alias = format!("aipass/{model}");
            let models = ensure_table(&mut doc, "models")?;
            models[&alias] = toml_edit::Item::Table(toml_edit::Table::new());
            let selected = models[&alias].as_table_mut().context("invalid model")?;
            selected["provider"] = value("aipass");
            selected["model"] = value(model);
            selected["max_context_size"] = value(32768);
            doc["default_model"] = value(alias);
            let (mut plan, content) = make_plan(home, tool, path, doc.to_string());
            if key.is_none() {
                extra(
                    &mut plan,
                    helper_path(home, "kimi-code"),
                    shell_env(entry, &entry.env_key, None),
                );
            }
            Ok((plan, content))
        }
        ToolId::Zed => {
            anyhow::ensure!(
                api != ModelApi::Gemini,
                "Zed integration requires an OpenAI or Anthropic endpoint"
            );
            let model = model(entry)?;
            let path = zed_config_path(home);
            let mut root = json_object(&path)?;
            let group = if api == ModelApi::Messages {
                "anthropic_compatible"
            } else {
                "openai_compatible"
            };
            let providers = object(&mut root, "language_models")?;
            for family in ["anthropic_compatible", "openai_compatible"] {
                if let Some(existing) = providers.get_mut(family) {
                    existing
                        .as_object_mut()
                        .context("invalid Zed provider map")?
                        .remove("aipass");
                }
            }
            let group = providers.entry(group).or_insert_with(|| json!({}));
            let mut provider = json!({ "api_url": endpoint, "available_models": [{ "name": model, "max_tokens": 32768 }] });
            if api != ModelApi::Messages {
                provider["available_models"][0]["capabilities"] =
                    json!({"chat_completions": api != ModelApi::Responses});
            }
            group
                .as_object_mut()
                .context("invalid Zed provider map")?
                .insert("aipass".into(), provider);
            object(&mut root, "agent")?.insert(
                "default_model".into(),
                json!({"provider":"aipass", "model": model}),
            );
            let (mut plan, content) =
                make_plan(home, tool, path, serde_json::to_string_pretty(&root)?);
            extra(
                &mut plan,
                helper_path(home, "zed"),
                shell_env(entry, "AIPASS_API_KEY", key),
            );
            Ok((plan, content))
        }
        ToolId::DeepseekHarness => {
            anyhow::ensure!(
                api != ModelApi::Gemini,
                "DeepSeek Harness integration requires an OpenAI or Anthropic endpoint"
            );
            let model = model(entry)?;
            let path = dsh_home(home).join("profiles/web/cordis.patch.yml");
            let mut root = read_yaml(&path, json!([]))?;
            let plugins = root
                .as_array_mut()
                .context("profile patch must be an array")?;
            let config = dsh_plugin(plugins, "llm-pi-ai")?;
            object(config, "providers")?.insert("aipass".into(), json!({
                "displayName": "AIPass", "apiKeyEnv": "AIPASS_DSH_API_KEY", "baseURL": endpoint,
                "api": match api { ModelApi::Responses => "openai-responses", ModelApi::Messages => "anthropic-messages", _ => "openai-completions" },
                "models": [{"id": model}]
            }));
            let defaults = dsh_plugin(plugins, "agent-default-model")?;
            defaults["provider"] = json!("aipass");
            defaults["model"] = json!(model);
            let (mut plan, content) = make_plan(home, tool, path, serde_yaml_ng::to_string(&root)?);
            if let Some(key) = key {
                let path = dsh_home(home).join(".credentials.yaml");
                let mut credentials = read_yaml(&path, json!({"version":1}))?;
                anyhow::ensure!(
                    credentials["version"] == 1,
                    "unsupported Harness credential format"
                );
                object(&mut credentials, "refs")?.insert("AIPASS_DSH_API_KEY".into(), json!(key));
                extra(&mut plan, path, serde_yaml_ng::to_string(&credentials)?);
            } else {
                // A saved ref outranks the environment. Refuse to misreport a switch.
                let credentials = read_yaml(&dsh_home(home).join(".credentials.yaml"), json!({}))?;
                anyhow::ensure!(
                    credentials["refs"]["AIPASS_DSH_API_KEY"].is_null(),
                    "saved Harness credential requires plaintext mode"
                );
                extra(
                    &mut plan,
                    helper_path(home, "deepseek-harness"),
                    shell_env(entry, "AIPASS_DSH_API_KEY", None),
                );
            }
            Ok((plan, content))
        }
        ToolId::AntigravityCli => {
            anyhow::ensure!(
                api == ModelApi::Gemini,
                "Antigravity CLI requires a Gemini-native endpoint"
            );
            let path = home.join(".gemini/antigravity-cli/settings.json");
            let mut root = json_object(&path)?;
            root["modelProvider"] = json!("gemini");
            let (mut plan, content) =
                make_plan(home, tool, path, serde_json::to_string_pretty(&root)?);
            let mut env = shell_env(entry, "GEMINI_API_KEY", key);
            env.push_str(&env_literal(
                "GOOGLE_GEMINI_BASE_URL",
                endpoint.trim_end_matches('/').trim_end_matches("/v1beta"),
            ));
            extra(&mut plan, helper_path(home, "antigravity-cli"), env);
            Ok((plan, content))
        }
        _ => anyhow::bail!("unsupported additional Agent"),
    }
}
fn dsh_plugin<'a>(plugins: &'a mut Vec<Value>, id: &str) -> Result<&'a mut Value> {
    anyhow::ensure!(
        plugins.iter().filter(|p| p["id"] == id).count() <= 1,
        "duplicate Harness provider plugin; resolve the existing profile first"
    );
    let index = plugins.iter().position(|p| p["id"] == id);
    let index = index.unwrap_or_else(|| {
        plugins.push(json!({"id":id,"config":{}}));
        plugins.len() - 1
    });
    let plugin = plugins[index]
        .as_object_mut()
        .context("invalid profile plugin")?;
    let config = plugin.entry("config").or_insert_with(|| json!({}));
    anyhow::ensure!(config.is_object(), "invalid profile config");
    Ok(config)
}
#[cfg(test)]
mod tests;
