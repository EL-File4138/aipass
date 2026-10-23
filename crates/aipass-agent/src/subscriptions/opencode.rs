//! OpenCode Go and Zen inference. Model API families are explicit metadata.
use super::*;
fn go(c: &Context) -> bool {
    c.provider == "opencode-go"
}
fn root(c: &Context) -> &'static str {
    if go(c) {
        "https://opencode.ai/inference/go"
    } else {
        "https://opencode.ai/inference"
    }
}
fn signed(c: &Context, r: RequestBuilder) -> Result<RequestBuilder> {
    Ok(
        r.header("User-Agent", concat!("AIPass/", env!("CARGO_PKG_VERSION")))
            .header("x-opencode-session", &c.session)
            .bearer_auth(c.token()?),
    )
}
fn key_identity(key: &str) -> String {
    format!("key-{:x}", sha2::Sha256::digest(key.as_bytes()))
}
pub(super) fn initial(provider: &str) -> Result<Value> {
    let v: Value = serde_json::from_str(include_str!("opencode_models.json"))
        .map_err(|_| "Invalid OpenCode model metadata")?;
    Ok(v[if provider == "opencode-go" {
        "opencode-go"
    } else {
        "opencode"
    }]
    .clone())
}
pub(super) async fn login(c: &mut Context, _method: usize) -> Result<Value> {
    let path = native_import::root(&c.auth)
        .map(Ok)
        .unwrap_or_else(directory)?
        .join("auth.json");
    let data = crate::subscription_import::reader::PrivateAuth(read_json(&path)?);
    let id = if go(c) { "opencode-go" } else { "opencode" };
    let key = s(&data.0[id], "key");
    if key.is_empty() {
        return Err("Connect this service in OpenCode first, then import its API key".into());
    }
    Ok(json!({"type":"api","key":key,"accountId":key_identity(key)}))
}
pub(super) fn directory() -> Result<std::path::PathBuf> {
    Ok(aipass_config_writers::xdg_data_home(&home()?).join("opencode"))
}
pub(super) async fn fresh(c: &mut Context) -> Result<()> {
    if c.token()? == "public" {
        return Err(
            "OpenCode's public free tier requires OpenCode; connect with a Console API key".into(),
        );
    }
    if native_import::root(&c.auth).is_some() {
        let mut next = login(c, 1).await?;
        crate::community::ensure_owner(&c.auth, &next)?;
        native_import::erase(&mut next);
    }
    if c.auth["accountId"].is_null() {
        let mut next = c.auth.clone();
        next["accountId"] = json!(key_identity(c.token()?));
        c.save(next).await?;
    }
    Ok(())
}
fn wire(row: &Value, known: &Value) -> Option<&'static str> {
    let npm = row["provider"]["npm"]
        .as_str()
        .or(row["api"]["npm"].as_str());
    match npm {
        Some("@ai-sdk/anthropic") => return Some("anthropic"),
        Some("@ai-sdk/openai") => return Some("responses"),
        Some("@ai-sdk/google") => return Some("gemini"),
        Some("@ai-sdk/openai-compatible") => return Some("chat"),
        _ => {}
    }
    for endpoint in row["supported_endpoints"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if endpoint.ends_with("/responses") {
            return Some("responses");
        }
        if endpoint.ends_with("/messages") {
            return Some("anthropic");
        }
        if endpoint.ends_with("/chat/completions") {
            return Some("chat");
        }
        if endpoint.contains("generateContent") {
            return Some("gemini");
        }
    }
    match s(known, "wire") {
        "anthropic" => Some("anthropic"),
        "responses" => Some("responses"),
        "gemini" => Some("gemini"),
        "chat" => Some("chat"),
        _ => None,
    }
}
pub(super) async fn models(c: &Context) -> Result<Value> {
    let value = json_request(signed(c, c.client.get(format!("{}/v1/models", root(c))))?).await?;
    let mut known = initial(&c.provider)?;
    // Keep last verified provider-specific metadata while refreshing new IDs.
    for (id, m) in c.models.as_object().into_iter().flatten() {
        if wire(&Value::Null, m).is_some() {
            known[id] = m.clone();
        }
    }
    if let Ok(metadata) = json_request(c.client.get("https://models.dev/api.json")).await {
        merge_metadata(
            &mut known,
            &metadata,
            if go(c) { "opencode-go" } else { "opencode" },
        );
    }
    parse_models(&value, &known)
}
fn parse_models(v: &Value, known: &Value) -> Result<Value> {
    let mut out = json!({});
    if v["has_more"] == true || v["data"].as_array().is_some_and(|a| a.len() > 4096) {
        return Err("OpenCode returned an incomplete model catalog".into());
    }
    for row in v["data"]
        .as_array()
        .ok_or("OpenCode returned no model catalog")?
        .iter()
        .take(4096)
    {
        let id = s(row, "id");
        if id.is_empty() {
            continue;
        }
        let Some(wire) = wire(row, &known[id]) else {
            continue;
        };
        let mut m = if known[id].is_object() {
            known[id].clone()
        } else {
            model(
                id,
                row["name"].as_str().unwrap_or(id),
                wire,
                "",
                row["context_length"].as_u64().unwrap_or(0),
                0,
            )
        };
        m["wire"] = json!(wire);
        if row["free"].is_boolean() {
            m["free"] = row["free"].clone();
        }
        if row["cost"].is_object() {
            m["cost"] = row["cost"].clone();
        }
        out[id] = m;
    }
    if out.as_object().is_none_or(|m| m.is_empty()) {
        return Err("OpenCode returned no supported models; refresh after updating AIPass".into());
    }
    Ok(out)
}
fn merge_metadata(known: &mut Value, feed: &Value, provider: &str) {
    let catalog = &feed[provider];
    for (id, row) in catalog["models"]
        .as_object()
        .into_iter()
        .flatten()
        .take(4096)
    {
        if id.is_empty() || id.len() > 256 {
            continue;
        }
        // Model-level npm overrides the catalog default. Endpoints are metadata
        // only: all requests still use AIPass's fixed inference host.
        let npm = row["provider"]["npm"]
            .as_str()
            .or(catalog["npm"].as_str())
            .unwrap_or("");
        let Some(family) = wire(&json!({"provider":{"npm":npm}}), &Value::Null) else {
            continue;
        };
        let mut m = model(
            id,
            row["name"].as_str().unwrap_or(id),
            family,
            "",
            row["limit"]["context"].as_u64().unwrap_or(0),
            row["limit"]["output"].as_u64().unwrap_or(0),
        );
        for key in [
            "cost",
            "attachment",
            "reasoning",
            "tool_call",
            "modalities",
            "interleaved",
        ] {
            if let Some(value) = row.get(key) {
                m[key] = value.clone();
            }
        }
        known[id] = m;
    }
}
pub(super) async fn usage(c: &Context) -> Result<Value> {
    if !go(c) {
        return Ok(json!({"plan":"OpenCode Zen","windows":[],"quotaUnavailable":true}));
    }
    // Inference moved to /inference/go; billing remains on the Zen API.
    let value = json_request(signed(
        c,
        c.client.get("https://opencode.ai/zen/go/v1/usage"),
    )?)
    .await?;
    let windows = super::plan_quota::opencode(&value, &c.models)?;
    Ok(json!({"plan":"OpenCode Go","windows":windows}))
}

fn endpoint(base: &str, wire: &str, id: &str) -> Result<String> {
    if id.is_empty()
        || id
            .bytes()
            .any(|b| !b.is_ascii_alphanumeric() && !b"-_.".contains(&b))
    {
        return Err("Invalid OpenCode model ID".into());
    }
    Ok(match wire {
        "chat" => format!("{base}/openai/v1/chat/completions"),
        "responses" => format!("{base}/openai/v1/responses"),
        "anthropic" => format!("{base}/anthropic/v1/messages"),
        "gemini" => format!("{base}/google/v1beta/models/{id}:streamGenerateContent?alt=sse"),
        _ => return Err("Unknown OpenCode model API".into()),
    })
}
pub(super) async fn generate(c: &mut Context, m: &Value, mut body: Value) -> Result<()> {
    let wire = s(m, "wire");
    let id = s(&body, "model").to_owned();
    let url = endpoint(root(c), wire, &id)?;
    if wire == "gemini" {
        body.as_object_mut()
            .ok_or("Invalid Gemini request")?
            .remove("model");
    }
    let mut request = signed(c, c.client.post(url))?;
    if wire == "anthropic" {
        request = request.header("anthropic-version", "2023-06-01");
    }
    c.pipe(send(request.json(&body)).await?).await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refreshed_metadata_uses_the_matching_catalog_and_never_remote_hosts() {
        let mut known = json!({});
        let feed = json!({"opencode-go":{"npm":"@ai-sdk/openai-compatible","models":{"future":{"name":"Future","provider":{"npm":"@ai-sdk/anthropic","api":"https://untrusted.invalid"},"cost":{"input":0,"output":0}}}},"opencode":{"models":{"zen-only":{"provider":{"npm":"@ai-sdk/google"}}}}});
        merge_metadata(&mut known, &feed, "opencode-go");
        let catalog = parse_models(
            &json!({"data":[{"id":"future"},{"id":"zen-only"},{"id":"unverified"}]}),
            &known,
        )
        .unwrap();
        assert_eq!(catalog["future"]["wire"], "anthropic");
        assert!(catalog.get("zen-only").is_none());
        assert!(catalog.get("unverified").is_none());
        assert!(!catalog.to_string().contains("untrusted.invalid"));
        assert!(super::super::plan_quota::free(&catalog["future"]));
        assert!(endpoint(
            "https://opencode.ai/inference/go",
            s(&catalog["future"], "wire"),
            "future"
        )
        .unwrap()
        .starts_with("https://opencode.ai/inference/go/anthropic/"));
    }
    #[test]
    fn request_identity_is_ours_and_session_is_stable_for_all_api_families() {
        let (mut c, _, _) = super::super::tests::context(Client::new());
        c.auth = json!({"type":"api","key":"fake-key"});
        c.provider = "opencode-go".into();
        for wire in ["chat", "responses", "anthropic", "gemini"] {
            let request = signed(&c, c.client.post(endpoint(root(&c), wire, "one").unwrap()))
                .unwrap()
                .build()
                .unwrap();
            assert_eq!(request.headers()["x-opencode-session"], "test-session");
            assert!(request.headers()["User-Agent"]
                .to_str()
                .unwrap()
                .starts_with("AIPass/"));
            assert_eq!(request.headers()["Authorization"], "Bearer fake-key");
            assert!(request.url().path().starts_with("/inference/go/"));
        }
    }
    #[test]
    fn same_model_uses_each_services_own_protocol_and_unknown_is_not_guessed() {
        let go = initial("opencode-go").unwrap();
        let zen = initial("opencode-zen").unwrap();
        assert_eq!(go["minimax-m3"]["wire"], "anthropic");
        let v = parse_models(&json!({"data":[{"id":"minimax-m3"},{"id":"future"}]}), &go).unwrap();
        assert_eq!(v["minimax-m3"]["wire"], go["minimax-m3"]["wire"]);
        assert!(v.get("future").is_none());
        assert!(!zen.as_object().unwrap().is_empty());
        assert_eq!(
            endpoint(
                "https://opencode.ai/inference/go",
                "anthropic",
                "minimax-m3"
            )
            .unwrap(),
            "https://opencode.ai/inference/go/anthropic/v1/messages"
        );
        assert!(endpoint("https://opencode.ai/inference", "gemini", "../../other").is_err());
    }
    #[tokio::test]
    async fn public_key_cannot_be_saved_as_a_working_subscription() {
        let (mut c, _, _) = super::super::tests::context(Client::new());
        c.provider = "opencode-zen".into();
        c.auth = json!({"type":"api","key":"public"});
        assert!(fresh(&mut c)
            .await
            .unwrap_err()
            .contains("free tier requires OpenCode"));
    }
}
