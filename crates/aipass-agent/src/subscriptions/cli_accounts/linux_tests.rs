use super::*;
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
#[ignore = "private dbus-run-session and an unlocked disposable GNOME keyring only"]
fn copilot_secret_service_rotation_and_file_fallback() {
    assert_eq!(
        std::env::var("AIPASS_TEST_SECRET_SERVICE").as_deref(),
        Ok("1")
    );
    assert!(std::env::var("HOME")
        .unwrap()
        .starts_with("/tmp/aipass-native-"));
    let root = tempfile::tempdir_in(std::env::var_os("HOME").unwrap()).unwrap();
    let login = format!("fixture-{}", uuid::Uuid::new_v4());
    let account = format!("https://github.com:{login}");
    let config = root.path().join("config.json");
    let with_file = |token: Option<&str>| {
        let mut value = json!({"lastLoggedInUser":{"login":login,"host":"https://github.com"}});
        if let Some(token) = token {
            value["copilotTokens"] = json!({&account:token});
        }
        std::fs::write(&config, value.to_string()).unwrap();
    };
    let set_keyring = |token: &str| {
        let mut child = Command::new("secret-tool")
            .args([
                "store",
                "--label=AIPass disposable fixture",
                "service",
                "copilot-cli",
                "account",
                &account,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(token.as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
    };
    with_file(Some("synthetic-file"));
    assert_eq!(
        read("copilot", root.path()).unwrap()["access"],
        "synthetic-file"
    );
    set_keyring("synthetic-system");
    assert_eq!(
        read("copilot", root.path()).unwrap()["access"],
        "synthetic-system"
    );
    set_keyring("synthetic-rotated");
    assert_eq!(
        read("copilot", root.path()).unwrap()["access"],
        "synthetic-rotated"
    );
    with_file(None);
    assert_eq!(
        read("copilot", root.path()).unwrap()["access"],
        "synthetic-rotated"
    );
    assert!(Command::new("secret-tool")
        .args(["clear", "service", "copilot-cli", "account", &account])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success());
    assert!(read("copilot", root.path()).is_err());
    with_file(Some("synthetic-restored"));
    assert_eq!(
        read("copilot", root.path()).unwrap()["access"],
        "synthetic-restored"
    );
}
