//! Repair and quit actions shared by native tray frontends.
use super::*;

pub(super) fn install_login_agent_async(app: AppHandle, feedback: TrayFeedback) {
    thread::spawn(move || {
        let result = crate::runtime_lifecycle::RUNTIME
            .start()
            .and_then(|_starting| {
                let client = agent_client(&app)?;
                let desktop_binary = std::env::current_exe().map_err(|err| err.to_string())?;
                // Windows login starts the hidden desktop, which owns Agent
                // readiness. Normal tray repair must not require SCM/admin.
                #[cfg(not(windows))]
                {
                    let agent_binary =
                        aipass_agent::agent_binary_path().map_err(|err| err.to_string())?;
                    aipass_agent::install_agent_autostart(&agent_binary, &client.config.vault_dir)
                        .map_err(|err| err.to_string())?;
                }
                install_tray_autostart_for_current_desktop(
                    &desktop_binary,
                    &client.config.vault_dir,
                )?;
                crate::ensure_agent_running_for_desktop_inner(&client)
            });
        match result {
            Ok(_) => {
                feedback.reset_repair_text();
                refresh_status(&app, &feedback);
            }
            Err(err) => {
                eprintln!("failed to install AIPass agent autostart: {err}");
                feedback.agent_transient(&tr("tray.agent.autostartFailed"));
            }
        }
    });
}

pub(super) fn quit_aipass_async(app: AppHandle, feedback: TrayFeedback) {
    thread::spawn(move || {
        let _ = crate::logging::log_event("desktop.quit.requested", &[("source", "tray")]);
        let result = crate::runtime_lifecycle::RUNTIME
            .quit()
            .and_then(|mut quitting| {
                #[cfg(target_os = "macos")]
                {
                    // Drain startup/repair before unloading supervision. Do not send
                    // Quit to our own singleton or kill its child mid-shutdown.
                    let stopped = agent_client(&app).and_then(|client| {
                        aipass_agent::suspend_tray_autostart(&client.config.vault_dir)
                            .map(|_| ())
                            .map_err(|err| err.to_string())
                    });
                    if let Err(err) = stopped {
                        *quitting = false;
                        return Err(err);
                    }
                }
                #[cfg(not(target_os = "macos"))]
                let _ = &mut quitting;
                Ok(())
            });
        if let Err(err) = result {
            eprintln!("failed to stop AIPass tray autostart before quit: {err}");
            feedback.agent_transient(&tr("tray.quitFailed"));
            return;
        }
        crate::ALLOW_PROCESS_EXIT.store(true, std::sync::atomic::Ordering::SeqCst);
        #[cfg(target_os = "macos")]
        crate::tray_swift::shutdown();
        let _ = crate::logging::log_event("desktop.quit.exiting", &[]);
        app.exit(0);
    });
}
