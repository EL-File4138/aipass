//! Subscription HTTP contracts using immediate CLI access.
use super::*;

pub(crate) async fn generate(c: &mut Context, model: &Value, mut body: Value) -> Result<()> {
    if c.provider == "codex" {
        // Attributes can have coarse timestamp resolution. Never submit a paid
        // generation using a completed discovery cache entry.
        super::session::fresh_for_generation(c).await?;
    }
    if c.provider == "codex" && !super::reset::admit(c).await? {
        return Ok(());
    }
    let req = match c.provider.as_str() {
        "codex" => {
            body = aipass_proxy::prepare_codex_subscription(body)?;
            let mut headers = reqwest::header::HeaderMap::new();
            if let Ok(session) = reqwest::header::HeaderValue::from_str(&c.session) {
                headers.insert("session-id", session.clone());
                headers.insert("thread-id", session);
            }
            aipass_proxy::codex_subscription_headers(&mut headers, body.to_string().as_bytes());
            signed(c, c.client.post(format!("{CODEX}/responses")))?
                .headers(headers)
                .json(&body)
        }
        "copilot" => {
            let r = signed(
                c,
                c.client.post(format!(
                    "{}{}",
                    copilot_api(c).await?,
                    path(s(model, "wire"))
                )),
            )?
            .header("x-request-id", uuid::Uuid::new_v4().to_string())
            .headers(aipass_proxy::copilot_request_headers(&body));
            let r = if s(model, "wire") == "anthropic" {
                r.header("anthropic-version", "2023-06-01")
            } else {
                r
            };
            r.json(&body)
        }
        "gemini-cli" => {
            let model = s(&body, "model").to_owned();
            body.as_object_mut()
                .ok_or("invalid Gemini request")?
                .remove("model");
            signed(c, c.client.post(format!("{GEMINI}:streamGenerateContent?alt=sse")))?
                .json(&json!({"model":model,"project":project(c).await?,"user_prompt_id":c.session,"request":body}))
        }
        _ => return Err("unknown CLI subscription".into()),
    };
    c.pipe(send(req).await?).await
}

const CODEX: &str = "https://chatgpt.com/backend-api/codex";
const GEMINI: &str = "https://cloudcode-pa.googleapis.com/v1internal";

pub(super) fn signed(c: &Context, r: RequestBuilder) -> Result<RequestBuilder> {
    let mut r = r.bearer_auth(c.token()?);
    if c.provider == "codex" {
        let mut headers = reqwest::header::HeaderMap::new();
        aipass_proxy::codex_subscription_headers(&mut headers, b"{}");
        r = r
            .header("chatgpt-account-id", &c.native_workspace)
            .headers(headers);
    }
    if c.provider == "copilot" {
        r = r.headers(aipass_proxy::copilot_cli_headers());
    }

    Ok(r)
}

pub(crate) async fn models(c: &mut Context) -> Result<Value> {
    if c.provider == "gemini-cli" {
        let quota = google(c, "retrieveUserQuota", json!({"project":project(c).await?})).await?;
        let mut models = json!({});
        for b in quota["buckets"].as_array().into_iter().flatten() {
            let id = s(b, "modelId");
            if !id.is_empty() {
                models[id] = model(id, id, "gemini", GEMINI, 1000000, 65536);
            }
        }
        return Ok(models);
    }
    let api = if c.provider == "codex" {
        CODEX.to_owned()
    } else {
        copilot_api(c).await?
    };
    let url = if c.provider == "codex" {
        aipass_proxy::codex_subscription_models_url(CODEX, &reqwest::header::HeaderMap::new())?
    } else {
        format!("{api}/models")
    };
    let v = json_request(signed(c, c.client.get(url))?).await?;
    let mut out = json!({});
    let list = v["models"]
        .as_array()
        .or(v["data"].as_array())
        .ok_or("CLI backend returned no models")?;
    for m in list {
        let id = m["slug"].as_str().or(m["id"].as_str()).unwrap_or("");
        if id.is_empty() {
            continue;
        }
        if c.provider == "codex" && m["visibility"] == "hide" {
            continue;
        }
        let wire = if c.provider == "codex" {
            "responses"
        } else {
            let Some(wire) = super::copilot::wire(m) else {
                continue;
            };
            wire
        };
        out[id] = model(
            id,
            m["display_name"]
                .as_str()
                .or(m["name"].as_str())
                .unwrap_or(id),
            wire,
            &api,
            m["context_window"]
                .as_u64()
                .or_else(|| {
                    m.pointer("/capabilities/limits/max_context_window_tokens")
                        .and_then(Value::as_u64)
                })
                .unwrap_or(0),
            m.pointer("/capabilities/limits/max_output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        );
        if c.provider == "copilot" {
            out[id]["premium"] = json!(m.pointer("/billing/is_premium") == Some(&json!(true)));
            out[id]["attachment"] =
                json!(m.pointer("/capabilities/supports/vision") == Some(&json!(true)));
            if let Some(multiplier) = m
                .pointer("/billing/multiplier")
                .and_then(Value::as_f64)
                .filter(|n| n.is_finite() && *n >= 0.0)
            {
                out[id]["premiumMultiplier"] = json!(multiplier);
            }
        }
    }
    Ok(out)
}

pub(super) fn copilot_user_request(c: &Context) -> Result<RequestBuilder> {
    let request = signed(
        c,
        c.client.get(super::copilot::user_url(&c.native_workspace)),
    )?;
    // Replace, rather than append a second Authorization header.
    let mut headers = reqwest::header::HeaderMap::new();
    let mut authorization =
        reqwest::header::HeaderValue::from_str(&format!("token {}", c.token()?))
            .map_err(|_| "invalid CLI access token")?;
    authorization.set_sensitive(true);
    headers.insert(reqwest::header::AUTHORIZATION, authorization);
    Ok(request.headers(headers))
}

pub(super) async fn copilot_api(c: &Context) -> Result<String> {
    let v = json_request(copilot_user_request(c)?).await?;
    super::copilot::api(
        v.pointer("/endpoints/api").and_then(Value::as_str),
        &c.native_workspace,
    )
}

pub(super) async fn google(c: &Context, method: &str, body: Value) -> Result<Value> {
    json_request(
        signed(c, c.client.post(format!("{GEMINI}:{method}")))?
            .header("User-Agent", "GeminiCLI/0.34.0")
            .json(&body),
    )
    .await
}
pub(super) async fn project(c: &Context) -> Result<String> {
    let v = google(
        c,
        "loadCodeAssist",
        super::gemini::setup(&c.native_workspace),
    )
    .await?;
    let id = v["cloudaicompanionProject"].as_str().or(v
        .pointer("/cloudaicompanionProject/id")
        .and_then(Value::as_str));
    if let Some(id) = id.filter(|s| !s.is_empty()) {
        return Ok(id.into());
    }
    if !c.native_workspace.is_empty() && v["currentTier"].is_object() {
        return Ok(c.native_workspace.clone());
    }
    // Let the official CLI perform entitlement onboarding. Never invent a project.
    Err("Gemini Code Assist has no project yet; finish setup in Gemini CLI and reconnect".into())
}

pub(crate) async fn usage(c: &mut Context) -> Result<Value> {
    if c.provider == "gemini-cli" {
        let v = google(c, "retrieveUserQuota", json!({"project":project(c).await?})).await?;
        let windows = v["buckets"].as_array().into_iter().flatten().filter_map(|b|b["remainingFraction"].as_f64().map(|f|json!({"name":b["modelId"],"models":[b["modelId"]],"used":100.0*(1.0-f),"resetsAt":b["resetTime"]}))).collect::<Vec<_>>();
        if windows.is_empty() {
            return Err("Gemini returned no allowance data".into());
        }
        return Ok(json!({"windows":windows}));
    }
    if c.provider == "copilot" {
        let v = json_request(copilot_user_request(c)?).await?;
        return copilot_usage(&v, &c.models);
    }

    codex_usage(&read_limits(c).await?)
}

pub(super) async fn read_limits(c: &mut Context) -> Result<Value> {
    let path = root(&c.auth)?;
    let mut cmd = command(c, "codex", &path)?;
    cmd.arg("app-server");
    let v = rpc(cmd, c, "account/rateLimits/read", json!(null), false, false).await?;
    if v["accountId"]
        .as_str()
        .is_some_and(|id| id != c.native_workspace)
    {
        return Err("Codex quota belongs to a different workspace".into());
    }
    Ok(v)
}
