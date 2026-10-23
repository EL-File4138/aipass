use super::*;

#[test]
fn local_grants_are_encrypted_bound_and_excluded_from_sync_and_backup() {
    let root = tempfile::tempdir().unwrap();
    let password = SecretString::new("fixture-password");
    let mut vault = Vault::create(root.path(), &password).unwrap().vault;
    let generation = Uuid::new_v4();
    let token = SecretString::new("synthetic-single-use-refresh");
    vault
        .write_local_subscription(generation, "device-a", &token)
        .unwrap();
    assert_eq!(
        vault
            .read_local_subscription(generation, "device-a")
            .unwrap()
            .unwrap()
            .expose(),
        token.expose()
    );
    assert!(vault
        .read_local_subscription(generation, "device-b")
        .is_err());
    assert!(vault
        .read_local_subscription(Uuid::new_v4(), "device-a")
        .unwrap()
        .is_none());
    let bytes = fs::read(
        root.path()
            .join("local-subscriptions")
            .join(format!("{generation}.aiplocal")),
    )
    .unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains(token.expose()));
    for include_audit in [false, true] {
        assert!(exportable_files(root.path(), include_audit)
            .unwrap()
            .iter()
            .all(|p| !p.starts_with("local-subscriptions")));
    }
    let backup = vault.export_encrypted(&password).unwrap();
    let restore = tempfile::tempdir().unwrap();
    Vault::import_encrypted(restore.path(), &password, &backup).unwrap();
    let restored = Vault::open(restore.path(), &password).unwrap();
    assert!(restored
        .read_local_subscription(generation, "device-a")
        .unwrap()
        .is_none());
    fs::create_dir_all(restore.path().join("local-subscriptions")).unwrap();
    fs::write(
        restore
            .path()
            .join("local-subscriptions")
            .join(format!("{generation}.aiplocal")),
        &bytes,
    )
    .unwrap();
    assert!(restored
        .read_local_subscription(generation, "device-a")
        .is_err());
    vault.advance_epoch_and_rewrap("fixture").unwrap();
    vault
        .change_master_password(&SecretString::new("new-fixture-password"))
        .unwrap();
    assert_eq!(
        vault
            .read_local_subscription(generation, "device-a")
            .unwrap()
            .unwrap()
            .expose(),
        token.expose()
    );
    drop(vault);
    let reopened = Vault::open(root.path(), &SecretString::new("new-fixture-password")).unwrap();
    assert!(reopened
        .read_local_subscription(generation, "device-a")
        .unwrap()
        .is_some());
}

#[test]
fn damaged_local_grants_never_fall_back_to_a_shared_token() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::create(root.path(), &SecretString::new("fixture-password"))
        .unwrap()
        .vault;
    let generation = Uuid::new_v4();
    vault
        .write_local_subscription(generation, "device-a", &SecretString::new("token"))
        .unwrap();
    let path = root
        .path()
        .join("local-subscriptions")
        .join(format!("{generation}.aiplocal"));
    let mut grant: LocalGrant = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    grant.version = 2;
    fs::write(&path, serde_json::to_vec(&grant).unwrap()).unwrap();
    assert!(vault
        .read_local_subscription(generation, "device-a")
        .is_err());
    grant.version = 1;
    let first = &grant.payload.ciphertext_b64[..1];
    let replacement = if first == "A" { "B" } else { "A" };
    grant.payload.ciphertext_b64.replace_range(..1, replacement);
    fs::write(&path, serde_json::to_vec(&grant).unwrap()).unwrap();
    assert!(vault
        .read_local_subscription(generation, "device-a")
        .is_err());
}
