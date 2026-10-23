use super::*;
pub(crate) mod identity;
mod quota;
use sha2::{Digest, Sha256};
const SERVER: &str = "https://server.codeium.com";
fn server(c: &Context) -> Result<String> {
    identity::normalize_server(&c.auth)
}
fn native(source: &Value) -> Result<Value> {
    let root = if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or(home()?.join("AppData/Roaming"))
    } else {
        aipass_config_writers::xdg_data_home(&home()?)
    };
    let path = native_import::root(source)
        .unwrap_or_else(|| root.join("devin"))
        .join("credentials.toml");
    let raw = zeroize::Zeroizing::new(
        std::fs::read_to_string(path).map_err(|_| "Devin CLI is not signed in")?,
    );
    if raw.len() > LIMIT {
        return Err("Devin credentials exceed limit".into());
    }
    let v: toml::Value = toml::from_str(&raw).map_err(|_| "invalid Devin CLI credentials")?;
    let key = v
        .get("windsurf_api_key")
        .and_then(toml::Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("Devin CLI has no session token")?;
    Ok(
        json!({"type":"api","key":key,"metadata":{"server":v.get("api_server_url").and_then(toml::Value::as_str).unwrap_or(SERVER),"cli":true}}),
    )
}
pub(super) async fn login(c: &mut Context, method: usize) -> Result<Value> {
    if method == 1 {
        return identity::resolve(&c.client, &native(&c.auth)?).await;
    }
    let cb = loopback::Loopback::bind(&[0]).await?;
    let redirect = format!("http://127.0.0.1:{}/callback", cb.port);
    let verifier = format!("{}{}", hex_id(), hex_id());
    let state = format!("{}{}", hex_id(), hex_id());
    let mut url = url::Url::parse("https://app.devin.ai/auth/cli/continue").unwrap();
    url.query_pairs_mut()
        .append_pair("redirect_uri", &redirect)
        .append_pair("state", &state)
        .append_pair("prompt", "select_account")
        .append_pair(
            "code_challenge",
            &URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
        )
        .append_pair("code_challenge_method", "S256")
        .append_pair("cli_pkce_marker", "1");
    c.challenge(url.as_str(), "Sign in to Devin in the browser.", false)
        .await?;
    let p = cb
        .wait(&["https://app.devin.ai"], |r| {
            r.method == "GET" && r.path == "/callback" && r.fields["state"] == state
        })
        .await?
        .fields;
    let v = json_request(
        c.client
            .post(format!(
                "{SERVER}/exa.seat_management_pb.SeatManagementService/ExchangeDevinCLIPKCECode"
            ))
            .json(&json!({"code":p["code"],"code_verifier":verifier,"redirect_uri":redirect})),
    )
    .await?;
    let key = v["sessionToken"]
        .as_str()
        .or(v["session_token"].as_str())
        .filter(|s| !s.is_empty())
        .ok_or("Devin returned no session token")?;
    identity::resolve(
        &c.client,
        &json!({"type":"api","key":key,"metadata":{"server":SERVER}}),
    )
    .await
}
pub(super) async fn fresh(c: &mut Context) -> Result<()> {
    use crate::subscription_import::reader::PrivateAuth;
    let mut candidate = PrivateAuth(c.auth.clone());
    if c.auth["nativeSource"].is_object() {
        let mut local = PrivateAuth(native(&c.auth)?);
        if identity::normalize_server(&local.0)? != identity::normalize_server(&c.auth)? {
            return Err("Devin server changed; reconnect explicitly".into());
        }
        // Validate the current source key against the established scope. The
        // saved key may already be revoked after the official CLI replaced it.
        candidate.0["key"] = local.0["key"].take();
    }
    let mut verified = PrivateAuth(identity::resolve(&c.client, &candidate.0).await?);
    crate::community::ensure_owner(&c.auth, &verified.0)?;
    if verified.0 != c.auth {
        c.save(verified.0.take()).await?;
    }
    Ok(())
}
fn executable() -> Result<std::path::PathBuf> {
    aipass_config_writers::cli_discovery::executable_candidates(&["devin"])
        .into_iter()
        .next()
        .ok_or_else(|| "Devin CLI is not installed".into())
}
async fn families(c: &Context) -> Result<Value> {
    let exe = executable()?;
    let temp = tempfile::Builder::new()
        .prefix("aipass-devin-")
        .tempdir()
        .map_err(|_| "cannot create Devin workspace")?;
    let dir = temp.path().join("devin");
    std::fs::create_dir(&dir).map_err(|_| "cannot create Devin workspace")?;
    let value = toml::to_string(&std::collections::BTreeMap::from([
        ("windsurf_api_key", c.token()?.to_owned()),
        ("api_server_url", server(c)?),
        ("devin_webapp_host", "app.devin.ai".into()),
        ("devin_api_url", "https://api.devin.ai".into()),
    ]))
    .map_err(|_| "cannot encode Devin credentials")?;
    let mut file = std::fs::OpenOptions::new();
    file.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        file.mode(0o600);
    }
    use std::io::Write;
    file.open(dir.join("credentials.toml"))
        .and_then(|mut f| f.write_all(value.to_string().as_bytes()))
        .map_err(|_| "cannot stage Devin credentials")?;
    let mut cmd = native_cli::command(&exe, c);
    cmd.args(["models", "list", "--format", "json"])
        .env("XDG_DATA_HOME", temp.path())
        .env("APPDATA", temp.path())
        .current_dir(temp.path());
    let output = native_cli::run(cmd, 30).await?;
    let v: Value = serde_json::from_str(&output).map_err(|_| "invalid Devin model list")?;
    let families:Vec<_>=v["families"].as_array().into_iter().flatten().filter(|f|!matches!(s(f,"family_uid"),""|"adaptive"|"fusion")).map(|f|json!({"uid":f["family_uid"],"label":f["family_label"],"aliases":f["aliases"],"models":f["variants"].as_array().into_iter().flatten().map(|v|json!({"id":v["model_uid"],"name":v["label"],"context":v["max_context_tokens"],"output":v["max_output_tokens"]})).collect::<Vec<_>>()})).collect();
    if families.is_empty() {
        return Err("Devin returned no model families".into());
    }
    Ok(json!(families))
}
pub(super) async fn models(c: &mut Context) -> Result<Value> {
    let families = match families(c).await {
        Ok(v) => v,
        Err(_) => serde_json::from_str(include_str!("devin_families.json"))
            .map_err(|_| "invalid Devin model metadata")?,
    };
    let mut out = json!({});
    for f in families.as_array().into_iter().flatten() {
        let id = s(f, "uid");
        let variants = f["models"].as_array().cloned().unwrap_or_default();
        let context = variants
            .iter()
            .filter_map(|m| m["context"].as_u64())
            .max()
            .unwrap_or(0);
        let output = variants
            .iter()
            .filter_map(|m| m["output"].as_u64())
            .max()
            .unwrap_or(0);
        let mut ids = vec![(id.to_owned(), s(f, "label").to_owned())];
        for tier in ["fast", "priority"] {
            if variants
                .iter()
                .any(|m| s(m, "id").ends_with(&format!("-{tier}")))
            {
                ids.push((format!("{id}-{tier}"), format!("{} {tier}", s(f, "label"))));
            }
        }
        for m in &variants {
            ids.push((s(m, "id").into(), s(m, "name").into()));
        }
        for (id, name) in ids {
            if id.is_empty() {
                continue;
            }
            let mut item = model(&id, &name, "chat", SERVER, context, output);
            item["devin_families"] = json!([f]);
            out[&id] = item;
        }
    }
    Ok(out)
}
pub(super) async fn usage(c: &mut Context) -> Result<Value> {
    let v = identity::status(&c.client, &c.auth).await?;
    let checked = crate::subscription_import::reader::PrivateAuth(identity::resolved(&c.auth, &v)?);
    crate::community::ensure_owner(&c.auth, &checked.0)?;
    let mut usage = quota::normalize(&v)?;
    let tiers = model_cost_tiers(c).await;
    quota::scope(&mut usage, tiers.as_ref().ok(), &c.models);
    Ok(usage)
}

async fn model_cost_tiers(c: &Context) -> Result<std::collections::BTreeMap<String, u64>> {
    use aipass_proxy_conversion::providers::devin::models;
    let response = send(
        c.client
            .post(format!(
                "{}/exa.api_server_pb.ApiServerService/GetCliModelConfigs",
                server(c)?
            ))
            .header("Content-Type", "application/proto")
            .header("Connect-Protocol-Version", "1")
            .timeout(Duration::from_secs(15))
            .body(models::request(c.token()?, vendor_platform())),
    )
    .await?;
    if !response.status().is_success() {
        return Err("Devin model cost tiers unavailable".into());
    }
    models::cost_tiers(&read_bytes(response, LIMIT).await?)
}

pub(super) async fn generate(c: &mut Context, m: &Value, body: Value) -> Result<()> {
    use aipass_proxy_conversion::providers::devin::{request, variant, DevinStream};
    let fallback: Value = serde_json::from_str(include_str!("devin_families.json"))
        .map_err(|_| "invalid Devin model metadata")?;
    let families = m.get("devin_families").unwrap_or(&fallback);
    let uid = variant(families, s(&body, "model"), s(&body, "reasoning_effort"));
    let req = request(&body, &uid, c.token()?, vendor_platform())?;
    let response = send(
        c.client
            .post(format!(
                "{}/exa.api_server_pb.ApiServerService/GetChatMessage",
                server(c)?
            ))
            .header("Content-Type", "application/connect+proto")
            .header("Connect-Protocol-Version", "1")
            .header("Authorization", format!("Basic {}", c.token()?))
            .body(req),
    )
    .await?;
    let mut converter = DevinStream::new(s(&body, "model"), &format!("chatcmpl-{}", hex_id()));
    c.converted(response, &mut converter).await
}
