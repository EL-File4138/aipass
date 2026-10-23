use super::*;
fn status(org: &str, name: &str) -> Value {
    json!({"userStatus":{"userId":"user-a","email":"alice@example.test","teamId":format!("team-{org}"),
        "planStatus":{"planInfo":{"devinInfo":{"orgId":org,"accountDisplayName":name}}}}})
}
fn auth() -> Value {
    json!({"type":"api","key":"synthetic-key","metadata":{"server":SERVER}})
}

#[test]
fn organizations_are_distinct_and_names_are_not_identity() {
    let a = resolved(&auth(), &status("a", "First")).unwrap();
    let b = resolved(&auth(), &status("b", "Second")).unwrap();
    assert_ne!(a["accountId"], b["accountId"]);
    assert_eq!(a["metadata"]["email"], "alice@example.test");
    let renamed = resolved(&a, &status("a", "Renamed")).unwrap();
    assert_eq!(renamed["accountId"], a["accountId"]);
    crate::community::ensure_owner(&a, &renamed).unwrap();
    let mut changed_email = status("a", "Renamed");
    changed_email["userStatus"]["email"] = json!("new-email@example.test");
    let updated = resolved(&a, &changed_email).unwrap();
    assert_eq!(updated["accountId"], a["accountId"]);
    crate::community::ensure_owner(&a, &updated).unwrap();
    assert!(resolved(&a, &status("b", "First")).is_err());
    let mut forged = a.clone();
    forged["devinScope"] = b["devinScope"].clone();
    assert!(crate::community::ensure_owner(&a, &forged).is_err());
    forged = a.clone();
    forged["accountId"] = json!("unscoped-user");
    assert!(crate::community::ensure_owner(&a, &forged).is_err());
    forged = a.clone();
    forged["metadata"]["server"] = json!("https://foreign.example.test");
    assert!(crate::community::ensure_owner(&a, &forged).is_err());
}

#[test]
fn legacy_anchors_must_match_and_unknown_scopes_do_not_deduplicate() {
    let mut legacy = auth();
    legacy["accountId"] = json!("user-a");
    resolved(&legacy, &status("a", "A")).unwrap();
    legacy["accountId"] = json!("different-user");
    assert!(resolved(&legacy, &status("a", "A")).is_err());
    assert!(resolved(&auth(), &json!({})).is_err());
    let unscoped = json!({"userStatus":{"email":"alice@example.test"}});
    let a = resolved(&auth(), &unscoped).unwrap();
    let b = resolved(&auth(), &unscoped).unwrap();
    assert_ne!(a["accountId"], b["accountId"]);
    assert_eq!(
        resolved(&a, &unscoped).unwrap()["accountId"],
        a["accountId"]
    );
    let enriched = json!({"userStatus":{"userId":"added-later","email":"alice@example.test"}});
    assert_eq!(
        resolved(&a, &enriched).unwrap()["accountId"],
        a["accountId"]
    );
}

#[tokio::test]
async fn changed_cli_key_is_checked_against_the_existing_scope_before_save() {
    for org in ["a", "b"] {
        let root = tempfile::tempdir().unwrap();
        let (client, server, request) =
            crate::subscriptions::tests::https_fixture(status(org, "A")).await;
        let (mut c, tx, mut rx) = crate::subscriptions::tests::context(client);
        c.provider = "devin".into();
        let mut initial = auth();
        initial["metadata"]["server"] = json!(server);
        initial["nativeSource"] = json!({"provider":"devin","root":root.path(),"selector":""});
        initial["nativeDevice"] = json!("fixture-device");
        c.auth = resolved(&initial, &status("a", "A")).unwrap();
        let old = c.auth.clone();
        std::fs::write(
            root.path().join("credentials.toml"),
            format!("windsurf_api_key = \"current-key\"\napi_server_url = \"{server}\"\n"),
        )
        .unwrap();
        if org == "a" {
            let ack = async {
                let frame = rx.recv().await.unwrap();
                assert_eq!(frame["type"], "auth");
                assert_eq!(frame["value"]["key"], "current-key");
                tx.send(json!({"type":"ack","id":frame["id"],"ok":true}))
                    .await
                    .unwrap();
            };
            let (result, ()) = tokio::join!(super::super::fresh(&mut c), ack);
            result.unwrap();
            assert_eq!(c.auth["key"], "current-key");
            assert_eq!(c.auth["accountId"], old["accountId"]);
            assert_eq!(c.auth["nativeSource"], old["nativeSource"]);
        } else {
            assert!(super::super::fresh(&mut c)
                .await
                .unwrap_err()
                .contains("organization changed"));
            assert_eq!(c.auth, old);
            assert!(rx.try_recv().is_err());
        }
        let wire = request.await.unwrap();
        assert!(wire.contains("current-key"));
        assert!(!wire.contains("synthetic-key"));
    }
}
