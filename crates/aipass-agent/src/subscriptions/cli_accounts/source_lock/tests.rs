use super::*;

#[tokio::test]
async fn lock_child() {
    let Some(root) = std::env::var_os("AIPASS_RENEW_LOCK_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let _lock = acquire(&root).await.unwrap();
    std::fs::write(root.join("held"), b"held").unwrap();
    loop {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn process_lock_is_not_stolen_and_recovers_after_crash() {
    let root = tempfile::tempdir().unwrap();
    let separate = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "subscriptions::cli_accounts::source_lock::tests::lock_child",
            "--nocapture",
        ])
        .env("AIPASS_RENEW_LOCK_TEST_ROOT", root.path())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !root.path().join("held").exists() && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let held = root.path().join("held").exists();
    let busy = acquire_with_timeout(root.path(), Duration::from_millis(100)).await;
    let independent = acquire_with_timeout(separate.path(), Duration::from_millis(100)).await;
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(held, "child failed to acquire the lock");
    assert!(busy.is_err());
    assert!(independent.is_ok());
    let lock = acquire_with_timeout(root.path(), Duration::from_secs(1))
        .await
        .unwrap();
    drop(lock);
    assert!(root.path().join(".aipass-grok-renew.lock").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn symlinked_homes_share_the_source_lock() {
    let root = tempfile::tempdir().unwrap();
    let alias = tempfile::tempdir().unwrap();
    let path = alias.path().join("profile");
    std::os::unix::fs::symlink(root.path(), &path).unwrap();
    let _lock = acquire(root.path()).await.unwrap();
    assert!(acquire_with_timeout(&path, Duration::from_millis(50))
        .await
        .is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn file_symlink_cannot_split_the_locked_source_during_cli_replacement() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("target.json");
    std::fs::write(&target, b"{}").unwrap();
    std::os::unix::fs::symlink(target, root.path().join("auth.json")).unwrap();
    assert!(acquire(root.path()).await.unwrap_err().contains("regular"));
}
