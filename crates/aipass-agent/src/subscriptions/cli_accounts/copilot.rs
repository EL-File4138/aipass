//! Shared pure Copilot contracts; local grant reads remain in this crate.
pub(super) use aipass_proxy::{
    copilot_api as api, copilot_host as host, copilot_model_wire as wire,
    copilot_user_url as user_url,
};
