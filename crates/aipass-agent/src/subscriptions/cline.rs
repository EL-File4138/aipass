//! Cline device accounts and API keys, including ClinePass models.
use super::*;
const API: &str = "https://api.cline.bot/api/v1";
const CLIENT: &str = "client_01K3A541FN8TA3EPPHTD2325AR";

fn headers(request: RequestBuilder, session: &str) -> RequestBuilder {
    request
        .header("HTTP-Referer", "https://cline.bot")
        .header("X-Title", "Cline")
        .header("User-Agent", "Cline/3.0.68")
        .header("X-CLIENT-TYPE", "cline-cli")
        .header("X-CLIENT-VERSION", "3.0.68")
        .header("X-PLATFORM", "cli")
        .header("X-PLATFORM-VERSION", "3.0.68")
        .header("X-CORE-VERSION", "0.0.90")
        .header("X-IS-MULTIROOT", "false")
        .header("X-Task-ID", session)
}
fn signed(c: &Context, request: RequestBuilder) -> Result<RequestBuilder> {
    let token = if c.auth["type"] == "oauth" {
        format!("workos:{}", c.token()?)
    } else {
        c.token()?.to_owned()
    };
    Ok(headers(request, &c.session).bearer_auth(token))
}
async fn api(request: RequestBuilder) -> Result<Value> {
    let v = json_request(request).await?;
    if v["success"] == false {
        return Err("Cline could not complete the account request".into());
    }
    Ok(if v.get("success").is_some() {
        v["data"].clone()
    } else {
        v
    })
}
fn token(v: &Value, previous: &Value) -> Result<Value> {
    let access = s(v, "accessToken");
    let refresh = v["refreshToken"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(s(previous, "refresh"));
    if access.is_empty() || refresh.is_empty() {
        return Err("Cline returned incomplete credentials".into());
    }
    let user = &v["userInfo"];
    let id = user["clineUserId"]
        .as_str()
        .or(user["subject"].as_str())
        .or(previous["accountId"].as_str())
        .filter(|v| !v.is_empty())
        .ok_or("Cline returned no account identity")?;
    let expiry = device::date(&v["expiresAt"])
        .and_then(|s| {
            time::OffsetDateTime::parse(&s, &time::format_description::well_known::Rfc3339).ok()
        })
        .map(|t| t.unix_timestamp().max(0) as u64 * 1000)
        .unwrap_or_else(|| expires(access));
    if expiry <= now() {
        return Err("Cline returned an expired token or no token expiry".into());
    }
    let mut next = previous.clone();
    next["type"] = json!("oauth");
    next["access"] = json!(access);
    next["refresh"] = json!(refresh);
    next["expires"] = json!(expiry);
    next["accountId"] = json!(id);
    if let Some(email) = user["email"].as_str().filter(|s| !s.is_empty()) {
        next["metadata"] = json!({"email":email});
    }
    Ok(next)
}
pub(super) async fn login(c: &mut Context, _method: usize) -> Result<Value> {
    let d = json_request(
        c.client
            .post("https://api.workos.com/user_management/authorize/device")
            .form(&[("client_id", CLIENT)]),
    )
    .await?;
    if s(&d, "device_code").is_empty() {
        return Err("Cline returned no device authorization".into());
    }
    device::authorize(c, &d).await?;
    let fields = vec![
        ("grant_type".into(), device::GRANT.into()),
        ("client_id".into(), CLIENT.into()),
        ("device_code".into(), s(&d, "device_code").into()),
    ];
    let grant = crate::subscription_import::reader::PrivateAuth(
        device::poll(
            c,
            "https://api.workos.com/user_management/authenticate",
            device::Fields(fields),
            d["expires_in"].as_u64().unwrap_or(300),
            d["interval"].as_u64().unwrap_or(5) * 1000,
        )
        .await?,
    );
    // Registration trades a single-use grant. Cancellation waits for its result.
    let _guard = c.refresh_guard()?;
    let v = api(
        headers(c.client.post(format!("{API}/auth/register")), &c.session).json(
            &json!({"accessToken":grant.0["access_token"],"refreshToken":grant.0["refresh_token"]}),
        ),
    )
    .await?;
    token(&v, &json!({}))
}
pub(super) async fn fresh(c: &mut Context) -> Result<()> {
    if c.auth["type"] == "api" {
        let user = api(signed(c, c.client.get(format!("{API}/users/me")))?).await?;
        let id = user["clineUserId"]
            .as_str()
            .or(user["subject"].as_str())
            .or(user["id"].as_str())
            .filter(|s| !s.is_empty())
            .ok_or("Cline returned no account identity")?;
        let mut next = c.auth.clone();
        next["accountId"] = json!(id);
        if let Some(email) = user["email"].as_str() {
            next["metadata"]["email"] = json!(email);
        }
        crate::community::ensure_owner(&c.auth, &next)?;
        if next != c.auth {
            c.save(next).await?;
        }
        return Ok(());
    }
    if c.auth["expires"].as_u64().unwrap_or(0) > now() + 300000 {
        return Ok(());
    }
    let _guard = c.refresh_guard()?;
    let v = api(
        headers(c.client.post(format!("{API}/auth/refresh")), &c.session)
            .json(&json!({"refreshToken":c.auth["refresh"],"grantType":"refresh_token"})),
    )
    .await?;
    c.save(token(&v, &c.auth)?).await
}
pub(super) async fn models(c: &Context) -> Result<Value> {
    let cloud = api(headers(
        c.client.get(format!("{API}/ai/cline/models")),
        &c.session,
    ))
    .await?;
    let feed = api(headers(
        c.client.get(format!("{API}/ai/cline/recommended-models")),
        &c.session,
    ))
    .await?;
    parse_models(&cloud, &feed)
}
fn parse_models(cloud: &Value, feed: &Value) -> Result<Value> {
    let mut out = json!({});
    for m in cloud["data"]
        .as_array()
        .or(cloud.as_array())
        .into_iter()
        .flatten()
        .take(4096)
    {
        let id = s(m, "id");
        if id.is_empty() {
            continue;
        }
        let mut item = model(
            id,
            m["name"].as_str().unwrap_or(id),
            "chat",
            API,
            m["context_length"].as_u64().unwrap_or(0),
            0,
        );
        item["attachment"] = json!(m["architecture"]["input_modalities"]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v == "image")));
        out[id] = item;
    }
    for group in ["recommended", "free", "clinePass"] {
        for m in feed[group].as_array().into_iter().flatten().take(4096) {
            let id = m.as_str().or(m["id"].as_str()).unwrap_or("");
            if id.is_empty() {
                continue;
            }
            if out[id].is_null() {
                out[id] = model(id, m["name"].as_str().unwrap_or(id), "chat", API, 0, 0);
            }
            if group == "clinePass" {
                out[id]["subscriptionOnly"] = json!(true);
            }
        }
    }
    if out.as_object().is_none_or(|o| o.is_empty()) {
        return Err("Cline returned no models".into());
    }
    Ok(out)
}
pub(super) async fn usage(c: &Context) -> Result<Value> {
    let user = api(signed(c, c.client.get(format!("{API}/users/me")))?).await?;
    let id = user["clineUserId"]
        .as_str()
        .or(user["subject"].as_str())
        .or(user["id"].as_str())
        .ok_or("Cline returned no account identity")?;
    if c.auth["accountId"] != id {
        return Err("Cline account changed; reconnect explicitly".into());
    }
    let escaped: String = url::form_urlencoded::byte_serialize(id.as_bytes()).collect();
    let balance = api(signed(
        c,
        c.client.get(format!("{API}/users/{escaped}/balance")),
    )?)
    .await;
    let (status, limits) = raw_json(signed(
        c,
        c.client.get(format!("{API}/users/me/plan/usage-limits")),
    )?)
    .await?;
    if status != 404 && !(200..300).contains(&status) {
        return Err(format!("ClinePass allowance unavailable (HTTP {status})"));
    }
    let limits = if limits.get("success").is_some() {
        &limits["data"]
    } else {
        &limits
    };
    account_usage(&user, balance.as_ref().ok(), limits, &c.models)
}
fn account_usage(
    user: &Value,
    balance: Option<&Value>,
    limits: &Value,
    models: &Value,
) -> Result<Value> {
    let mut value = parse_usage(user, balance, limits);
    let plan_models: Vec<_> = models
        .as_object()
        .into_iter()
        .flat_map(|m| m.iter())
        .filter(|(id, model)| model["subscriptionOnly"] == true || id.starts_with("cline-pass/"))
        .map(|(id, _)| id.clone())
        .collect();
    for window in value["windows"].as_array_mut().into_iter().flatten() {
        window["models"] = json!(plan_models);
    }
    if value["balance"].is_null() && value["windows"].as_array().is_none_or(|a| a.is_empty()) {
        return Err("Cline returned no balance or allowance".into());
    }
    // Missing paid balance is unknown. Valid ClinePass windows stay current;
    // a top-level error would cause Agent routing to discard all of them.
    Ok(value)
}
fn parse_usage(user: &Value, balance: Option<&Value>, limits: &Value) -> Value {
    let mut windows = Vec::new();
    for l in limits["limits"].as_array().into_iter().flatten() {
        let (name, span) = match s(l, "type") {
            "five_hour" => ("5 hours", 18000),
            "weekly" => ("Weekly", 604800),
            "monthly" => ("Monthly", 2592000),
            _ => continue,
        };
        if let Some(n) = device::percent(&l["percentUsed"]).filter(|n| *n >= 0.) {
            windows.push(json!({"name":name,"span":span,"used":n.clamp(0.,100.),"models":[],"notModels":[],"resetsAt":device::date(&l["resetsAt"])}));
        }
    }
    // ClinePass limits apply only to plan models, never the paid API balance.
    for window in &mut windows {
        window["models"] = json!(["cline-pass/*"]);
    }
    let mut out = json!({"windows":windows});
    if let Some(plan) = user["plan"]
        .as_str()
        .or(user["planName"].as_str())
        .or(user["planType"].as_str())
    {
        out["plan"] = json!(plan);
    }
    if let Some(n) = balance
        .and_then(|v| device::percent(&v["balance"]))
        .filter(|n| *n >= 0.)
    {
        out["balance"] = json!(format!("${:.4}", n / 1e6));
    }
    out
}
pub(super) async fn generate(c: &mut Context, _model: &Value, body: Value) -> Result<()> {
    c.pipe(send(signed(c, c.client.post(format!("{API}/chat/completions")))?.json(&body)).await?)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plan_feed_complements_paid_catalog_and_quota_is_scoped() {
        let m = parse_models(
            &json!({"data":[{"id":"paid/model","context_length":100000}]}),
            &json!({"clinePass":[{"id":"cline-pass/one"}],"free":["cline-free/two"]}),
        )
        .unwrap();
        assert!(m.get("paid/model").is_some());
        assert!(m.get("cline-free/two").is_some());
        assert_eq!(m["cline-pass/one"]["subscriptionOnly"], true);
        let v = parse_usage(
            &json!({"plan":"ClinePass"}),
            Some(&json!({"balance":2500000})),
            &json!({"limits":[{"type":"weekly","percentUsed":0},{"type":"monthly"},{"type":"five_hour","percentUsed":-1}]}),
        );
        assert_eq!(v["balance"], "$2.5000");
        assert_eq!(v["windows"].as_array().unwrap().len(), 1);
        assert_eq!(v["windows"][0]["models"][0], "cline-pass/*");
    }
    #[test]
    fn rotated_tokens_keep_identity_and_require_expiry() {
        let before = json!({"type":"oauth","accountId":"alice","refresh":"old"});
        let next=token(&json!({"accessToken":"new","refreshToken":"rotated","expiresAt":"2030-01-01T00:00:00Z","userInfo":{"clineUserId":"bob"}}),&before).unwrap();
        assert!(crate::community::ensure_owner(&before, &next).is_err());
        assert!(token(
            &json!({"accessToken":"new","userInfo":{"clineUserId":"alice"}}),
            &before
        )
        .is_err());
        assert!(token(
            &json!({"accessToken":"expired","expiresAt":"2020-01-01T00:00:00Z"}),
            &before
        )
        .is_err());
    }
    #[test]
    fn allowance_scope_uses_catalog_membership_and_never_expands_to_paid_models() {
        let models = parse_models(
            &json!({"data":[{"id":"paid/model"}]}),
            &json!({"clinePass":[{"id":"subscription/model"}]}),
        )
        .unwrap();
        let usage = account_usage(
            &json!({}),
            None,
            &json!({"limits":[{"type":"weekly","percentUsed":100}]}),
            &models,
        )
        .unwrap();
        assert_eq!(usage["windows"][0]["models"], json!(["subscription/model"]));
        assert!(usage["error"].is_null());
        assert_eq!(usage["windows"][0]["used"].as_f64(), Some(100.));
        assert!(usage["balance"].is_null());
        let usage = account_usage(
            &json!({}),
            None,
            &json!({"limits":[{"type":"weekly","percentUsed":100}]}),
            &json!({"paid/model":{}}),
        )
        .unwrap();
        // An explicit empty model set matches no model in proxy routing.
        assert_eq!(usage["windows"][0]["models"], json!([]));
        assert!(account_usage(&json!({}), None, &json!({}), &models).is_err());
        assert!(account_usage(
            &json!({}),
            Some(&json!({"balance":-1})),
            &json!({}),
            &models
        )
        .is_err());
    }
}
