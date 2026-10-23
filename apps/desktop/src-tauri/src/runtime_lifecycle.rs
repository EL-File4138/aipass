//! Coordinate desktop-owned agent startup with bundle replacement.
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

pub(crate) static RUNTIME: RuntimeLifecycle = RuntimeLifecycle(RwLock::new(false));

pub(crate) struct RuntimeLifecycle(RwLock<bool>);

impl RuntimeLifecycle {
    pub(crate) fn start(&self) -> Result<RwLockReadGuard<'_, bool>, String> {
        // A status poll must never block the native event loop behind an
        // installer that needs that loop to restart the application.
        let guard = self
            .0
            .try_read()
            .map_err(|_| "AIPass runtime is suspended".to_string())?;
        if *guard {
            return Err("AIPass is quitting".into());
        }
        Ok(guard)
    }

    pub(crate) fn update(&self) -> Result<RwLockWriteGuard<'_, bool>, String> {
        // Finish any startup already in flight before shutting down its agent.
        let guard = self
            .0
            .write()
            .map_err(|_| "AIPass runtime lifecycle lock is poisoned".to_string())?;
        if *guard {
            return Err("AIPass is quitting".into());
        }
        Ok(guard)
    }

    pub(crate) fn quit(&self) -> Result<RwLockWriteGuard<'_, bool>, String> {
        let mut guard = self.update()?;
        *guard = true;
        Ok(guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    #[test]
    fn update_waits_for_existing_startup_and_rejects_watchdog_restarts() {
        let lifecycle = Arc::new(RuntimeLifecycle(RwLock::new(false)));
        let starting = lifecycle.start().unwrap();
        let (entered, entered_rx) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let worker = lifecycle.clone();
        let updating = std::thread::spawn(move || {
            let _guard = worker.update().unwrap();
            entered.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        assert!(entered_rx.recv_timeout(Duration::from_millis(100)).is_err());
        drop(starting);
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(lifecycle.start().is_err());
        release.send(()).unwrap();
        updating.join().unwrap();
        // Failed installs release the guard before restoring normal startup.
        assert!(lifecycle.start().is_ok());
    }

    #[test]
    fn quit_drains_startup_and_permanently_rejects_repairs_and_watchdogs() {
        let lifecycle = Arc::new(RuntimeLifecycle(RwLock::new(false)));
        let starting = lifecycle.start().unwrap();
        let (stopped, stopped_rx) = mpsc::channel();
        let worker = lifecycle.clone();
        let quitting = std::thread::spawn(move || {
            drop(worker.quit().unwrap());
            stopped.send(()).unwrap();
        });
        assert!(stopped_rx.recv_timeout(Duration::from_millis(100)).is_err());
        drop(starting);
        stopped_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        quitting.join().unwrap();
        assert!(lifecycle.start().is_err());
        assert!(lifecycle.start().is_err());
        assert!(lifecycle.update().is_err());
    }
}
