//! Shared discovery for GUI detection, vendor login, import and renewal.
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub fn search_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    let path = std::env::var_os("PATH");
    let mut dirs: Vec<PathBuf> = path
        .as_deref()
        .map(std::env::split_paths)
        .map(Iterator::collect)
        .unwrap_or_default();
    dirs.extend(login_shell_path_dirs());
    dirs.extend(binary_search_dirs(None, home.as_deref()));
    for (key, suffix) in [
        ("PNPM_HOME", ""),
        ("NPM_CONFIG_PREFIX", "bin"),
        ("npm_config_prefix", "bin"),
        ("CARGO_HOME", "bin"),
        ("BUN_INSTALL", "bin"),
        ("VOLTA_HOME", "bin"),
        ("NVM_DIR", "versions/node"),
        ("FNM_DIR", "node-versions"),
        ("ASDF_DATA_DIR", "shims"),
        ("MISE_DATA_DIR", "shims"),
    ] {
        if let Some(root) = std::env::var_os(key)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
        {
            if key == "NVM_DIR" {
                append_node_bins(&mut dirs, &root.join(suffix), false);
            } else if key == "FNM_DIR" {
                append_node_bins(&mut dirs, &root.join(suffix), true);
            } else {
                dirs.push(root.join(suffix));
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|p| p.is_absolute() && seen.insert(p.clone()));
    dirs
}
pub fn executable_candidates(names: &[&str]) -> Vec<PathBuf> {
    executable_candidates_in(&search_dirs(), names)
}
/// Keep GUI detection and vendor login consistent for ambiguous CLI names.
pub fn tool_executables(tool: &crate::ToolId) -> Vec<PathBuf> {
    tool_executables_in(
        &search_dirs(),
        tool,
        std::env::var_os("GROK_BIN_DIR").as_deref().map(Path::new),
    )
}
pub(crate) fn tool_executables_in(
    dirs: &[PathBuf],
    tool: &crate::ToolId,
    grok_override: Option<&Path>,
) -> Vec<PathBuf> {
    use crate::ToolId;
    let names: &[&str] = match tool {
        ToolId::ClaudeCode => &["claude"],
        ToolId::Codex => &["codex"],
        ToolId::GeminiCli => &["gemini"],
        ToolId::OpenCode => &["opencode"],
        ToolId::Grok => &["grok"],
        ToolId::Pi => &["pi"],
        ToolId::Cursor => &["cursor-agent", "agent", "cursor-agent-local"],
    };
    let mut candidates = executable_candidates_in(dirs, names);
    match tool {
        ToolId::Cursor => candidates.retain(|p| {
            p.file_stem().is_some_and(|n| n != "agent")
                || p.canonicalize()
                    .is_ok_and(|p| p.to_string_lossy().contains("cursor-agent"))
        }),
        ToolId::Grok => {
            candidates.retain(|p| {
                p.canonicalize()
                    .is_ok_and(|p| p.to_string_lossy().replace('\\', "/").contains("/.grok/"))
            });
            if let Some(root) = grok_override.filter(|p| p.is_absolute()) {
                let mut explicit = executable_candidates_in(&[root.to_owned()], names);
                explicit.extend(candidates);
                candidates = explicit;
            }
        }
        _ => {}
    }
    candidates
}
pub(crate) fn executable_candidates_in(dirs: &[PathBuf], names: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in dirs.iter().filter(|p| p.is_absolute()) {
        for name in names {
            for candidate in binary_candidates(name) {
                let p = dir.join(candidate);
                if is_executable(&p) && !out.contains(&p) {
                    out.push(p);
                }
            }
        }
    }
    out
}
pub fn command_path(exe: &Path) -> Option<OsString> {
    command_path_in(exe, search_dirs())
}
fn command_path_in(exe: &Path, search: Vec<PathBuf>) -> Option<OsString> {
    let mut dirs: Vec<PathBuf> = exe
        .parent()
        .filter(|p| p.is_absolute())
        .map(Path::to_owned)
        .into_iter()
        .collect();
    if let Ok(real) = exe.canonicalize() {
        if let Some(parent) = real.parent() {
            dirs.push(parent.to_owned());
        }
    }
    dirs.extend(search);
    std::env::join_paths(command_search_dirs(dirs)).ok()
}
fn command_search_dirs(dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    // GUI PATH often contains only the distro's old Node. Keep explicit custom
    // PATH entries in order, but put discovered user runtimes ahead of system bins.
    #[cfg(unix)]
    {
        let (mut user, system): (Vec<_>, Vec<_>) = dirs.into_iter().partition(|p| {
            !matches!(
                p.to_str(),
                Some(
                    "/usr/bin"
                        | "/bin"
                        | "/usr/local/bin"
                        | "/opt/homebrew/bin"
                        | "/home/linuxbrew/.linuxbrew/bin"
                        | "/snap/bin"
                )
            )
        });
        user.extend(system);
        user
    }
    #[cfg(not(unix))]
    dirs
}
pub fn is_executable(path: &Path) -> bool {
    let Ok(meta) = path.metadata() else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 == 0 {
            return false;
        }
    }
    true
}
fn append_node_bins(dirs: &mut Vec<PathBuf>, root: &Path, fnm: bool) {
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut versions: Vec<_> = entries
            .flatten()
            .filter_map(|e| {
                let name = e
                    .file_name()
                    .to_string_lossy()
                    .trim_start_matches('v')
                    .to_owned();
                semver::Version::parse(&name).ok().map(|v| (v, e.path()))
            })
            .collect();
        versions.sort_by(|a, b| b.0.cmp(&a.0));
        dirs.extend(versions.into_iter().map(|(_, p)| {
            if fnm {
                p.join("installation/bin")
            } else {
                p.join("bin")
            }
        }));
    }
}
fn binary_search_dirs(path_env: Option<&OsStr>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = path_env
        .map(|path_env| std::env::split_paths(path_env).collect())
        .unwrap_or_default();
    if let Some(home) = home {
        for rel in [
            ".local/bin",
            ".cargo/bin",
            ".kiro/bin",
            "bin",
            ".opencode/bin",
            ".grok/bin",
            ".pi/bin",
            ".bun/bin",
            "Library/pnpm",
            ".local/share/pnpm",
            ".npm-global/bin",
            ".volta/bin",
            ".asdf/shims",
            ".local/share/mise/shims",
            ".proto/shims",
            ".proto/bin",
        ] {
            dirs.push(home.join(rel));
        }
        append_node_bins(&mut dirs, &home.join(".nvm/versions/node"), false);
        append_node_bins(
            &mut dirs,
            &home.join(".local/share/fnm/node-versions"),
            true,
        );
    }
    for system in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/snap/bin",
        "/home/linuxbrew/.linuxbrew/bin",
    ] {
        dirs.push(PathBuf::from(system));
    }
    #[cfg(windows)]
    {
        // npm/pnpm/volta/scoop shims live outside the PATH a GUI app inherits.
        let env_dirs: [(&str, &str); 4] = [
            ("APPDATA", "npm"),
            ("LOCALAPPDATA", "Programs\\nodejs"),
            ("LOCALAPPDATA", "Volta\\bin"),
            ("LOCALAPPDATA", "pnpm"),
        ];
        for (var, rel) in env_dirs {
            if let Some(base) = std::env::var_os(var) {
                dirs.push(PathBuf::from(base).join(rel));
            }
        }
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            let profile = PathBuf::from(profile);
            for rel in [".local\\bin", ".bun\\bin", "scoop\\shims"] {
                dirs.push(profile.join(rel));
            }
        }
    }
    dirs
}

/// The PATH directories the user's login shell would see, resolved once per
/// process. Empty when the shell cannot be probed (e.g. Windows).
fn login_shell_path_dirs() -> Vec<PathBuf> {
    static CACHE: OnceLock<Vec<PathBuf>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            login_shell_path()
                .map(|path| std::env::split_paths(&path).collect())
                .unwrap_or_default()
        })
        .clone()
}

#[cfg(not(windows))]
fn login_shell_path() -> Option<OsString> {
    const START: &str = "__AIPASS_PATH_START__";
    const END: &str = "__AIPASS_PATH_END__";
    let script = format!("printf '{START}'; printenv PATH; printf '{END}'");

    let mut shells: Vec<PathBuf> = std::env::var_os("SHELL")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    shells.push(PathBuf::from(if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else {
        "/bin/bash"
    }));

    for shell in shells {
        let mut command = std::process::Command::new(shell);
        let output = run_command_with_timeout(
            command.args(["-ilc", &script]),
            std::time::Duration::from_secs(4),
        );
        if let Some(path) = output
            .filter(|output| output.status.success())
            .and_then(|output| parse_marked_path(&output.stdout, START, END))
        {
            return Some(path);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("/bin/launchctl");
        if let Some(output) = run_command_with_timeout(
            command.args(["getenv", "PATH"]),
            std::time::Duration::from_secs(2),
        ) {
            if output.status.success() {
                let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !value.is_empty() {
                    return Some(OsString::from(value));
                }
            }
        }
    }
    None
}

#[cfg(windows)]
fn login_shell_path() -> Option<OsString> {
    None
}

/// Extract the PATH value printed between two marker strings. Interactive
/// shells may echo prompts or profile output around the markers.
#[cfg(not(windows))]
fn parse_marked_path(stdout: &[u8], start: &str, end: &str) -> Option<OsString> {
    let text = String::from_utf8_lossy(stdout);
    let after_start = text.rsplit_once(start)?.1;
    let path = after_start.split_once(end)?.0.trim();
    if path.is_empty() {
        None
    } else {
        Some(OsString::from(path))
    }
}

/// Run a command capturing stdout, giving up after `timeout`. Shells sourced
/// by `-ilc` can block on profile scripts, so a hung child is killed.
#[cfg(not(windows))]
fn run_command_with_timeout(
    command: &mut std::process::Command,
    timeout: std::time::Duration,
) -> Option<std::process::Output> {
    use std::io::{Read, Seek, SeekFrom};
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    let mut capture = tempfile::tempfile().ok()?;
    command
        .stdin(Stdio::null())
        .stdout(capture.try_clone().ok()?)
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().ok()?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        if capture.metadata().ok()?.len() > 128 * 1024 {
            stop(&mut child);
            return None;
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            _ => {
                stop(&mut child);
                return None;
            }
        }
    };
    capture.seek(SeekFrom::Start(0)).ok()?;
    let mut stdout = Vec::new();
    capture.take(128 * 1024 + 1).read_to_end(&mut stdout).ok()?;
    (stdout.len() <= 128 * 1024).then_some(std::process::Output {
        status,
        stdout,
        stderr: Vec::new(),
    })
}
#[cfg(not(windows))]
fn stop(child: &mut std::process::Child) {
    #[cfg(unix)]
    if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn binary_candidates(name: &str) -> Vec<OsString> {
    if cfg!(windows) {
        // npm-installed CLIs are `.cmd`/`.bat` shims, so honor PATHEXT like
        // the Windows command resolver does.
        let pathext =
            std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
        let mut candidates: Vec<OsString> = pathext
            .split(';')
            .filter(|ext| !ext.is_empty())
            .map(|ext| OsString::from(format!("{name}{ext}")))
            .collect();
        candidates.push(OsString::from(name));
        candidates
    } else {
        vec![OsString::from(name)]
    }
}

#[cfg(test)]
mod tests;
