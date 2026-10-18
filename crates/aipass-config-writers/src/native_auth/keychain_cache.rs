//! Coalesce interactive Keychain reads. No credential reaches UI or disk here.
use super::*;
use std::{
    collections::HashMap,
    sync::{Arc, Condvar, Mutex, OnceLock},
    time::{Duration, Instant},
};
#[cfg(target_os = "macos")]
mod macos;

type Secret = Option<Zeroizing<Vec<u8>>>;
type Version = Option<(u64, u64)>;
type Snapshot = (Version, Secret, Instant);
type Slots = Arc<Mutex<HashMap<(String, String), Arc<Slot>>>>;
trait Source: Send + Sync + 'static {
    /// Attributes only; fail rather than opening any authentication UI.
    fn version(&self) -> Result<Option<(u64, u64)>>;
    fn read(&self) -> Result<Secret>;
}
#[derive(Default)]
struct State {
    epoch: u64,
    refusal_epoch: u64,
    pending: Option<Arc<Read>>,
    value: Option<Snapshot>,
    denied: Option<Instant>,
}
#[derive(Default)]
struct Read {
    // A later fresh read must not overwrite the outcome awaited by this one's
    // callers. Only those callers retain this result after the worker finishes.
    result: Mutex<Option<std::result::Result<Secret, ()>>>,
}
#[derive(Default)]
struct Slot {
    state: Mutex<State>,
    changed: Condvar,
}
#[derive(Default)]
struct Cache {
    slots: Slots,
}
impl Cache {
    fn slot(&self, service: &str, account: &str) -> Result<Arc<Slot>> {
        let mut slots = self
            .slots
            .lock()
            .map_err(|_| anyhow::anyhow!("credential reader unavailable"))?;
        let key = (service.to_owned(), account.to_owned());
        if let Some(slot) = slots.get(&key) {
            return Ok(slot.clone());
        }
        if slots.len() >= 32 {
            let old = slots
                .iter()
                .find(|(_, v)| {
                    Arc::strong_count(v) == 1
                        && v.state.lock().is_ok_and(|s| {
                            s.pending.is_none()
                                && s.denied.is_none_or(|until| Instant::now() >= until)
                        })
                })
                .map(|(k, _)| k.clone());
            if let Some(old) = old {
                slots.remove(&old);
            } else {
                bail!("credential authorization capacity reached");
            }
        }
        let slot = Arc::new(Slot::default());
        slots.insert(key, slot.clone());
        Ok(slot)
    }
    fn clear(&self) {
        if let Ok(slots) = self.slots.lock() {
            for slot in slots.values() {
                if let Ok(mut state) = slot.state.lock() {
                    state.epoch = state.epoch.wrapping_add(1);
                    state.value = None;
                    slot.changed.notify_all();
                }
            }
        }
    }
    fn invalidate_item(&self, service: &str, account: &str, retry: bool) {
        if let Ok(slots) = self.slots.lock() {
            if let Some(slot) = slots.get(&(service.to_owned(), account.to_owned())) {
                if let Ok(mut state) = slot.state.lock() {
                    state.epoch = state.epoch.wrapping_add(1);
                    state.value = None;
                    if retry {
                        state.refusal_epoch = state.refusal_epoch.wrapping_add(1);
                        state.denied = None;
                    }
                    slot.changed.notify_all();
                }
            }
        }
    }
    fn read(
        &self,
        service: &str,
        account: &str,
        source: Arc<dyn Source>,
        cached: bool,
        wait: Duration,
    ) -> Result<Secret> {
        let version = source.version()?;
        let slot = self.slot(service, account)?;
        let mut state = slot
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("credential reader unavailable"))?;
        // The attributes query distinguishes absence from an ACL/auth error.
        // No secret read is needed for an absent item; waiting for one creates
        // a spurious revocation race during unrelated vault locks.
        if version.is_none() {
            state.value = None;
            return Ok(None);
        }
        if cached {
            if let Some((old, value, when)) = &state.value {
                if *old == version && when.elapsed() < Duration::from_secs(10) {
                    return Ok(value.clone());
                }
            }
        }
        // Do not retain a secret after the native item changed or disappeared.
        if state
            .value
            .as_ref()
            .is_some_and(|(old, _, _)| *old != version)
        {
            state.value = None;
        }
        if state.denied.is_some_and(|until| Instant::now() < until) {
            bail!("system credential authorization was declined; reconnect to retry");
        }
        let epoch = state.epoch;
        let refusal_epoch = state.refusal_epoch;
        let read = state.pending.clone().unwrap_or_default();
        if state.pending.is_none() {
            state.pending = Some(read.clone());
            let target = slot.clone();
            let completion = read.clone();
            let worker = std::thread::Builder::new()
                .name("aipass-keychain-read".into())
                .spawn(move || {
                    let mut refused = false;
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        // Recheck after the read: rotation during an OS prompt must
                        // never label stale bytes with the new item's attributes.
                        let before = source.version()?;
                        let value = source.read().inspect_err(|_| refused = true)?;
                        if source.version()? != before || before.is_none() && value.is_some() {
                            bail!("native credential changed while authorizing; retry");
                        }
                        Ok((before, value, Instant::now()))
                    }))
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("credential reader unavailable")));
                    if let Ok(mut state) = target.state.lock() {
                        state.pending = None;
                        let mut outcome = Err(());
                        if state.epoch == epoch {
                            match result {
                                Ok(value) => {
                                    outcome = Ok(value.1.clone());
                                    state.value = Some(value);
                                    state.denied = None;
                                }
                                Err(_) => {
                                    state.value = None;
                                    state.denied =
                                        refused.then(|| Instant::now() + Duration::from_secs(600));
                                }
                            }
                        }
                        // Lock can invalidate secret bytes while the original
                        // OS prompt is still open. Keep its refusal unless the
                        // user explicitly retried/replaced this exact item.
                        if refused && state.refusal_epoch == refusal_epoch {
                            state.denied = Some(Instant::now() + Duration::from_secs(600));
                        }
                        if let Ok(mut result) = completion.result.lock() {
                            *result = Some(outcome);
                        }
                        target.changed.notify_all();
                    }
                });
            if worker.is_err() {
                state.pending = None;
                bail!("credential reader unavailable");
            }
        }
        let deadline = Instant::now() + wait;
        loop {
            if state.epoch != epoch {
                bail!("credential read invalidated; retry");
            }
            if let Some(result) = read
                .result
                .lock()
                .map_err(|_| anyhow::anyhow!("credential reader unavailable"))?
                .as_ref()
            {
                return result.clone().map_err(|_| {
                    anyhow::anyhow!(
                        "system credential authorization unavailable; reconnect to retry"
                    )
                });
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                bail!("system credential authorization pending; approve it and retry");
            }
            state = slot
                .changed
                .wait_timeout(state, left)
                .map_err(|_| anyhow::anyhow!("credential reader unavailable"))?
                .0;
        }
    }
}
fn cache() -> Result<&'static Cache> {
    static CACHE: OnceLock<std::result::Result<Cache, String>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let cache = Cache::default();
            let slots = Arc::downgrade(&cache.slots);
            // One janitor, independent of request traffic. Cached secret buffers
            // are erased within twenty seconds even if no further reads arrive.
            std::thread::Builder::new()
                .name("aipass-keychain-expiry".into())
                .spawn(move || loop {
                    std::thread::sleep(Duration::from_secs(10));
                    let Some(slots) = slots.upgrade() else {
                        break;
                    };
                    if let Ok(slots) = slots.lock() {
                        for slot in slots.values() {
                            if let Ok(mut state) = slot.state.lock() {
                                if state.value.as_ref().is_some_and(|(_, _, when)| {
                                    when.elapsed() >= Duration::from_secs(10)
                                }) {
                                    state.value = None;
                                }
                            }
                        }
                    };
                })
                .map_err(|_| "credential cache expiry unavailable".to_string())?;
            Ok(cache)
        })
        .as_ref()
        .map_err(|_| anyhow::anyhow!("credential cache unavailable"))
}
/// Erase positive snapshots/epochs without reopening declined authorization.
/// An unanswered OS prompt stays coalesced until its worker finishes.
pub fn invalidate_reads() {
    if let Ok(cache) = cache() {
        cache.clear();
    }
}

#[cfg(target_os = "macos")]
pub(super) fn invalidate_item(service: &str, account: &str, retry: bool) {
    if let Ok(cache) = cache() {
        cache.invalidate_item(service, account, retry);
    }
}

#[cfg(target_os = "macos")]
pub(super) fn exists(service: &str, account: &str) -> Result<bool> {
    macos::Keychain {
        service: service.into(),
        account: account.into(),
    }
    .version()
    .map(|v| v.is_some())
}
#[cfg(target_os = "macos")]
pub(super) fn read(service: &str, account: &str, cached: bool) -> Result<Secret> {
    cache()?.read(
        service,
        account,
        Arc::new(macos::Keychain {
            service: service.into(),
            account: account.into(),
        }),
        cached,
        Duration::from_secs(5),
    )
}
#[cfg(test)]
mod tests;
