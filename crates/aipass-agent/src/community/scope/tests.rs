use super::*;

fn setup(vault: &Vault) -> (Uuid, String, Account, String) {
    let auth = json!({"type":"api","key":"synthetic-devin-key","accountId":"user-a","metadata":{"server":"https://server.codeium.com"}});
    let old = Account {
        provider: "devin".into(),
        generation: Uuid::new_v4(),
        local_binding: None,
        auth: SensitiveString::new(auth.to_string()),
        models: json!({"old-model":{}}),
        revision: 3,
        native_method: Some(1),
        identity: "user-a".into(),
        feedback: Default::default(),
    };
    let id = persist(vault, old.clone()).unwrap();
    // Recreate the pre-upgrade runtime extension, without using a new-code login.
    let raw = serde_json::to_string(&old).unwrap();
    vault
        .set_provider_runtime_extension(id, KEY, Some(&SecretString::new(&raw)))
        .unwrap();
    vault
        .set_provider_runtime_extension(id, KEY_V2, None)
        .unwrap();
    let marker = vault.reveal_secret(id).unwrap();
    (id, marker, old, raw)
}
fn resolved(old: &Account, org: &str) -> Value {
    crate::subscriptions::devin::identity::resolved(&serde_json::from_str(old.auth.expose()).unwrap(),
        &json!({"userStatus":{"userId":"user-a","email":"alice@example.test","teamId":format!("team-{org}"),"planStatus":{"planInfo":{"devinInfo":{"orgId":org}}}}})).unwrap()
}

#[test]
fn migration_is_atomic_preserves_ids_and_rejects_stale_writes() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::create(root.path(), &SecretString::new("fixture-password"))
        .unwrap()
        .vault;
    let (id, marker, old, raw) = setup(&vault);
    let before = vault.get_provider_summary(id).unwrap();
    vault
        .update_provider_subscription(id, Some(SubscriptionSnapshot::default()))
        .unwrap();
    let next = migrated(&old, &resolved(&old, "a")).unwrap();
    let bundle = SecretString::new(serde_json::to_string(&next).unwrap());
    commit(&vault, id, &marker, &raw, &next, &bundle).unwrap();
    commit(&vault, id, &marker, &raw, &next, &bundle).unwrap();
    let after = vault.get_provider_summary(id).unwrap();
    assert_eq!(before.secret_refs[0].id, after.secret_refs[0].id);
    assert_eq!(before.title, after.title);
    assert_eq!(
        after.account_identity.as_deref(),
        Some(next.identity.as_str())
    );
    assert!(after.subscription.is_none());
    assert!(vault.provider_runtime_extension(id, KEY).unwrap().is_none());
    assert!(read(&vault, id, &marker).is_err());
    assert!(write(&vault, id, &marker, 3, &raw).is_err());
    let new_marker = vault.reveal_secret(id).unwrap();
    let saved = decode(read(&vault, id, &new_marker).unwrap().expose()).unwrap();
    assert_eq!(saved.models, json!({}));
    assert_ne!(saved.generation, old.generation);
}

#[test]
fn changed_revision_and_duplicate_scope_never_retarget_the_old_entry() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::create(root.path(), &SecretString::new("fixture-password"))
        .unwrap()
        .vault;
    let (id, marker, mut old, raw) = setup(&vault);
    let next = migrated(&old, &resolved(&old, "a")).unwrap();
    let bundle = SecretString::new(serde_json::to_string(&next).unwrap());
    old.revision += 1;
    vault
        .set_provider_runtime_extension(
            id,
            KEY,
            Some(&SecretString::new(serde_json::to_string(&old).unwrap())),
        )
        .unwrap();
    assert!(commit(&vault, id, &marker, &raw, &next, &bundle).is_err());
    assert_eq!(vault.reveal_secret(id).unwrap(), marker);
    persist(&vault, next.clone()).unwrap();
    assert!(commit(&vault, id, &marker, &raw, &next, &bundle)
        .unwrap_err()
        .message
        .contains("already exists"));
    assert_eq!(vault.reveal_secret(id).unwrap(), marker);
}
