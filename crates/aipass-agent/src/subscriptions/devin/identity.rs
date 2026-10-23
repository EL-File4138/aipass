//! The billing scope comes from the key's own status, independently of labels.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Scope {
    server: String,
    principal_kind: String,
    principal: String,
    org: Option<String>,
    team: Option<String>,
    // Unknown scope must never deduplicate two authorizations by email alone.
    unscoped_binding: Option<String>,
}

fn field(value: &Value, name: &str) -> Option<String> {
    value[name]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 1024)
        .map(str::to_owned)
}

pub(crate) fn normalize_server(auth: &Value) -> Result<String> {
    let mut url = url::Url::parse(auth["metadata"]["server"].as_str().unwrap_or(SERVER))
        .map_err(|_| "invalid Devin server")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("invalid Devin server".into());
    }
    if url.port() == Some(443) {
        let _ = url.set_port(None);
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

pub(crate) fn resolved(auth: &Value, status: &Value) -> Result<Value> {
    let user = status
        .get("userStatus")
        .filter(|v| v.is_object())
        .ok_or("Devin returned no account identity")?;
    let old = auth
        .get("devinScope")
        .map(|v| serde_json::from_value::<Scope>(v.clone()))
        .transpose()
        .map_err(|_| "invalid Devin account scope")?;
    let (kind, principal) = if old.as_ref().is_some_and(|s| s.principal_kind == "email") {
        ("email", field(user, "email"))
    } else if let Some(id) = field(user, "userId") {
        ("userId", Some(id))
    } else {
        ("email", field(user, "email"))
    };
    let principal = principal.ok_or("Devin returned no account identity")?;
    let info = &user["planStatus"]["planInfo"]["devinInfo"];
    let org = field(info, "orgId");
    let team = field(user, "teamId");
    let scope = Scope {
        server: normalize_server(auth)?,
        principal_kind: kind.into(),
        principal,
        unscoped_binding: if org.is_none() && team.is_none() {
            Some(
                old.as_ref()
                    .and_then(|s| s.unscoped_binding.clone())
                    .unwrap_or_else(hex_id),
            )
        } else {
            None
        },
        org,
        team,
    };
    if old.as_ref().is_some_and(|old| old != &scope) {
        return Err("Devin account or organization changed; reconnect explicitly".into());
    }
    // Legacy anchors are compared to the live status before an Agent migration.
    if old.is_none() {
        if let Some(id) = auth.get("accountId").and_then(Value::as_str) {
            if Some(id) != user["userId"].as_str() && Some(id) != user["email"].as_str() {
                return Err("Devin account ownership changed; reconnect explicitly".into());
            }
        }
        if let Some(email) = auth.pointer("/metadata/email").and_then(Value::as_str) {
            if Some(email) != user["email"].as_str() {
                return Err("Devin account ownership changed; reconnect explicitly".into());
            }
        }
    }
    let mut next = auth.clone();
    let bytes = serde_json::to_vec(&scope).map_err(|_| "invalid Devin scope")?;
    next["accountId"] = json!(format!("devin:v2:{}", URL_SAFE_NO_PAD.encode(bytes)));
    next["devinScope"] = serde_json::to_value(scope).map_err(|_| "invalid Devin scope")?;
    next["metadata"]["server"] = json!(normalize_server(auth)?);
    if let Some(email) = field(user, "email") {
        next["metadata"]["email"] = json!(email);
    }
    if let Some(name) = field(info, "accountDisplayName") {
        next["metadata"]["organization"] = json!(name);
    }
    Ok(next)
}

pub(crate) fn ensure(before: &Value, after: &Value) -> Result<()> {
    if let Some(scope) = before.get("devinScope") {
        let scope: Scope =
            serde_json::from_value(scope.clone()).map_err(|_| "invalid Devin account scope")?;
        let expected = format!(
            "devin:v2:{}",
            URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(&scope).map_err(|_| "invalid Devin account scope")?)
        );
        if after.get("devinScope") != before.get("devinScope")
            || s(before, "accountId") != expected
            || s(after, "accountId") != expected
            || normalize_server(before)? != scope.server
            || normalize_server(after)? != scope.server
        {
            return Err("Devin account or organization changed; reconnect explicitly".into());
        }
    }
    Ok(())
}

pub(crate) async fn resolve(client: &Client, auth: &Value) -> Result<Value> {
    let status = status(client, auth).await?;
    resolved(auth, &status)
}

pub(crate) fn resolve_saved(auth: &Value, proxy: &UpstreamProxyConfig) -> Result<Value> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Devin identity executor unavailable")?;
    runtime.block_on(async { resolve(&transport(Some(proxy))?, auth).await })
}

pub(super) async fn status(client: &Client, auth: &Value) -> Result<Value> {
    if s(auth, "key").is_empty() {
        return Err("Devin sign-in is missing".into());
    }
    json_request(client.post(format!("{}/exa.seat_management_pb.SeatManagementService/GetUserStatus", normalize_server(auth)?))
        .header("Connect-Protocol-Version", "1")
        .json(&json!({"metadata":{"apiKey":auth["key"],"ideName":"devin-cli","ideVersion":"3000.11.3","extensionName":"devin-cli","extensionVersion":"3000.11.3","locale":"en","os":vendor_platform()}}))).await
}

#[cfg(test)]
mod tests;
