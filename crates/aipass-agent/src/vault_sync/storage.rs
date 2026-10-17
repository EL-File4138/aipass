//! Durable sync staging and bounded local snapshot storage.
mod cache_retention;
pub(super) use cache_retention::prune as prune_cache;

use crate::session::{AgentState, StoredSyncSettings};
use aipass_storage::atomic_write_bytes;
use aipass_sync::{snapshot_id, valid_snapshot_id, SnapshotRemote};
use aipass_vault::Vault;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Checkpoint {
    pub(super) heads: Vec<String>,
    pub(super) revision: Option<String>,
}

pub(super) fn checkpoint_path(root: &Path, target: &str) -> PathBuf {
    root.join("sync-state")
        .join(format!("{}.json", snapshot_id(target.as_bytes())))
}

fn outbox_path(root: &Path, target: &str) -> PathBuf {
    root.join("sync-outbox")
        .join(snapshot_id(target.as_bytes()))
}

pub(crate) fn cache_path(root: &Path, id: &str) -> Result<PathBuf> {
    if !valid_snapshot_id(id) {
        bail!("invalid snapshot id");
    }
    Ok(root.join("sync-cache").join(format!("{id}.aipsnapshot")))
}

fn snapshot_path(outbox: &Path, id: &str) -> Result<PathBuf> {
    if !valid_snapshot_id(id) {
        bail!("invalid snapshot id");
    }
    Ok(outbox.join(format!("{id}.aipsnapshot")))
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

#[derive(Serialize, Deserialize)]
struct Compaction {
    old_head: Vec<String>,
    old_ids: Vec<String>,
    new_id: String,
}

fn finish_compaction(root: &Path, outbox: &Path, journal: &Path, tx: &Compaction) -> Result<()> {
    for id in &tx.old_ids {
        remove_if_exists(&snapshot_path(outbox, id)?)?;
        remove_if_exists(&cache_path(root, id)?)?;
        remove_if_exists(&cache_path(root, id)?.with_extension("aipstate"))?;
    }
    remove_if_exists(journal)
}

fn recover_compaction(root: &Path, outbox: &Path, checkpoint: &Checkpoint) -> Result<()> {
    let journal = outbox.join("compaction.json");
    if !journal.exists() {
        return Ok(());
    }
    let tx: Compaction = serde_json::from_slice(&fs::read(&journal)?)?;
    if !valid_snapshot_id(&tx.new_id) || tx.old_ids.iter().any(|id| !valid_snapshot_id(id)) {
        bail!("invalid sync compaction journal");
    }
    if checkpoint.heads == vec![tx.new_id.clone()] {
        finish_compaction(root, outbox, &journal, &tx)
    } else if checkpoint.heads == tx.old_head {
        remove_if_exists(&snapshot_path(outbox, &tx.new_id)?)?;
        remove_if_exists(&cache_path(root, &tx.new_id)?)?;
        remove_if_exists(&journal)
    } else {
        bail!("sync checkpoint changed during compaction recovery")
    }
}

fn pending_paths(outbox: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let mut pending = BTreeMap::new();
    if !outbox.exists() {
        return Ok(pending);
    }
    for entry in fs::read_dir(outbox)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|name| name.strip_suffix(".aipsnapshot"))
        else {
            continue;
        };
        if valid_snapshot_id(id) && entry.file_type()?.is_file() {
            pending.insert(id.to_string(), entry.path());
        }
    }
    Ok(pending)
}

#[derive(Deserialize)]
struct SnapshotPrefix {
    format: String,
    version: u16,
    parents: Vec<String>,
}

fn parse_prefix(first: &[u8]) -> Result<SnapshotPrefix> {
    let marker = b",\"createdAt\":";
    let end = first
        .windows(marker.len())
        .position(|window| window == marker)
        .context("pending sync snapshot header is too large or invalid")?;
    let mut header = first[..end].to_vec();
    header.push(b'}');
    let prefix: SnapshotPrefix = serde_json::from_slice(&header)?;
    if prefix.format != "aipass-vault-sync"
        || !matches!(prefix.version, 1 | 2)
        || prefix
            .parents
            .iter()
            .any(|parent| !valid_snapshot_id(parent))
    {
        bail!("invalid pending sync snapshot header");
    }
    Ok(prefix)
}

// Snapshot fields are serialized in a fixed order. The discarded middle of an
// old offline queue is needed only for parent links, so do not read its many
// full-vault payloads while holding the unlocked session lock.
fn snapshot_prefix(path: &Path) -> Result<SnapshotPrefix> {
    let mut file = File::open(path)?;
    let mut first = [0u8; 16 * 1024];
    let len = file.read(&mut first)?;
    parse_prefix(&first[..len])
}

fn verify_hash(path: &Path, id: &str) -> Result<()> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let len = file.read(&mut buffer)?;
        if len == 0 {
            break;
        }
        hasher.update(&buffer[..len]);
    }
    if format!("{:x}", hasher.finalize()) != id {
        bail!("pending sync snapshot hash mismatch");
    }
    Ok(())
}

fn pending_base(
    pending: &BTreeMap<String, PathBuf>,
    checkpoint: &Checkpoint,
) -> Result<Vec<String>> {
    if pending.is_empty() {
        return Ok(checkpoint.heads.clone());
    }
    if checkpoint.heads.len() != 1 || !pending.contains_key(&checkpoint.heads[0]) {
        bail!("pending sync snapshots do not match the checkpoint");
    }
    let mut parents = BTreeMap::new();
    for (id, path) in pending {
        parents.insert(id.clone(), snapshot_prefix(path)?.parents);
    }
    let mut seen = BTreeSet::new();
    let mut stack = checkpoint.heads.clone();
    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        stack.extend(
            parents[&id]
                .iter()
                .filter(|parent| pending.contains_key(*parent))
                .cloned(),
        );
    }
    if seen.len() != pending.len() {
        bail!("pending sync snapshots contain a disconnected branch");
    }
    let roots = parents
        .iter()
        .filter(|(_, ancestors)| ancestors.iter().all(|id| !pending.contains_key(id)))
        .collect::<Vec<_>>();
    if roots.len() != 1 {
        bail!("pending sync snapshots do not have one confirmed base");
    }
    let root_id = roots[0].0;
    verify_hash(&pending[root_id], root_id)?;
    let head_id = &checkpoint.heads[0];
    if head_id != root_id {
        verify_hash(&pending[head_id], head_id)?;
    }
    Ok(roots[0].1.clone())
}

fn stage_linked_snapshot(root: &Path, outbox: &Path, id: &str, bytes: &[u8]) -> Result<()> {
    let cache = cache_path(root, id)?;
    atomic_write_bytes(&cache, bytes)?;
    fs::create_dir_all(outbox)?;
    let pending = snapshot_path(outbox, id)?;
    if pending.exists() {
        if verify_hash(&pending, id).is_ok() {
            return Ok(());
        }
        bail!("pending sync snapshot already exists with different content");
    }
    if fs::hard_link(&cache, &pending).is_err() {
        // A vault may live on a filesystem without hard links. Coalescing
        // still bounds the queue to one snapshot there.
        atomic_write_bytes(pending, bytes)?;
    }
    Ok(())
}

// Caller holds the session lock. Only an established target has authenticated
// parents; first synchronization must inspect the remote before publishing.
pub(super) fn queue_local_changes(root: &Path, vault: &Vault, target: &str) -> Result<()> {
    let path = checkpoint_path(root, target);
    if !path.exists() {
        return Ok(());
    }
    vault.ensure_sync_ready()?;
    let checkpoint: Checkpoint = serde_json::from_slice(&fs::read(&path)?)?;
    let outbox = outbox_path(root, target);
    recover_compaction(root, &outbox, &checkpoint)?;
    // A locked bootstrap has not authenticated its checkpoint yet. Conflict
    // resolution also leaves revision unset until its joined branch is built.
    if checkpoint.revision.is_none() {
        return Ok(());
    }
    let pending = pending_paths(&outbox)?;
    let revision = vault.sync_revision()?;
    if checkpoint.revision.as_ref() == Some(&revision)
        && pending.len() <= 1
        && pending
            .values()
            .all(|path| snapshot_prefix(path).is_ok_and(|prefix| prefix.version == 2))
    {
        return Ok(());
    }
    let base = pending_base(&pending, &checkpoint)?;
    let bytes = vault.export_sync_snapshot_with_parents(base)?;
    let id = snapshot_id(&bytes);
    let tx = Compaction {
        old_head: checkpoint.heads,
        old_ids: pending.keys().cloned().collect(),
        new_id: id.clone(),
    };
    fs::create_dir_all(&outbox)?;
    let journal = outbox.join("compaction.json");
    atomic_write_bytes(&journal, &serde_json::to_vec(&tx)?)?;
    stage_linked_snapshot(root, &outbox, &id, &bytes)?;
    atomic_write_bytes(
        &path,
        &serde_json::to_vec(&Checkpoint {
            heads: vec![id],
            revision: Some(revision),
        })?,
    )?;
    finish_compaction(root, &outbox, &journal, &tx)
}

pub(crate) fn queue_configured_changes(
    state: &AgentState,
    vault: &Vault,
    settings: &StoredSyncSettings,
) -> Result<()> {
    let target = if let Some(dir) = crate::sync_watch::folder_sync_dir(settings) {
        format!("folder:{}", dir.display())
    } else if settings.mode == aipass_agent_protocol::SyncMode::WebDav {
        let Some(url) = &settings.webdav_url else {
            return Ok(());
        };
        format!("webdav:{url}")
    } else if settings.mode == aipass_agent_protocol::SyncMode::ICloud {
        let Ok(account) = fs::read_to_string(state.vault_dir.join("sync-state/cloudkit-account"))
        else {
            return Ok(());
        };
        format!("cloudkit:{account}")
    } else {
        return Ok(());
    };
    queue_local_changes(&state.vault_dir, vault, &target)
}

pub(super) struct StagedRemote<'a> {
    pub(super) downloaded: Vec<String>,
    pub(super) root: &'a Path,
    pub(super) outbox: &'a Path,
}

impl SnapshotRemote for StagedRemote<'_> {
    fn list(&self) -> Result<Vec<String>> {
        Ok(self.downloaded.clone())
    }
    fn get(&self, id: &str) -> Result<Vec<u8>> {
        Ok(fs::read(cache_path(self.root, id)?)?)
    }
    fn publish(&self, bytes: &[u8]) -> Result<String> {
        let id = snapshot_id(bytes);
        stage_linked_snapshot(self.root, self.outbox, &id, bytes)?;
        Ok(id)
    }
}

pub(super) fn flush_outbox(outbox: &Path, remote: &impl SnapshotRemote) -> Result<usize> {
    let mut count = 0;
    for (id, path) in pending_paths(outbox)? {
        let bytes = fs::read(&path)?;
        if snapshot_id(&bytes) != id {
            bail!("pending sync snapshot hash mismatch");
        }
        remote.publish(&bytes)?;
        remove_if_exists(&path)?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_compaction_keeps_the_snapshot_named_by_the_checkpoint() {
        for committed in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let outbox = root.join("sync-outbox/test");
            fs::create_dir_all(&outbox).unwrap();
            let old = "a".repeat(64);
            let new = "b".repeat(64);
            let old_path = snapshot_path(&outbox, &old).unwrap();
            let new_path = snapshot_path(&outbox, &new).unwrap();
            atomic_write_bytes(&old_path, b"old").unwrap();
            atomic_write_bytes(&new_path, b"new").unwrap();
            atomic_write_bytes(cache_path(root, &old).unwrap(), b"old").unwrap();
            atomic_write_bytes(cache_path(root, &new).unwrap(), b"new").unwrap();
            let tx = Compaction {
                old_head: vec![old.clone()],
                old_ids: vec![old.clone()],
                new_id: new.clone(),
            };
            atomic_write_bytes(
                outbox.join("compaction.json"),
                &serde_json::to_vec(&tx).unwrap(),
            )
            .unwrap();
            let checkpoint = Checkpoint {
                heads: vec![if committed { new.clone() } else { old.clone() }],
                revision: None,
            };
            recover_compaction(root, &outbox, &checkpoint).unwrap();
            assert_eq!(old_path.exists(), !committed);
            assert_eq!(new_path.exists(), committed);
            assert!(!outbox.join("compaction.json").exists());
        }
    }
}
