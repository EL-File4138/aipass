//! Thin authenticated Agent adapters for local account imports.
use crate::commands::agent_request_no_unlock_async;
use aipass_agent_protocol::*;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::AppHandle;
use uuid::Uuid;

/// Resolve the location from the authenticated Agent task, never a UI path.
#[tauri::command]
pub(crate) async fn subscription_import_open_location(
    app: AppHandle,
    ticket: Uuid,
    source_id: Uuid,
) -> Result<(), String> {
    let task: SubscriptionImportTask =
        agent_request_no_unlock_async(app, AgentRequest::SubscriptionImportPoll { ticket }).await?;
    let root = location_from_task(&task, source_id)?;
    crate::run_blocking(move || open_directory(&root)).await
}

fn location_from_task(task: &SubscriptionImportTask, source_id: Uuid) -> Result<PathBuf, String> {
    task.results
        .iter()
        .find(|result| result.source_id == source_id)
        .map(|result| result.source.root.clone())
        .ok_or_else(|| "account source is no longer available".into())
}

#[cfg(test)]
mod location_tests {
    use super::*;

    #[test]
    fn opens_only_directories_and_rejects_invalid_locations() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("login.json");
        std::fs::write(&file, "fixture").unwrap();
        assert!(validate_directory(root.path()).is_ok());
        for invalid in [
            file,
            root.path().join("missing"),
            PathBuf::from("relative"),
            root.path().join(".."),
        ] {
            assert!(validate_directory(&invalid).is_err());
        }
    }

    #[test]
    fn resolves_only_a_source_in_the_authenticated_import_task() {
        let root = tempfile::tempdir().unwrap();
        let id = Uuid::new_v4();
        let task = SubscriptionImportTask {
            ticket: Uuid::new_v4(),
            phase: "complete".into(),
            total: 1,
            completed: 1,
            results: vec![SubscriptionImportResult {
                source_id: id,
                source: SubscriptionImportSource {
                    provider: "codex".into(),
                    root: root.path().into(),
                    selector: String::new(),
                },
                account_identity: None,
                status: SubscriptionImportStatus::NotFound,
                entry_id: None,
                error_code: None,
                action: None,
            }],
        };
        assert_eq!(location_from_task(&task, id).unwrap(), root.path());
        assert!(location_from_task(&task, Uuid::new_v4()).is_err());
    }
}

fn validate_directory(path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path.components().any(|c| matches!(c, Component::ParentDir))
        || !path.is_dir()
    {
        return Err("account directory is missing or invalid".into());
    }
    Ok(())
}

fn open_directory(path: &Path) -> Result<(), String> {
    validate_directory(path)?;
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer");
    #[cfg(target_os = "linux")]
    let mut command = Command::new("xdg-open");
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return Err("opening directories is unsupported on this platform".into());
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        // Separate argv prevents shell interpretation and opens only directories.
        let mut child = command
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "could not open account directory")?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                // Explorer may delegate to an existing process and exit nonzero.
                #[cfg(target_os = "windows")]
                Ok(Some(_)) => return Ok(()),
                #[cfg(not(target_os = "windows"))]
                Ok(Some(_)) => return Err("could not open account directory".into()),
                Err(_) => return Err("could not open account directory".into()),
                Ok(None) if std::time::Instant::now() >= deadline => return Ok(()),
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(25)),
            }
        }
    }
}

#[tauri::command]
pub(crate) async fn subscription_import_start(
    app: AppHandle,
    input: SubscriptionImportInput,
) -> Result<SubscriptionImportTask, String> {
    agent_request_no_unlock_async(app, AgentRequest::SubscriptionImportStart { input }).await
}
#[tauri::command]
pub(crate) async fn subscription_import_poll(
    app: AppHandle,
    ticket: Uuid,
) -> Result<SubscriptionImportTask, String> {
    agent_request_no_unlock_async(app, AgentRequest::SubscriptionImportPoll { ticket }).await
}
#[tauri::command]
pub(crate) async fn subscription_import_cancel(
    app: AppHandle,
    ticket: Uuid,
) -> Result<SubscriptionImportTask, String> {
    agent_request_no_unlock_async(app, AgentRequest::SubscriptionImportCancel { ticket }).await
}

#[tauri::command]
pub(crate) async fn official_accounts_refresh(
    app: AppHandle,
    provider_ids: Option<Vec<String>>,
) -> Result<Vec<OfficialAccountRefreshResult>, String> {
    agent_request_no_unlock_async(
        app,
        AgentRequest::OfficialAccountsRefresh {
            provider_ids: provider_ids.unwrap_or_default(),
        },
    )
    .await
}
