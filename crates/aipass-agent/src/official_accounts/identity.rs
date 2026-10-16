//! Upgrade email-only Claude records from their pinned local CLI profile.
use super::*;
use aipass_config_writers::native_auth::{claude_identity, parse, Resource};

pub(super) fn upgrade_entry(
    vault: &Vault,
    entry: &aipass_vault::EntrySummary,
    target: Option<&str>,
) -> anyhow::Result<bool> {
    if entry.provider_id.as_deref() != Some("anthropic")
        || entry.provider_kind != ProviderKind::Official
        || entry.credential_kind != CredentialKind::OAuth
        || entry.archived_at.is_some()
        || entry.deleted_at.is_some()
    {
        return Ok(false);
    }
    let Some(mut reference) =
        claude_reference(vault, entry.id).map_err(|e| anyhow::anyhow!(e.message))?
    else {
        return Ok(false);
    };
    // A synced path must never be interpreted as this machine's account.
    if cli_accounts::check_device(&reference).is_err() {
        return Ok(false);
    }
    let expected = reference["accountId"].as_str().unwrap_or("");
    if entry.account_identity.as_deref() != Some(expected) || expected.contains("::org::") {
        return Ok(false);
    }
    let path = PathBuf::from(reference["nativeHome"].as_str().unwrap_or(""));
    if !path.is_absolute() {
        return Ok(false);
    }
    let profile = claude_identity::profile_path(&path, &home().join(".claude"));
    let Some(bytes) = Resource::File(profile).read()? else {
        return Ok(false);
    };
    let profile = parse(&bytes)?;
    let scoped = claude_identity::identity(&Value::Null, &profile.0)?;
    if target.is_some_and(|target| target != scoped)
        || !claude_identity::is_legacy_of(expected, &scoped)
    {
        return Ok(false);
    }
    reference["accountId"] = json!(scoped);
    vault.bind_cli_subscription(
        entry.id,
        "anthropic",
        &scoped,
        &format!("aipass:claude-cli:{}", Uuid::new_v4()),
        "claude_cli_home",
        &SecretString::new(reference.to_string()),
    )?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_identity_upgrade_notifies_readers_even_with_quota_tracking_disabled() {
        use crate::session::{SessionInfo, SessionState};
        use std::sync::atomic::Ordering;
        let dir = tempfile::tempdir().unwrap();
        let state = crate::server::tests::sync_test_state(dir.path().join("vault"));
        let vault = Vault::create(&state.vault_dir, &SecretString::new("test password"))
            .unwrap()
            .vault;
        let path = dir.path().join("claude");
        std::fs::create_dir(&path).unwrap();
        let id = persist_claude_login(
            &vault,
            &crate::claude_cli::NativeAccount {
                home: path.clone(),
                identity: "alice@example.test".into(),
                plan: None,
            },
        )
        .unwrap();
        let options = aipass_agent_protocol::ProviderRuntimeOptions {
            quota_tracking: false,
            ..Default::default()
        };
        crate::provider_runtime::save(&vault, id, options).unwrap();
        std::fs::write(
            path.join(".claude.json"),
            json!({"oauthAccount":{"emailAddress":"alice@example.test","organizationUuid":"a"}})
                .to_string(),
        )
        .unwrap();
        let now = time::OffsetDateTime::now_utc();
        *state.session.lock().unwrap() = SessionState::Unlocked(Box::new(SessionInfo {
            id: Uuid::new_v4(),
            vault,
            unlocked_at: now,
            last_activity_at: now,
        }));
        refresh_registered_accounts(&state).unwrap();
        assert_eq!(state.sync_revision.load(Ordering::Relaxed), 1);
        with_vault(&state, false, |vault| {
            assert_eq!(
                vault
                    .get_provider_summary(id)
                    .unwrap()
                    .account_identity
                    .as_deref(),
                Some("alice@example.test::org::a")
            );
            Ok(())
        })
        .unwrap();
        refresh_registered_accounts(&state).unwrap();
        assert_eq!(state.sync_revision.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn logging_into_one_organization_does_not_upgrade_a_sibling_binding() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::create(
            dir.path().join("vault"),
            &SecretString::new("test password"),
        )
        .unwrap()
        .vault;
        let b = dir.path().join("b");
        std::fs::create_dir(&b).unwrap();
        let id = persist_claude_login(
            &vault,
            &crate::claude_cli::NativeAccount {
                home: b.clone(),
                identity: "alice@example.test".into(),
                plan: None,
            },
        )
        .unwrap();
        std::fs::write(
            b.join(".claude.json"),
            json!({"oauthAccount":{"emailAddress":"alice@example.test","organizationUuid":"b"}})
                .to_string(),
        )
        .unwrap();
        let before = vault.get_provider_summary(id).unwrap();
        let reference = claude_reference(&vault, id).unwrap();
        persist_claude_login(
            &vault,
            &crate::claude_cli::NativeAccount {
                home: dir.path().join("a"),
                identity: "alice@example.test::org::a".into(),
                plan: None,
            },
        )
        .unwrap();
        assert_eq!(
            vault.get_provider_summary(id).unwrap().updated_at,
            before.updated_at
        );
        assert_eq!(claude_reference(&vault, id).unwrap(), reference);
    }

    #[test]
    fn same_email_in_two_organizations_keeps_separate_entries_and_reconnects_only_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::create(
            dir.path().join("vault"),
            &SecretString::new("test password"),
        )
        .unwrap()
        .vault;
        let account = |org: &str| crate::claude_cli::NativeAccount {
            home: dir.path().join(org),
            identity: format!("alice@example.test::org::{org}"),
            plan: Some("team".into()),
        };
        let a = persist_claude_login(&vault, &account("a")).unwrap();
        let b = persist_claude_login(&vault, &account("b")).unwrap();
        assert_ne!(a, b);
        let a_before = vault.get_provider_summary(a).unwrap();
        let a_ref = claude_reference(&vault, a).unwrap();
        let b_before = vault.get_provider_summary(b).unwrap();
        let mut reconnect = account("b");
        reconnect.home = dir.path().join("new-b");
        assert_eq!(persist_claude_login(&vault, &reconnect).unwrap(), b);
        assert_eq!(vault.list_provider_summaries().unwrap().len(), 2);
        assert_eq!(
            vault.get_provider_summary(a).unwrap().updated_at,
            a_before.updated_at
        );
        assert_eq!(claude_reference(&vault, a).unwrap(), a_ref);
        assert_eq!(
            vault.get_provider_summary(b).unwrap().secret_refs[0].id,
            b_before.secret_refs[0].id
        );
        assert_eq!(
            claude_reference(&vault, b).unwrap().unwrap()["nativeHome"],
            json!(reconnect.home)
        );
    }

    #[test]
    fn legacy_upgrade_preserves_ids_and_never_adopts_another_email_or_device() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::create(
            dir.path().join("vault"),
            &SecretString::new("test password"),
        )
        .unwrap()
        .vault;
        let path = dir.path().join("account");
        std::fs::create_dir(&path).unwrap();
        let native = crate::claude_cli::NativeAccount {
            home: path.clone(),
            identity: "alice@example.test".into(),
            plan: None,
        };
        let id = persist_claude_login(&vault, &native).unwrap();
        let before = vault.get_provider_summary(id).unwrap();
        let write_profile = |email| {
            std::fs::write(
                path.join(".claude.json"),
                json!({"oauthAccount":{"emailAddress":email,"organizationUuid":"a"}}).to_string(),
            )
            .unwrap()
        };
        write_profile("bob@example.test");
        assert!(!upgrade_entry(&vault, &before, None).unwrap());
        assert_eq!(
            vault.get_provider_summary(id).unwrap().account_identity,
            before.account_identity
        );
        write_profile("alice@example.test");
        let mut foreign = claude_reference(&vault, id).unwrap().unwrap();
        foreign["nativeDevice"] = json!("another-device");
        vault
            .set_provider_runtime_extension(
                id,
                "claude_cli_home",
                Some(&SecretString::new(foreign.to_string())),
            )
            .unwrap();
        assert!(!upgrade_entry(&vault, &before, None).unwrap());
        foreign["nativeDevice"] = json!(cli_accounts::device().unwrap());
        vault
            .set_provider_runtime_extension(
                id,
                "claude_cli_home",
                Some(&SecretString::new(foreign.to_string())),
            )
            .unwrap();
        assert!(upgrade_entry(&vault, &before, None).unwrap());
        let after = vault.get_provider_summary(id).unwrap();
        assert_eq!(
            after.account_identity.as_deref(),
            Some("alice@example.test::org::a")
        );
        assert_eq!(after.secret_refs[0].id, before.secret_refs[0].id);
        assert!(!upgrade_entry(&vault, &after, None).unwrap());
        assert_eq!(
            claude_reference(&vault, id).unwrap().unwrap()["accountId"],
            json!("alice@example.test::org::a")
        );
    }
}
