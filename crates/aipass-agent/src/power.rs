//! Native power/session signals. The Agent applies the encrypted-vault policy.
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{spawn_power_watcher, system_idle_seconds};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{spawn_power_watcher, system_idle_seconds};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{spawn_power_watcher, system_idle_seconds};

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn spawn_power_watcher(_state: std::sync::Arc<crate::session::AgentState>) {}
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn system_idle_seconds() -> Option<f64> {
    None
}
