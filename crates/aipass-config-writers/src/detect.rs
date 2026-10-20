use crate::models::ToolId;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolDetection {
    pub tool: ToolId,
    pub binary_found: bool,
    pub local_runtime_found: bool,
    pub config_path: Option<PathBuf>,
}

const TOOLS: [ToolId; 7] = [
    ToolId::ClaudeCode,
    ToolId::Codex,
    ToolId::GeminiCli,
    ToolId::OpenCode,
    ToolId::Grok,
    ToolId::Pi,
    ToolId::Cursor,
];

/// Detect supported agent CLI tools by looking for their binaries and their
/// config directories under the user's home directory.
///
/// GUI apps on macOS run with a minimal PATH, so user-level installs such as
/// `~/.opencode/bin` or `~/.local/bin` are invisible there. The binary search
/// therefore resolves the PATH the user's login shell would see (the same
/// strategy tools like T3 Code use), falls back to `launchctl` on macOS, and
/// finally probes well-known install locations under the home directory.
pub fn detect_tools() -> Vec<ToolDetection> {
    let home = home_dir();
    let search_dirs = crate::cli_discovery::search_dirs();
    TOOLS
        .iter()
        .map(|tool| ToolDetection {
            tool: tool.clone(),
            binary_found: !crate::cli_discovery::tool_executables_in(
                &search_dirs,
                tool,
                std::env::var_os("GROK_BIN_DIR").as_deref().map(Path::new),
            )
            .is_empty(),
            local_runtime_found: *tool == ToolId::Cursor
                && !crate::cli_discovery::executable_candidates_in(
                    &search_dirs,
                    &["cursor-agent-local"],
                )
                .is_empty(),
            config_path: home.as_deref().and_then(|home| config_dir(home, tool)),
        })
        .collect()
}

fn config_dir(home: &Path, tool: &ToolId) -> Option<PathBuf> {
    match tool {
        ToolId::ClaudeCode => first_existing([
            std::env::var_os("CLAUDE_CONFIG_DIR")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(".claude")),
            // Claude Code writes this file on first run, before any directory.
            home.join(".claude.json"),
        ]),
        ToolId::Codex => {
            let dir = crate::utils::resolve_codex_dir(home);
            dir.exists().then_some(dir)
        }
        ToolId::GeminiCli => first_existing([crate::gemini_config_dir(home)]),
        ToolId::OpenCode => {
            let mut candidates = Vec::new();
            // opencode honors XDG_CONFIG_HOME when it is set.
            if let Some(directory) = crate::opencode_config_path(home).parent() {
                candidates.push(directory.to_owned());
            }
            candidates.push(home.join(".config").join("opencode"));
            // The official installer always creates ~/.opencode for the binary.
            candidates.push(home.join(".opencode"));
            first_existing(candidates)
        }
        ToolId::Grok => first_existing([home.join(".grok")]),
        ToolId::Pi => first_existing([home.join(".pi").join("agent"), home.join(".pi")]),
        ToolId::Cursor => first_existing([
            home.join(".cursor"),
            crate::xdg_config_home(home).join("cursor"),
            home.join(".local").join("share").join("cursor-agent"),
        ]),
    }
}

fn first_existing(paths: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    paths.into_iter().find(|path| path.exists())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn config_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn config_dir_reports_existing_tool_directories() {
        let _guard = config_env_lock().lock().unwrap();
        let dir = tempdir().unwrap();
        assert_eq!(config_dir(dir.path(), &ToolId::ClaudeCode), None);

        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        assert_eq!(config_dir(dir.path(), &ToolId::ClaudeCode), Some(claude));

        let opencode = dir.path().join(".config").join("opencode");
        std::fs::create_dir_all(&opencode).unwrap();
        assert_eq!(config_dir(dir.path(), &ToolId::OpenCode), Some(opencode));
        assert_eq!(config_dir(dir.path(), &ToolId::GeminiCli), None);

        let grok = dir.path().join(".grok");
        std::fs::create_dir_all(&grok).unwrap();
        assert_eq!(config_dir(dir.path(), &ToolId::Grok), Some(grok));

        let pi = dir.path().join(".pi").join("agent");
        std::fs::create_dir_all(&pi).unwrap();
        assert_eq!(config_dir(dir.path(), &ToolId::Pi), Some(pi));
    }

    #[test]
    fn config_dir_falls_back_to_install_marks() {
        let _guard = config_env_lock().lock().unwrap();
        let dir = tempdir().unwrap();
        let home = dir.path();

        // Only the opencode install dir exists (fresh install, never launched).
        let install_dir = home.join(".opencode");
        std::fs::create_dir_all(install_dir.join("bin")).unwrap();
        assert_eq!(
            config_dir(home, &ToolId::OpenCode),
            Some(install_dir.clone())
        );

        // The XDG config dir wins once it exists.
        let xdg = home.join("xdg");
        let xdg_opencode = xdg.join("opencode");
        std::fs::create_dir_all(&xdg_opencode).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &xdg);
        let found = config_dir(home, &ToolId::OpenCode);
        std::env::remove_var("XDG_CONFIG_HOME");
        assert_eq!(found, Some(xdg_opencode));

        // Claude Code may only have written ~/.claude.json so far.
        let claude_json = home.join(".claude.json");
        std::fs::write(&claude_json, "{}").unwrap();
        assert_eq!(config_dir(home, &ToolId::ClaudeCode), Some(claude_json));
    }

    #[test]
    fn detect_tools_covers_every_supported_tool() {
        let _guard = config_env_lock().lock().unwrap();
        let detections = detect_tools();
        assert_eq!(detections.len(), TOOLS.len());
        for tool in TOOLS {
            assert!(detections.iter().any(|detection| detection.tool == tool));
        }
    }
}
