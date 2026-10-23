//! Cloud key gateways, distinct from local Ollama and fixed-price plans.
use super::*;
mod quota;
pub(super) const PROVIDERS: &[&str] = &["ollama-cloud", "kilo-gateway"];
fn base(c: &Context) -> &'static str {
    if c.provider == "ollama-cloud" {
        "https://ollama.com/v1"
    } else {
        "https://ai-gateway.kilo.ai/api/v1"
    }
}
fn organization(c: &Context) -> Result<&str> {
    let org = c.auth["metadata"]["organization"]
        .as_str()
        .unwrap_or("")
        .trim();
    if org.len() > 128
        || org
            .bytes()
            .any(|b| !b.is_ascii_alphanumeric() && b != b'-' && b != b'_')
    {
        return Err("Invalid Kilo organization ID".into());
    }
    Ok(org)
}
fn signed(c: &Context, r: RequestBuilder) -> Result<RequestBuilder> {
    let mut r = r
        .bearer_auth(c.token()?)
        .header("User-Agent", concat!("AIPass/", env!("CARGO_PKG_VERSION")));
    if c.provider == "kilo-gateway" {
        let org = organization(c)?;
        if !org.is_empty() {
            r = r.header("X-KiloCode-OrganizationId", org);
        }
        r = r.header("X-KiloCode-TaskId", &c.session);
    }
    Ok(r)
}
pub(super) async fn fresh(c: &mut Context) -> Result<()> {
    if c.auth["type"] != "api" || c.token()?.trim().is_empty() {
        return Err("A cloud API key is required".into());
    }
    let org = if c.provider == "kilo-gateway" {
        organization(c)?
    } else {
        ""
    };
    let identity = format!(
        "{}:{org}:key-{:x}",
        c.provider,
        sha2::Sha256::digest(c.token()?.as_bytes())
    );
    if let Some(previous) = c.auth["accountId"].as_str() {
        if previous != identity {
            return Err("The gateway key or organization changed; reconnect explicitly".into());
        }
    } else {
        let mut next = c.auth.clone();
        next["accountId"] = json!(identity);
        c.save(next).await?;
    }
    Ok(())
}
fn catalog(provider: &str, v: &Value) -> Result<Value> {
    let cloud = provider == "ollama-cloud";
    let rows = v[if cloud { "models" } else { "data" }]
        .as_array()
        .ok_or("Gateway returned no model catalog")?;
    let mut out = json!({});
    for row in rows.iter().take(4096) {
        let id = s(row, if cloud { "name" } else { "id" });
        if id.is_empty() || id.len() > 256 || id.chars().any(char::is_control) {
            continue;
        }
        let caps = row["capabilities"].as_array();
        if cloud && caps.is_some_and(|a| !a.iter().any(|v| v == "completion")) {
            continue;
        }
        if !cloud
            && row["architecture"]["output_modalities"]
                .as_array()
                .is_some_and(|a| !a.iter().any(|v| v == "text"))
        {
            continue;
        }
        let mut m = model(
            id,
            row["name"].as_str().unwrap_or(id),
            "chat",
            "",
            row["context_length"]
                .as_u64()
                .or(row["top_provider"]["context_length"].as_u64())
                .unwrap_or(0),
            row["top_provider"]["max_completion_tokens"]
                .as_u64()
                .unwrap_or(0),
        );
        if cloud {
            m["tool_call"] = json!(caps.is_some_and(|a| a.iter().any(|v| v == "tools")));
            m["reasoning"] = json!(caps.is_some_and(|a| a.iter().any(|v| v == "thinking")));
            m["attachment"] = json!(caps.is_some_and(|a| a.iter().any(|v| v == "vision")));
        } else {
            m["tool_call"] = json!(row["supported_parameters"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v == "tools")));
            m["attachment"] = json!(row["architecture"]["input_modalities"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v == "image")));
        }
        out[id] = m;
    }
    if out.as_object().is_none_or(|m| m.is_empty()) {
        return Err("Gateway returned no chat models".into());
    }
    Ok(out)
}
pub(super) async fn models(c: &Context) -> Result<Value> {
    let url = if c.provider == "ollama-cloud" {
        "https://ollama.com/api/tags".to_owned()
    } else {
        format!("{}/models", base(c))
    };
    let v = json_request(signed(c, c.client.get(url))?).await?;
    catalog(&c.provider, &v)
}
pub(super) async fn usage(c: &Context) -> Result<Value> {
    if c.provider == "ollama-cloud" {
        let value =
            json_request(signed(c, c.client.get("https://ollama.com/api/balance"))?).await?;
        return quota::ollama(&value);
    }
    Ok(json!({"plan":"Kilo Gateway","windows":[],"quotaUnavailable":true}))
}
pub(super) async fn generate(c: &mut Context, _: &Value, body: Value) -> Result<()> {
    c.pipe(
        send(signed(c, c.client.post(format!("{}/chat/completions", base(c))))?.json(&body))
            .await?,
    )
    .await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_cloud_names_are_preserved_and_non_chat_models_are_filtered() {
        let v=catalog("ollama-cloud",&json!({"models":[{"name":"gemma4:31b","capabilities":["completion","tools","vision"]},{"name":"embed","capabilities":["embedding"]}]})).unwrap();
        assert_eq!(v.as_object().unwrap().len(), 1);
        assert_eq!(v["gemma4:31b"]["tool_call"], true);
        assert!(v.get("gemma4:cloud").is_none());
        let v=catalog("kilo-gateway",&json!({"data":[{"id":"vendor/model:free","name":"Model","supported_parameters":["tools"],"architecture":{"output_modalities":["text"]}},{"id":"image","architecture":{"output_modalities":["image"]}}]})).unwrap();
        assert_eq!(v.as_object().unwrap().len(), 1);
        assert_eq!(v["vendor/model:free"]["wire"], "chat");
    }
    #[test]
    fn organization_and_stable_task_are_sent_only_to_kilo_with_our_identity() {
        let (mut c, _, _) = super::super::tests::context(Client::new());
        c.provider = "kilo-gateway".into();
        c.auth = json!({"type":"api","key":"synthetic","metadata":{"organization":"org-a"}});
        let r = signed(&c, c.client.post(base(&c)))
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(r.headers()["X-KiloCode-OrganizationId"], "org-a");
        assert_eq!(r.headers()["X-KiloCode-TaskId"], "test-session");
        c.auth["metadata"]["organization"] = json!("org-a\r\nX-Injected: true");
        assert!(signed(&c, c.client.post(base(&c))).is_err());
        c.provider = "ollama-cloud".into();
        let r = signed(&c, c.client.post(base(&c)))
            .unwrap()
            .build()
            .unwrap();
        assert!(!r.headers().contains_key("X-KiloCode-OrganizationId"));
    }
}
