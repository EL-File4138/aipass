use crate::session::{current_policy, lock_session, AgentState};
use aipass_agent_protocol::LockReason;
use std::sync::{atomic::Ordering, Arc};
use std::time::Duration as StdDuration;

/// Seconds since the last system-wide input event (keyboard/mouse), or `None` when
/// the platform exposes no such signal.
pub fn system_idle_seconds() -> Option<f64> {
    use objc2_core_graphics::{CGEventSource, CGEventSourceStateID, CGEventType};
    const ANY_INPUT_EVENT_TYPE: CGEventType = CGEventType(0xFFFF_FFFF);
    let seconds = CGEventSource::seconds_since_last_event_type(
        CGEventSourceStateID::CombinedSessionState,
        ANY_INPUT_EVENT_TYPE,
    );
    (seconds.is_finite() && seconds >= 0.0).then_some(seconds)
}

/// Lock the session when the OS sleeps or the screen locks, honoring the policy
/// flags.
pub fn spawn_power_watcher(state: Arc<AgentState>) {
    use block2::RcBlock;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSDistributedNotificationCenter, NSNotification, NSString};
    use std::ptr::NonNull;

    std::thread::spawn(move || {
        // NSWorkspace/distributed notifications are delivered on the thread's run loop.
        let workspace = NSWorkspace::sharedWorkspace();
        let workspace_center = workspace.notificationCenter();
        let distributed = NSDistributedNotificationCenter::defaultCenter();

        let make_handler = |reason: LockReason| {
            let state = state.clone();
            RcBlock::new(move |_note: NonNull<NSNotification>| {
                let lock = match reason {
                    LockReason::SystemSleep => current_policy(&state)
                        .map(|p| p.lock_on_sleep)
                        .unwrap_or(true),
                    LockReason::ScreenLock => current_policy(&state)
                        .map(|p| p.lock_on_screen_lock)
                        .unwrap_or(true),
                    _ => true,
                };
                if lock {
                    lock_session(&state, reason.clone());
                }
            })
        };

        let sleep_name = unsafe { objc2_app_kit::NSWorkspaceWillSleepNotification };
        let _sleep_obs = unsafe {
            workspace_center.addObserverForName_object_queue_usingBlock(
                Some(sleep_name),
                None,
                None,
                &make_handler(LockReason::SystemSleep),
            )
        };

        let lock_name = NSString::from_str("com.apple.screenIsLocked");
        let _lock_obs = unsafe {
            distributed.addObserverForName_object_queue_usingBlock(
                Some(&lock_name),
                None,
                None,
                &make_handler(LockReason::ScreenLock),
            )
        };

        // Keep the observers alive and pump the run loop for the agent's lifetime.
        loop {
            if state.shutdown.load(Ordering::Relaxed) {
                break;
            }
            objc2_core_foundation::CFRunLoop::run_in_mode(
                unsafe { objc2_core_foundation::kCFRunLoopDefaultMode },
                1.0,
                false,
            );
            // A run loop without a registered source returns immediately. Keep
            // that state from turning this watcher into a busy loop.
            std::thread::sleep(StdDuration::from_millis(50));
        }
        let _ = (_sleep_obs, _lock_obs);
    });
}
