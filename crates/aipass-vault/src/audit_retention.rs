//! Local audit events are useful for recovery but are not remote sync data.
use super::*;
use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};

const MAX_AUDIT_ALLOCATED_BYTES: u64 = 64 * 1024 * 1024;
const RETAIN_FOR: StdDuration = StdDuration::from_secs(30 * 24 * 60 * 60);
const CHECK_EVERY: StdDuration = StdDuration::from_secs(60 * 60);

impl Vault {
    pub(super) fn audit(
        &self,
        action: &str,
        record_id: Option<Uuid>,
        detail: Option<&str>,
    ) -> Result<(), VaultError> {
        let event = AuditEvent {
            id: Uuid::new_v4(),
            at: OffsetDateTime::now_utc(),
            action: action.to_string(),
            record_id,
            detail: detail.map(redact_detail),
        };
        self.write_envelope(
            self.root
                .join("audit")
                .join(format!("{}.aipaudit", event.id)),
            event.id,
            "audit_event",
            1,
            &serde_json::to_vec(&event)?,
        )?;
        // Retention is maintenance, never a reason to fail an already
        // committed vault operation. A failed pass retries next time.
        let _ = maintain(&self.root);
        Ok(())
    }
}

pub(super) fn maintain(root: &Path) -> std::io::Result<()> {
    let now = SystemTime::now();
    let now_secs = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let marker = root.join("audit-retention.state");
    if let Ok(previous) = fs::read_to_string(&marker).and_then(|s| {
        s.trim()
            .parse::<u64>()
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidData))
    }) {
        if previous <= now_secs && now_secs - previous < CHECK_EVERY.as_secs() {
            return Ok(());
        }
    }
    let dir = root.join("audit");
    if !dir.exists() {
        return Ok(());
    }
    let mut files = Vec::new();
    let mut total = 0u64;
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().extension().is_none_or(|ext| ext != "aipaudit")
            || !entry.file_type()?.is_file()
        {
            continue;
        }
        let metadata = entry.metadata()?;
        let modified = metadata.modified()?;
        let allocated = allocated_bytes(&metadata);
        total = total.saturating_add(allocated);
        files.push((modified, entry.path(), allocated));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let cutoff = now.checked_sub(RETAIN_FOR).unwrap_or(UNIX_EPOCH);
    for (modified, path, len) in files {
        if modified >= cutoff && total <= MAX_AUDIT_ALLOCATED_BYTES {
            break;
        }
        fs::remove_file(path)?;
        total = total.saturating_sub(len);
    }
    atomic_write_bytes(marker, now_secs.to_string().as_bytes())
}

#[cfg(unix)]
fn allocated_bytes(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn allocated_bytes(metadata: &fs::Metadata) -> u64 {
    metadata.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prunes_old_audit_files_without_touching_other_vault_data() {
        let dir = tempfile::tempdir().unwrap();
        let audit = dir.path().join("audit");
        fs::create_dir(&audit).unwrap();
        let old = audit.join("old.aipaudit");
        let fresh = audit.join("fresh.aipaudit");
        fs::write(&old, b"old").unwrap();
        fs::write(&fresh, b"fresh").unwrap();
        let old_time = SystemTime::now() - RETAIN_FOR - StdDuration::from_secs(10);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(old_time)
            .unwrap();
        fs::write(dir.path().join("manifest.aipmanifest"), b"leave me").unwrap();
        maintain(dir.path()).unwrap();
        assert!(!old.exists());
        assert!(fresh.exists());
        assert!(dir.path().join("manifest.aipmanifest").exists());
    }
}
