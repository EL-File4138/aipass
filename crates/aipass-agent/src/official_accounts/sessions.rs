//! Encrypted Claude checkpoints for completed turns. Raw transcripts are
//! confined to the CLI's own store and the authenticated Rust boundary.
use super::*;
use aipass_agent_protocol::{AgentErrorCode, ClaudeSessionCheckpoint, ClaudeSessionOperation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
const KEY: &str = "claude_session_checkpoints_v1";
const MAX_TRANSCRIPT: usize = 4 * 1024 * 1024;
#[derive(Default, Serialize, Deserialize)]
struct Saved {
    marker: String,
    checkpoints: Vec<ClaudeSessionCheckpoint>,
}

pub(crate) fn handle(
    state: &Arc<AgentState>,
    id: Uuid,
    marker: &str,
    operation: ClaudeSessionOperation,
) -> ServiceResult<Option<ClaudeSessionCheckpoint>> {
    with_vault(state, false, |vault| {
        crate::claude_bridge::validate_account(vault, id, marker)?;
        let expected = vault.fingerprint_secret(marker);
        let mut saved = vault
            .provider_runtime_extension(id, KEY)
            .map_err(map_vault_error)?
            .map(|raw| serde_json::from_str::<Saved>(raw.expose()).map_err(ServiceError::internal))
            .transpose()?
            .unwrap_or_default();
        if saved.marker != expected {
            saved = Saved {
                marker: expected,
                ..Default::default()
            };
        }
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        saved
            .checkpoints
            .retain(|c| c.observed_at <= now && c.observed_at >= now - 86400);
        match operation {
            ClaudeSessionOperation::Load {
                owner,
                context,
                history,
            } => Ok(saved
                .checkpoints
                .into_iter()
                .find(|c| c.owner == owner && c.context == context && c.history == history)),
            ClaudeSessionOperation::Save { checkpoint } => {
                if checkpoint.native_session.is_nil()
                    || checkpoint.project_key.is_empty()
                    || checkpoint.project_key.len() > 1024
                    || !checkpoint
                        .project_key
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                    || !checkpoint.workspace.is_absolute()
                    || checkpoint.transcript.expose().len() > MAX_TRANSCRIPT
                    || checkpoint.call_ids.len() > 4096
                    || checkpoint
                        .call_ids
                        .iter()
                        .any(|id| id.len() > 256 || !id.starts_with("toolu_aipass_claude_"))
                    || checkpoint.observed_at < now - 300
                    || checkpoint.observed_at > now + 30
                {
                    return Err(ServiceError::new(
                        AgentErrorCode::ValidationFailed,
                        "Invalid Claude session checkpoint",
                    ));
                }
                saved.checkpoints.retain(|c| {
                    c.owner != checkpoint.owner
                        || c.context != checkpoint.context
                        || c.history != checkpoint.history
                });
                saved.checkpoints.push(checkpoint);
                saved.checkpoints.sort_by_key(|c| c.observed_at);
                if saved.checkpoints.len() > 8 {
                    saved.checkpoints.drain(..saved.checkpoints.len() - 8);
                }
                vault
                    .set_provider_runtime_extension(
                        id,
                        KEY,
                        Some(&SecretString::new(
                            serde_json::to_string(&saved).map_err(ServiceError::internal)?,
                        )),
                    )
                    .map_err(map_vault_error)?;
                Ok(None)
            }
        }
    })
}

pub(crate) fn owner(state: &Arc<AgentState>, calls: Vec<String>) -> ServiceResult<Option<Uuid>> {
    let invalid = || {
        ServiceError::new(
            AgentErrorCode::ValidationFailed,
            "Claude tool session expired or history mixes accounts",
        )
    };
    if calls.len() > 4096
        || calls
            .iter()
            .any(|s| s.len() > 256 || !s.starts_with("toolu_aipass_claude_"))
    {
        return Err(invalid());
    }
    if calls.is_empty() {
        return Ok(None);
    }
    with_vault(state, false, |vault| {
        let mut owners = std::collections::HashMap::new();
        let config = state
            .proxy
            .lock()
            .map_err(|_| invalid())?
            .load_config(vault)?;
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        for entry in vault.list_provider_summaries().map_err(map_vault_error)? {
            if entry.provider_id.as_deref() != Some("anthropic")
                || entry.deleted_at.is_some()
                || entry.archived_at.is_some()
            {
                continue;
            }
            let Some(raw) = vault
                .provider_runtime_extension(entry.id, KEY)
                .map_err(map_vault_error)?
            else {
                continue;
            };
            let saved: Saved =
                serde_json::from_str(raw.expose()).map_err(ServiceError::internal)?;
            if saved.marker
                != vault
                    .fingerprint_secret(&vault.reveal_secret(entry.id).map_err(map_vault_error)?)
            {
                continue;
            }
            for checkpoint in saved
                .checkpoints
                .into_iter()
                .filter(|c| c.observed_at <= now && c.observed_at >= now - 86400)
            {
                if !config.routes.iter().any(|r| {
                    r.enabled
                        && r.targets.iter().any(|t| {
                            t.enabled
                                && t.id == checkpoint.owner
                                && t.provider_entry_id == entry.id
                                && aipass_provider_registry::primary_secret_ref(&entry.secret_refs)
                                    .is_some_and(|s| s.id == t.secret_id)
                        })
                }) {
                    continue;
                }
                for call in checkpoint
                    .call_ids
                    .into_iter()
                    .filter(|c| calls.contains(c))
                {
                    if owners
                        .insert(call, checkpoint.owner)
                        .is_some_and(|old| old != checkpoint.owner)
                    {
                        return Err(invalid());
                    }
                }
            }
        }
        let mut owner = None;
        for call in calls {
            let target = owners.get(&call).copied().ok_or_else(invalid)?;
            if owner.is_some_and(|old| old != target) {
                return Err(invalid());
            }
            owner = Some(target);
        }
        Ok(owner)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{SessionInfo, SessionState};
    #[test]
    fn checkpoints_are_encrypted_and_bound_to_generation_owner_context_and_history() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::server::tests::sync_test_state(dir.path().join("vault"));
        let password = SecretString::new("checkpoint-test-password");
        let vault = Vault::create(&state.vault_dir, &password).unwrap().vault;
        let id = persist_claude_login(
            &vault,
            &crate::claude_cli::NativeAccount {
                home: dir.path().join("cli-home"),
                identity: "alice::org::a".into(),
                plan: None,
            },
        )
        .unwrap();
        let marker = vault.reveal_secret(id).unwrap();
        let now = time::OffsetDateTime::now_utc();
        *state.session.lock().unwrap() = SessionState::Unlocked(Box::new(SessionInfo {
            id: Uuid::new_v4(),
            vault,
            unlocked_at: now,
            last_activity_at: now,
        }));
        let checkpoint = ClaudeSessionCheckpoint {
            owner: Uuid::new_v4(),
            context: [1; 32],
            history: [2; 32],
            native_session: Uuid::new_v4(),
            project_key: "-private-workspace".into(),
            workspace: dir.path().join("workspace"),
            call_ids: vec![],
            transcript: aipass_agent_protocol::SensitiveString::new(
                "synthetic-private-conversation",
            ),
            observed_at: now.unix_timestamp(),
        };
        handle(
            &state,
            id,
            &marker,
            ClaudeSessionOperation::Save {
                checkpoint: checkpoint.clone(),
            },
        )
        .unwrap();
        let loaded = handle(
            &state,
            id,
            &marker,
            ClaudeSessionOperation::Load {
                owner: checkpoint.owner,
                context: [1; 32],
                history: [2; 32],
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(loaded.transcript.expose(), "synthetic-private-conversation");
        assert!(handle(
            &state,
            id,
            &marker,
            ClaudeSessionOperation::Load {
                owner: Uuid::new_v4(),
                context: [1; 32],
                history: [2; 32]
            }
        )
        .unwrap()
        .is_none());
        assert!(handle(
            &state,
            id,
            &marker,
            ClaudeSessionOperation::Load {
                owner: checkpoint.owner,
                context: [9; 32],
                history: [2; 32]
            }
        )
        .unwrap()
        .is_none());
        assert!(handle(
            &state,
            id,
            "changed-generation",
            ClaudeSessionOperation::Load {
                owner: checkpoint.owner,
                context: [1; 32],
                history: [2; 32]
            }
        )
        .is_err());
        let reopened = Vault::open(&state.vault_dir, &password).unwrap();
        assert!(reopened
            .provider_runtime_extension(id, KEY)
            .unwrap()
            .unwrap()
            .expose()
            .contains("synthetic-private-conversation"));
        fn scan(path: &std::path::Path) {
            for entry in std::fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    scan(&path)
                } else {
                    assert!(!std::fs::read(path)
                        .unwrap()
                        .windows(b"synthetic-private-conversation".len())
                        .any(|b| b == b"synthetic-private-conversation"));
                }
            }
        }
        scan(&state.vault_dir);
    }
}
