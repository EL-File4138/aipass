//! Native secret clipboard lifetime, independent of WebView focus and timers.
use aipass_agent_protocol::{AgentRequest, SecretValue, SensitiveString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use uuid::Uuid;

trait Clipboard {
    fn read(&mut self) -> Result<SensitiveString, String>;
    fn write(&mut self, value: &str) -> Result<(), String>;
    fn clear(&mut self) -> Result<(), String>;
}

#[derive(Default)]
struct NativeClipboard(Option<arboard::Clipboard>);
impl NativeClipboard {
    fn get(&mut self) -> Result<&mut arboard::Clipboard, String> {
        if self.0.is_none() {
            self.0 = Some(arboard::Clipboard::new().map_err(|_| "Clipboard unavailable.")?);
        }
        self.0
            .as_mut()
            .ok_or_else(|| "Clipboard unavailable.".into())
    }
}
impl Clipboard for NativeClipboard {
    fn read(&mut self) -> Result<SensitiveString, String> {
        self.get()?
            .get_text()
            .map(SensitiveString::new)
            .map_err(|_| "Could not read clipboard.".into())
    }
    fn write(&mut self, value: &str) -> Result<(), String> {
        let clipboard = self.get()?;
        #[cfg(target_os = "macos")]
        let result = {
            use arboard::SetExtApple;
            clipboard.set().exclude_from_history().text(value)
        };
        #[cfg(target_os = "windows")]
        let result = {
            use arboard::SetExtWindows;
            clipboard.set().exclude_from_monitoring().text(value)
        };
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let result = clipboard.set_text(value);
        result.map_err(|_| "Could not copy secret.".into())
    }
    fn clear(&mut self) -> Result<(), String> {
        self.get()?
            .clear()
            .map_err(|_| "Could not clear clipboard.".into())
    }
}

struct Pending {
    value: SensitiveString,
    deadline: Instant,
    retries: u8,
}
#[derive(Default)]
struct ClipboardState<C> {
    clipboard: C,
    pending: Option<Pending>,
    // Lock/exit invalidates a copy still waiting for confirmation or the Agent.
    epoch: u64,
}
impl<C: Clipboard> ClipboardState<C> {
    fn copy(&mut self, epoch: u64, value: SensitiveString, seconds: u16) -> Result<bool, String> {
        if self.epoch != epoch {
            return Ok(false);
        }
        self.clipboard.write(value.expose())?;
        self.pending = Some(Pending {
            value,
            deadline: Instant::now() + Duration::from_secs(u64::from(seconds)),
            retries: 0,
        });
        Ok(true)
    }
    fn clear_matching(&mut self) -> Result<(), String> {
        let Some(pending) = &self.pending else {
            return Ok(());
        };
        // Never clear on a failed read: it could now contain unrelated content.
        let current = self.clipboard.read()?;
        if current == pending.value {
            self.clipboard.clear()?;
        }
        self.pending = None;
        Ok(())
    }
    fn invalidate(&mut self) -> Result<(), String> {
        self.epoch = self.epoch.wrapping_add(1);
        let result = self.clear_matching();
        if result.is_err() {
            if let Some(pending) = &mut self.pending {
                pending.deadline = Instant::now();
            }
        }
        result
    }
}

type Shared = (Mutex<ClipboardState<NativeClipboard>>, Condvar);
fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| (Mutex::new(ClipboardState::default()), Condvar::new()))
}
fn start_worker() {
    static WORKER: OnceLock<()> = OnceLock::new();
    WORKER.get_or_init(|| {
        std::thread::spawn(|| {
            let (mutex, changed) = shared();
            let Ok(mut state) = mutex.lock() else { return; };
            loop {
                let delay = state.pending.as_ref().map(|p| p.deadline.saturating_duration_since(Instant::now()));
                state = match delay {
                    None => match changed.wait(state) { Ok(state) => state, Err(_) => return },
                    Some(delay) if !delay.is_zero() => match changed.wait_timeout(state, delay) {
                        Ok((state, _)) => state, Err(_) => return,
                    },
                    Some(_) => {
                        if state.clear_matching().is_err() {
                            log::warn!("Secret clipboard cleanup failed; clipboard contents were preserved.");
                            if let Some(pending) = &mut state.pending {
                                pending.retries += 1;
                                pending.deadline = Instant::now() + Duration::from_secs(1);
                                // Bound secret retention when the OS clipboard stays unavailable.
                                if pending.retries >= 3 { state.pending = None; }
                            }
                        }
                        state
                    }
                };
            }
        });
    });
}

pub(crate) fn clear_now() -> Result<(), String> {
    let result = shared()
        .0
        .lock()
        .map_err(|_| "Clipboard unavailable.")?
        .invalidate();
    shared().1.notify_one();
    result
}
#[tauri::command]
pub(crate) async fn clipboard_clear_sensitive() -> Result<(), String> {
    crate::run_blocking(clear_now).await
}

static COPYING: AtomicBool = AtomicBool::new(false);
struct CopyGuard;
impl CopyGuard {
    fn acquire() -> Option<Self> {
        COPYING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self)
    }
}
impl Drop for CopyGuard {
    fn drop(&mut self) {
        COPYING.store(false, Ordering::Release);
    }
}

fn prepare_copy(app: &AppHandle) -> Result<Option<(u64, u16)>, String> {
    let epoch = shared()
        .0
        .lock()
        .map_err(|_| "Clipboard unavailable.")?
        .epoch;
    let preferences = crate::load_preferences(app)?;
    if preferences.confirm_secret_copy {
        let language = crate::commands::load_language_settings(app, preferences.locale)?;
        let zh = language.resolved_locale == aipass_agent_protocol::UiLocale::ZhCn;
        let (title, message, copy, cancel) = if zh {
            (
                "复制凭据？",
                "剪贴板内容可能被其他应用读取。",
                "复制",
                "取消",
            )
        } else {
            (
                "Copy credential?",
                "Other apps may read clipboard contents.",
                "Copy",
                "Cancel",
            )
        };
        let mut dialog = app.dialog().message(message).title(title).buttons(
            MessageDialogButtons::OkCancelCustom(copy.into(), cancel.into()),
        );
        if let Some(window) = app.get_webview_window("main") {
            dialog = dialog.parent(&window);
        }
        if !dialog.blocking_show() {
            return Ok(None);
        }
    }
    Ok(Some((
        epoch,
        crate::preferences::normalize_clipboard_seconds(preferences.clipboard_clear_seconds),
    )))
}
fn write(epoch: u64, value: SensitiveString, seconds: u16) -> Result<bool, String> {
    start_worker();
    let result = shared()
        .0
        .lock()
        .map_err(|_| "Clipboard unavailable.")?
        .copy(epoch, value, seconds);
    shared().1.notify_one();
    result
}

#[tauri::command]
pub(crate) async fn secret_copy(app: AppHandle, id: Uuid, field: String) -> Result<bool, String> {
    crate::run_blocking(move || {
        let Some(_guard) = CopyGuard::acquire() else {
            return Ok(false);
        };
        let Some((epoch, seconds)) = prepare_copy(&app)? else {
            return Ok(false);
        };
        let secret: SecretValue =
            crate::agent_request(&app, AgentRequest::SecretRevealField { id, field })?;
        write(epoch, secret.secret, seconds)
    })
    .await
}
#[tauri::command]
pub(crate) async fn clipboard_write_sensitive(
    app: AppHandle,
    value: SensitiveString,
) -> Result<bool, String> {
    crate::run_blocking(move || {
        let Some(_guard) = CopyGuard::acquire() else {
            return Ok(false);
        };
        let Some((epoch, seconds)) = prepare_copy(&app)? else {
            return Ok(false);
        };
        write(epoch, value, seconds)
    })
    .await
}

#[cfg(test)]
mod tests;
