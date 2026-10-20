//! Official binary discovery and bounded capability probes.
use super::*;

pub(crate) fn executable(provider: &str) -> Result<PathBuf> {
    let name = match provider {
        "gemini-cli" => "gemini",
        v => v,
    };
    let candidates = if provider == "grok" {
        aipass_config_writers::cli_discovery::tool_executables(&aipass_config_writers::ToolId::Grok)
    } else {
        aipass_config_writers::cli_discovery::executable_candidates(&[name])
    };
    if let Some(path) = candidates.into_iter().next() {
        return Ok(path);
    }
    Err(format!(
        "Install or update the official {name} CLI, then retry in AIPass"
    ))
}

pub(crate) fn status(provider: &str) -> aipass_agent_protocol::ClaudeCliStatus {
    let unavailable = |reason: &str| aipass_agent_protocol::ClaudeCliStatus {
        available: false,
        reason: Some(reason.into()),
        version: None,
    };
    if !matches!(provider, "codex" | "grok" | "copilot" | "gemini-cli") {
        return unavailable("unsupported");
    }
    let Ok(exe) = executable(provider) else {
        return unavailable("missing");
    };
    let probe = |args: &[&str]| -> Option<String> {
        let file = tempfile::tempfile().ok()?;
        let mut cmd = std::process::Command::new(&exe);
        cmd.env(
            "PATH",
            aipass_config_writers::cli_discovery::command_path(&exe)?,
        )
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(file.try_clone().ok()?)
        .stderr(std::process::Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        fn stop(child: &mut std::process::Child) {
            #[cfg(unix)]
            if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
                let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        let mut child = cmd.spawn().ok()?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match child.try_wait() {
                Err(_) => {
                    stop(&mut child);
                    return None;
                }
                Ok(Some(status)) => {
                    if !status.success() {
                        return None;
                    }
                    break;
                }
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                Ok(None) => {
                    stop(&mut child);
                    return None;
                }
            }
            if file.metadata().map_or(true, |m| m.len() > 128 * 1024) {
                stop(&mut child);
                return None;
            }
        }
        use std::io::{Read, Seek, SeekFrom};
        let mut file = file;
        file.seek(SeekFrom::Start(0)).ok()?;
        let mut value = String::new();
        file.take(128 * 1024).read_to_string(&mut value).ok()?;
        Some(value)
    };
    let Some(version) = probe(&["--version"]) else {
        return unavailable("unusable");
    };
    let args = if provider == "gemini-cli" {
        vec!["--help"]
    } else {
        vec!["login", "--help"]
    };
    let Some(help) = probe(&args) else {
        return unavailable("unsupported");
    };
    let supported = match provider {
        "codex" => version.contains("codex") && help.contains("Manage login"),
        "grok" => help.contains("Sign in to Grok") && help.contains("--device-auth"),
        "copilot" => help.contains("--web-flow"),
        "gemini-cli" => help.contains("Gemini CLI") && help.contains("--acp"),
        _ => false,
    };
    if !supported {
        return unavailable("unsupported");
    }
    aipass_agent_protocol::ClaudeCliStatus {
        available: true,
        reason: None,
        version: Some(version.trim().chars().take(100).collect()),
    }
}
