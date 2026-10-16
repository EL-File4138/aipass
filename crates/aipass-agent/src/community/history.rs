//! Encrypted reasoning and compaction belong to their exact account binding.
use super::*;
use sha2::{Digest, Sha256};
pub(super) fn digests(payload: &Value) -> Vec<String> {
    payload["input"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(aipass_proxy_conversion::response_item_ciphertexts)
        .map(|s| format!("{:x}", Sha256::digest(s.as_bytes())))
        .collect()
}

pub(crate) fn owner(
    state: &Arc<AgentState>,
    digests: Vec<String>,
    targets: Vec<Uuid>,
) -> ServiceResult<Option<Uuid>> {
    let invalid = || {
        invalid("Encrypted subscription history is unavailable or mixes accounts; start a new conversation")
    };
    if digests.len() > 4096
        || digests
            .iter()
            .any(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(invalid());
    }
    if digests.is_empty() {
        return Ok(None);
    }
    with_vault(state, false, |vault| {
        let config = state
            .proxy
            .lock()
            .map_err(|_| invalid())?
            .load_config(vault)?;
        let mut owners = HashMap::new();
        let mut has_codex = false;
        let now = time::OffsetDateTime::now_utc().unix_timestamp().max(0) as u64;
        for entry in vault.list_provider_summaries().map_err(map_vault_error)? {
            if entry.provider_id.as_deref() != Some("codex")
                || entry.deleted_at.is_some()
                || entry.archived_at.is_some()
            {
                continue;
            }
            if !config.routes.iter().any(|r| {
                r.enabled
                    && r.targets.iter().any(|t| {
                        t.enabled
                            && t.provider_entry_id == entry.id
                            && (targets.is_empty() || targets.contains(&t.id))
                    })
            }) {
                continue;
            }
            has_codex = true;
            let marker = vault.reveal_secret(entry.id).map_err(map_vault_error)?;
            let account = match read(vault, entry.id, &marker).and_then(|raw| decode(raw.expose()))
            {
                Ok(a) => a,
                Err(_) => continue,
            };
            for history in account
                .feedback
                .history
                .into_iter()
                .filter(|h| h.observed_at <= now && h.observed_at >= now.saturating_sub(86400))
            {
                if !digests.contains(&history.digest)
                    || !config.routes.iter().any(|r| {
                        r.enabled
                            && r.targets.iter().any(|t| {
                                t.id == history.owner
                                    && t.provider_entry_id == entry.id
                                    && t.enabled
                                    && (targets.is_empty() || targets.contains(&t.id))
                                    && aipass_provider_registry::primary_secret_ref(
                                        &entry.secret_refs,
                                    )
                                    .is_some_and(|s| s.id == t.secret_id)
                            })
                    })
                {
                    continue;
                }
                if owners
                    .insert(history.digest, history.owner)
                    .is_some_and(|old| old != history.owner)
                {
                    return Err(invalid());
                }
            }
        }
        let mut owner = None;
        if !has_codex {
            return Ok(None);
        }
        for digest in digests {
            let target = owners.get(&digest).copied().ok_or_else(invalid)?;
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
    fn message_and_tool_content_hashes_ignore_arbitrary_payloads() {
        let payload = json!({"input":[
            {"type":"agent_message","encrypted_content":"first"},
            {"type":"message","content":[{"type":"encrypted_content","data":"second"}]},
            {"type":"function_call_output","output":{"type":"encrypted_content","data":"ordinary result"}},
            {"role":"user","content":[{"type":"input_text","text":"{\"encrypted_content\":\"ordinary text\"}"}]}
        ]});
        assert_eq!(
            digests(&payload),
            ["first", "second"].map(|s| format!("{:x}", Sha256::digest(s.as_bytes())))
        );
    }
    #[test]
    fn encrypted_history_requires_the_original_enabled_account_and_api_only_routes_are_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::server::tests::sync_test_state(dir.path().join("vault"));
        let vault = Vault::create(
            &state.vault_dir,
            &SecretString::new("history-test-password"),
        )
        .unwrap()
        .vault;
        let id = register_cli(
            &vault,
            "codex",
            json!({"type":"oauth","accountId":"alice","access":"synthetic"}),
        )
        .unwrap();
        let digest = digests(
            &json!({"input":[{"type":"compaction","encrypted_content":"private-opaque-value"}]}),
        )[0]
        .clone();
        let target = Uuid::new_v4();
        let marker = vault.reveal_secret(id).unwrap();
        let mut account = decode(read(&vault, id, &marker).unwrap().expose()).unwrap();
        account
            .feedback
            .history
            .push(crate::subscriptions::feedback::HistoryItem {
                digest: digest.clone(),
                owner: target,
                observed_at: time::OffsetDateTime::now_utc().unix_timestamp() as u64,
            });
        account.revision += 1;
        write(
            &vault,
            id,
            &marker,
            0,
            &serde_json::to_string(&account).unwrap(),
        )
        .unwrap();
        let secret = vault.get_provider_summary(id).unwrap().secret_refs[0]
            .id
            .clone();
        let now = time::OffsetDateTime::now_utc();
        *state.session.lock().unwrap() = SessionState::Unlocked(Box::new(SessionInfo {
            id: Uuid::new_v4(),
            vault,
            unlocked_at: now,
            last_activity_at: now,
        }));
        // There is no Codex target on an API-only route; its opaque API history
        // must be left to the existing API transport ownership rules.
        assert!(owner(&state, vec![digest.clone()], vec![Uuid::new_v4()])
            .unwrap()
            .is_none());
        with_vault(&state,false,|vault| {
            let mut service=state.proxy.lock().unwrap();
            let mut config=service.load_config(vault)?;
            config.routes=vec![aipass_proxy::ProxyRouteConfig {id:Uuid::new_v4(),name:"test".into(),token:"fixture-local-token".into(),
                inbound_protocol:aipass_proxy::Protocol::OpenAiResponses,upstream_protocol:aipass_proxy::Protocol::OpenAiResponses,
                conversion_enabled:true,strategy:Default::default(),targets:vec![],retry:Default::default(),enabled:true}];
            config.routes[0].targets=vec![serde_json::from_value(json!({"id":target,"providerEntryId":id,"secretId":secret,
                "label":"codex","baseUrl":"https://community.aipass.invalid/v1","authScheme":"bearer","headers":[],"priority":0,"weight":1,"enabled":true})).unwrap()];
            service.set_config(vault,config)?;Ok(())
        }).unwrap();
        assert_eq!(
            owner(&state, vec![digest.clone()], vec![target]).unwrap(),
            Some(target)
        );
        assert!(owner(&state, vec!["a".repeat(64)], vec![target]).is_err());
        assert!(owner(&state, vec![digest], vec![Uuid::new_v4()])
            .unwrap()
            .is_none());
    }
}
