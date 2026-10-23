//! Cooperating AIPass processes serialize renewal at the vendor credential source.
//! Never delete this file: unlinking a locked inode permits a second lock owner.
use super::*;
use std::fs::{File, OpenOptions, TryLockError};

pub(super) async fn acquire(root: &Path) -> Result<File> {
    acquire_with_timeout(root, Duration::from_secs(35)).await
}

async fn acquire_with_timeout(root: &Path, timeout: Duration) -> Result<File> {
    let source = root.join("auth.json");
    if std::fs::symlink_metadata(&source).is_ok_and(|m| !m.is_file()) {
        // CLI atomic replacement can sever a file symlink and split the source
        // after locking. Directory aliases remain supported by canonicalization.
        return Err("Grok credential file must be regular; reconnect this account".into());
    }
    let dir = if source.exists() {
        std::fs::canonicalize(source)
            .map_err(|_| "CLI credential source unavailable")?
            .parent()
            .ok_or("invalid CLI credential source")?
            .to_owned()
    } else {
        std::fs::canonicalize(root).map_err(|_| "CLI credential source unavailable")?
    };
    let path = dir.join(".aipass-grok-renew.lock");
    if std::fs::symlink_metadata(&path).is_ok_and(|m| !m.is_file()) {
        return Err("invalid CLI renewal lock".into());
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(&path)
        .map_err(|_| "CLI renewal lock unavailable")?;
    let opened = file
        .metadata()
        .map_err(|_| "CLI renewal lock unavailable")?;
    let named = std::fs::symlink_metadata(&path).map_err(|_| "CLI renewal lock unavailable")?;
    if !opened.is_file() || !named.is_file() {
        return Err("invalid CLI renewal lock".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if opened.dev() != named.dev() || opened.ino() != named.ino() {
            return Err("CLI renewal lock changed".into());
        }
    }
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::WouldBlock) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(TryLockError::WouldBlock) => return Err("CLI renewal is busy; retry later".into()),
            Err(TryLockError::Error(_)) => return Err("CLI renewal lock unavailable".into()),
        }
    }
}

#[cfg(test)]
mod tests;
