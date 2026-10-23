//! Transactional replacement of a vault and its matching sync settings.
use super::*;

fn import_directory(
    root: &Path,
    input: &Path,
    password: &aipass_crypto::SecretString,
) -> Result<()> {
    let vault = Vault::open(input, password).context("cannot unlock source vault")?;
    let bytes = vault.export_sync_snapshot()?;
    VaultSyncSnapshot::parse(&bytes)?.bootstrap(root)?;
    Vault::open(root, password)?;
    Ok(())
}

pub(crate) fn import_file(
    state: &Arc<AgentState>,
    input: &Path,
    password: aipass_agent_protocol::SensitiveString,
) -> ServiceResult<()> {
    let _sync = state
        .sync_lock
        .lock()
        .map_err(|_| ServiceError::internal(anyhow::anyhow!("sync lock poisoned")))?;
    let root = &state.vault_dir;
    let parent = root
        .parent()
        .context("vault parent missing")
        .map_err(ServiceError::internal)?;
    let stage = tempfile::tempdir_in(parent).map_err(ServiceError::internal)?;
    let secret = aipass_crypto::SecretString::new(password.into_inner());
    if input.is_dir() {
        import_directory(stage.path(), input, &secret).map_err(ServiceError::internal)?;
    } else {
        let bytes = fs::read(input).map_err(ServiceError::internal)?;
        if let Ok(snapshot) = VaultSyncSnapshot::parse(&bytes) {
            snapshot
                .bootstrap(stage.path())
                .map_err(crate::session::map_vault_error)?;
            Vault::open(stage.path(), &secret).map_err(crate::session::map_vault_error)?;
        } else {
            let export = serde_json::from_slice(&bytes).map_err(ServiceError::internal)?;
            Vault::import_encrypted(stage.path(), &secret, &export)
                .map_err(crate::session::map_vault_error)?;
        }
    }
    // The imported vault carries its disconnected settings through the same
    // directory rename; it can never inherit the old vault's sync target.
    let settings = serde_json::to_vec(&crate::session::PersistedSyncSettings::default())
        .map_err(ServiceError::internal)?;
    install_import(state, stage.path(), &settings)?;
    crate::sync_watch::start_sync_watcher_for_current_settings(state);
    Ok(())
}

fn install_import(state: &Arc<AgentState>, stage: &Path, settings: &[u8]) -> ServiceResult<()> {
    atomic_write_bytes(stage.join("pending-import-settings.json"), settings)
        .map_err(ServiceError::internal)?;
    let mut session = state
        .session
        .lock()
        .map_err(|_| ServiceError::internal(anyhow::anyhow!("session lock poisoned")))?;
    let mut proxy = state
        .proxy
        .lock()
        .map_err(|_| ServiceError::internal(anyhow::anyhow!("proxy lock poisoned")))?;
    proxy.suspend_for_import()?;
    // Directory replacement must never leave an unlocked session pointing at
    // a different or missing vault, even if both install and rollback fail.
    *session = SessionState::Locked;
    if let Ok(mut cached) = state.webdav_transport.lock() {
        *cached = None;
    }
    state.session_changed.notify_all();
    let root = &state.vault_dir;
    let backup = root.with_file_name(format!("vault-import-backup-{}", uuid::Uuid::new_v4()));
    let had_root = root.exists();
    let mut replaced = false;
    let result = (|| {
        if had_root {
            fs::rename(root, &backup).map_err(ServiceError::internal)?;
        }
        if let Err(err) = fs::rename(stage, root) {
            if had_root {
                fs::rename(&backup, root).map_err(ServiceError::internal)?;
            }
            return Err(ServiceError::internal(err));
        }
        replaced = true;
        state
            .sync_revision
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        crate::session::finish_import_settings(root, settings).map_err(ServiceError::internal)
    })();
    // Reopen against the installed directory, including after a settings
    // journal failure; on rollback, reopen the original database instead.
    let reopened = proxy.resume_after_import(replaced || !root.is_dir());
    result.and(reopened)
}

pub(crate) fn import_sync(
    state: &Arc<AgentState>,
    update: aipass_agent_protocol::SyncSettingsUpdate,
    password: aipass_agent_protocol::SensitiveString,
) -> ServiceResult<()> {
    let _sync = state
        .sync_lock
        .lock()
        .map_err(|_| ServiceError::internal(anyhow::anyhow!("sync lock poisoned")))?;
    if state.vault_dir.join("manifest.aipmanifest").exists() {
        return Err(crate::session::map_vault_error(
            aipass_vault::VaultError::AlreadyExists,
        ));
    }
    let settings = crate::session::apply_sync_settings_update(
        crate::session::StoredSyncSettings::default(),
        update,
    );
    let stage = tempfile::tempdir_in(
        state
            .vault_dir
            .parent()
            .context("vault parent missing")
            .map_err(ServiceError::internal)?,
    )
    .map_err(ServiceError::internal)?;
    let (bytes, id, target) = if let Some(dir) = crate::sync_watch::folder_sync_dir(&settings) {
        let (bytes, id) = single_snapshot(&aipass_sync::FolderSnapshotRemote(&dir))
            .map_err(ServiceError::internal)?;
        (bytes, id, format!("folder:{}", dir.display()))
    } else if settings.mode == aipass_agent_protocol::SyncMode::ICloud {
        let remote = crate::cloudkit::Remote::connect(&state.cloudkit).map_err(remote_error)?;
        let (bytes, id) = single_snapshot(&remote).map_err(ServiceError::internal)?;
        check_cloud_account(stage.path(), &remote.account).map_err(ServiceError::internal)?;
        (bytes, id, format!("cloudkit:{}", remote.account))
    } else if settings.mode == aipass_agent_protocol::SyncMode::WebDav {
        let url = settings
            .webdav_url
            .as_ref()
            .context("WebDAV URL is required")
            .map_err(ServiceError::internal)?;
        let dav_password = match &settings.webdav_password {
            Some(crate::session::StoredSyncSecret::Plaintext(value)) => {
                Some(value.clone().into_inner())
            }
            _ => None,
        };
        let client =
            aipass_sync::HttpWebDavClient::new(url, settings.webdav_username.clone(), dav_password)
                .map_err(ServiceError::internal)?;
        let (bytes, id) = single_snapshot(&aipass_sync::WebDavSnapshotRemote(&client))
            .map_err(ServiceError::internal)?;
        (bytes, id, format!("webdav:{url}"))
    } else {
        return Err(ServiceError::internal(anyhow::anyhow!(
            "sync folder unavailable"
        )));
    };
    VaultSyncSnapshot::parse(&bytes)
        .map_err(crate::session::map_vault_error)?
        .bootstrap(stage.path())
        .map_err(crate::session::map_vault_error)?;
    let vault = Vault::open(
        stage.path(),
        &aipass_crypto::SecretString::new(password.into_inner()),
    )
    .map_err(crate::session::map_vault_error)?;
    let checkpoint = Checkpoint {
        heads: vec![id.clone()],
        revision: Some(
            vault
                .sync_revision()
                .map_err(crate::session::map_vault_error)?,
        ),
    };
    atomic_write_bytes(
        cache_path(stage.path(), &id).map_err(ServiceError::internal)?,
        &bytes,
    )
    .map_err(ServiceError::internal)?;
    atomic_write_bytes(
        checkpoint_path(stage.path(), &target),
        &serde_json::to_vec(&checkpoint).map_err(ServiceError::internal)?,
    )
    .map_err(ServiceError::internal)?;
    let (persisted, saved) =
        crate::session::prepare_sync_settings(&vault, &settings).map_err(ServiceError::internal)?;
    drop(vault);
    install_import(state, stage.path(), &persisted)?;
    crate::sync_watch::restart_sync_watcher(state, &saved);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aipass_crypto::SecretString;

    fn state(root: &Path) -> Arc<AgentState> {
        let state = crate::server::tests::sync_test_state(root.to_owned());
        crate::session::set_session_vault(
            &state,
            Vault::create(root, &SecretString::new("master"))
                .unwrap()
                .vault,
        );
        state
    }
    fn log(root: &Path, message: &str) {
        rusqlite::Connection::open(root.join("proxy-usage.sqlite"))
            .unwrap()
            .execute(
                "INSERT INTO proxy_diagnostics(timestamp, level, message) VALUES (1, 'info', ?1)",
                [message],
            )
            .unwrap();
    }

    #[test]
    fn imported_vault_uses_its_own_database_and_retains_the_old_backup() {
        let dir = tempfile::tempdir().unwrap();
        let state = state(&dir.path().join("vault"));
        log(&state.vault_dir, "old-vault");
        let incoming = dir.path().join("incoming");
        let incoming_vault = Vault::create(&incoming, &SecretString::new("master"))
            .unwrap()
            .vault;
        import_file(&state, &incoming, "master".into()).unwrap();
        assert!(crate::session::session_status(&state).unwrap().locked);
        assert!(state.proxy.lock().unwrap().logs().unwrap().is_empty());
        log(&state.vault_dir, "new-vault");
        assert_eq!(
            state.proxy.lock().unwrap().logs().unwrap()[0].message,
            "new-vault"
        );
        let installed = Vault::open(&state.vault_dir, &SecretString::new("master")).unwrap();
        assert_eq!(installed.vault_id(), incoming_vault.vault_id());
        let backup = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("vault-import-backup-")
            })
            .unwrap();
        let retained = aipass_proxy::UsageStore::open(backup.join("proxy-usage.sqlite")).unwrap();
        assert_eq!(retained.logs().unwrap().len(), 1);
        assert_eq!(retained.logs().unwrap()[0].message, "old-vault");
    }

    #[test]
    fn failed_directory_install_rolls_back_and_reopens_original_usage() {
        let dir = tempfile::tempdir().unwrap();
        let state = state(&dir.path().join("vault"));
        log(&state.vault_dir, "old-vault");
        // Renaming the original vault moves this deliberately invalid stage,
        // so installing it fails after the first rename and exercises rollback.
        let stage = state.vault_dir.join("incoming");
        fs::create_dir(&stage).unwrap();
        assert!(install_import(&state, &stage, b"{}").is_err());
        assert!(crate::session::session_status(&state).unwrap().locked);
        assert_eq!(
            state.proxy.lock().unwrap().logs().unwrap()[0].message,
            "old-vault"
        );
        log(&state.vault_dir, "after-rollback");
        assert_eq!(state.proxy.lock().unwrap().logs().unwrap().len(), 2);
        Vault::open(&state.vault_dir, &SecretString::new("master")).unwrap();
    }
}
