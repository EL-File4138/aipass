//! Official CLIs own grants. This facade exposes only local references and operations.
use super::*;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

mod backend;
mod copilot;
mod discovery;
mod gemini;
mod grok;
mod quota;
mod reset;
mod session;
mod source_lock;
mod store;

#[cfg(test)]
use backend::copilot_user_request;
pub(super) use backend::{generate, models, usage};
pub(super) use discovery::executable;
pub(crate) use discovery::status;
pub(super) use grok::recover as recover_grok;
use quota::{codex_usage, copilot_usage};
pub(super) use reset::consume_reset;
use session::{command, rpc};
pub(super) use session::{force_refresh, fresh, login, verify};
#[cfg(test)]
use store::config_home;
pub(crate) use store::{
    check_device, default_home, device, handoff_home, import_reference, new_home, reference,
};
use store::{read, root};

#[cfg(all(test, target_os = "linux"))]
mod linux_tests;
#[cfg(test)]
mod tests;
