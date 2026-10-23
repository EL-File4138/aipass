use super::*;

pub(super) fn write(root: &Path, user: &str, token: &str) {
    std::fs::write(
        root.join("auth.json"),
        json!({"https://auth.x.ai":{
            "email":user,"key":token,"expires_at":"2040-01-01T00:00:00Z"
        }})
        .to_string(),
    )
    .unwrap();
}
pub(super) fn context(root: &Path) -> Context {
    let (mut c, _, _) = super::super::super::tests::context(Client::new());
    c.provider = "grok".into();
    c.auth = reference("grok", root).unwrap();
    c.native_token = Some(aipass_agent_protocol::SensitiveString::new("old"));
    c
}

#[tokio::test]
async fn already_rotated_credentials_do_not_launch_a_cli() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "alice", "new");
    let mut c = context(root.path());
    renew_with(&mut c, true, Some("old"), |_, _| panic!("already renewed"))
        .await
        .unwrap();
    assert_eq!(c.token().unwrap(), "new");
    assert!(!c.auth.to_string().contains("new"));
}

#[tokio::test]
async fn torn_reads_retry_without_using_a_cached_owner() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "alice", "old");
    let mut c = context(root.path());
    std::fs::write(root.path().join("auth.json"), "{").unwrap();
    let repair = async {
        tokio::time::sleep(Duration::from_millis(120)).await;
        write(root.path(), "alice", "new");
    };
    let (result, ()) = tokio::join!(fresh(&mut c, false), repair);
    result.unwrap();
    assert_eq!(c.token().unwrap(), "new");
    std::fs::write(root.path().join("auth.json"), "").unwrap();
    assert_eq!(fresh(&mut c, false).await.unwrap_err(), TORN);
    assert!(c.native_token.is_none());
    write(root.path(), "bob", "foreign");
    assert!(fresh(&mut c, false)
        .await
        .unwrap_err()
        .contains("account changed"));
    assert!(c.native_token.is_none());
}

#[cfg(unix)]
#[tokio::test]
async fn cli_exit_status_is_not_proof_of_rotation() {
    for (script, replacement, success) in [
        ("exit 0", None, false),
        ("cp after.json auth.json; exit 0", Some("new"), true),
        ("cp after.json auth.json; exit 7", Some("new"), true),
        ("exit 7", None, false),
    ] {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "alice", "old");
        let mut c = context(root.path());
        if let Some(token) = replacement {
            write(root.path(), "alice", token);
            std::fs::rename(
                root.path().join("auth.json"),
                root.path().join("after.json"),
            )
            .unwrap();
            write(root.path(), "alice", "old");
        }
        let result = renew_with(&mut c, true, Some("old"), |c, path| {
            let mut cmd = native_cli::command(Path::new("/bin/sh"), c);
            cmd.current_dir(path).args(["-c", script]);
            Ok(cmd)
        })
        .await;
        assert_eq!(result.is_ok(), success);
        if success {
            assert_eq!(c.token().unwrap(), "new");
        } else {
            assert!(c.native_token.is_none());
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn renewal_cannot_switch_owner() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "alice", "old");
    let mut c = context(root.path());
    write(root.path(), "bob", "new");
    std::fs::rename(
        root.path().join("auth.json"),
        root.path().join("after.json"),
    )
    .unwrap();
    write(root.path(), "alice", "old");
    let error = renew_with(&mut c, true, Some("old"), |c, path| {
        let mut cmd = native_cli::command(Path::new("/bin/sh"), c);
        cmd.current_dir(path)
            .args(["-c", "cp after.json auth.json"]);
        Ok(cmd)
    })
    .await
    .unwrap_err();
    assert!(error.contains("account changed"));
    assert!(c.native_token.is_none());
}
