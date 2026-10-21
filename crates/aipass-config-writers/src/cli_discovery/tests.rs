use super::*;
use tempfile::tempdir;
#[cfg(unix)]
#[test]
fn ambiguous_aliases_require_vendor_paths_or_an_explicit_grok_override() {
    use crate::ToolId;
    let root = tempdir().unwrap();
    let bin = root.path().join("bin");
    install(&bin.join("agent"));
    install(&bin.join("grok"));
    let dirs = [bin.clone()];
    assert!(tool_executables_in(&dirs, &ToolId::Cursor, None).is_empty());
    assert!(tool_executables_in(&dirs, &ToolId::Grok, None).is_empty());
    assert!(tool_executables_in(&dirs, &ToolId::Grok, Some(Path::new("relative"))).is_empty());
    assert_eq!(
        tool_executables_in(&dirs, &ToolId::Grok, Some(&bin)),
        vec![bin.join("grok")]
    );
    std::fs::remove_file(bin.join("agent")).unwrap();
    let real_cursor = root.path().join("cursor-agent/versions/current/agent");
    install(&real_cursor);
    std::os::unix::fs::symlink(&real_cursor, bin.join("agent")).unwrap();
    assert_eq!(
        tool_executables_in(&dirs, &ToolId::Cursor, None),
        vec![bin.join("agent")]
    );
    let grok = root.path().join(".grok/bin/grok");
    install(&grok);
    assert_eq!(
        tool_executables_in(
            &[bin, grok.parent().unwrap().to_owned()],
            &ToolId::Grok,
            None
        ),
        vec![grok]
    );
}
#[cfg(unix)]
#[test]
fn system_installed_cli_does_not_put_system_node_before_user_node() {
    let user_node = PathBuf::from("/home/alice/.nvm/versions/node/v26.0.0/bin");
    let path = command_path_in(
        Path::new("/usr/bin/aipass-fixture-vendor"),
        vec![PathBuf::from("/usr/bin"), user_node.clone()],
    )
    .unwrap();
    let dirs: Vec<_> = std::env::split_paths(&path).collect();
    assert_eq!(dirs[0], user_node);
    assert!(dirs.iter().any(|p| p == Path::new("/usr/bin")));
}
#[cfg(unix)]
#[test]
fn vendor_subprocess_prefers_discovered_user_node_over_distro_node() {
    let dirs = [
        "/usr/bin",
        "/custom/bin",
        "/home/alice/.nvm/versions/node/v26.0.0/bin",
        "/bin",
    ];
    let sorted = command_search_dirs(dirs.into_iter().map(PathBuf::from).collect());
    assert_eq!(sorted[0], Path::new("/custom/bin"));
    assert_eq!(
        sorted[1],
        Path::new("/home/alice/.nvm/versions/node/v26.0.0/bin")
    );
    assert_eq!(sorted[2], Path::new("/usr/bin"));
}
#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}
#[cfg(not(unix))]
fn make_executable(_: &Path) {}
fn install(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        if cfg!(windows) {
            "MZfixture"
        } else {
            "#!/bin/sh\n"
        },
    )
    .unwrap();
    make_executable(path);
}
#[test]
fn native_installs_nvm_fnm_and_permissions_match_real_execution() {
    let temp = tempdir().unwrap();
    let home = temp.path();
    for (rel, name) in [
        (".local/bin", "claude"),
        (".cargo/bin", "codex"),
        (
            ".local/share/fnm/node-versions/v26.0.0/installation/bin",
            "gemini",
        ),
        (".nvm/versions/node/v9.0.0/bin", "grok"),
        (".nvm/versions/node/v26.0.0/bin", "grok"),
    ] {
        install(&home.join(rel).join(binary_candidates(name).remove(0)));
    }
    let dirs = binary_search_dirs(None, Some(home));
    for name in ["claude", "codex", "gemini", "grok"] {
        assert!(
            !executable_candidates_in(&dirs, &[name]).is_empty(),
            "{name}"
        );
    }
    let grok = executable_candidates_in(&dirs, &["grok"]);
    assert!(grok[0].to_string_lossy().contains("v26.0.0"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bad = home.join(".local/bin/opencode");
        install(&bad);
        std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(executable_candidates_in(&dirs, &["opencode"]).is_empty());
    }
    assert!(executable_candidates_in(&[PathBuf::from(".")], &["claude"]).is_empty());
}
#[cfg(unix)]
#[test]
fn shell_probe_is_bounded_for_noisy_or_inherited_stdout() {
    let mut command = std::process::Command::new("/bin/sh");
    command.args(["-c", "yes noise"]);
    assert!(
        run_command_with_timeout(&mut command, std::time::Duration::from_millis(150)).is_none()
    );
    let mut command = std::process::Command::new("/bin/sh");
    command.args(["-c", "sleep 2 & printf done"]);
    let start = std::time::Instant::now();
    assert!(
        run_command_with_timeout(&mut command, std::time::Duration::from_millis(150)).is_some()
    );
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
}
#[cfg(not(windows))]
#[test]
fn markers_ignore_profile_noise() {
    assert_eq!(
        parse_marked_path(
            b"noise__START__/usr/bin:/bin\n__END__",
            "__START__",
            "__END__"
        ),
        Some(OsString::from("/usr/bin:/bin"))
    );
}

#[test]
fn windows_resolver_rejects_extensionless_files_and_corrupt_binaries() {
    let root = tempdir().unwrap();
    for name in ["tool", "tool.ps1", "tool.exe", "tool.cmd"] {
        std::fs::write(root.path().join(name), "not an executable").unwrap();
    }
    assert!(!windows_entrypoint(&root.path().join("tool")));
    assert!(!windows_entrypoint(&root.path().join("tool.ps1")));
    assert!(!windows_entrypoint(&root.path().join("tool.exe")));
    assert!(windows_entrypoint(&root.path().join("tool.cmd")));
    std::fs::write(root.path().join("tool.exe"), "MZfixture").unwrap();
    assert!(windows_entrypoint(&root.path().join("tool.exe")));
}
#[test]
fn removed_npm_package_invalidates_both_unix_and_windows_shims() {
    let root = tempdir().unwrap();
    let target = root.path().join("node_modules/vendor/bin.js");
    for content in [
        r#"node "$basedir/node_modules/vendor/bin.js" "$@""#,
        r#"node "%dp0%\node_modules\vendor\bin.js" %*"#,
        r#"node "%~dp0\node_modules\vendor\bin.js" %*"#,
    ] {
        let shim = root.path().join("tool.cmd");
        std::fs::write(&shim, content).unwrap();
        assert!(!valid_shim_targets(&shim));
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "fixture").unwrap();
        assert!(valid_shim_targets(&shim));
        std::fs::remove_file(&target).unwrap();
    }
}

#[test]
fn npm_grok_and_command_code_shims_require_existing_vendor_entry_points() {
    let root = tempdir().unwrap();
    for (tool, name, package) in [
        (crate::ToolId::Grok, "grok", "@xai-official/grok"),
        (crate::ToolId::CommandCode, "cmd", "command-code"),
    ] {
        let path = root.path().join(if cfg!(windows) {
            format!("{name}.cmd")
        } else {
            name.into()
        });
        let target = format!("node_modules/{package}/bin.js");
        std::fs::write(
            &path,
            if cfg!(windows) {
                format!("node \"%dp0%\\{target}\" %*")
            } else {
                format!("#!/bin/sh\nnode \"$basedir/{target}\"")
            },
        )
        .unwrap();
        make_executable(&path);
        assert!(tool_executables_in(&[root.path().into()], &tool, None).is_empty());
        std::fs::create_dir_all(root.path().join(&target).parent().unwrap()).unwrap();
        std::fs::write(root.path().join(&target), "fixture").unwrap();
        assert_eq!(
            tool_executables_in(&[root.path().into()], &tool, None),
            vec![path]
        );
    }
}

#[test]
fn wsl_wrappers_are_not_native_windows_agent_installations() {
    assert!(windows_wsl_shim(
        "@echo off\r\nwsl.exe -d Ubuntu kimi %*\r\n"
    ));
    assert!(windows_wsl_shim(
        r#"@"C:\Windows\System32\wsl.exe" -d Ubuntu kimi %*"#
    ));
    assert!(!windows_wsl_shim(
        r#"node "%dp0%\node_modules\vendor\bin.js" %*"#
    ));
}

#[cfg(unix)]
#[test]
fn command_shim_identity_does_not_rewrite_the_path_used_for_file_io() {
    let root = tempdir().unwrap();
    let bin = root.path().join(r"bin \ literal O'Brien 中文");
    let js = bin.join("node_modules/command-code/bin.js");
    std::fs::create_dir_all(js.parent().unwrap()).unwrap();
    std::fs::write(js, "fixture").unwrap();
    let shim = bin.join("cmd");
    std::fs::write(
        &shim,
        r#"#!/bin/sh
node "$basedir/node_modules/command-code/bin.js"
"#,
    )
    .unwrap();
    make_executable(&shim);
    assert_eq!(
        tool_executables_in(&[bin], &crate::ToolId::CommandCode, None),
        vec![shim]
    );
}
