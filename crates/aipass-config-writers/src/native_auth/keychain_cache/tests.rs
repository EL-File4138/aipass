use super::*;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
struct Fake {
    version: AtomicU64,
    reads: AtomicUsize,
    gate: Mutex<bool>,
    changed: Condvar,
    deny: bool,
}
impl Fake {
    fn new(deny: bool) -> Arc<Self> {
        Arc::new(Self {
            version: AtomicU64::new(1),
            reads: AtomicUsize::new(0),
            gate: Mutex::new(false),
            changed: Condvar::new(),
            deny,
        })
    }
    fn release(&self) {
        *self.gate.lock().unwrap() = true;
        self.changed.notify_all();
    }
}
impl Source for Fake {
    fn version(&self) -> Result<Option<(u64, u64)>> {
        let v = self.version.load(Ordering::SeqCst);
        Ok((v > 0).then_some((1, v)))
    }
    fn read(&self) -> Result<Secret> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let mut gate = self.gate.lock().unwrap();
        while !*gate {
            gate = self.changed.wait(gate).unwrap();
        }
        if self.deny {
            bail!("denied private-credential-never-echo");
        }
        Ok(Some(Zeroizing::new(
            self.version.load(Ordering::SeqCst).to_string().into_bytes(),
        )))
    }
}

#[test]
fn fresh_read_waiters_keep_their_result_when_later_reads_finish() {
    let cache = Arc::new(Cache::default());
    let source = Fake::new(false);
    source.release();
    let barrier = Arc::new(std::sync::Barrier::new(16));
    let threads: Vec<_> = (0..16)
        .map(|_| {
            let (cache, source, barrier) = (cache.clone(), source.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..64 {
                    let result = cache.read(
                        "Codex Auth",
                        "concurrent-fresh",
                        source.clone(),
                        false,
                        Duration::from_secs(2),
                    );
                    assert_eq!(result.unwrap().unwrap().as_slice(), b"1");
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}

#[test]
fn capacity_cannot_evict_declined_accounts_or_slots_with_live_waiters() {
    let cache = Cache::default();
    let mut active = Vec::new();
    for n in 0..32 {
        let slot = cache.slot("Codex Auth", &n.to_string()).unwrap();
        if n % 2 == 0 {
            slot.state.lock().unwrap().denied = Some(Instant::now() + Duration::from_secs(600));
        } else {
            active.push(slot);
        }
    }
    assert!(cache.slot("Codex Auth", "another").is_err());
    active.clear();
    assert!(cache.slot("Codex Auth", "another").is_ok());
    for n in (0..32).step_by(2) {
        assert!(cache
            .slot("Codex Auth", &n.to_string())
            .unwrap()
            .state
            .lock()
            .unwrap()
            .denied
            .is_some());
    }
}
#[test]
fn concurrent_pending_reads_share_one_prompt_and_success_until_attributes_change() {
    let cache = Arc::new(Cache::default());
    let source = Fake::new(false);
    let read = |cached| {
        cache.read(
            "Codex Auth",
            "test",
            source.clone(),
            cached,
            Duration::from_millis(5),
        )
    };
    assert!(read(true).unwrap_err().to_string().contains("pending"));
    let mut threads = Vec::new();
    for _ in 0..8 {
        let cache = cache.clone();
        let source = source.clone();
        threads.push(std::thread::spawn(move || {
            cache.read("Codex Auth", "test", source, true, Duration::from_secs(2))
        }));
    }
    source.release();
    for thread in threads {
        assert_eq!(thread.join().unwrap().unwrap().unwrap().as_slice(), b"1");
    }
    assert_eq!(source.reads.load(Ordering::SeqCst), 1);
    source.version.store(2, Ordering::SeqCst);
    assert_eq!(read(true).unwrap().unwrap().as_slice(), b"2");
    assert_eq!(source.reads.load(Ordering::SeqCst), 2);
    assert_eq!(read(false).unwrap().unwrap().as_slice(), b"2");
    assert_eq!(
        source.reads.load(Ordering::SeqCst),
        3,
        "fresh backups must bypass completed cached reads"
    );
}
#[test]
fn declined_reads_wait_for_explicit_retry_without_echoing_errors() {
    let cache = Cache::default();
    let source = Fake::new(true);
    source.release();
    for _ in 0..4 {
        let error = cache
            .read(
                "Codex Auth",
                "test",
                source.clone(),
                true,
                Duration::from_secs(2),
            )
            .unwrap_err()
            .to_string();
        assert!(!error.contains("private-credential"));
    }
    assert_eq!(source.reads.load(Ordering::SeqCst), 1);
    cache.invalidate_item("Codex Auth", "test", true);
    assert!(cache
        .read(
            "Codex Auth",
            "test",
            source.clone(),
            true,
            Duration::from_secs(2)
        )
        .is_err());
    assert_eq!(source.reads.load(Ordering::SeqCst), 2);
}
#[test]
fn invalidation_cannot_resurrect_an_unanswered_read_or_create_another_prompt() {
    let cache = Cache::default();
    let source = Fake::new(false);
    assert!(cache
        .read(
            "Codex Auth",
            "test",
            source.clone(),
            true,
            Duration::from_millis(5)
        )
        .is_err());
    cache.clear();
    assert!(cache
        .read(
            "Codex Auth",
            "test",
            source.clone(),
            true,
            Duration::from_millis(5)
        )
        .is_err());
    assert_eq!(source.reads.load(Ordering::SeqCst), 1);
    source.release();
    let slot = cache.slot("Codex Auth", "test").unwrap();
    let state = slot.state.lock().unwrap();
    let (state, _) = slot
        .changed
        .wait_timeout_while(state, Duration::from_secs(2), |s| s.pending.is_some())
        .unwrap();
    assert!(state.value.is_none());
    drop(state);
    assert_eq!(
        cache
            .read(
                "Codex Auth",
                "test",
                source.clone(),
                true,
                Duration::from_secs(2)
            )
            .unwrap()
            .unwrap()
            .as_slice(),
        b"1"
    );
    assert_eq!(source.reads.load(Ordering::SeqCst), 2);
}

#[test]
fn retrying_one_account_and_vault_invalidation_do_not_reopen_another_accounts_prompt() {
    let cache = Cache::default();
    let source = Fake::new(true);
    source.release();
    for account in ["one", "two"] {
        assert!(cache
            .read(
                "Codex Auth",
                account,
                source.clone(),
                true,
                Duration::from_secs(2)
            )
            .is_err());
    }
    assert_eq!(source.reads.load(Ordering::SeqCst), 2);
    cache.clear();
    cache.invalidate_item("Codex Auth", "one", true);
    assert!(cache
        .read(
            "Codex Auth",
            "two",
            source.clone(),
            true,
            Duration::from_secs(2)
        )
        .is_err());
    assert_eq!(source.reads.load(Ordering::SeqCst), 2);
    assert!(cache
        .read(
            "Codex Auth",
            "one",
            source.clone(),
            true,
            Duration::from_secs(2)
        )
        .is_err());
    assert_eq!(source.reads.load(Ordering::SeqCst), 3);
}

#[test]
fn vault_lock_during_an_open_prompt_keeps_a_later_refusal_in_cooldown() {
    let cache = Cache::default();
    let source = Fake::new(true);
    assert!(cache
        .read(
            "Codex Auth",
            "test",
            source.clone(),
            true,
            Duration::from_millis(5)
        )
        .is_err());
    cache.clear();
    source.release();
    let slot = cache.slot("Codex Auth", "test").unwrap();
    let state = slot.state.lock().unwrap();
    let (state, _) = slot
        .changed
        .wait_timeout_while(state, Duration::from_secs(2), |s| s.pending.is_some())
        .unwrap();
    assert!(state.value.is_none());
    assert!(state.denied.is_some());
    drop(state);
    assert!(cache
        .read(
            "Codex Auth",
            "test",
            source.clone(),
            true,
            Duration::from_secs(2)
        )
        .is_err());
    assert_eq!(source.reads.load(Ordering::SeqCst), 1);
}
