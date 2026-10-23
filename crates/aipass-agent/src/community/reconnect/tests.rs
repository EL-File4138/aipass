use super::*;

fn account(identity: &str) -> Account {
    Account {
        provider: "qoder".into(), generation: Uuid::new_v4(), local_binding: None,
        auth: SensitiveString::new(json!({"type":"oauth","accountId":identity,"access":"fixture-access","refresh":"fixture-refresh"}).to_string()),
        models: json!({}), revision: 0, native_method: None,
        identity: identity.into(), feedback: Default::default(),
    }
}
fn setup() -> (tempfile::TempDir, Arc<AgentState>, Uuid) {
    let root = tempfile::tempdir().unwrap();
    let vault_dir = root.path().join("vault");
    let vault = Vault::create(&vault_dir, &SecretString::new("fixture-password"))
        .unwrap()
        .vault;
    let id = persist(&vault, account("alice")).unwrap();
    let state = crate::server::tests::sync_test_state(vault_dir);
    crate::session::set_session_vault(&state, vault);
    (root, state, id)
}

#[test]
fn reconnect_requires_the_same_provider_account_and_unlocked_session() {
    let (_root, state, id) = setup();
    assert!(Target::capture(&state, id, "factory").is_err());
    let target = Target::capture(&state, id, "qoder").unwrap();
    assert!(with_session(&state, Some(&target), |v| target.commit(v, account("bob"))).is_err());
    assert_eq!(
        with_session(&state, Some(&target), |v| target
            .commit(v, account("alice")))
        .unwrap(),
        id
    );
    let target = Target::capture(&state, id, "qoder").unwrap();
    let vault = Vault::open(&state.vault_dir, &SecretString::new("fixture-password")).unwrap();
    crate::session::set_session_vault(&state, vault);
    let error = with_session(&state, Some(&target), |v| {
        target.commit(v, account("alice"))
    })
    .unwrap_err();
    assert!(error.message.contains("session changed"));
    *state.session.lock().unwrap() = SessionState::Locked;
    assert!(Target::capture(&state, id, "qoder").is_err());
}

#[test]
fn concurrent_reconnect_and_archival_cannot_retarget_a_pending_login() {
    let (_root, state, id) = setup();
    let first = Target::capture(&state, id, "qoder").unwrap();
    let second = Target::capture(&state, id, "qoder").unwrap();
    with_session(&state, Some(&first), |v| first.commit(v, account("alice"))).unwrap();
    let error = with_session(&state, Some(&second), |v| {
        second.commit(v, account("alice"))
    })
    .unwrap_err();
    assert!(error.message.contains("authorization changed"));
    let target = Target::capture(&state, id, "qoder").unwrap();
    with_session(&state, Some(&target), |v| {
        v.archive_provider(id).map_err(map_vault_error)
    })
    .unwrap();
    assert!(with_session(&state, Some(&target), |v| target
        .commit(v, account("alice")))
    .is_err());
    assert!(Target::capture(&state, id, "qoder").is_err());
}

#[test]
fn synchronized_binding_changes_reject_a_pending_login() {
    let (_root, state, id) = setup();
    let target = Target::capture(&state, id, "qoder").unwrap();
    with_session(&state, Some(&target), |v| {
        let mut changed = account("alice");
        device::prepare(v, &mut changed)?;
        let descriptor = device::descriptor(&changed)?;
        v.set_provider_runtime_extension(
            id,
            device::KEY,
            Some(&SecretString::new(
                serde_json::to_string(&descriptor).unwrap(),
            )),
        )
        .map_err(map_vault_error)
    })
    .unwrap();
    let error = with_session(&state, Some(&target), |v| {
        target.commit(v, account("alice"))
    })
    .unwrap_err();
    assert!(error.message.contains("subscription changed"));
}
