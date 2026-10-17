//! Remove reconstructible snapshots no longer present in the active cloud.
use super::{cache_path, Checkpoint};
use aipass_sync::valid_snapshot_id;
use anyhow::{bail, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn checkpoint_heads(root: &Path) -> Result<BTreeSet<String>> {
    let mut heads = BTreeSet::new();
    let directory = root.join("sync-state");
    if !directory.exists() {
        return Ok(heads);
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(stem) = name.to_str().and_then(|name| name.strip_suffix(".json")) else {
            continue;
        };
        if valid_snapshot_id(stem) {
            let checkpoint: Checkpoint = serde_json::from_slice(&fs::read(entry.path())?)?;
            heads.extend(checkpoint.heads);
        } else if stem
            .strip_suffix("-conflicts")
            .is_some_and(valid_snapshot_id)
        {
            heads.extend(serde_json::from_slice::<Vec<String>>(&fs::read(
                entry.path(),
            )?)?);
        }
    }
    if heads.iter().any(|id| !valid_snapshot_id(id)) {
        bail!("invalid cached sync checkpoint");
    }
    Ok(heads)
}

fn pending_uploads(root: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let directory = root.join("sync-outbox");
    let mut pending = BTreeMap::new();
    if directory.exists() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.extend(super::pending_paths(&entry.path())?);
            }
        }
    }
    Ok(pending)
}

/// The caller holds both sync and session locks. Never remove outbox files,
/// checkpoint roots, their direct parents, or snapshots still in the cloud.
pub(crate) fn prune(root: &Path, remote_ids: &BTreeSet<String>) -> Result<usize> {
    let roots = checkpoint_heads(root)?;
    let pending = pending_uploads(root)?;
    let mut retained = remote_ids.clone();
    retained.extend(roots.iter().cloned());
    retained.extend(pending.keys().cloned());
    // Complete the mark phase before any deletion. If local recovery state is
    // corrupt, leave every cached body available for diagnosis/recovery.
    for id in roots {
        let path = cache_path(root, &id)?;
        if path.try_exists()? {
            super::verify_hash(&path, &id)?;
            retained.extend(super::snapshot_prefix(&path)?.parents);
        }
    }
    // Pending histories on inactive targets can be very large. Parent links
    // only add protected IDs here, so read their bounded headers rather than
    // loading or hashing every queued full-vault body under the session lock.
    for path in pending.values() {
        retained.extend(super::snapshot_prefix(path)?.parents);
    }
    let directory = root.join("sync-cache");
    let mut removed = 0;
    if directory.exists() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name();
            let Some((id, extension)) = name.to_str().and_then(|name| name.rsplit_once('.')) else {
                continue;
            };
            if valid_snapshot_id(id)
                && matches!(extension, "aipsnapshot" | "aipstate")
                && !retained.contains(id)
            {
                fs::remove_file(entry.path())?;
                removed += usize::from(extension == "aipsnapshot");
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aipass_crypto::SecretString;
    use aipass_storage::atomic_write_bytes;
    use aipass_sync::snapshot_id;
    use aipass_vault::Vault;

    #[test]
    fn removes_history_deleted_by_a_peer_and_keeps_active_cloud_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let stale = "a".repeat(64);
        let live = "b".repeat(64);
        for id in [&stale, &live] {
            let path = cache_path(root, id).unwrap();
            atomic_write_bytes(&path, b"encrypted cache body").unwrap();
            atomic_write_bytes(path.with_extension("aipstate"), b"metadata").unwrap();
        }
        let unrelated = root.join("sync-cache/notes.txt");
        fs::write(&unrelated, b"leave me").unwrap();
        assert_eq!(prune(root, &BTreeSet::from([live.clone()])).unwrap(), 1);
        assert!(!cache_path(root, &stale).unwrap().exists());
        assert!(!cache_path(root, &stale)
            .unwrap()
            .with_extension("aipstate")
            .exists());
        assert!(cache_path(root, &live).unwrap().exists());
        assert!(unrelated.exists());
    }

    #[test]
    fn keeps_other_targets_checkpoints_parents_and_unsent_uploads() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let vault = Vault::create(root, &SecretString::new("fixture-password"))
            .unwrap()
            .vault;
        let cache = |parents| {
            let bytes = vault.export_sync_snapshot_with_parents(parents).unwrap();
            let id = snapshot_id(&bytes);
            atomic_write_bytes(cache_path(root, &id).unwrap(), &bytes).unwrap();
            (id, bytes)
        };
        let (base, _) = cache(vec![]);
        let (head, _) = cache(vec![base.clone()]);
        let (pending_base, _) = cache(vec![]);
        let (pending, bytes) = cache(vec![pending_base.clone()]);
        let (orphan, _) = cache(vec![]);
        atomic_write_bytes(
            super::super::checkpoint_path(root, "inactive-target"),
            &serde_json::to_vec(&Checkpoint {
                heads: vec![head.clone()],
                revision: Some("verified".into()),
            })
            .unwrap(),
        )
        .unwrap();
        let outbox = root
            .join("sync-outbox/another-target")
            .join(format!("{pending}.aipsnapshot"));
        atomic_write_bytes(&outbox, &bytes).unwrap();
        assert_eq!(prune(root, &BTreeSet::new()).unwrap(), 1);
        for id in [base, head, pending_base, pending] {
            assert!(cache_path(root, &id).unwrap().exists());
        }
        assert!(outbox.exists());
        assert!(!cache_path(root, &orphan).unwrap().exists());
    }

    #[test]
    fn unreadable_checkpoint_aborts_before_removing_any_body() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let cached = cache_path(root, &"a".repeat(64)).unwrap();
        atomic_write_bytes(&cached, b"preserve for recovery").unwrap();
        atomic_write_bytes(super::super::checkpoint_path(root, "target"), b"invalid").unwrap();
        assert!(prune(root, &BTreeSet::new()).is_err());
        assert!(cached.exists());
    }
}
