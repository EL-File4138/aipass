//! Bound immutable CloudKit history after a fully authenticated sync.
use super::{cache_path, checkpoint_path, Checkpoint};
use crate::cloudkit::Remote;
use aipass_sync::{snapshot_id, SnapshotRemote};
use aipass_vault::VaultSyncSnapshot;
use anyhow::{bail, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use time::{Duration, OffsetDateTime};

// A peer which has been offline longer than this may need manual conflict
// resolution. Keep every authenticated branch head regardless of age.
const RETAIN_DAYS: i64 = 30;
// Each entry is a full encrypted vault, not a delta. A byte budget alone
// retained hundreds of redundant copies of small vaults after migration.
const MAX_HISTORY_SNAPSHOTS: usize = 8;
const MAX_HISTORY_BYTES: u64 = 8 * 1024 * 1024;
const MAX_DELETIONS_PER_SYNC: usize = 32;

struct HistoryNode {
    parents: Vec<String>,
    created_at: OffsetDateTime,
    bytes: u64,
}

pub(super) fn prune(root: &Path, remote: &Remote<'_>) -> Result<usize> {
    let target = format!("cloudkit:{}", remote.account);
    let checkpoint: Checkpoint =
        serde_json::from_slice(&fs::read(checkpoint_path(root, &target))?)?;
    if checkpoint.revision.is_none() {
        return Ok(0);
    }
    let mut snapshots = BTreeMap::new();
    // An eventually consistent listing may omit the acknowledged child.
    // Include it from the authenticated cache so its parent stays protected.
    for id in remote
        .list()?
        .into_iter()
        .chain(checkpoint.heads.iter().cloned())
    {
        if snapshots.contains_key(&id) {
            continue;
        }
        let bytes = fs::read(cache_path(root, &id)?)?;
        if snapshot_id(&bytes) != id {
            bail!("cached CloudKit snapshot hash mismatch");
        }
        let snapshot = VaultSyncSnapshot::parse(&bytes)?;
        snapshots.insert(
            id,
            HistoryNode {
                parents: snapshot.parents,
                created_at: snapshot.created_at,
                bytes: bytes.len() as u64,
            },
        );
    }
    let expired = expired_ids(&snapshots, &checkpoint.heads, OffsetDateTime::now_utc());
    for id in &expired {
        remote.delete(id)?;
        let path = cache_path(root, id)?;
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let metadata = path.with_extension("aipstate");
        if let Err(error) = fs::remove_file(metadata) {
            if error.kind() != std::io::ErrorKind::NotFound {
                return Err(error.into());
            }
        }
    }
    Ok(expired.len())
}

fn expired_ids(
    snapshots: &BTreeMap<String, HistoryNode>,
    checkpoint_heads: &[String],
    now: OffsetDateTime,
) -> Vec<String> {
    let mut heads = snapshots.keys().cloned().collect::<BTreeSet<_>>();
    for snapshot in snapshots.values() {
        for parent in &snapshot.parents {
            heads.remove(parent);
        }
    }
    heads.extend(checkpoint_heads.iter().cloned());
    let cutoff = now - Duration::days(RETAIN_DAYS);
    // A peer on a branch's direct parent can still merge after history is
    // bounded. Older peers may need explicit conflict resolution.
    let current_heads = heads.clone();
    for id in current_heads {
        if let Some(snapshot) = snapshots.get(&id) {
            heads.extend(snapshot.parents.iter().cloned());
        }
    }
    let mut candidates = snapshots
        .iter()
        .filter(|(id, _)| !heads.contains(*id))
        .map(|(id, snapshot)| (snapshot.created_at, id.clone()))
        .collect::<Vec<_>>();
    candidates.sort();
    let mut remaining_bytes = snapshots.values().map(|node| node.bytes).sum::<u64>();
    let mut remaining_count = snapshots.len();
    let mut expired = Vec::new();
    for (created_at, id) in candidates {
        if expired.len() == MAX_DELETIONS_PER_SYNC {
            break;
        }
        if created_at >= cutoff
            && remaining_bytes <= MAX_HISTORY_BYTES
            && remaining_count <= MAX_HISTORY_SNAPSHOTS
        {
            break;
        }
        remaining_bytes = remaining_bytes.saturating_sub(snapshots[&id].bytes);
        remaining_count -= 1;
        expired.push(id);
    }
    expired
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_old_branch_heads_and_current_checkpoint_while_pruning_ancestors() {
        let now = OffsetDateTime::now_utc();
        let old = now - Duration::days(40);
        let mut nodes = BTreeMap::new();
        for (id, parents, created_at) in [
            ("grandparent", vec![], old),
            ("base", vec!["grandparent"], old),
            ("old-head", vec!["base"], old),
            ("recent", vec!["base"], now),
            ("checkpoint", vec!["base"], old),
        ] {
            nodes.insert(
                id.into(),
                HistoryNode {
                    parents: parents.into_iter().map(str::to_owned).collect(),
                    created_at,
                    bytes: 1,
                },
            );
        }
        assert_eq!(
            expired_ids(&nodes, &["checkpoint".into()], now),
            vec!["grandparent"]
        );
    }

    #[test]
    fn caps_recent_history_without_deleting_head_or_direct_parent() {
        let now = OffsetDateTime::now_utc();
        let nodes = BTreeMap::from([
            (
                "old".into(),
                HistoryNode {
                    parents: vec![],
                    created_at: now - Duration::hours(2),
                    bytes: 80 * 1024 * 1024,
                },
            ),
            (
                "base".into(),
                HistoryNode {
                    parents: vec!["old".into()],
                    created_at: now - Duration::hours(1),
                    bytes: 80 * 1024 * 1024,
                },
            ),
            (
                "head".into(),
                HistoryNode {
                    parents: vec!["base".into()],
                    created_at: now,
                    bytes: 1,
                },
            ),
        ]);
        assert_eq!(expired_ids(&nodes, &["head".into()], now), vec!["old"]);
    }

    #[test]
    fn upgraded_small_vault_history_converges_to_eight_full_copies() {
        let now = OffsetDateTime::now_utc();
        let mut nodes = BTreeMap::new();
        // Five old 11 MiB snapshots plus the recent 400 KiB copies reproduce
        // the post-upgrade cache that previously stayed near 128 MiB.
        for i in 0..178 {
            nodes.insert(
                format!("{i:03}"),
                HistoryNode {
                    parents: (i > 0)
                        .then(|| format!("{:03}", i - 1))
                        .into_iter()
                        .collect(),
                    created_at: now - Duration::seconds(178 - i),
                    bytes: if i < 5 { 11 * 1024 * 1024 } else { 400 * 1024 },
                },
            );
        }
        let checkpoint = vec!["177".into()];
        let mut passes = 0;
        loop {
            let expired = expired_ids(&nodes, &checkpoint, now);
            if expired.is_empty() {
                break;
            }
            assert!(expired.len() <= MAX_DELETIONS_PER_SYNC);
            for id in expired {
                nodes.remove(&id);
            }
            assert!(nodes.contains_key("176") && nodes.contains_key("177"));
            passes += 1;
            assert!(passes <= 6);
        }
        assert_eq!(nodes.len(), 8);
        assert_eq!(
            nodes.values().map(|node| node.bytes).sum::<u64>(),
            3200 * 1024
        );
    }

    #[test]
    fn protected_branches_and_checkpoint_parents_can_exceed_the_budget() {
        let now = OffsetDateTime::now_utc();
        let mut nodes = BTreeMap::new();
        for i in 0..12 {
            nodes.insert(
                format!("parent-{i}"),
                HistoryNode {
                    parents: vec![],
                    created_at: now,
                    bytes: 1024 * 1024,
                },
            );
            nodes.insert(
                format!("head-{i}"),
                HistoryNode {
                    parents: vec![format!("parent-{i}")],
                    created_at: now,
                    bytes: 1024 * 1024,
                },
            );
        }
        assert!(expired_ids(&nodes, &["head-0".into()], now).is_empty());
    }
}
