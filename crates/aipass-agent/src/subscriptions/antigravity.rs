//! Antigravity Google authorization and account-scoped Code Assist requests.
use super::*;
const CLIENT: &str = "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
// Installed application OAuth credentials are public client identifiers.
const CLIENT_KEY: &str = "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";
const TOKEN: &str = "https://oauth2.googleapis.com/token";
const BASE: &str = "https://daily-cloudcode-pa.googleapis.com/v1internal";
const SETUP: &str = "https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist";

fn signed(c: &Context, r: RequestBuilder) -> Result<RequestBuilder> {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        other => other,
    };
    Ok(r.bearer_auth(c.token()?).header(
        "User-Agent",
        format!("antigravity/hub/2.24.0 {}/{}", os, arch),
    ))
}
pub(super) async fn login(c: &mut Context, _method: usize) -> Result<Value> {
    let listener = loopback::Loopback::bind(&[0]).await?;
    let redirect = format!("http://localhost:{}/oauth-callback", listener.port);
    let state = hex_id();
    let verifier = SecretString::new(format!("{}{}", hex_id(), hex_id()));
    let challenge = URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier.expose().as_bytes()));
    let mut url = url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth").unwrap();
    url.query_pairs_mut().extend_pairs([("client_id",CLIENT),("redirect_uri",&redirect),("response_type","code"),("access_type","offline"),("prompt","consent"),("state",&state),("code_challenge",&challenge),("code_challenge_method","S256"),("scope","https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/cclog https://www.googleapis.com/auth/experimentsandconfigs")]);
    c.challenge(
        url.as_str(),
        "Sign in with your Antigravity Google account",
        false,
    )
    .await?;
    let cb = listener
        .wait(&[], |cb| {
            cb.method == "GET" && cb.path == "/oauth-callback" && cb.fields["state"] == state
        })
        .await?;
    let code = cb.fields["code"]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or("Google sign-in was not approved")?;
    let _guard = c.refresh_guard()?;
    let v = crate::subscription_import::reader::PrivateAuth(
        json_request(c.client.post(TOKEN).form(&[
            ("grant_type", "authorization_code"),
            ("client_id", CLIENT),
            ("client_secret", CLIENT_KEY),
            ("redirect_uri", &redirect),
            ("code", code),
            ("code_verifier", verifier.expose()),
        ]))
        .await?,
    );
    let mut auth = device::token(&v.0, &json!({}))?;
    if s(&auth, "refresh").is_empty() {
        return Err("Google returned no refresh grant; reconnect Antigravity".into());
    }
    let user = json_request(
        c.client
            .get("https://www.googleapis.com/oauth2/v2/userinfo")
            .bearer_auth(s(&auth, "access")),
    )
    .await?;
    let id = s(&user, "id");
    if id.is_empty() {
        return Err("Google returned no account identity".into());
    }
    auth["accountId"] = json!(id);
    auth["metadata"] = json!({"email":user["email"]});
    Ok(auth)
}
pub(super) async fn fresh(c: &mut Context) -> Result<()> {
    if c.auth["expires"].as_u64().unwrap_or(0) <= now() + 120000 {
        let _guard = c.refresh_guard()?;
        let v = crate::subscription_import::reader::PrivateAuth(
            json_request(c.client.post(TOKEN).form(&[
                ("grant_type", "refresh_token"),
                ("client_id", CLIENT),
                ("client_secret", CLIENT_KEY),
                ("refresh_token", s(&c.auth, "refresh")),
            ]))
            .await?,
        );
        c.save(device::token(&v.0, &c.auth)?).await?;
    }
    if s(&c.auth["metadata"], "project").is_empty() {
        let v = json_request(
            signed(c, c.client.post(SETUP))?.json(&json!({"metadata":{"ideType":"ANTIGRAVITY"}})),
        )
        .await?;
        let project = v["cloudaicompanionProject"]
            .as_str()
            .or(v["cloudaicompanionProject"]["id"].as_str())
            .filter(|s| !s.is_empty())
            .ok_or("Finish account setup in Antigravity, then reconnect")?;
        let mut auth = c.auth.clone();
        auth["metadata"]["project"] = json!(project);
        auth["metadata"]["plan"] = v["paidTier"]["name"]
            .as_str()
            .or(v["currentTier"]["name"].as_str())
            .map(|s| json!(s))
            .unwrap_or(Value::Null);
        auth["metadata"]["tier"] = v["paidTier"]["id"]
            .as_str()
            .or(v["currentTier"]["id"].as_str())
            .map(|s| json!(s))
            .unwrap_or(Value::Null);
        c.save(auth).await?;
    }
    Ok(())
}
async fn catalog(c: &Context) -> Result<Value> {
    let mut body = json!({"project":c.auth["metadata"]["project"]});
    if let Some(tier) = c.auth["metadata"]["tier"]
        .as_str()
        .filter(|s| !s.is_empty())
    {
        body["entitlement"] = json!({"userTier":tier});
    }
    json_request(signed(c, c.client.post(format!("{BASE}:fetchAvailableModels")))?.json(&body))
        .await
}
pub(super) async fn models(c: &Context) -> Result<Value> {
    parse_models(&catalog(c).await?)
}
fn parse_models(v: &Value) -> Result<Value> {
    let rows = v["models"]
        .as_object()
        .ok_or("Antigravity returned no model catalog")?;
    let mut out = json!({});
    for (id, row) in rows.iter().take(4096) {
        if row["disabled"] == true || id.starts_with("chat_") || id.starts_with("tab_") {
            continue;
        }
        let mut m = model(
            id,
            row["displayName"].as_str().unwrap_or(id),
            "gemini",
            BASE,
            row["maxTokens"].as_u64().unwrap_or(0),
            row["maxOutputTokens"].as_u64().unwrap_or(0),
        );
        m["attachment"] = json!(row["supportsImages"] == true);
        out[id] = m;
    }
    if out.as_object().is_none_or(|m| m.is_empty()) {
        return Err("Antigravity returned no available models".into());
    }
    Ok(out)
}
pub(super) async fn usage(c: &Context) -> Result<Value> {
    let v = catalog(c).await?;
    let mut windows = vec![];
    for (id, row) in v["models"]
        .as_object()
        .ok_or("Antigravity returned no allowance")?
    {
        if row["disabled"] == true || !c.models[id].is_object() {
            continue;
        }
        if let Some(left) = device::percent(&row["quotaInfo"]["remainingFraction"])
            .filter(|n| (0.0..=1.0).contains(n))
        {
            windows.push(json!({"name":row["displayName"].as_str().unwrap_or(id),"models":[id],"span":18000,"used":(100.*(1.-left)).clamp(0.,100.),"resetsAt":device::date(&row["quotaInfo"]["resetTime"])}));
        }
    }
    if windows.is_empty() {
        return Err("Antigravity returned no model allowance data".into());
    }
    Ok(json!({"plan":c.auth["metadata"]["plan"],"windows":windows}))
}
fn envelope(c: &Context, mut body: Value) -> Result<Value> {
    let model = body
        .as_object_mut()
        .ok_or("Invalid Antigravity request")?
        .remove("model")
        .ok_or("Antigravity model is required")?;
    let hash = sha2::Sha256::digest(c.session.as_bytes());
    let session = u64::from_be_bytes(hash[..8].try_into().unwrap()) & i64::MAX as u64;
    body["sessionId"] = json!(format!("-{session}"));
    Ok(
        json!({"model":model,"project":c.auth["metadata"]["project"],"userAgent":"antigravity","requestType":"agent","requestId":format!("agent-{}",hex_id()),"request":body}),
    )
}
pub(super) async fn generate(c: &mut Context, _model: &Value, body: Value) -> Result<()> {
    c.pipe(
        send(
            signed(
                c,
                c.client
                    .post(format!("{BASE}:streamGenerateContent?alt=sse")),
            )?
            .json(&envelope(c, body)?),
        )
        .await?,
    )
    .await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn envelope_binds_project_and_stable_session_without_rewriting_tools() {
        let (mut c, _, _) = super::super::tests::context(Client::new());
        c.auth = json!({"metadata":{"project":"project-a"}});
        let request = json!({"model":"gemini","contents":[{"role":"user","parts":[{"text":"hello"}]}],"tools":[{"functionDeclarations":[{"name":"lookup","parameters":{"type":"OBJECT"}}]}]});
        let first = envelope(&c, request.clone()).unwrap();
        let second = envelope(&c, request.clone()).unwrap();
        assert_eq!(first["project"], "project-a");
        assert_eq!(
            first["request"]["sessionId"],
            second["request"]["sessionId"]
        );
        assert_ne!(first["requestId"], second["requestId"]);
        assert_eq!(first["request"]["tools"], request["tools"]);
        assert!(first["request"].get("model").is_none());
        c.session = "another-conversation".into();
        assert_ne!(
            first["request"]["sessionId"],
            envelope(&c, request).unwrap()["request"]["sessionId"]
        );
    }
    #[test]
    fn disabled_and_completion_models_are_not_published() {
        let models=parse_models(&json!({"models":{"gemini":{"displayName":"Gemini","supportsImages":true},"tab_internal":{},"disabled":{"disabled":true}}})).unwrap();
        assert_eq!(models.as_object().unwrap().len(), 1);
        assert_eq!(models["gemini"]["wire"], "gemini");
    }
}
