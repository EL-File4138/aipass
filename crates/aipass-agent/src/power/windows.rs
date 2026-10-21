//! Session-local input and system notifications for directly launched Agents.
use crate::session::{current_policy, lock_session, AgentState};
use aipass_agent_protocol::LockReason;
use std::{
    cell::RefCell,
    ptr::null_mut,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::{
        LibraryLoader::GetModuleHandleW,
        RemoteDesktop::{
            WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
            NOTIFY_FOR_THIS_SESSION,
        },
        SystemInformation::GetTickCount,
    },
    UI::{
        Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
        WindowsAndMessaging::*,
    },
};

// Clippy 1.99's Windows GNU fallback-TLS expansion misses the existing const.
thread_local! {
    #[cfg_attr(target_env = "gnu", allow(clippy::missing_const_for_thread_local))]
    static STATE: RefCell<Option<Arc<AgentState>>> = const { RefCell::new(None) };
}

pub fn system_idle_seconds() -> Option<f64> {
    let mut input = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // Both counters are DWORD ticks. Wrapping subtraction handles the 49-day rollover.
    unsafe {
        (GetLastInputInfo(&mut input) != 0)
            .then(|| GetTickCount().wrapping_sub(input.dwTime) as f64 / 1000.0)
    }
}

fn reason(message: u32, event: usize) -> Option<LockReason> {
    match (message, event as u32) {
        (WM_POWERBROADCAST, PBT_APMSUSPEND | PBT_APMRESUMEAUTOMATIC) => {
            Some(LockReason::SystemSleep)
        }
        (
            WM_WTSSESSION_CHANGE,
            WTS_SESSION_LOCK | WTS_SESSION_LOGOFF | WTS_REMOTE_DISCONNECT | WTS_CONSOLE_DISCONNECT,
        ) => Some(LockReason::ScreenLock),
        _ => None,
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    event: WPARAM,
    data: LPARAM,
) -> LRESULT {
    if let Some(reason) = reason(message, event) {
        STATE.with(|slot| {
            if let Some(state) = slot.borrow().as_ref() {
                let allowed = current_policy(state)
                    .map(|policy| match reason {
                        LockReason::SystemSleep => policy.lock_on_sleep,
                        _ => policy.lock_on_screen_lock,
                    })
                    .unwrap_or(true);
                if allowed {
                    lock_session(state, reason);
                }
            }
        });
    }
    DefWindowProcW(hwnd, message, event, data)
}

pub fn spawn_power_watcher(state: Arc<AgentState>) {
    std::thread::spawn(move || unsafe {
        STATE.with(|slot| *slot.borrow_mut() = Some(state.clone()));
        let class = widestring::u16cstr!("AIPassAgentSessionMonitor");
        let instance = GetModuleHandleW(std::ptr::null());
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        RegisterClassW(&window_class);
        // A hidden top-level window receives power broadcasts; HWND_MESSAGE does not.
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            crate::logging::write_component_log(
                crate::logging::AGENT_LOG,
                "ERROR",
                "Windows session monitor could not create its notification window",
            );
            STATE.with(|slot| slot.borrow_mut().take());
            return;
        }
        let mut registered = false;
        let mut retry = std::time::Instant::now();
        while !state.shutdown.load(Ordering::Relaxed) {
            // WTS may not be ready immediately after login. Retry rather than
            // silently losing screen-lock notifications for this process lifetime.
            if !registered && std::time::Instant::now() >= retry {
                registered = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) != 0;
                retry = std::time::Instant::now() + Duration::from_secs(5);
            }
            let mut message = MSG::default();
            while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if registered {
            WTSUnRegisterSessionNotification(hwnd);
        }
        DestroyWindow(hwnd);
        STATE.with(|slot| slot.borrow_mut().take());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_lock_and_sleep_events_lock_the_vault() {
        assert_eq!(
            reason(WM_POWERBROADCAST, PBT_APMSUSPEND as usize),
            Some(LockReason::SystemSleep)
        );
        assert_eq!(
            reason(WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK as usize),
            Some(LockReason::ScreenLock)
        );
        assert_eq!(
            reason(WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK as usize),
            None
        );
        assert_eq!(
            reason(WM_POWERBROADCAST, PBT_APMPOWERSTATUSCHANGE as usize),
            None
        );
    }
    #[test]
    fn native_input_clock_is_available_in_the_test_session() {
        assert!(system_idle_seconds().is_some_and(|seconds| seconds >= 0.0));
    }
}
