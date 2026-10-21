use super::*;
use aipass_provider_registry::{AuthScheme, InterfaceType};
use tempfile::tempdir;

fn entry() -> ToolEntry {
    ToolEntry {
        id: uuid::Uuid::new_v4(),
        secret_id: Some("selected-key".into()),
        title: "Fixture".into(),
        provider_id: None,
        endpoint: Some("https://fixture.test/v1".into()),
        interface_type: InterfaceType::OpenAiCompatible,
        auth_scheme: AuthScheme::Bearer,
        env_key: "AIPASS_KEY_FIXTURE".into(),
        default_model: Some("fixture-model".into()),
        api_key: Some("private-fixture-key".into()),
        supports_websockets: None,
    }
}
fn write(path: &Path, raw: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, raw).unwrap();
}

#[test]
fn kimi_preserves_preferences_and_selects_a_protocol_specific_model() {
    let dir = tempdir().unwrap();
    let entry = entry();
    let target = kimi_config_dir(dir.path()).join("config.toml");
    write(
        &target,
        "telemetry = false\n[providers.original]\ntype = 'openai'\napi_key = 'old-private-key'\n",
    );
    let (plan, content) = plan_additional_tool(
        dir.path(),
        ToolId::KimiCode,
        &entry,
        false,
        Some(ModelApi::Responses),
    )
    .unwrap();
    let doc = content.parse::<toml_edit::DocumentMut>().unwrap();
    assert_eq!(
        doc["providers"]["aipass"]["type"].as_str(),
        Some("openai_responses")
    );
    assert_eq!(
        doc["providers"]["aipass"]["api_key_env"].as_str(),
        Some("AIPASS_KEY_FIXTURE")
    );
    assert_eq!(
        doc["providers"]["original"]["api_key"].as_str(),
        Some("old-private-key")
    );
    assert_eq!(doc["default_model"].as_str(), Some("aipass/fixture-model"));
    assert_eq!(doc["telemetry"].as_bool(), Some(false));
    assert!(!content.contains("private-fixture-key"));
    assert!(!plan.preview.contains("old-private-key"));
    assert!(plan.extra_writes[0]
        .content
        .contains("--secret-id 'selected-key'"));
    assert!(std::fs::read_to_string(target)
        .unwrap()
        .contains("telemetry = false"));
}

#[test]
fn zed_keeps_keys_out_of_settings_and_removes_the_previous_protocol_slot() {
    let dir = tempdir().unwrap();
    let entry = entry();
    let target = zed_config_path(dir.path());
    write(
        &target,
        r#"{"theme":"Original","language_models":{"anthropic_compatible":{"aipass":{"api_url":"https://old.test"},"keep":{"api_url":"https://keep.test"}}}}"#,
    );
    let (plan, content) = plan_additional_tool(
        dir.path(),
        ToolId::Zed,
        &entry,
        true,
        Some(ModelApi::Responses),
    )
    .unwrap();
    let root: Value = serde_json::from_str(&content).unwrap();
    assert_eq!(root["theme"], "Original");
    assert_eq!(root["agent"]["default_model"]["provider"], "aipass");
    assert_eq!(
        root["language_models"]["openai_compatible"]["aipass"]["available_models"][0]
            ["capabilities"]["chat_completions"],
        false
    );
    assert!(root["language_models"]["anthropic_compatible"]["aipass"].is_null());
    assert_eq!(
        root["language_models"]["anthropic_compatible"]["keep"]["api_url"],
        "https://keep.test"
    );
    assert!(!content.contains("private-fixture-key"));
    assert!(plan.extra_writes[0]
        .content
        .contains("export AIPASS_API_KEY="));
}

#[test]
fn harness_preserves_yaml_plugins_refs_and_records_and_redacts_multiline_secrets() {
    let dir = tempdir().unwrap();
    let entry = entry();
    let patch = dsh_home(dir.path()).join("profiles/web/cordis.patch.yml");
    write(&patch, "- id: llm-pi-ai\n  config:\n    retryPolicy:\n      maxRetries: 7\n    providers:\n      original:\n        api: openai-completions\n- id: unrelated\n  config:\n    keep: true\n");
    let credentials = dsh_home(dir.path()).join(".credentials.yaml");
    write(&credentials, "version: 1\nrefs:\n  ORIGINAL_ENV: |\n    old-private-value\n    second-private-line\nrecords:\n  llm-pi-ai/example:\n    kind: grant\n    payload:\n      custom: old-private-grant\n");
    let mut entry = entry;
    entry.interface_type = InterfaceType::AnthropicMessages;
    entry.auth_scheme = AuthScheme::XApiKey;
    let (plan, content) = plan_additional_tool(
        dir.path(),
        ToolId::DeepseekHarness,
        &entry,
        true,
        Some(ModelApi::Messages),
    )
    .unwrap();
    let root: Value = serde_yaml_ng::from_str(&content).unwrap();
    assert_eq!(root[0]["config"]["retryPolicy"]["maxRetries"], 7);
    assert_eq!(
        root[0]["config"]["providers"]["aipass"]["api"],
        "anthropic-messages"
    );
    assert_eq!(root[1]["config"]["keep"], true);
    assert_eq!(root[2]["config"]["provider"], "aipass");
    let saved: Value = serde_yaml_ng::from_str(&plan.extra_writes[0].content).unwrap();
    assert_eq!(saved["refs"]["AIPASS_DSH_API_KEY"], "private-fixture-key");
    assert!(saved["refs"]["ORIGINAL_ENV"]
        .as_str()
        .unwrap()
        .contains("old-private-value"));
    let redacted = crate::redact_config(&plan.extra_writes[0].content);
    for secret in [
        "private-fixture-key",
        "old-private-value",
        "second-private-line",
        "old-private-grant",
    ] {
        assert!(!redacted.contains(secret));
    }
    let before = std::fs::read(&credentials).unwrap();
    let backup_key = [42; 32];
    crate::apply_plan_encrypted(&plan, &content, &backup_key).unwrap();
    assert_eq!(
        read_yaml(&credentials, json!({})).unwrap()["refs"]["AIPASS_DSH_API_KEY"],
        "private-fixture-key"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&credentials)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let backup = std::fs::read_to_string(&plan.backup_path).unwrap();
    assert!(!backup.contains("old-private-value"));
    crate::rollback_encrypted(&plan.backup_path, &backup_key).unwrap();
    assert_eq!(std::fs::read(&credentials).unwrap(), before);
    assert!(plan_additional_tool(dir.path(), ToolId::DeepseekHarness, &entry, false, None).is_ok());
    write(
        &credentials,
        "version: 1\nrefs:\n  AIPASS_DSH_API_KEY: existing-private-key\n",
    );
    assert!(
        plan_additional_tool(dir.path(), ToolId::DeepseekHarness, &entry, false, None).is_err()
    );
}

#[test]
fn antigravity_selects_gemini_and_requires_a_sourced_environment() {
    let dir = tempdir().unwrap();
    let mut entry = entry();
    assert!(plan_additional_tool(dir.path(), ToolId::AntigravityCli, &entry, false, None).is_err());
    entry.interface_type = InterfaceType::Gemini;
    entry.auth_scheme = AuthScheme::GoogleApiKey;
    entry.endpoint = Some("https://gemini.test/v1beta/".into());
    let target = dir.path().join(".gemini/antigravity-cli/settings.json");
    write(&target, "{\"theme\":\"dark\"}");
    let (plan, content) =
        plan_additional_tool(dir.path(), ToolId::AntigravityCli, &entry, false, None).unwrap();
    let root: Value = serde_json::from_str(&content).unwrap();
    assert_eq!(root["modelProvider"], "gemini");
    assert_eq!(root["theme"], "dark");
    assert!(plan.extra_writes[0]
        .content
        .contains("GEMINI_API_KEY=\"$(aipass get"));
    assert!(plan.extra_writes[0].content.contains("https://gemini.test"));
    assert!(!plan.extra_writes[0].content.contains("/v1beta"));
}

#[test]
fn invalid_existing_configuration_and_missing_models_fail_before_writes() {
    let dir = tempdir().unwrap();
    let mut entry = entry();
    write(&zed_config_path(dir.path()), "[]");
    assert!(plan_additional_tool(dir.path(), ToolId::Zed, &entry, true, None).is_err());
    entry.default_model = Some("  ".into());
    assert!(plan_additional_tool(dir.path(), ToolId::KimiCode, &entry, true, None).is_err());
    assert_eq!(
        std::fs::read_to_string(zed_config_path(dir.path())).unwrap(),
        "[]"
    );
}

#[test]
fn sourced_environment_treats_shell_metacharacters_as_literal_credentials() {
    let dir = tempdir().unwrap();
    let mut entry = entry();
    let marker = dir.path().join("must-not-exist");
    let key = format!(
        "fixture'$(touch {})`touch {}`$HOME",
        marker.display(),
        marker.display()
    );
    entry.api_key = Some(key.clone());
    let (plan, _) = plan_additional_tool(dir.path(), ToolId::Zed, &entry, true, None).unwrap();
    let helper = &plan.extra_writes[0];
    write(&helper.target_path, &helper.content);
    let result = std::process::Command::new("sh")
        .arg("-c")
        .arg(". \"$1\"; printf '%s' \"$AIPASS_API_KEY\"")
        .arg("fixture")
        .arg(&helper.target_path)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(String::from_utf8(result.stdout).unwrap(), key);
    assert!(!marker.exists());
}

#[test]
fn malformed_config_diagnostics_do_not_echo_credentials() {
    let dir = tempdir().unwrap();
    let entry = entry();
    write(
        &kimi_config_dir(dir.path()).join("config.toml"),
        "api_key = 'private-parser-fixture",
    );
    let error = plan_additional_tool(dir.path(), ToolId::KimiCode, &entry, true, None).unwrap_err();
    assert!(!format!("{error:#}").contains("private-parser-fixture"));
    let path = dsh_home(dir.path()).join("profiles/web/cordis.patch.yml");
    write(&path, "- id: a\n  id: b\n");
    assert!(plan_additional_tool(dir.path(), ToolId::DeepseekHarness, &entry, true, None).is_err());
}

#[test]
fn anthropic_sdk_writers_preserve_gateway_prefix_without_duplicating_v1() {
    let dir = tempdir().unwrap();
    let mut entry = entry();
    entry.interface_type = InterfaceType::AnthropicMessages;
    entry.auth_scheme = AuthScheme::XApiKey;
    entry.endpoint = Some("https://fixture.test/gateway/v1/".into());
    for tool in [ToolId::KimiCode, ToolId::Zed, ToolId::DeepseekHarness] {
        let (_, content) =
            plan_additional_tool(dir.path(), tool.clone(), &entry, true, None).unwrap();
        let base = match tool {
            ToolId::KimiCode => content.parse::<toml_edit::DocumentMut>().unwrap()["providers"]
                ["aipass"]["base_url"]
                .as_str()
                .unwrap()
                .to_owned(),
            ToolId::Zed => serde_json::from_str::<Value>(&content).unwrap()["language_models"]
                ["anthropic_compatible"]["aipass"]["api_url"]
                .as_str()
                .unwrap()
                .to_owned(),
            _ => serde_yaml_ng::from_str::<Value>(&content).unwrap()[0]["config"]["providers"]
                ["aipass"]["baseURL"]
                .as_str()
                .unwrap()
                .to_owned(),
        };
        assert_eq!(
            format!("{base}/v1/messages"),
            "https://fixture.test/gateway/v1/messages"
        );
    }
}
