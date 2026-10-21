use crate::models::ToolId;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolDetection {
    pub tool: ToolId,
    pub binary_found: bool,
    pub local_runtime_found: bool,
    pub extension_found: bool,
    pub config_path: Option<PathBuf>,
}
impl ToolDetection {
    pub fn is_installed(&self) -> bool {
        if self.tool == ToolId::Cursor {
            self.local_runtime_found
        } else {
            self.binary_found || self.local_runtime_found || self.extension_found
        }
    }
}

const TOOLS: [ToolId; 22] = [
    ToolId::ClaudeCode,
    ToolId::Codex,
    ToolId::GeminiCli,
    ToolId::OpenCode,
    ToolId::Grok,
    ToolId::Pi,
    ToolId::Cursor,
    ToolId::KimiCode,
    ToolId::Zed,
    ToolId::DeepseekHarness,
    ToolId::AntigravityCli,
    ToolId::MuseCode,
    ToolId::HermesAgent,
    ToolId::CommandCode,
    ToolId::Codebuddy,
    ToolId::Workbuddy,
    ToolId::VscodeChat,
    ToolId::Cline,
    ToolId::Trae,
    ToolId::Qoder,
    ToolId::Zcode,
    ToolId::JetbrainsCopilot,
];

/// Detect installed Agents through executable entry points, app bundles or
/// extensions paired with their actual host. Configuration paths are metadata.
///
/// GUI apps on macOS run with a minimal PATH, so user-level installs such as
/// `~/.opencode/bin` or `~/.local/bin` are invisible there. The binary search
/// therefore resolves the PATH the user's login shell would see (the same
/// strategy tools like T3 Code use), falls back to `launchctl` on macOS, and
/// finally probes well-known install locations under the home directory.
pub fn detect_tools() -> Vec<ToolDetection> {
    let home = crate::user_home_dir();
    let search_dirs = crate::cli_discovery::search_dirs();
    detect_tools_in(home.as_deref(), &search_dirs)
}

pub fn detect_tools_in(home: Option<&Path>, search_dirs: &[PathBuf]) -> Vec<ToolDetection> {
    TOOLS
        .iter()
        .map(|tool| detect_tool_in(home, search_dirs, tool))
        .collect()
}

pub fn detect_tool_in(
    home: Option<&Path>,
    search_dirs: &[PathBuf],
    tool: &ToolId,
) -> ToolDetection {
    ToolDetection {
        tool: tool.clone(),
        binary_found: crate::installed_apps::desktop_app_installed(home, tool)
            || !crate::cli_discovery::tool_executables_in(
                search_dirs,
                tool,
                std::env::var_os("GROK_BIN_DIR").as_deref().map(Path::new),
            )
            .is_empty(),
        local_runtime_found: *tool == ToolId::Cursor
            && !crate::cli_discovery::executable_candidates_in(
                search_dirs,
                &["cursor-agent-local"],
            )
            .is_empty(),
        extension_found: home.is_some_and(|home| match tool {
            ToolId::Cline => cline_extension_installed(home, search_dirs),
            ToolId::JetbrainsCopilot => jetbrains_copilot_installed(home),
            _ => false,
        }),
        config_path: home.and_then(|home| config_dir(home, tool)),
    }
}

// A retained IDE settings directory alone is not installation evidence. Require
// the Copilot plugin's executable JAR under an actual plugin install directory.
fn jetbrains_copilot_installed(home: &Path) -> bool {
    let roots = jetbrains_plugin_roots(home);
    let hosts = crate::installed_apps::jetbrains_hosts(home);
    roots.iter().any(|root| {
        std::fs::read_dir(root).is_ok_and(|ides| {
            ides.flatten().any(|ide| {
                if !hosts.contains(ide.file_name().to_string_lossy().as_ref()) {
                    return false;
                }
                let lib = ide.path().join("plugins/github-copilot-intellij/lib");
                std::fs::read_dir(lib).is_ok_and(|jars| {
                    jars.flatten().any(|jar| {
                        let name = jar.file_name();
                        let name = name.to_string_lossy();
                        (name.starts_with("copilot-intellij")
                            || name.starts_with("github-copilot-intellij")
                            || name == "core.jar"
                            || name.starts_with("core-"))
                            && name.ends_with(".jar")
                            && jar.path().is_file()
                            && jar_header(&jar.path())
                    })
                })
            })
        })
    })
}

fn jetbrains_plugin_roots(home: &Path) -> Vec<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join("AppData/Roaming"));
    #[cfg(target_os = "macos")]
    let base = home.join("Library/Application Support");
    #[cfg(not(any(windows, target_os = "macos")))]
    let base = crate::xdg_data_home(home);
    vec![base.join("JetBrains"), base.join("Google")]
}

fn jar_header(path: &Path) -> bool {
    use std::io::Read;
    let mut header = [0; 4];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut header))
        .is_ok()
        && header == *b"PK\x03\x04"
}

fn cline_extension_installed(home: &Path, dirs: &[PathBuf]) -> bool {
    #[cfg(target_os = "linux")]
    let flatpak =
        crate::installed_apps::flatpak_executable(Some(home), &ToolId::VscodeChat).is_some();
    #[cfg(not(target_os = "linux"))]
    let flatpak = false;
    cline_extension_installed_in(
        home,
        [
            crate::installed_apps::vscode_host_installed(home, dirs, false),
            crate::installed_apps::vscode_host_installed(home, dirs, true),
            flatpak,
        ],
    )
}

fn cline_extension_installed_in(home: &Path, hosts: [bool; 3]) -> bool {
    [
        ".vscode/extensions",
        ".vscode-insiders/extensions",
        ".var/app/com.visualstudio.code/.vscode/extensions",
    ]
    .into_iter()
    .zip(hosts)
    .any(|(root, installed)| {
        installed
            && std::fs::read_dir(home.join(root)).is_ok_and(|files| {
                let obsolete = std::fs::read_to_string(home.join(root).join(".obsolete"))
                    .ok()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                    .unwrap_or_default();
                files.flatten().any(|file| {
                    if obsolete
                        .get(file.file_name().to_string_lossy().as_ref())
                        .is_some_and(|v| v == true)
                    {
                        return false;
                    }
                    if !file
                        .file_name()
                        .to_string_lossy()
                        .starts_with("saoudrizwan.claude-dev-")
                    {
                        return false;
                    }
                    std::fs::read_to_string(file.path().join("package.json"))
                        .ok()
                        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                        .is_some_and(|p| {
                            p["publisher"] == "saoudrizwan"
                                && p["name"] == "claude-dev"
                                && p["main"].as_str().is_some_and(|main| {
                                    let path = Path::new(main);
                                    !path.is_absolute()
                                        && !path
                                            .components()
                                            .any(|c| c == std::path::Component::ParentDir)
                                        && file.path().join(path).is_file()
                                })
                        })
                })
            })
    })
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
        ToolId::MuseCode
        | ToolId::HermesAgent
        | ToolId::CommandCode
        | ToolId::Codebuddy
        | ToolId::Workbuddy => crate::byok_config_paths(home, tool)
            .into_iter()
            .find(|p| p.exists()),
        ToolId::VscodeChat
        | ToolId::Cline
        | ToolId::Trae
        | ToolId::Qoder
        | ToolId::Zcode
        | ToolId::JetbrainsCopilot => None,
        ToolId::KimiCode => first_existing([crate::kimi_config_dir(home)]),
        ToolId::Zed => first_existing([crate::zed_config_path(home).parent()?.to_owned()]),
        ToolId::DeepseekHarness => first_existing([crate::dsh_home(home)]),
        ToolId::AntigravityCli => first_existing([home.join(".gemini/antigravity-cli")]),
        ToolId::Grok => first_existing([crate::grok_config_dir(home)]),
        ToolId::Pi => first_existing([crate::pi_config_dir(home)]),
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
    #[test]
    fn cline_detection_requires_the_installed_extension_entry_point() {
        let root = tempdir().unwrap();
        let extension = root
            .path()
            .join(".vscode/extensions/saoudrizwan.claude-dev-1.0.0");
        std::fs::create_dir_all(&extension).unwrap();
        assert!(!cline_extension_installed_in(
            root.path(),
            [true, false, false]
        ));
        std::fs::write(
            extension.join("package.json"),
            r#"{"publisher":"saoudrizwan","name":"claude-dev","main":"dist/extension.js"}"#,
        )
        .unwrap();
        assert!(!cline_extension_installed_in(
            root.path(),
            [true, false, false]
        ));
        std::fs::create_dir_all(extension.join("dist")).unwrap();
        std::fs::write(extension.join("dist/extension.js"), "fixture").unwrap();
        assert!(cline_extension_installed_in(
            root.path(),
            [true, false, false]
        ));
        std::fs::write(
            root.path().join(".vscode/extensions/.obsolete"),
            r#"{"saoudrizwan.claude-dev-1.0.0":true}"#,
        )
        .unwrap();
        assert!(!cline_extension_installed_in(
            root.path(),
            [true, false, false]
        ));
    }
    #[test]
    fn cline_extension_requires_its_own_host_edition() {
        let root = tempdir().unwrap();
        for (relative, host) in [
            (".vscode/extensions", 0),
            (".vscode-insiders/extensions", 1),
            (".var/app/com.visualstudio.code/.vscode/extensions", 2),
        ] {
            let extension = root
                .path()
                .join(relative)
                .join("saoudrizwan.claude-dev-1.0.0");
            std::fs::create_dir_all(extension.join("dist")).unwrap();
            std::fs::write(
                extension.join("package.json"),
                r#"{"publisher":"saoudrizwan","name":"claude-dev","main":"dist/extension.js"}"#,
            )
            .unwrap();
            std::fs::write(extension.join("dist/extension.js"), "fixture").unwrap();
            for installed in 0..3 {
                let mut hosts = [false; 3];
                hosts[installed] = true;
                assert_eq!(
                    cline_extension_installed_in(root.path(), hosts),
                    installed == host,
                    "extension in {relative}, installed host {installed}"
                );
            }
            assert!(!cline_extension_installed_in(root.path(), [false; 3]));
            std::fs::remove_dir_all(extension).unwrap();
        }
    }

    #[test]
    fn jetbrains_settings_without_plugin_code_do_not_count_as_installed() {
        let home = tempdir().unwrap();
        let lib = jetbrains_plugin_roots(home.path())[0]
            .join("IntelliJIdea2026.2/plugins/github-copilot-intellij/lib");
        std::fs::create_dir_all(&lib).unwrap();
        assert!(!jetbrains_copilot_installed(home.path()));
        std::fs::write(lib.join("copilot-intellij-1.13.jar"), b"PK\x03\x04fixture").unwrap();
        assert!(!jetbrains_copilot_installed(home.path()));
        let ide = home.path().join("Applications/IDE");
        std::fs::create_dir_all(ide.join("bin")).unwrap();
        std::fs::write(ide.join("product-info.json"), r#"{"dataDirectoryName":"IntelliJIdea2026.2","launch":[{"launcherPath":"bin/idea.exe"}]}"#).unwrap();
        std::fs::write(ide.join("bin/idea.exe"), b"MZfixture").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                ide.join("bin/idea.exe"),
                std::fs::Permissions::from_mode(0o700),
            )
            .unwrap();
        }
        assert!(
            jetbrains_copilot_installed(home.path()),
            "hosts: {:?}",
            crate::installed_apps::jetbrains_hosts(home.path())
        );
        std::fs::remove_file(lib.join("copilot-intellij-1.13.jar")).unwrap();
        std::fs::write(lib.join("core-1.18.0-261.jar"), b"PK\x03\x04fixture").unwrap();
        assert!(jetbrains_copilot_installed(home.path()));
        std::fs::remove_file(ide.join("bin/idea.exe")).unwrap();
        assert!(!jetbrains_copilot_installed(home.path()));
    }
}
