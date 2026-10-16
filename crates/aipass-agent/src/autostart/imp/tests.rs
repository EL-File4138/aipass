use super::*;

#[test]
fn suspending_tray_supervision_preserves_the_desktop_child() {
    let temp = tempfile::tempdir().unwrap();
    let paths = MacosAutostartPaths {
        plist_path: temp.path().join("tray.plist"),
        supervisor_path: temp.path().join("supervisor.sh"),
        log_dir: temp.path().to_path_buf(),
        out_log: temp.path().join("out.log"),
        err_log: temp.path().join("err.log"),
        supervisor_log: temp.path().join("supervisor.out.log"),
        supervisor_err_log: temp.path().join("supervisor.err.log"),
        stop_child_path: temp.path().join("stop-child"),
    };
    let service_name = format!("dev.aipass.test.tray.{}", uuid::Uuid::new_v4());
    let desktop = temp.path().join("desktop.sh");
    let pid_path = temp.path().join("desktop.pid");
    write_supervisor(
                &desktop,
                &format!(
                    "#!/bin/sh\necho $$ > {pid}.pending\nmv {pid}.pending {pid}\n/bin/sleep 300 &\nchild=$!\ntrap 'kill \"$child\"; exit' TERM INT\nwait \"$child\"\n",
                    pid = shell_quote(&pid_path.display().to_string())
                ),
            )
            .unwrap();
    let script = macos_tray_supervisor_script(
        &service_name,
        &paths.plist_path,
        &desktop,
        &temp.path().join("vault"),
        &paths.out_log,
        &paths.err_log,
        &paths.stop_child_path,
        &temp.path().join("desktop.sock"),
    )
    .unwrap();
    write_supervisor(&paths.supervisor_path, &script).unwrap();
    fs::write(
        &paths.plist_path,
        macos_plist(
            &service_name,
            &paths.supervisor_path,
            &paths.supervisor_log,
            &paths.supervisor_err_log,
            true,
        ),
    )
    .unwrap();

    // Only register this fixture's unique service; never change HOME
    // or touch the user's real AIPass LaunchAgents or vault.
    struct Cleanup<'a> {
        service: &'a str,
        paths: &'a MacosAutostartPaths,
        pid_path: &'a Path,
    }
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            let _ = write_stop_child_flag(&self.paths.stop_child_path);
            let _ = unload_launch_agent(self.service, &self.paths.plist_path);
            if let Ok(pid) = fs::read_to_string(self.pid_path) {
                let _ = Command::new("kill")
                    .arg(pid.trim())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }
    }
    let _cleanup = Cleanup {
        service: &service_name,
        paths: &paths,
        pid_path: &pid_path,
    };
    load_launch_agent(&service_name, &paths.plist_path).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !pid_path.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let pid = fs::read_to_string(&pid_path).expect("supervised desktop started");
    assert!(launch_agent_running(&service_name));

    // A previous interrupted stop must not make suspension kill the
    // process that is about to replace the bundle.
    write_stop_child_flag(&paths.stop_child_path).unwrap();
    suspend_tray_service(&service_name, &paths).unwrap();
    assert!(!paths.stop_child_path.exists());
    assert!(!launch_agent_running(&service_name));
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(Command::new("kill")
        .args(["-0", pid.trim()])
        .status()
        .unwrap()
        .success());
    assert!(paths.plist_path.exists());
    assert!(paths.supervisor_path.exists());

    // Update failure can restore supervision without replacing the
    // desktop process that still owns the window.
    load_launch_agent(&service_name, &paths.plist_path).unwrap();
    assert!(launch_agent_running(&service_name));
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(fs::read_to_string(&pid_path).unwrap(), pid);
    suspend_tray_service(&service_name, &paths).unwrap();
    assert!(Command::new("kill")
        .args(["-0", pid.trim()])
        .status()
        .unwrap()
        .success());
}

#[test]
fn macos_agent_supervisor_waits_for_existing_agent_and_suppresses_tray_launch() {
    let script = macos_supervisor_script(
        "dev.aipass.agent.test",
        Path::new("/tmp/dev.aipass.agent.test.plist"),
        Path::new("/tmp/aipass-agent"),
        Path::new("/tmp/aipass-vault"),
        Path::new("/tmp/agent.out.log"),
        Path::new("/tmp/agent.err.log"),
    );

    assert!(script.contains("pgrep -f \"aipass-agent --vault $VAULT\""));
    assert!(script.contains("wait_for_existing_agent"));
    assert!(script.contains("AIPASS_AGENT_SUPPRESS_TRAY=1 \"$AGENT\" --vault \"$VAULT\""));
}

#[test]
fn macos_tray_supervisor_launches_tray_target_and_waits_for_existing_desktop() {
    let script = macos_tray_supervisor_script(
        "dev.aipass.desktop.tray.test",
        Path::new("/tmp/dev.aipass.desktop.tray.test.plist"),
        Path::new("/tmp/aipass-desktop"),
        Path::new("/tmp/aipass-vault"),
        Path::new("/tmp/tray.out.log"),
        Path::new("/tmp/tray.err.log"),
        Path::new("/tmp/tray.stop-child"),
        Path::new("/tmp/desktop-dev-bundle.sock"),
    )
    .expect("tray supervisor script");

    assert!(script.contains("pgrep -f \"$DESKTOP\""));
    assert!(script.contains("wait_for_existing_desktop"));
    assert!(script.contains("AIPASS_WINDOW_TARGET=tray AIPASS_VAULT_DIR=\"$VAULT\" \"$DESKTOP\""));
    assert!(script.contains("SINGLETON_SOCKET='/tmp/desktop-dev-bundle.sock'"));
    assert!(script.contains("STOP_CHILD="));
    assert!(script.contains("if [ -f \"$STOP_CHILD\" ]; then"));
}

#[test]
fn autostart_file_match_requires_current_contents_and_executable_supervisor() {
    let temp = tempfile::tempdir().unwrap();
    let supervisor_path = temp.path().join("service.sh");
    let plist_path = temp.path().join("service.plist");
    fs::write(&supervisor_path, "supervisor").unwrap();
    fs::write(&plist_path, "plist").unwrap();
    fs::set_permissions(&supervisor_path, fs::Permissions::from_mode(0o755)).unwrap();
    let paths = MacosAutostartPaths {
        plist_path,
        supervisor_path: supervisor_path.clone(),
        log_dir: temp.path().join("logs"),
        out_log: temp.path().join("out.log"),
        err_log: temp.path().join("err.log"),
        supervisor_log: temp.path().join("supervisor.out.log"),
        supervisor_err_log: temp.path().join("supervisor.err.log"),
        stop_child_path: temp.path().join("stop-child"),
    };

    assert!(autostart_files_match(&paths, "supervisor", "plist"));
    fs::set_permissions(&supervisor_path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(!autostart_files_match(&paths, "supervisor", "plist"));
    fs::set_permissions(&supervisor_path, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!autostart_files_match(&paths, "changed", "plist"));
    assert!(!autostart_files_match(&paths, "supervisor", "changed"));
}
