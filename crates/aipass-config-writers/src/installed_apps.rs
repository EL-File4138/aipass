use crate::ToolId;
use std::path::{Path, PathBuf};

pub(super) fn desktop_app_installed(home: Option<&Path>, tool: &ToolId) -> bool {
    desktop_app_variant_installed(home, tool, None)
}

pub(super) fn vscode_host_installed(home: &Path, dirs: &[PathBuf], insiders: bool) -> bool {
    let names: &[&str] = if insiders {
        &["code-insiders"]
    } else {
        &["code"]
    };
    desktop_app_variant_installed(Some(home), &ToolId::VscodeChat, Some(insiders))
        || crate::cli_discovery::executable_candidates_in(dirs, names)
            .iter()
            .any(|path| {
                // Flatpak uses its sandbox's extension directory, even when a
                // native-looking `code` wrapper launches the sandbox.
                !cfg!(target_os = "linux")
                    || !std::fs::read_to_string(path).is_ok_and(|script| {
                        script.contains("flatpak") && script.contains("com.visualstudio.code")
                    })
            })
}

fn desktop_app_variant_installed(
    home: Option<&Path>,
    tool: &ToolId,
    vscode_insiders: Option<bool>,
) -> bool {
    #[cfg(target_os = "macos")]
    {
        let apps: &[&str] = match tool {
            ToolId::Zed => &["Zed.app"],
            ToolId::Zcode => &["ZCode.app", "Zcode.app"],
            ToolId::VscodeChat => match vscode_insiders {
                Some(false) => &["Visual Studio Code.app"],
                Some(true) => &["Visual Studio Code - Insiders.app"],
                None => &[
                    "Visual Studio Code.app",
                    "Visual Studio Code - Insiders.app",
                ],
            },
            ToolId::Codebuddy => &["CodeBuddy.app", "CodeBuddy CN.app"],
            ToolId::Workbuddy => &["WorkBuddy.app"],
            ToolId::Trae => &["TRAE.app", "Trae.app", "Trae CN.app"],
            ToolId::Qoder => &["Qoder IDE.app", "Qoder.app"],
            _ => &[],
        };
        apps.iter().any(|app| {
            [
                Some(PathBuf::from("/Applications")),
                home.map(|h| h.join("Applications")),
            ]
            .into_iter()
            .flatten()
            .any(|root| {
                let bundle = root.join(app);
                let info = bundle.join("Contents/Info.plist");
                let output = std::process::Command::new("/usr/bin/plutil")
                    .args(["-extract", "CFBundleExecutable", "raw", "-o", "-"])
                    .arg(info)
                    .output();
                output
                    .ok()
                    .filter(|o| o.status.success())
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .is_some_and(|name| {
                        let name = name.trim();
                        !name.is_empty()
                            && !name.contains(['/', '\\'])
                            && crate::cli_discovery::is_executable(
                                &bundle.join("Contents/MacOS").join(name),
                            )
                    })
            })
        })
    }
    #[cfg(target_os = "windows")]
    {
        let relatives: &[&str] = match tool {
            ToolId::Zed => &["Zed/zed.exe"],
            ToolId::Zcode => &["ZCode/ZCode.exe", "ZCode/Zcode.exe"],
            ToolId::VscodeChat => match vscode_insiders {
                Some(false) => &["Microsoft VS Code/Code.exe"],
                Some(true) => &["Microsoft VS Code Insiders/Code - Insiders.exe"],
                None => &[
                    "Microsoft VS Code/Code.exe",
                    "Microsoft VS Code Insiders/Code - Insiders.exe",
                ],
            },
            ToolId::Codebuddy => &["CodeBuddy/CodeBuddy.exe"],
            ToolId::Workbuddy => &["WorkBuddy/WorkBuddy.exe"],
            ToolId::Trae => &["Trae/Trae.exe", "Trae CN/Trae CN.exe"],
            ToolId::Qoder => &["Qoder IDE/Qoder IDE.exe", "Qoder/Qoder.exe"],
            _ => &[],
        };
        let roots = [
            std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
            home.map(|p| p.join("AppData/Local")),
            std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Programs")),
            std::env::var_os("ProgramFiles").map(PathBuf::from),
            std::env::var_os("ProgramFiles(x86)").map(PathBuf::from),
            home.map(|p| p.join("AppData/Local/Programs")),
        ];
        roots.into_iter().flatten().any(|root| {
            relatives
                .iter()
                .any(|path| crate::cli_discovery::is_executable(&root.join(path)))
        })
    }
    #[cfg(target_os = "linux")]
    {
        let names: &[&str] = match tool {
            ToolId::VscodeChat => match vscode_insiders {
                Some(false) => &["code"],
                Some(true) => &["code-insiders"],
                None => &["code", "code-insiders"],
            },
            ToolId::Zed => &["zed", "zeditor"],
            ToolId::Zcode => &["zcode", "ZCode"],
            ToolId::Codebuddy => &["codebuddy", "CodeBuddy"],
            ToolId::Workbuddy => &["workbuddy", "WorkBuddy"],
            ToolId::Trae => &["trae", "trae-cn"],
            ToolId::Qoder => &["qoder-ide", "qoder"],
            _ => &[],
        };
        (vscode_insiders.is_none() && flatpak_executable(home, tool).is_some())
            || names.iter().any(|name| {
                [
                    Some(PathBuf::from("/usr/share")),
                    Some(PathBuf::from("/opt")),
                    home.map(|p| p.join(".local/share")),
                ]
                .into_iter()
                .flatten()
                .any(|root| crate::cli_discovery::is_executable(&root.join(name).join(name)))
            })
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = (home, tool, vscode_insiders);
        false
    }
}

/// JetBrains keeps plugin directories after uninstall. Match them against a
/// product-info manifest whose launcher still exists, including Toolbox apps.
pub(super) fn jetbrains_hosts(home: &Path) -> std::collections::HashSet<String> {
    let mut roots = vec![
        home.join("Applications"),
        home.join(".local/share/JetBrains/Toolbox/apps"),
        home.join("Library/Application Support/JetBrains/Toolbox/apps"),
        home.join("AppData/Local/JetBrains/Toolbox/apps"),
    ];
    #[cfg(target_os = "macos")]
    roots.push(PathBuf::from("/Applications"));
    #[cfg(target_os = "linux")]
    roots.push(PathBuf::from("/opt"));
    #[cfg(windows)]
    for key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Some(root) = std::env::var_os(key)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
        {
            roots.push(root.join("JetBrains"));
        }
    }
    jetbrains_hosts_in(&roots)
}
fn jetbrains_hosts_in(roots: &[PathBuf]) -> std::collections::HashSet<String> {
    let mut hosts = std::collections::HashSet::new();
    let mut queue: Vec<_> = roots.iter().rev().map(|p| (p.clone(), 0)).collect();
    let mut visited = 0;
    while let Some((dir, depth)) = queue.pop() {
        visited += 1;
        if visited > 2048 {
            break;
        }
        for relative in ["product-info.json", "Contents/Resources/product-info.json"] {
            let info = dir.join(relative);
            if let Ok(raw) = std::fs::read_to_string(&info) {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                    let launcher = value["launch"].as_array().is_some_and(|launches| {
                        launches.iter().any(|launch| {
                            launch["launcherPath"].as_str().is_some_and(|path| {
                                let path = info.parent().unwrap().join(path);
                                path.canonicalize()
                                    .ok()
                                    .zip(dir.canonicalize().ok())
                                    .is_some_and(|(path, root)| {
                                        path.starts_with(root)
                                            && crate::cli_discovery::is_executable(&path)
                                    })
                            })
                        })
                    });
                    if launcher {
                        if let Some(name) = value["dataDirectoryName"].as_str() {
                            hosts.insert(name.to_owned());
                        }
                    }
                }
            }
        }
        if depth < 4 {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                queue.extend(
                    entries
                        .flatten()
                        .filter(|f| f.file_type().is_ok_and(|t| t.is_dir()))
                        .take(256)
                        .map(|f| (f.path(), depth + 1)),
                );
            }
        }
    }
    hosts
}

#[cfg(target_os = "linux")]
pub(super) fn flatpak_executable(home: Option<&Path>, tool: &ToolId) -> Option<PathBuf> {
    let id = match tool {
        ToolId::Zed => "dev.zed.Zed",
        ToolId::VscodeChat => "com.visualstudio.code",
        _ => return None,
    };
    [
        home.map(|p| p.join(".local/share/flatpak/exports/bin")),
        Some(PathBuf::from("/var/lib/flatpak/exports/bin")),
    ]
    .into_iter()
    .flatten()
    .map(|root| root.join(id))
    .find(|p| crate::cli_discovery::is_executable(p))
}

#[cfg(target_os = "linux")]
pub(super) fn zed_uses_flatpak(home: &Path) -> bool {
    flatpak_executable(Some(home), &ToolId::Zed).is_some()
        && crate::cli_discovery::tool_executables(&ToolId::Zed)
            .iter()
            .all(|path| {
                std::fs::read_to_string(path).is_ok_and(|script| {
                    script.contains("flatpak") && script.contains("dev.zed.Zed")
                })
            })
}
