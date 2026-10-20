use super::*;
#[test]
fn stalled_store_is_bounded_without_a_worker_or_a_prompt() {
    let start = std::time::Instant::now();
    let result: Result<()> = bounded(std::future::pending());
    assert!(result.unwrap_err().to_string().contains("timed out"));
    assert!(start.elapsed() < Duration::from_secs(5));
}
#[test]
#[ignore = "private dbus-run-session and an unlocked disposable GNOME keyring only"]
fn isolated_linux_secret_service_roundtrip() {
    assert_eq!(
        std::env::var("AIPASS_TEST_SECRET_SERVICE").as_deref(),
        Ok("1")
    );
    assert!(std::env::var("HOME")
        .unwrap()
        .starts_with("/tmp/aipass-native-"));
    let service = format!("aipass-fixture-{}", uuid::Uuid::new_v4());
    let account = "synthetic-alice";
    assert!(read(&service, account).unwrap().is_none());
    write(&service, account, Some(b"fixture-first")).unwrap();
    assert_eq!(
        read(&service, account).unwrap().unwrap().as_slice(),
        b"fixture-first"
    );
    assert!(read(&service, "another-account").unwrap().is_none());
    assert!(read_keytar(&service, account).unwrap().is_none());
    write(&service, account, Some(b"fixture-rotated")).unwrap();
    assert_eq!(
        read(&service, account).unwrap().unwrap().as_slice(),
        b"fixture-rotated"
    );
    bounded(async {
        let ss = SecretService::connect(EncryptionType::Dh)
            .await
            .map_err(unavailable)?;
        let collection = ss.get_default_collection().await.map_err(unavailable)?;
        collection
            .create_item(
                "keytar fixture",
                HashMap::from([("service", service.as_str()), ("account", account)]),
                b"fixture-keytar",
                true,
                "text/plain",
            )
            .await
            .map_err(unavailable)?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        read_keytar(&service, account).unwrap().unwrap().as_slice(),
        b"fixture-keytar"
    );
    // Rust and keytar schemas must not steal each other's credential for the same identity.
    assert_eq!(
        read(&service, account).unwrap().unwrap().as_slice(),
        b"fixture-rotated"
    );
    write(&service, account, None).unwrap();
    assert!(read(&service, account).unwrap().is_none());
    write(&service, account, Some(b"fixture-before-lock")).unwrap();
    let codex_home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join("codex");
    std::fs::create_dir(&codex_home).unwrap();
    let config = codex_home.join("config.toml");
    std::fs::write(&config, "cli_auth_credentials_store = 'keyring'").unwrap();
    let codex = super::super::NativeStore::codex(&codex_home).unwrap();
    assert!(matches!(
        codex.primary,
        super::super::Resource::Keychain { .. }
    ));
    codex
        .primary
        .write(Some(br#"{"fixture":"keyring"}"#))
        .unwrap();
    assert_eq!(codex.credentials().unwrap().0["fixture"], "keyring");
    std::fs::write(codex_home.join("auth.json"), br#"{"fixture":"file"}"#).unwrap();
    std::fs::write(&config, "cli_auth_credentials_store = 'auto'").unwrap();
    assert_eq!(
        super::super::NativeStore::codex(&codex_home)
            .unwrap()
            .credentials()
            .unwrap()
            .0["fixture"],
        "keyring"
    );
    bounded(async {
        let ss = SecretService::connect(EncryptionType::Dh)
            .await
            .map_err(unavailable)?;
        ss.get_default_collection()
            .await
            .map_err(unavailable)?
            .lock()
            .await
            .map_err(unavailable)?;
        Ok(())
    })
    .unwrap();
    let start = std::time::Instant::now();
    let error = read(&service, account).unwrap_err().to_string();
    assert!(error.contains("locked"));
    assert!(!error.contains("fixture-before-lock"));
    assert!(start.elapsed() < Duration::from_secs(3));
    // Official auto mode can read its file while locked, but explicit keyring cannot downgrade.
    let fallback = super::super::NativeStore::codex(&codex_home).unwrap();
    assert_eq!(fallback.credentials().unwrap().0["fixture"], "file");
    assert!(fallback.fallback.unwrap().read().is_err());
    std::fs::write(config, "cli_auth_credentials_store = 'keyring'").unwrap();
    assert!(super::super::NativeStore::codex(&codex_home)
        .unwrap()
        .credentials()
        .is_err());
}
