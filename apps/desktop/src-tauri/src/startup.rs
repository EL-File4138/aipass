//! Desktop-owned Agent readiness and login registration.
use crate::{
    agent_client, repair_bundled_native_host_manifest, runtime_check, runtime_lifecycle, singleton,
};
use aipass_agent::AgentClient;
use std::{path::Path, thread};
use tauri::AppHandle;

pub(crate) fn ensure_agent_running_for_desktop(client: &AgentClient) -> Result<(), String> {
    let _starting = runtime_lifecycle::RUNTIME.start()?;
    ensure_agent_running_for_desktop_inner(client)
}

pub(crate) fn ensure_agent_running_for_desktop_inner(client: &AgentClient) -> Result<(), String> {
    if runtime_check::active() {
        return client
            .ensure_running_for_desktop_companion()
            .map_err(|err| err.to_string());
    }
    #[cfg(target_os = "macos")]
    {
        client.ensure_running().map_err(|err| err.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        client
            .ensure_running_for_desktop_companion()
            .map_err(|err| err.to_string())
    }
}

pub(crate) fn install_tray_autostart_for_current_desktop(
    desktop_binary: &Path,
    vault_dir: &Path,
) -> Result<(), String> {
    #[cfg(windows)]
    if !singleton::should_install_tray_autostart() {
        return Err("Windows login registration requires an installed release build".into());
    }
    #[cfg(target_os = "macos")]
    {
        let singleton_socket =
            singleton::current_singleton_socket_path().map_err(|err| err.to_string())?;
        aipass_agent::install_tray_autostart_with_socket(
            desktop_binary,
            vault_dir,
            &singleton_socket,
        )
        .map(|_| ())
        .map_err(|err| err.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        aipass_agent::install_tray_autostart(desktop_binary, vault_dir)
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) fn ensure_tray_autostart_for_current_desktop(
    binary: &Path,
    vault: &Path,
) -> Result<(), String> {
    aipass_agent::install_tray_autostart(binary, vault)
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[cfg(target_os = "macos")]
pub(crate) fn ensure_tray_autostart_for_current_desktop(
    desktop_binary: &Path,
    vault_dir: &Path,
) -> Result<(), String> {
    let singleton_socket =
        singleton::current_singleton_socket_path().map_err(|err| err.to_string())?;
    aipass_agent::ensure_tray_autostart_with_socket(desktop_binary, vault_dir, &singleton_socket)
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[cfg(target_os = "macos")]
pub(crate) fn stop_tray_autostart_for_current_desktop(vault_dir: &Path) -> Result<(), String> {
    let singleton_socket =
        singleton::current_singleton_socket_path().map_err(|err| err.to_string())?;
    aipass_agent::stop_tray_autostart_with_socket(vault_dir, &singleton_socket)
        .map(|_| ())
        .map_err(|err| err.to_string())
}

pub(crate) fn ensure_agent_resident_async(app: AppHandle) {
    thread::spawn(move || {
        let Ok(_starting) = runtime_lifecycle::RUNTIME.start() else {
            return;
        };
        let client = match agent_client(&app) {
            Ok(client) => client,
            Err(err) => {
                eprintln!("failed to resolve AIPass agent config: {err}");
                return;
            }
        };

        if runtime_check::active() {
            let _ = ensure_agent_running_for_desktop_inner(&client);
            return;
        }

        #[cfg(target_os = "macos")]
        match aipass_agent::agent_binary_path() {
            Ok(agent_binary) => {
                if let Err(err) =
                    aipass_agent::ensure_agent_autostart(&agent_binary, &client.config.vault_dir)
                {
                    eprintln!("failed to refresh AIPass agent autostart: {err}");
                }
            }
            Err(err) => {
                eprintln!("failed to resolve AIPass agent binary: {err}");
            }
        }

        if singleton::should_install_tray_autostart() {
            match std::env::current_exe() {
                Ok(desktop_binary) => {
                    if let Err(err) = ensure_tray_autostart_for_current_desktop(
                        &desktop_binary,
                        &client.config.vault_dir,
                    ) {
                        eprintln!("failed to refresh AIPass tray autostart: {err}");
                    }
                }
                Err(err) => {
                    eprintln!("failed to resolve AIPass desktop binary: {err}");
                }
            }
        }

        if let Err(err) = ensure_agent_running_for_desktop_inner(&client) {
            eprintln!("failed to ensure AIPass agent is running: {err}");
        }
        if let Err(err) = repair_bundled_native_host_manifest(&app) {
            eprintln!("failed to repair bundled AIPass native host manifest: {err}");
        }
    });
}
