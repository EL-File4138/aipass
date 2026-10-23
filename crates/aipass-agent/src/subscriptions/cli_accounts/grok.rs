//! Grok Build owns renewal; rejected tokens are never renewed by a second OAuth engine.
use super::*;
use store::PrivateJson;

const TORN: &str = "Grok Build credentials are temporarily unreadable; retry shortly";

pub(super) fn read_once(root: &Path) -> Result<PrivateJson> {
    use std::io::Read;
    let file = std::fs::File::open(root.join("auth.json")).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => "Grok Build login is missing; reconnect this account",
        std::io::ErrorKind::PermissionDenied => "Grok Build credential permission denied",
        _ => "Grok Build credential store unavailable",
    })?;
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    file.take(LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| TORN)?;
    if bytes.len() > LIMIT {
        return Err("Grok Build credentials exceed limit".into());
    }
    let raw = PrivateJson(serde_json::from_slice(&bytes).map_err(|_| TORN)?);
    let item = raw
        .as_object()
        .into_iter()
        .flatten()
        .find_map(|(issuer, v)| {
            (issuer.trim_end_matches('/') == "https://auth.x.ai" && !s(v, "key").is_empty())
                .then_some(v)
        })
        .ok_or("Grok Build login is missing; reconnect this account")?;
    if s(item, "email").is_empty() {
        return Err("Grok Build account identity is missing; reconnect this account".into());
    }
    let expiry = item["expires_at"]
        .as_str()
        .and_then(|s| {
            time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
        })
        .map(|t| t.unix_timestamp())
        .ok_or("Grok Build token expiry is unavailable; reconnect this account")?;
    Ok(PrivateJson(
        json!({"access":item["key"],"accountId":item["email"],"expires":expiry}),
    ))
}

async fn read_current(root: &Path, expected: &str) -> Result<PrivateJson> {
    for attempt in 0..6 {
        match read_once(root) {
            Err(e) if e == TORN && attempt < 5 => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            result => {
                let value = result?;
                if s(&value, "accountId") != expected {
                    return Err("CLI account changed; reconnect it explicitly".into());
                }
                return Ok(value);
            }
        }
    }
    unreachable!()
}

fn usable(value: &Value, margin: i64) -> bool {
    value["expires"]
        .as_i64()
        .is_some_and(|t| t > (now() / 1000) as i64 + margin)
}

pub(super) async fn fresh(c: &mut Context, force: bool) -> Result<()> {
    let rejected = if force {
        c.native_token
            .as_ref()
            .map(|v| aipass_agent_protocol::SensitiveString::new(v.expose()))
    } else {
        None
    };
    renew(c, force, rejected.as_ref().map(|v| v.expose())).await
}

pub(crate) async fn recover(c: &mut Context) -> Result<()> {
    let rejected = aipass_agent_protocol::SensitiveString::new(c.token()?);
    renew(c, true, Some(rejected.expose())).await
}

async fn renew(c: &mut Context, force: bool, rejected: Option<&str>) -> Result<()> {
    renew_with(c, force, rejected, |c, path| command(c, "grok", path)).await
}

async fn renew_with(
    c: &mut Context,
    force: bool,
    rejected: Option<&str>,
    make_command: impl FnOnce(&Context, &Path) -> Result<tokio::process::Command>,
) -> Result<()> {
    c.native_token = None;
    check_device(&c.auth)?;
    let path = root(&c.auth)?;
    let expected = s(&c.auth, "accountId").to_owned();
    let mut value = read_current(&path, &expected).await?;
    let initial = aipass_agent_protocol::SensitiveString::new(s(&value, "access"));
    let rejected = rejected.or_else(|| force.then_some(initial.expose()));
    if (force && rejected.is_none_or(|old| s(&value, "access") == old)) || !usable(&value, 300) {
        // Waiting is cancellable. Once rotation starts, retain the existing
        // cancellation guard until the authoritative source has been reread.
        let _lock = source_lock::acquire(&path).await?;
        let _refresh = c.refresh_guard()?;
        value = read_current(&path, &expected).await?;
        let changed = rejected.is_some_and(|old| s(&value, "access") != old);
        if (!force || changed) && usable(&value, if changed { 0 } else { 300 }) {
            c.native_token = Some(aipass_agent_protocol::SensitiveString::new(s(
                &value, "access",
            )));
            return Ok(());
        }
        let mut cmd = make_command(c, &path)?;
        cmd.arg("models")
            .env("BROWSER", "www-browser")
            .env("NO_BROWSER", "true")
            .env("CI", "true");
        let outcome = native_cli::run(cmd, 30).await;
        value = read_current(&path, &expected).await?;
        if !usable(&value, 0) || rejected.is_some_and(|old| s(&value, "access") == old) {
            return Err(
                "Grok Build could not renew this sign-in; reconnect this account in AIPass".into(),
            );
        }
        // A CLI can exit unsuccessfully after writing a valid rotated token.
        // Never forward raw CLI output or replace a verified owner on this path.
        drop(outcome);
    }
    if !usable(&value, 0) {
        return Err("Grok Build sign-in expired; reconnect this account".into());
    }
    c.native_token = Some(aipass_agent_protocol::SensitiveString::new(s(
        &value, "access",
    )));
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(all(test, unix))]
mod process_tests;
