//! Redact structured values before computing a diff, including multiline TOML.
use crate::native_auth::PrivateJson;
use serde_json::Value;
use std::sync::LazyLock;
use toml_edit::{DocumentMut, Item, Table, Value as TomlValue};

fn sensitive(key: &str) -> bool {
    let key = key.to_ascii_lowercase().replace(['_', '-'], "");
    [
        "apikey",
        "apitoken",
        "aipasskey",
        "accesstoken",
        "refreshtoken",
        "idtoken",
        "authtoken",
        "bearertoken",
        "authorization",
        "secret",
        "password",
        "cookie",
    ]
    .iter()
    .any(|part| key.contains(part))
        || key.ends_with("token")
        || matches!(key.as_str(), "auth" | "key" | "refs" | "records")
}
fn helper(key: &str, value: &str) -> bool {
    use base64::Engine;
    static COMMAND: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"^aipass get [0-9a-f-]{36}(?: --secret-id '[0-9a-f-]{36}'| --field api_key)? --reveal$")
            .unwrap()
    });
    if key != "apiKeyHelper" {
        return false;
    }
    if COMMAND.is_match(value) {
        return true;
    }
    let Some(encoded) =
        value.strip_prefix("powershell.exe -NoProfile -NonInteractive -EncodedCommand ")
    else {
        return false;
    };
    if encoded.len() > 2048 {
        return false;
    }
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
        return false;
    };
    if bytes.len() % 2 != 0 {
        return false;
    }
    let units = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    let Ok(script) = String::from_utf16(&units) else {
        return false;
    };
    script
        .strip_prefix("$ErrorActionPreference = 'Stop'; & ")
        .and_then(|s| s.strip_suffix("; exit $LASTEXITCODE"))
        .is_some_and(|command| COMMAND.is_match(command))
}
fn json(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if sensitive(key) && !value.as_str().is_some_and(|v| helper(key, v)) {
                    let _old =
                        PrivateJson(std::mem::replace(value, Value::String("[redacted]".into())));
                } else {
                    json(value);
                }
            }
        }
        Value::Array(values) => values.iter_mut().for_each(json),
        _ => {}
    }
}
fn table(table: &mut Table) {
    for (key, item) in table.iter_mut() {
        if sensitive(key.get()) {
            *item = toml_edit::value("[redacted]");
        } else {
            redact_item(item);
        }
    }
}
fn redact_item(item: &mut Item) {
    match item {
        Item::Table(t) => table(t),
        Item::ArrayOfTables(tables) => tables.iter_mut().for_each(table),
        Item::Value(value) => redact_value(value),
        Item::None => {}
    }
}
fn redact_value(value: &mut TomlValue) {
    match value {
        TomlValue::InlineTable(t) => {
            for (key, value) in t.iter_mut() {
                if sensitive(key.get()) {
                    *value = TomlValue::from("[redacted]");
                } else {
                    redact_value(value);
                }
            }
        }
        TomlValue::Array(values) => values.iter_mut().for_each(redact_value),
        _ => {}
    }
}
pub fn redact_config(content: &str) -> String {
    if let Ok(value) = json5::from_str::<Value>(content) {
        let mut value = PrivateJson(value);
        json(&mut value.0);
        return serde_json::to_string_pretty(&value.0).unwrap_or_else(|_| "[redacted]".into());
    }
    if let Ok(mut doc) = content.parse::<DocumentMut>() {
        table(doc.as_table_mut());
        return doc.to_string();
    }
    if let Ok(value) = serde_yaml_ng::from_str::<Value>(content) {
        if value.is_object() || value.is_array() {
            let mut value = PrivateJson(value);
            json(&mut value.0);
            return serde_yaml_ng::to_string(&value.0).unwrap_or_else(|_| "[redacted]".into());
        }
    }
    if content.lines().any(|line| {
        line.trim_start().starts_with("export ") || line.trim_start().starts_with("$env:")
    }) {
        return content
            .lines()
            .map(|line| {
                if let Some((key, _)) = line.split_once('=') {
                    let name = key
                        .trim()
                        .trim_start_matches("export ")
                        .trim_start_matches("$env:")
                        .trim();
                    if sensitive(name) {
                        return format!("{key}= '[redacted]'");
                    }
                }
                line.to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    // Shell env helpers and legacy line diffs are not structured documents.
    static SECRET: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
        r#"(?i)((?:[\w.-]*["']?)(?:api[_-]?key|aipass[_-]?key|token|authorization|auth|secret|password|cookie)[\w.-]*["']?\s*[:=]\s*)("""[\s\S]*?"""|'''[\s\S]*?'''|"(?:\\.|[^"\\])*"|'[^']*'|[^\r\n]+)"#
    ).unwrap()
    });
    SECRET.replace_all(content, "$1\"[redacted]\"").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_helper_previews_preserve_only_exact_generated_commands() {
        let entry = crate::ToolEntry {
            id: uuid::Uuid::new_v4(),
            secret_id: Some(uuid::Uuid::new_v4().to_string()),
            title: "fixture".into(),
            provider_id: None,
            endpoint: None,
            interface_type: aipass_provider_registry::InterfaceType::AnthropicMessages,
            auth_scheme: aipass_provider_registry::AuthScheme::XApiKey,
            env_key: "API_KEY".into(),
            default_model: None,
            api_key: None,
            supports_websockets: None,
        };
        for secret in [None, entry.secret_id.clone()] {
            let entry = crate::ToolEntry {
                secret_id: secret,
                ..entry.clone()
            };
            for windows in [false, true] {
                let command = entry.credential_command_for(windows);
                let raw = serde_json::json!({"apiKeyHelper":command}).to_string();
                let preview: Value = serde_json::from_str(&redact_config(&raw)).unwrap();
                assert_eq!(preview["apiKeyHelper"], command);
                assert!(!helper(
                    "apiKeyHelper",
                    &format!("{command}; echo private-secret")
                ));
            }
        }
        use base64::Engine;
        let script = "$ErrorActionPreference = 'Stop'; & echo private-secret; exit $LASTEXITCODE";
        let encoded = base64::engine::general_purpose::STANDARD.encode(
            script
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        assert!(!helper(
            "apiKeyHelper",
            &format!("powershell.exe -NoProfile -NonInteractive -EncodedCommand {encoded}")
        ));
        assert!(!helper(
            "apiKeyHelper",
            "powershell.exe -NoProfile -NonInteractive -EncodedCommand invalid"
        ));
    }
    #[test]
    fn structured_tokens_are_redacted_regardless_of_literal_shape() {
        for raw in [
            "experimental_bearer_token = '''\nfake-private-token\n'''\nmodel = 'safe-model'\n",
            "[model_providers.aipass.http_headers]\nAuthorization = [\"fake-private-token\"]\n",
            r#"{"nested":[{"accessToken":"fake-private-token","refreshToken":"fake-private-token","apiKeyHelper":"echo fake-private-token"}]}"#,
            "[model_providers.custom.auth]\ncommand = 'echo'\nargs = ['fake-private-token']\n",
            r#"{"auth":{"command":"echo","args":["fake-private-token"]},"api_token":"fake-private-token"}"#,
            "export AIPASS_KEY_FIXTURE='fake-private-token'\nexport AIPASS_PROXY_TOKEN='fake-private-token'\n",
            "$env:AIPASS_API_KEY = 'prefix''fake-private-token'\r\n",
            r#"{"env":{"AIPASS_KEY_FIXTURE":"fake-private-token","AIPASS_PROXY_TOKEN":"fake-private-token"}}"#,
        ] {
            assert!(!redact_config(raw).contains("fake-private-token"));
        }
        assert!(redact_config("model = 'safe-model'\n").contains("safe-model"));
    }
}
