use super::*;
use aipass_provider_registry::{AuthScheme, InterfaceType};

fn entry() -> ToolEntry {
    ToolEntry {
        id: uuid::Uuid::new_v4(),
        secret_id: Some("key".into()),
        title: "Fixture".into(),
        provider_id: None,
        endpoint: Some("https://fixture.test/v1".into()),
        interface_type: InterfaceType::OpenAiCompatible,
        auth_scheme: AuthScheme::Bearer,
        env_key: "FIXTURE_KEY".into(),
        default_model: Some("fixture-model".into()),
        api_key: Some("private-fixture".into()),
        supports_websockets: None,
    }
}

#[test]
fn muse_pins_response_endpoint_and_keeps_helper_keys_out_of_settings() {
    let home = tempfile::tempdir().unwrap();
    let mut entry = entry();
    assert!(
        crate::plan_additional_tool(home.path(), ToolId::MuseCode, &entry, false, None).is_err()
    );
    entry.endpoint = Some("https://api.meta.ai/v1".into());
    let (plan, content) =
        crate::plan_additional_tool(home.path(), ToolId::MuseCode, &entry, false, None).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    assert_eq!(settings["endpoint_transport"]["auth"], "bearer");
    assert_eq!(
        settings["endpoint_transport"]["base_url"],
        "https://api.meta.ai/v1"
    );
    assert!(!content.contains("private-fixture"));
    assert!(plan.extra_writes[0].content.contains("META_API_KEY"));
    assert!(plan.extra_writes[0].content.contains("--secret-id 'key'"));
}

#[test]
fn hermes_preserves_preferences_and_selects_exact_api_and_credential() {
    let home = tempfile::tempdir().unwrap();
    let entry = entry();
    let path = byok_config_paths(home.path(), &ToolId::HermesAgent)[0].clone();
    assert!(path.starts_with(home.path()));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "model: ''\nagent:\n  max_turns: 3\n").unwrap();
    let (plan, content) = crate::plan_additional_tool(
        home.path(),
        ToolId::HermesAgent,
        &entry,
        false,
        Some(ModelApi::Responses),
    )
    .unwrap();
    let root: Value = serde_yaml_ng::from_str(&content).unwrap();
    assert_eq!(root["agent"]["max_turns"], 3);
    assert_eq!(root["model"]["api_mode"], "codex_responses");
    assert_eq!(root["model"]["api_key"], "${AIPASS_HERMES_API_KEY}");
    assert!(plan.extra_writes[0].content.contains("aipass get"));
}

#[test]
fn command_code_uses_credential_helper_and_keeps_native_auth_untouched() {
    let home = tempfile::tempdir().unwrap();
    let entry = entry();
    let (plan, content) = crate::plan_additional_tool(
        home.path(),
        ToolId::CommandCode,
        &entry,
        false,
        Some(ModelApi::Responses),
    )
    .unwrap();
    let root: Value = serde_json::from_str(&content).unwrap();
    let provider = root["provider"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(provider["api"], "openai-responses");
    assert!(provider["apiKey"]
        .as_str()
        .unwrap()
        .starts_with("!aipass get"));
    assert!(!content.contains("private-fixture"));
    assert_eq!(plan.extra_writes.len(), 1);
    assert!(plan.extra_writes[0].target_path.ends_with("settings.json"));
}

#[test]
fn tencent_editors_preserve_other_models_and_make_chat_url_explicit() {
    let home = tempfile::tempdir().unwrap();
    let entry = entry();
    let path = home.path().join(".codebuddy/models.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path,r#"{"models":[{"id":"keep","apiKey":"old-private","url":"https://old.test/v1/chat/completions"}],"availableModels":["keep"]}"#).unwrap();
    for tool in [ToolId::Codebuddy, ToolId::Workbuddy] {
        let (plan, content) =
            crate::plan_additional_tool(home.path(), tool, &entry, true, None).unwrap();
        let root: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(root["models"][0]["apiKey"], "old-private");
        assert_eq!(
            root["models"][1]["url"],
            "https://fixture.test/v1/chat/completions"
        );
        assert_eq!(root["availableModels"], json!(["keep", "fixture-model"]));
        assert!(!plan.preview.contains("private-fixture"));
        assert!(!plan.preview.contains("old-private"));
    }
    assert!(
        crate::plan_additional_tool(home.path(), ToolId::Codebuddy, &entry, false, None).is_err()
    );
}
