//! BYOK formats published by Meta, Nous Research, Command Code and Tencent.
use crate::additional_tools::{
    configured_home, extra, helper_path, json_object, make_plan, model, object, shell_env, ModelApi,
};
use crate::{ConfigPlan, ToolEntry, ToolId};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub fn byok_config_paths(home: &Path, tool: &ToolId) -> Vec<PathBuf> {
    match tool {
        ToolId::MuseCode => vec![
            crate::xdg_config_home(home).join("muse/settings.json"),
            crate::xdg_config_home(home).join("muse/auth.json"),
            helper_path(home, "muse-code"),
        ],
        ToolId::HermesAgent => vec![
            configured_home(home, "HERMES_HOME", ".hermes").join("config.yaml"),
            helper_path(home, "hermes-agent"),
        ],
        ToolId::CommandCode => vec![
            home.join(".commandcode/providers.json"),
            home.join(".commandcode/settings.json"),
            home.join(".commandcode/auth.json"),
        ],
        // WorkBuddy explicitly retains CodeBuddy's user-level models.json.
        ToolId::Codebuddy | ToolId::Workbuddy => vec![home.join(".codebuddy/models.json")],
        _ => vec![],
    }
}

pub(super) fn plan_byok_tool(
    home: &Path,
    tool: ToolId,
    entry: &ToolEntry,
    plaintext: bool,
    api: ModelApi,
) -> Result<(ConfigPlan, String)> {
    let paths = byok_config_paths(home, &tool);
    let path = paths
        .first()
        .context("use the Agent's model setup guide")?
        .clone();
    let model = model(entry)?;
    let endpoint = entry
        .endpoint
        .as_deref()
        .context("Agent endpoint required")?;
    let key = if plaintext {
        Some(entry.api_key.as_deref().context("API key required")?)
    } else {
        None
    };
    match tool {
        ToolId::MuseCode => {
            anyhow::ensure!(endpoint.trim_end_matches('/') == "https://api.meta.ai/v1", "Muse Code requires a Meta model catalog; custom Responses endpoints are not supported by this integration");
            anyhow::ensure!(
                api == ModelApi::Responses,
                "Muse Code requires the OpenAI Responses API"
            );
            let mut root = json_object(&path)?;
            root["schema_version"] = json!(1);
            root["provider"] = json!("meta");
            root["model"] = json!(model);
            root["endpoint_transport"] = json!({"base_url":endpoint, "auth":"bearer"});
            let (mut plan, content) =
                make_plan(home, tool, path, serde_json::to_string_pretty(&root)?);
            if let Some(key) = key {
                let mut auth = json_object(&paths[1])?;
                auth["schema_version"] = json!(1);
                object(&mut auth, "providers")?.insert("meta".into(), json!({"api_key":key}));
                extra(
                    &mut plan,
                    paths[1].clone(),
                    serde_json::to_string_pretty(&auth)?,
                );
            } else {
                extra(
                    &mut plan,
                    paths[2].clone(),
                    shell_env(entry, "META_API_KEY", None),
                );
            }
            Ok((plan, content))
        }
        ToolId::HermesAgent => {
            anyhow::ensure!(
                api != ModelApi::Gemini,
                "Hermes integration requires an OpenAI or Anthropic endpoint"
            );
            let mut root = crate::read_yaml(&path, json!({}))?;
            let model_config = json!({"provider":"custom", "default":model, "base_url":endpoint,
                "api_mode": match api {ModelApi::Responses=>"codex_responses",ModelApi::Messages=>"anthropic_messages",_=>"chat_completions"},
                "api_key":key.unwrap_or("${AIPASS_HERMES_API_KEY}")});
            if root["model"] == "" {
                root["model"] = json!({});
            }
            let selected = object(&mut root, "model")?;
            for key in [
                "provider", "default", "model", "base_url", "api_mode", "api_key", "key_env",
                "key_cmd",
            ] {
                selected.remove(key);
            }
            selected.extend(model_config.as_object().unwrap().clone());
            let (mut plan, content) = make_plan(home, tool, path, serde_yaml_ng::to_string(&root)?);
            if key.is_none() {
                extra(
                    &mut plan,
                    paths[1].clone(),
                    shell_env(entry, "AIPASS_HERMES_API_KEY", None),
                );
            }
            Ok((plan, content))
        }
        ToolId::CommandCode => {
            anyhow::ensure!(
                api != ModelApi::Gemini,
                "Command Code requires an OpenAI or Anthropic endpoint"
            );
            // A credential-specific namespace avoids colliding with a saved
            // vendor key in auth.json, which takes precedence over key helpers.
            let provider = format!(
                "aipass-{}-{}",
                entry.id.simple(),
                crate::utils::slug(entry.secret_id.as_deref().unwrap_or("proxy"))
            );
            let auth = json_object(&paths[2])?;
            anyhow::ensure!(!serde_json::to_string(&auth)?.contains(&provider), "saved Command Code authentication overrides this provider; remove it through /connect first");
            let mut root = json_object(&path)?;
            object(&mut root, "provider")?.insert(provider.clone(), json!({"name":"AIPass", "baseURL":endpoint,
                "api":match api {ModelApi::Responses=>"openai-responses",ModelApi::Messages=>"anthropic-messages",_=>"openai-completions"},
                "apiKey":format!("!{}", entry.credential_command()), "models":{model:{}}}));
            let (mut plan, content) =
                make_plan(home, tool, path, serde_json::to_string_pretty(&root)?);
            // Local routes have no vault credential ID. Use a sourced token
            // instead of generating an invalid `aipass get <route>` command.
            if let Some(key) = key {
                let mut root: Value = serde_json::from_str(&content)?;
                root["provider"][&provider]["apiKey"] = json!("$AIPASS_COMMAND_TOKEN");
                let content = serde_json::to_string_pretty(&root)?;
                extra(
                    &mut plan,
                    helper_path(home, "command-code"),
                    shell_env(entry, "AIPASS_COMMAND_TOKEN", Some(key)),
                );
                let mut settings = json_object(&paths[1])?;
                settings["model"] = json!(format!("{provider}/{model}"));
                extra(
                    &mut plan,
                    paths[1].clone(),
                    serde_json::to_string_pretty(&settings)?,
                );
                return Ok((plan, content));
            }
            let mut settings = json_object(&paths[1])?;
            settings["model"] = json!(format!("{provider}/{model}"));
            extra(
                &mut plan,
                paths[1].clone(),
                serde_json::to_string_pretty(&settings)?,
            );
            Ok((plan, content))
        }
        ToolId::Codebuddy | ToolId::Workbuddy => {
            anyhow::ensure!(
                api == ModelApi::ChatCompletions,
                "CodeBuddy and WorkBuddy models.json requires Chat Completions"
            );
            let key = key.context("editor model configuration requires explicit plaintext mode")?;
            let mut root = crate::utils::read_json_value(&path)
                .map_err(|_| anyhow::anyhow!("invalid editor model configuration"))?;
            let models = if root.is_array() {
                root.as_array_mut().unwrap()
            } else {
                root.as_object_mut()
                    .context("invalid editor models")?
                    .entry("models")
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                    .context("invalid editor model list")?
            };
            let configured = json!({"id":model,"name":format!("AIPass: {model}"),"vendor":"AIPass","apiKey":key,
                "url":full_endpoint(endpoint, "chat/completions"),"supportsToolCall":true});
            if let Some(old) = models.iter_mut().find(|m| m["id"] == model) {
                let old = old.as_object_mut().context("invalid model entry")?;
                old.extend(configured.as_object().unwrap().clone());
            } else {
                models.push(configured);
            }
            if let Some(available) = root.get_mut("availableModels") {
                let available = available
                    .as_array_mut()
                    .context("invalid availableModels")?;
                if !available.iter().any(|m| m == model) {
                    available.push(json!(model));
                }
            }
            Ok(make_plan(
                home,
                tool,
                path,
                serde_json::to_string_pretty(&root)?,
            ))
        }
        _ => anyhow::bail!("use the Agent's model setup guide"),
    }
}
fn full_endpoint(endpoint: &str, suffix: &str) -> String {
    let endpoint = endpoint.trim_end_matches('/');
    if endpoint.ends_with(suffix) {
        endpoint.into()
    } else {
        format!("{endpoint}/{suffix}")
    }
}

#[cfg(test)]
mod tests;
