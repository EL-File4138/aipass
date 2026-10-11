use super::*;
use aipass_config_writers::native_auth::PrivateJson;
use anyhow::{Context, Result};
use serde_json::{json, Value};

pub(super) fn paths(home: &Path, tool: &ToolConfigTool) -> Vec<PathBuf> {
    match tool {
        ToolConfigTool::Codex => vec![std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".codex"))
            .join("config.toml")],
        ToolConfigTool::ClaudeCode => vec![std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".claude"))
            .join("settings.json")],
        ToolConfigTool::GeminiCli => [
            ".env",
            "settings.json",
            "oauth_creds.json",
            "google_accounts.json",
        ]
        .into_iter()
        .map(|f| aipass_config_writers::gemini_config_dir(home).join(f))
        .collect(),
        ToolConfigTool::OpenCode => vec![
            aipass_config_writers::opencode_config_path(home),
            aipass_config_writers::opencode_auth_path(home),
        ],
        ToolConfigTool::Grok => vec![home.join(".grok/config.toml"), home.join(".grok/auth.json")],
        ToolConfigTool::Pi => vec![
            home.join(".pi/agent/models.json"),
            home.join(".pi/agent/settings.json"),
        ],
        ToolConfigTool::Cursor => vec![home.join(".aipass/tools/cursor-agent-local.env")],
    }
}
pub(super) fn document(path: &Path) -> Result<PrivateJson> {
    let Some(raw) = Resource::File(path.to_owned()).read()? else {
        return Ok(PrivateJson(json!({})));
    };
    let text = std::str::from_utf8(&raw).context("invalid credential configuration")?;
    let value = if path.extension().is_some_and(|e| e == "toml") {
        let doc: toml::Value = toml::from_str(text)
            .map_err(|_| anyhow::anyhow!("invalid credential configuration"))?;
        serde_json::to_value(doc)?
    } else {
        json5::from_str(text).map_err(|_| anyhow::anyhow!("invalid credential configuration"))?
    };
    anyhow::ensure!(value.is_object(), "invalid credential configuration");
    Ok(PrivateJson(value))
}
fn field<'a>(doc: &'a Value, key: &str) -> Option<&'a str> {
    doc.get(key)?.as_str().filter(|s| !s.trim().is_empty())
}
fn add(
    out: &mut Vec<Credential>,
    key: Option<&str>,
    endpoint: Option<&str>,
    interface: InterfaceType,
    auth: AuthScheme,
    model: Option<&str>,
) -> Result<()> {
    let Some(key) = key.filter(|s| !s.trim().is_empty()) else {
        return Ok(());
    };
    // References remain owned by the caller's environment/helper. Never execute a command.
    if key.starts_with("{env:")
        || key.starts_with("{file:")
        || key.starts_with('$')
        || key.starts_with('!')
    {
        return Ok(());
    }
    let endpoint = endpoint.context("credential endpoint unavailable")?;
    let url = url::Url::parse(endpoint).context("invalid credential endpoint")?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https"),
        "invalid credential endpoint"
    );
    out.push(Credential {
        key: Zeroizing::new(key.into()),
        endpoint: endpoint.into(),
        interface,
        auth,
        model: model.map(str::to_owned),
    });
    Ok(())
}
fn env(path: &Path) -> Result<PrivateJson> {
    let Some(raw) = Resource::File(path.to_owned()).read()? else {
        return Ok(PrivateJson(json!({})));
    };
    let text = std::str::from_utf8(&raw).context("invalid environment configuration")?;
    let mut values = PrivateJson(json!({}));
    for line in text.lines() {
        let line = line.trim().strip_prefix("export ").unwrap_or(line.trim());
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim_start().starts_with('#') {
            continue;
        }
        let value = value.trim();
        let value = if value.starts_with('"') {
            serde_json::from_str::<String>(value)
                .map_err(|_| anyhow::anyhow!("unsupported environment value"))?
        } else if value.starts_with('\'') && value.ends_with('\'') {
            value[1..value.len() - 1].to_owned()
        } else {
            value.split(" #").next().unwrap_or("").trim().to_owned()
        };
        values.0[key.trim()] = Value::String(value);
    }
    Ok(values)
}
pub(super) fn read(home: &Path, tool: &ToolConfigTool) -> Result<Vec<Credential>> {
    let files = paths(home, tool);
    let mut out = Vec::new();
    let doc = if matches!(tool, ToolConfigTool::GeminiCli | ToolConfigTool::Cursor) {
        env(&files[0])?
    } else {
        document(&files[0])?
    };
    match tool {
        ToolConfigTool::Codex => {
            let provider = field(&doc.0, "model_provider").unwrap_or("openai");
            let selected = &doc.0["model_providers"][provider];
            let endpoint = field(selected, "base_url").unwrap_or("https://api.openai.com/v1");
            let store = native(home, tool)?.context("native store unavailable")?;
            let auth = store
                .primary
                .read()?
                .map(|raw| aipass_config_writers::native_auth::parse(&raw))
                .transpose()?
                .unwrap_or_else(|| PrivateJson(json!({})));
            add(
                &mut out,
                field(&auth.0, "OPENAI_API_KEY"),
                Some(endpoint),
                InterfaceType::OpenAiCompatible,
                AuthScheme::Bearer,
                field(&doc.0, "model"),
            )?;
            if let Some(providers) = doc.0["model_providers"].as_object() {
                for value in providers.values() {
                    add(
                        &mut out,
                        field(value, "experimental_bearer_token"),
                        field(value, "base_url").or(Some("https://api.openai.com/v1")),
                        InterfaceType::OpenAiCompatible,
                        AuthScheme::Bearer,
                        field(&doc.0, "model"),
                    )?;
                }
            }
        }
        ToolConfigTool::ClaudeCode => {
            let env = &doc.0["env"];
            let endpoint = field(env, "ANTHROPIC_BASE_URL").unwrap_or("https://api.anthropic.com");
            for (key, auth) in [
                ("ANTHROPIC_AUTH_TOKEN", AuthScheme::Bearer),
                ("ANTHROPIC_API_KEY", AuthScheme::XApiKey),
            ] {
                add(
                    &mut out,
                    field(env, key),
                    Some(endpoint),
                    InterfaceType::AnthropicMessages,
                    auth,
                    field(env, "ANTHROPIC_MODEL"),
                )?;
            }
        }
        ToolConfigTool::GeminiCli => {
            let settings = document(&files[1])?;
            anyhow::ensure!(
                settings.0["security"]["auth"]["selectedType"]
                    .as_str()
                    .is_none_or(|s| matches!(s, "oauth-personal" | "gemini-api-key")),
                "unsupported Gemini authentication mode"
            );
            anyhow::ensure!(
                field(&doc.0, "GEMINI_API_KEY").is_some()
                    || field(&doc.0, "GOOGLE_API_KEY").is_none(),
                "Vertex API credential import is unavailable"
            );
            add(
                &mut out,
                field(&doc.0, "GEMINI_API_KEY"),
                field(&doc.0, "GOOGLE_GEMINI_BASE_URL")
                    .or(Some("https://generativelanguage.googleapis.com")),
                InterfaceType::Gemini,
                AuthScheme::GoogleApiKey,
                field(&doc.0, "GEMINI_MODEL"),
            )?;
        }
        ToolConfigTool::Cursor => {
            let endpoint = field(&doc.0, "CURSOR_LOCAL_AGENT_BASE_URL");
            let anthropic =
                endpoint.is_some_and(|s| s.trim_end_matches('/').ends_with("/messages"));
            add(
                &mut out,
                field(&doc.0, "CURSOR_LOCAL_AGENT_API_KEY"),
                endpoint,
                if anthropic {
                    InterfaceType::AnthropicMessages
                } else {
                    InterfaceType::OpenAiCompatible
                },
                if anthropic {
                    AuthScheme::XApiKey
                } else {
                    AuthScheme::Bearer
                },
                None,
            )?;
        }
        ToolConfigTool::OpenCode | ToolConfigTool::Pi => {
            let pi = *tool == ToolConfigTool::Pi;
            let settings = if pi {
                document(&files[1])?
            } else {
                PrivateJson(json!({}))
            };
            let auth_store = if pi {
                PrivateJson(json!({}))
            } else {
                document(&files[1])?
            };
            let selected = if pi {
                field(&settings.0, "defaultProvider")
            } else {
                field(&doc.0, "model").and_then(|s| s.split_once('/').map(|(p, _)| p))
            };
            let providers = &doc.0[if pi { "providers" } else { "provider" }];
            if let Some(selected) = selected.filter(|_| !pi) {
                let auth = &auth_store.0[selected];
                let options = &providers[selected]["options"];
                anyhow::ensure!(field(auth, "type") != Some("oauth") || field(options, "apiKey").is_some(), "native OpenCode subscription import is unavailable; retain the original configuration");
                // Built-in providers can use auth.json without a provider section.
                if providers.get(selected).is_none() && field(auth, "type") == Some("api") {
                    let anthropic = selected == "anthropic";
                    let google = selected == "google";
                    let endpoint = match selected {
                        "openai" => Some("https://api.openai.com/v1"),
                        "anthropic" => Some("https://api.anthropic.com"),
                        "google" => Some("https://generativelanguage.googleapis.com/v1beta"),
                        _ => None,
                    };
                    add(
                        &mut out,
                        field(auth, "key"),
                        endpoint,
                        if anthropic {
                            InterfaceType::AnthropicMessages
                        } else if google {
                            InterfaceType::Gemini
                        } else {
                            InterfaceType::OpenAiCompatible
                        },
                        if anthropic {
                            AuthScheme::XApiKey
                        } else if google {
                            AuthScheme::GoogleApiKey
                        } else {
                            AuthScheme::Bearer
                        },
                        field(&doc.0, "model").and_then(|s| s.split_once('/').map(|(_, m)| m)),
                    )?;
                }
            }
            if let Some(providers) = providers.as_object() {
                let mut ordered = providers.iter().collect::<Vec<_>>();
                ordered.sort_by_key(|(id, _)| selected != Some(id.as_str()));
                for (id, value) in ordered {
                    let options = if pi { value } else { &value["options"] };
                    let adapter = field(value, if pi { "api" } else { "npm" });
                    let anthropic =
                        adapter.is_some_and(|s| s.contains("anthropic")) || id == "anthropic";
                    let google = adapter
                        .is_some_and(|s| s == "@ai-sdk/google" || s == "google-generative-ai")
                        || id == "google";
                    if field(options, "apiKey").is_some()
                        || field(&auth_store.0[id], "type") == Some("api")
                    {
                        anyhow::ensure!(
                            adapter.is_none()
                                || anthropic
                                || google
                                || adapter.is_some_and(|s| s.contains("openai")),
                            "unsupported credential adapter"
                        );
                    }
                    let endpoint = field(options, if pi { "baseUrl" } else { "baseURL" }).or(
                        match id.as_str() {
                            "openai" => Some("https://api.openai.com/v1"),
                            "anthropic" => Some("https://api.anthropic.com"),
                            "google" => Some("https://generativelanguage.googleapis.com/v1beta"),
                            _ => None,
                        },
                    );
                    let model = if pi {
                        value["models"]
                            .as_array()
                            .and_then(|a| a.first())
                            .and_then(|m| field(m, "id"))
                    } else {
                        field(&doc.0, "model").and_then(|s| s.split_once('/').map(|(_, m)| m))
                    };
                    add(
                        &mut out,
                        field(options, "apiKey"),
                        endpoint,
                        if anthropic {
                            InterfaceType::AnthropicMessages
                        } else if google {
                            InterfaceType::Gemini
                        } else {
                            InterfaceType::OpenAiCompatible
                        },
                        if anthropic {
                            AuthScheme::XApiKey
                        } else if google {
                            AuthScheme::GoogleApiKey
                        } else {
                            AuthScheme::Bearer
                        },
                        model,
                    )?;
                    let auth = &auth_store.0[id];
                    if field(auth, "type") == Some("api") {
                        add(
                            &mut out,
                            field(auth, "key"),
                            endpoint,
                            if anthropic {
                                InterfaceType::AnthropicMessages
                            } else if google {
                                InterfaceType::Gemini
                            } else {
                                InterfaceType::OpenAiCompatible
                            },
                            if anthropic {
                                AuthScheme::XApiKey
                            } else if google {
                                AuthScheme::GoogleApiKey
                            } else {
                                AuthScheme::Bearer
                            },
                            model,
                        )?;
                    } else if selected == Some(id)
                        && field(auth, "type") == Some("oauth")
                        && field(options, "apiKey").is_none()
                    {
                        anyhow::bail!("native OpenCode subscription import is unavailable; retain the original configuration");
                    }
                }
            }
        }
        ToolConfigTool::Grok => {
            if let Some(models) = doc.0["model"].as_object() {
                let selected = field(&doc.0["models"], "default");
                let mut ordered = models.iter().collect::<Vec<_>>();
                ordered.sort_by_key(|(id, _)| selected != Some(id.as_str()));
                for (_, value) in ordered {
                    let anthropic = field(value, "api_backend") == Some("messages");
                    add(
                        &mut out,
                        field(value, "api_key"),
                        field(value, "base_url"),
                        if anthropic {
                            InterfaceType::AnthropicMessages
                        } else {
                            InterfaceType::OpenAiCompatible
                        },
                        if anthropic {
                            AuthScheme::XApiKey
                        } else {
                            AuthScheme::Bearer
                        },
                        field(value, "model"),
                    )?;
                }
            }
        }
    }
    Ok(out)
}
