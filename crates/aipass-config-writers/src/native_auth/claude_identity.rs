//! One Claude identity contract for login, discovery and native credential stores.
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

const ORGANIZATION: &str = "::org::";

pub fn profile_path(config_dir: &Path, default_dir: &Path) -> PathBuf {
    if config_dir == default_dir {
        config_dir
            .parent()
            .unwrap_or(config_dir)
            .join(".claude.json")
    } else {
        config_dir.join(".claude.json")
    }
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value[key].as_str().map(str::trim).filter(|s| !s.is_empty())
}

pub fn identity(status: &Value, profile: &Value) -> Result<String> {
    let account = &profile["oauthAccount"];
    let status_email = text(status, "email");
    let profile_email = text(account, "emailAddress");
    if status_email
        .zip(profile_email)
        .is_some_and(|(a, b)| !a.eq_ignore_ascii_case(b))
    {
        bail!("Claude account status and profile disagree");
    }
    let email = status_email
        .or(profile_email)
        .context("Claude account identity unavailable")?;
    let status_org = text(status, "orgId");
    let profile_org = text(account, "organizationUuid");
    if status_org.zip(profile_org).is_some_and(|(a, b)| a != b) {
        bail!("Claude organization status and profile disagree");
    }
    let email = email.to_ascii_lowercase();
    Ok(match status_org.or(profile_org) {
        Some(org) => format!("{email}{ORGANIZATION}{org}"),
        None => email,
    })
}

/// Only for a one-time upgrade anchored to the old entry's own local CLI home.
/// Never use this weaker comparison to authorize requests or credential writes.
pub fn is_legacy_of(legacy: &str, scoped: &str) -> bool {
    !legacy.contains(ORGANIZATION)
        && legacy != scoped
        && legacy.eq_ignore_ascii_case(
            scoped
                .split_once(ORGANIZATION)
                .map_or(scoped, |(email, _)| email),
        )
}

pub fn label(identity: &str) -> String {
    match identity.split_once(ORGANIZATION) {
        Some((email, org)) => format!("{email} ({org})"),
        None => identity.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn organizations_are_distinct_even_with_the_same_name_and_email() {
        let profile = |org| json!({"oauthAccount":{"emailAddress":"Alice@example.test","organizationUuid":org,"organizationName":"Team"}});
        let a = identity(&Value::Null, &profile("a")).unwrap();
        let b = identity(&Value::Null, &profile("b")).unwrap();
        assert_ne!(a, b);
        assert!(is_legacy_of("Alice@example.test", &a));
        assert!(!is_legacy_of(&a, &b));
        assert!(!is_legacy_of("bob@example.test", &a));
        assert!(is_legacy_of("ALICE@example.test", "alice@example.test"));
        assert!(!is_legacy_of("alice@example.test", "alice@example.test"));
        assert_ne!(
            a,
            identity(&json!({"email":"alice@example.test"}), &Value::Null).unwrap()
        );
    }

    #[test]
    fn conflicting_status_and_profile_fail_closed() {
        let p =
            json!({"oauthAccount":{"emailAddress":"alice@example.test","organizationUuid":"a"}});
        assert!(identity(&json!({"email":"bob@example.test"}), &p).is_err());
        assert!(identity(&json!({"email":"alice@example.test","orgId":"b"}), &p).is_err());
        assert_eq!(
            identity(&json!({"email":"ALICE@example.test","orgId":"a"}), &p).unwrap(),
            "alice@example.test::org::a"
        );
    }

    #[test]
    fn native_store_identity_detects_same_email_organization_changes_and_missing_scope() {
        use crate::native_auth::{NativeStore, Resource};
        let dir = tempfile::tempdir().unwrap();
        let auth = dir.path().join("credentials");
        let profile = dir.path().join("profile");
        std::fs::write(
            &auth,
            json!({"claudeAiOauth":{"accessToken":"fixture"}}).to_string(),
        )
        .unwrap();
        let store = NativeStore {
            tool: "claude-code".into(),
            home: dir.path().to_owned(),
            primary: Resource::File(auth),
            fallback: None,
            profile: Some(Resource::File(profile.clone())),
        };
        let write_profile = |org: Value| {
            std::fs::write(&profile,json!({"oauthAccount":{"emailAddress":"alice@example.test","organizationUuid":org}}).to_string()).unwrap()
        };
        write_profile(json!("a"));
        let first = store.identity().unwrap();
        assert_eq!(first.as_deref(), Some("alice@example.test::org::a"));
        write_profile(json!("b"));
        assert_ne!(store.identity().unwrap(), first);
        write_profile(Value::Null);
        assert_ne!(store.identity().unwrap(), first);
    }
}
