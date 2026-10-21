//! Exercise real Unix shell helpers and on-disk config transactions on macOS/Linux.
//! Vendor inference is deliberately outside this test: all credentials are synthetic.
#![cfg(unix)]

use aipass_config_writers::*;
use aipass_provider_registry::{AuthScheme, InterfaceType};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};
use tempfile::tempdir;

const KEY: &str = "sk-aipass-unix-integration-fixture";

fn entry(interface_type: InterfaceType, auth_scheme: AuthScheme) -> ToolEntry {
    ToolEntry {
        supports_websockets: Some(false),
        id: uuid::Uuid::new_v4(),
        secret_id: Some("selected-key".into()),
        title: "Unix integration".into(),
        provider_id: None,
        endpoint: Some("http://127.0.0.1:8787/v1".into()),
        interface_type,
        auth_scheme,
        env_key: "AIPASS_UNIX_FIXTURE_KEY".into(),
        default_model: Some("fixture-model".into()),
        api_key: Some(KEY.into()),
    }
}

fn shell(home: &Path, bin: &Path, script: &str, args: &[&str]) -> String {
    let result = Command::new("/bin/bash")
        .env_clear()
        .env("HOME", home)
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .args(["--noprofile", "--norc", "-euc", script, "aipass-fixture"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "shell helper failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}

fn roundtrip(plan: ConfigPlan, content: String) {
    let mut paths = vec![plan.target_path.clone()];
    paths.extend(plan.extra_writes.iter().map(|w| w.target_path.clone()));
    let mut backups = vec![plan.backup_path.clone()];
    backups.extend(plan.extra_writes.iter().map(|w| w.backup_path.clone()));
    let before: Vec<_> = paths.iter().map(|p| fs::read(p).ok()).collect();
    let key = [37; 32];
    apply_plan_encrypted(&plan, &content, &key).unwrap();
    assert_eq!(fs::read_to_string(&plan.target_path).unwrap(), content);
    for (path, backup) in paths.iter().zip(backups) {
        assert_eq!(fs::metadata(path).unwrap().permissions().mode() & 0o077, 0);
        let backup = fs::read(backup).unwrap();
        assert!(!backup.windows(KEY.len()).any(|w| w == KEY.as_bytes()));
    }
    rollback_encrypted(&plan.backup_path, &key).unwrap();
    for (path, original) in paths.iter().zip(before) {
        assert_eq!(fs::read(path).ok(), original, "{}", path.display());
    }
}

#[test]
fn unix_quick_integrations_execute_helpers_and_restore_all_files() {
    // This integration-test executable has one test. Its environment overrides
    // cannot race unit tests or reach the developer's actual configuration.
    let root = tempdir().unwrap();
    let home = root.path().join("home with spaces 中文");
    let bin = home.join("bin with spaces");
    fs::create_dir_all(&bin).unwrap();
    for (name, path) in [
        ("CODEX_HOME", home.join("codex override")),
        ("CLAUDE_CONFIG_DIR", home.join("claude override")),
        ("GEMINI_CLI_HOME", home.join("gemini override")),
        ("XDG_CONFIG_HOME", home.join("xdg config")),
        ("XDG_DATA_HOME", home.join("xdg data")),
        ("KIMI_CODE_HOME", home.join("kimi override")),
        ("DSH_HOME", home.join("harness override")),
        ("HERMES_HOME", home.join("hermes override")),
        ("GROK_HOME", home.join("grok override")),
        ("PI_CODING_AGENT_DIR", home.join("pi override")),
    ] {
        std::env::set_var(name, path);
    }
    let openai = entry(InterfaceType::OpenAiCompatible, AuthScheme::Bearer);
    let anthropic = entry(InterfaceType::AnthropicMessages, AuthScheme::XApiKey);
    let gemini = entry(InterfaceType::Gemini, AuthScheme::GoogleApiKey);
    let mut meta = openai.clone();
    meta.endpoint = Some("https://api.meta.ai/v1".into());

    // Validate the command's arguments instead of accepting any helper call.
    let executable = bin.join("aipass");
    fs::write(&executable, format!(
        "#!/bin/sh\n[ \"$1\" = get ] && [ \"$3\" = --secret-id ] && [ \"$4\" = selected-key ] && [ \"$5\" = --reveal ] || exit 71\nprintf '%s' '{KEY}'\n"
    )).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();

    let (_, content) = plan_claude_code(&home, &anthropic).unwrap();
    let settings: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(
        shell(&home, &bin, settings["apiKeyHelper"].as_str().unwrap(), &[]),
        KEY
    );
    // Pi 0.73.1 resolves a bare environment name, while newer builds support
    // interpolation. The command reference works with both and needs no source.
    let (_, content) = plan_pi(&home, &openai).unwrap();
    let models: serde_json::Value = serde_json::from_str(&content).unwrap();
    let provider = models["providers"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    let command = provider["apiKey"]
        .as_str()
        .unwrap()
        .strip_prefix('!')
        .unwrap();
    assert_eq!(shell(&home, &bin, command, &[]), KEY);
    for (plan, content, variable) in [
        {
            let (p, c) = plan_gemini_cli(&home, &gemini).unwrap();
            (p, c, "GEMINI_API_KEY")
        },
        {
            let (p, c) = plan_cursor_local(&home, &openai).unwrap();
            (p, c, "CURSOR_LOCAL_AGENT_API_KEY")
        },
    ] {
        fs::create_dir_all(plan.target_path.parent().unwrap()).unwrap();
        fs::write(&plan.target_path, &content).unwrap();
        assert!(!content.contains(KEY));
        assert_eq!(
            shell(
                &home,
                &bin,
                "source \"$1\"; printf '%s' \"${!2}\"",
                &[plan.target_path.to_str().unwrap(), variable,]
            ),
            KEY
        );
        fs::remove_file(plan.target_path).unwrap();
    }

    // Every catalog writer must apply and roll back its primary AND auxiliary
    // files, including Codex auth.json, Gemini auth selection and Pi defaults.
    for (plan, content) in [
        plan_codex_plaintext(&home, &openai).unwrap(),
        plan_claude_code_plaintext(&home, &anthropic).unwrap(),
        plan_gemini_cli_plaintext(&home, &gemini).unwrap(),
        plan_opencode_plaintext(&home, &openai).unwrap(),
        plan_grok_plaintext(&home, &openai).unwrap(),
        plan_pi_plaintext(&home, &openai).unwrap(),
        plan_cursor_local_plaintext(&home, &openai).unwrap(),
    ] {
        assert!(plan.target_path.starts_with(&home));
        assert!(!plan.preview.contains(KEY));
        roundtrip(plan, content);
    }
    type Writer = fn(&Path, &ToolEntry) -> anyhow::Result<(ConfigPlan, String)>;
    for writer in [plan_codex as Writer, plan_opencode, plan_grok, plan_pi] {
        let (plan, content) = writer(&home, &openai).unwrap();
        assert!(!content.contains(KEY));
        roundtrip(plan, content);
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
            for write in &plan.extra_writes {
                if write
                    .target_path
                    .extension()
                    .is_some_and(|ext| ext == "env")
                {
                    fs::create_dir_all(write.target_path.parent().unwrap()).unwrap();
                    fs::write(&write.target_path, &write.content).unwrap();
                    // Resolve every generated helper under a clean Unix shell.
                    shell(
                        &home,
                        &bin,
                        "source \"$1\"",
                        &[write.target_path.to_str().unwrap()],
                    );
                    fs::remove_file(&write.target_path).unwrap();
                }
            }
            assert!(!plan.preview.contains(KEY));
            roundtrip(plan, content);
        }
    }
    let (plan, content) = plan_gemini_cli(&home, &gemini).unwrap();
    fs::write(&plan.target_path, content).unwrap();
    fs::write(&executable, "#!/bin/sh\nexit 73\n").unwrap();
    shell(&home, &bin, "export GEMINI_API_KEY=stale; if source \"$1\"; then exit 91; fi; test -z \"$GEMINI_API_KEY\"", &[plan.target_path.to_str().unwrap()]);
}
