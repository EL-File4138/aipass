use super::*;

#[cfg(unix)]
mod processes;

fn account() -> Account {
    Account { provider:"qoder".into(), generation:Uuid::new_v4(), local_binding:None,
        auth:SensitiveString::new(json!({"type":"oauth","access":"fixture-access","refresh":"fixture-refresh","deviceRefresh":"fixture-device-refresh","accountId":"alice"}).to_string()),
        models:json!({}), revision:0, native_method:None, identity:"alice".into(), feedback:Default::default() }
}
#[test]
fn descriptors_never_contain_grants_and_local_refresh_does_not_change_sync_records() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::create(root.path(), &SecretString::new("fixture-password"))
        .unwrap()
        .vault;
    let id = persist(&vault, account()).unwrap();
    let stored_before = stored(&vault, id).unwrap().unwrap();
    assert!(!stored_before.expose().contains("fixture-"));
    assert_eq!(account::storage_key(&vault, id).unwrap(), KEY);
    assert!(vault
        .provider_runtime_extension(id, super::super::KEY)
        .unwrap()
        .is_none());
    let marker = vault.reveal_secret(id).unwrap();
    let current = decode(read(&vault, id, &marker).unwrap().expose()).unwrap();
    assert!(current.local_binding.is_some());
    let mut next = current.clone();
    let mut a = auth(&next).unwrap();
    a.0["refresh"] = json!("new-refresh");
    next.auth = SensitiveString::new(a.0.to_string());
    next.revision += 1;
    write(
        &vault,
        id,
        &marker,
        current.revision,
        &serde_json::to_string(&next).unwrap(),
    )
    .unwrap();
    assert_eq!(
        stored(&vault, id).unwrap().unwrap().expose(),
        stored_before.expose()
    );
    assert_eq!(
        decode(read(&vault, id, &marker).unwrap().expose())
            .unwrap()
            .revision,
        1
    );
    let secret_id = vault.get_provider_summary(id).unwrap().secret_refs[0]
        .id
        .clone();
    bind(&vault, id, account()).unwrap();
    assert_eq!(vault.reveal_secret(id).unwrap(), marker);
    assert_eq!(
        vault.get_provider_summary(id).unwrap().secret_refs[0].id,
        secret_id
    );
    let rebound = decode(read(&vault, id, &marker).unwrap().expose()).unwrap();
    assert_ne!(rebound.local_binding, current.local_binding);
    let mut late = next;
    late.revision = rebound.revision + 1;
    assert!(write(
        &vault,
        id,
        &marker,
        rebound.revision,
        &serde_json::to_string(&late).unwrap()
    )
    .is_err());
}

#[test]
fn legacy_grants_stay_recoverable_until_explicit_reauthorization() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::create(root.path(), &SecretString::new("fixture-password"))
        .unwrap()
        .vault;
    let old = account();
    let id = persist(&vault, old.clone()).unwrap();
    let raw = SecretString::new(serde_json::to_string(&old).unwrap());
    vault
        .set_provider_runtime_extension(id, super::super::KEY, Some(&raw))
        .unwrap();
    vault.set_provider_runtime_extension(id, KEY, None).unwrap();
    let marker = vault.reveal_secret(id).unwrap();
    assert!(read(&vault, id, &marker)
        .unwrap_err()
        .message
        .contains("reconnect"));
    assert_eq!(stored(&vault, id).unwrap().unwrap().expose(), raw.expose());
    let before = vault.get_provider_summary(id).unwrap();
    let mut other = account();
    other.identity = "bob".into();
    assert!(bind(&vault, id, other).is_err());
    assert!(bind(&vault, id, account())
        .unwrap_err()
        .message
        .contains("Fresh local authorization"));
    let mut fresh = account();
    fresh.auth = SensitiveString::new(json!({"type":"oauth","access":"new-access","refresh":"independent-refresh","deviceRefresh":"independent-device-refresh","accountId":"alice"}).to_string());
    bind(&vault, id, fresh).unwrap();
    assert!(vault
        .provider_runtime_extension(id, super::super::KEY)
        .unwrap()
        .is_none());
    assert!(read(&vault, id, &marker).is_err());
    let after = vault.get_provider_summary(id).unwrap();
    assert_eq!(before.secret_refs[0].id, after.secret_refs[0].id);
    assert_eq!(before.title, after.title);
}

#[test]
fn two_synced_vaults_keep_independent_grants_through_sync_and_local_reconnect() {
    let a_root = tempfile::tempdir().unwrap();
    let b_root = tempfile::tempdir().unwrap();
    let password = SecretString::new("fixture-password");
    let mut a = Vault::create(a_root.path(), &password).unwrap().vault;
    let id = persist(&a, account()).unwrap();
    let marker = a.reveal_secret(id).unwrap();
    let snapshot = a.export_sync_snapshot_with_parents(Vec::new()).unwrap();
    aipass_vault::VaultSyncSnapshot::parse(&snapshot)
        .unwrap()
        .bootstrap(b_root.path())
        .unwrap();
    let mut b = Vault::open(b_root.path(), &password).unwrap();
    assert!(read(&b, id, &marker)
        .unwrap_err()
        .message
        .contains("reconnect"));
    let shared_revision = b.sync_revision().unwrap();
    let mut own = account();
    own.auth = SensitiveString::new(json!({"type":"oauth","access":"device-b-access","refresh":"device-b-refresh","accountId":"alice"}).to_string());
    bind(&b, id, own).unwrap();
    assert_eq!(b.sync_revision().unwrap(), shared_revision);
    assert!(read(&a, id, &marker)
        .unwrap()
        .expose()
        .contains("fixture-refresh"));
    assert!(read(&b, id, &marker)
        .unwrap()
        .expose()
        .contains("device-b-refresh"));
    let snapshot = b.export_sync_snapshot_with_parents(Vec::new()).unwrap();
    a.apply_sync_snapshot(&snapshot).unwrap();
    assert!(read(&a, id, &marker)
        .unwrap()
        .expose()
        .contains("fixture-refresh"));
    a.advance_epoch_and_rewrap("fixture-local-rotation")
        .unwrap();
    b.apply_sync_snapshot(&a.export_sync_snapshot_with_parents(Vec::new()).unwrap())
        .unwrap();
    assert!(read(&b, id, &marker)
        .unwrap()
        .expose()
        .contains("device-b-refresh"));
    assert_eq!(
        a.get_provider_summary(id).unwrap().secret_refs[0].id,
        b.get_provider_summary(id).unwrap().secret_refs[0].id
    );
}
