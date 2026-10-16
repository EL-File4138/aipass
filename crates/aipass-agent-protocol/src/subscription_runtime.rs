//! Narrow internal subscription runtime observations. These never carry grants
//! or arbitrary vendor response bodies.
use crate::SensitiveString;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudeAllowanceObservation {
    pub scope: String,
    pub used_percent: f64,
    pub resets_at: Option<i64>,
    pub observed_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudeSessionCheckpoint {
    pub owner: Uuid,
    pub context: [u8; 32],
    pub history: [u8; 32],
    pub native_session: Uuid,
    pub project_key: String,
    pub workspace: std::path::PathBuf,
    pub call_ids: Vec<String>,
    pub transcript: SensitiveString,
    pub observed_at: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClaudeSessionOperation {
    Load {
        owner: Uuid,
        context: [u8; 32],
        history: [u8; 32],
    },
    Save {
        checkpoint: ClaudeSessionCheckpoint,
    },
}
