//! Run on native Windows CI: actual PowerShell, filesystem transactions and discovery.
#![cfg(windows)]
use aipass_config_writers::*;
use aipass_provider_registry::{AuthScheme, InterfaceType};
use std::{fs, path::Path, process::Command};

fn entry(interface_type: InterfaceType, auth_scheme: AuthScheme) -> ToolEntry {
    ToolEntry {
        supports_websockets: None,
        id: uuid::Uuid::new_v4(),
        secret_id: Some("selected O'Brien & $(throw) 中文".into()),
        title: "Windows fixture".into(),
        provider_id: None,
        endpoint: Some("http://127.0.0.1:8787/v1".into()),
        interface_type,
        auth_scheme,
        env_key: "FIXTURE_API_KEY".into(),
        default_model: Some("fixture-model".into()),
        api_key: Some("fixture-literal-key".into()),
    }
}
fn powershell(path: &Path, bin: &Path) {
    let search = std::env::join_paths(std::iter::once(bin.to_owned()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .unwrap();
    let out = Command::new("powershell.exe")
        .env("PATH", search)
        .args(["-NoProfile", "-NonInteractive", "-File"])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn roundtrip(plan: ConfigPlan, content: String, bin: &Path) {
    let mut paths = vec![plan.target_path.clone()];
    paths.extend(plan.extra_writes.iter().map(|w| w.target_path.clone()));
    let before: Vec<_> = paths.iter().map(|p| fs::read(p).ok()).collect();
    let backup_key = [41; 32];
    assert!(!plan.preview.contains("fixture-literal-key"));
    apply_plan_encrypted(&plan, &content, &backup_key).unwrap();
    for path in &paths {
        if path.extension().is_some_and(|ext| ext == "ps1") {
            powershell(path, bin);
        }
    }
    rollback_encrypted(&plan.backup_path, &backup_key).unwrap();
    for (path, original) in paths.iter().zip(before) {
        assert_eq!(fs::read(path).ok(), original, "{}", path.display());
    }
}
#[test]
fn native_windows_discovery_writing_helpers_and_rollback() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home O'Brien 中文");
    let bin = home.join("bin with spaces");
    fs::create_dir_all(&bin).unwrap();
    for (name, relative) in [
        ("HOME", ""),
        ("USERPROFILE", ""),
        ("APPDATA", "AppData/Roaming"),
        ("LOCALAPPDATA", "AppData/Local"),
        ("XDG_CONFIG_HOME", "xdg config"),
        ("XDG_DATA_HOME", "xdg data"),
        ("CODEX_HOME", "codex"),
        ("CLAUDE_CONFIG_DIR", "claude"),
        ("GEMINI_CLI_HOME", "gemini"),
        ("KIMI_CODE_HOME", "kimi"),
        ("DSH_HOME", "harness"),
        ("HERMES_HOME", "hermes"),
        ("GROK_HOME", ".grok"),
        ("PI_CODING_AGENT_DIR", "pi"),
    ] {
        std::env::set_var(name, home.join(relative));
    }
    std::env::set_var("PATHEXT", ".COM;.EXE;.BAT;.CMD");
    fs::write(
        bin.join("aipass.ps1"),
        "[Console]::Write('fixture-key'); exit 0",
    )
    .unwrap();
    // Bare Unix shims and retained configs are not Windows installations.
    fs::write(bin.join("kimi"), "#!/bin/sh\n").unwrap();
    fs::create_dir_all(home.join("kimi")).unwrap();
    assert!(
        !detect_tool_in(Some(&home), std::slice::from_ref(&bin), &ToolId::KimiCode).is_installed()
    );
    fs::write(
        bin.join("kimi.cmd"),
        r#"node "%dp0%\node_modules\@moonshot-ai\kimi-code\bin.js" %*"#,
    )
    .unwrap();
    assert!(
        !detect_tool_in(Some(&home), std::slice::from_ref(&bin), &ToolId::KimiCode).is_installed()
    );
    let js = bin.join("node_modules/@moonshot-ai/kimi-code/bin.js");
    fs::create_dir_all(js.parent().unwrap()).unwrap();
    fs::write(&js, "fixture").unwrap();
    assert!(
        detect_tool_in(Some(&home), std::slice::from_ref(&bin), &ToolId::KimiCode).is_installed()
    );
    fs::remove_file(js).unwrap();
    fs::write(bin.join("kimi.cmd"), "@wsl.exe -d Ubuntu kimi %*\r\n").unwrap();
    assert!(
        !detect_tool_in(Some(&home), std::slice::from_ref(&bin), &ToolId::KimiCode).is_installed()
    );
    assert_eq!(
        zed_config_path(&home),
        home.join("AppData/Roaming/Zed/settings.json")
    );
    let openai = entry(InterfaceType::OpenAiCompatible, AuthScheme::Bearer);
    let anthropic = entry(InterfaceType::AnthropicMessages, AuthScheme::XApiKey);
    let gemini = entry(InterfaceType::Gemini, AuthScheme::GoogleApiKey);
    let mut meta = openai.clone();
    meta.endpoint = Some("https://api.meta.ai/v1".into());
    // Every automatic writer, independent of vendor model-network access.
    for (plan, content) in [
        plan_codex_plaintext(&home, &openai).unwrap(),
        plan_claude_code_plaintext(&home, &anthropic).unwrap(),
        plan_gemini_cli_plaintext(&home, &gemini).unwrap(),
        plan_opencode_plaintext(&home, &openai).unwrap(),
        plan_grok_plaintext(&home, &openai).unwrap(),
        plan_pi_plaintext(&home, &openai).unwrap(),
        plan_cursor_local_plaintext(&home, &openai).unwrap(),
        plan_gemini_cli(&home, &gemini).unwrap(),
        plan_claude_code(&home, &anthropic).unwrap(),
        plan_pi(&home, &openai).unwrap(),
    ] {
        roundtrip(plan, content, &bin);
    }
    for (tool, entry, api) in [
        (ToolId::KimiCode, &openai, ModelApi::ChatCompletions),
        (ToolId::Zed, &openai, ModelApi::Responses),
        (ToolId::DeepseekHarness, &anthropic, ModelApi::Messages),
        (ToolId::AntigravityCli, &gemini, ModelApi::Gemini),
        (ToolId::MuseCode, &meta, ModelApi::Responses),
        (ToolId::HermesAgent, &anthropic, ModelApi::Messages),
        (ToolId::CommandCode, &openai, ModelApi::Responses),
        (ToolId::Codebuddy, &openai, ModelApi::ChatCompletions),
        (ToolId::Workbuddy, &openai, ModelApi::ChatCompletions),
    ] {
        for plaintext in [false, true] {
            if !plaintext && matches!(tool, ToolId::Codebuddy | ToolId::Workbuddy) {
                continue;
            }
            let (plan, content) =
                plan_additional_tool(&home, tool.clone(), entry, plaintext, Some(api)).unwrap();
            roundtrip(plan, content, &bin);
        }
    }
    // Command-reference quoting also works through cmd.exe, as used by Node.
    let search = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    let out = Command::new("cmd.exe")
        .env("PATH", search)
        .args(["/D", "/S", "/C", &openai.credential_command()])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "fixture-key");
}
