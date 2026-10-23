//! MiniMax Code account OAuth and its region-scoped model/allowance APIs.
use super::*;
const CLIENT: &str = "mcode-public";
struct Site {
    account: &'static str,
    llm: &'static str,
    agent: &'static str,
    platform: &'static str,
    region: &'static str,
    lang: &'static str,
}
fn site(provider: &str) -> Result<Site> {
    match provider {
        "minimax-code" => Ok(Site {
            account: "https://account.minimax.cn",
            llm: "https://agent.minimax.cn",
            agent: "https://agent.minimaxi.com",
            platform: "https://www.minimaxi.com",
            region: "cn",
            lang: "zh",
        }),
        "minimax-code-global" => Ok(Site {
            account: "https://account.minimax.io",
            llm: "https://agent.minimax.io",
            agent: "https://agent.minimax.io",
            platform: "https://platform.minimax.io",
            region: "en",
            lang: "en",
        }),
        _ => Err("Unknown MiniMax region".into()),
    }
}
pub(super) async fn login(c: &mut Context, _method: usize) -> Result<Value> {
    let site = site(&c.provider)?;
    let verifier = SecretString::new(format!("{}{}", hex_id(), hex_id()));
    let challenge = URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier.expose().as_bytes()));
    let d = crate::subscription_import::reader::PrivateAuth(
        json_request(
            c.client
                .post(format!("{}/oauth2/device/code", site.account))
                .form(&[
                    ("client_id", CLIENT),
                    ("scope", "agent.default"),
                    ("audience", "agent-backend"),
                    ("code_challenge", &challenge),
                    ("code_challenge_method", "S256"),
                ]),
        )
        .await?,
    );
    device::authorize(c, &d.0).await?;
    let by_user = s(&d.0, "device_code").is_empty();
    let code = if by_user {
        s(&d.0, "user_code")
    } else {
        s(&d.0, "device_code")
    };
    if code.is_empty() {
        return Err("MiniMax returned no device code".into());
    }
    let fields = vec![
        ("grant_type".into(), device::GRANT.into()),
        ("client_id".into(), CLIENT.into()),
        ("code_verifier".into(), verifier.expose().into()),
        (
            if by_user { "user_code" } else { "device_code" }.into(),
            code.into(),
        ),
    ];
    let expiry = d.0["expires_in"]
        .as_u64()
        .or(d.0["expired_in"].as_u64())
        .unwrap_or(600);
    let lifetime = if expiry > 1_000_000_000_000 {
        expiry.saturating_sub(now()) / 1000
    } else {
        expiry
    };
    let interval = d.0["interval"]
        .as_u64()
        .unwrap_or(if by_user { 5000 } else { 5 });
    let reply = crate::subscription_import::reader::PrivateAuth(
        device::poll(
            c,
            &format!("{}/oauth2/token", site.account),
            device::Fields(fields),
            lifetime,
            if by_user { interval } else { interval * 1000 },
        )
        .await?,
    );
    let mut auth = device::token(&reply.0, &json!({}))?;
    let user = account_call(c, &site, &auth, "/v1/api/user/info", None).await?;
    let user = &user["data"];
    let info = if user["userInfo"].is_object() {
        &user["userInfo"]
    } else {
        &user["user_info"]
    };
    let id = info["realUserID"]
        .as_str()
        .or(info["real_user_id"].as_str())
        .filter(|s| !s.is_empty())
        .ok_or("MiniMax returned no account identity")?;
    auth["accountId"] = json!(id);
    auth["metadata"] =
        json!({"userId":id,"email":info["userEmail"].as_str().or(info["email"].as_str())});
    Ok(auth)
}
pub(super) async fn fresh(c: &mut Context) -> Result<()> {
    if c.auth["expires"].as_u64().unwrap_or(0) > now() + 120000 {
        return Ok(());
    }
    let site = site(&c.provider)?;
    let _guard = c.refresh_guard()?;
    let v = crate::subscription_import::reader::PrivateAuth(
        c.refresh_json(
            "refresh",
            c.client
                .post(format!("{}/oauth2/token", site.account))
                .form(&[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", s(&c.auth, "refresh")),
                    ("client_id", CLIENT),
                    ("scope", "agent.default"),
                    ("audience", "agent-backend"),
                ]),
        )
        .await?,
    );
    c.save(device::token(&v.0, &c.auth)?).await
}
pub(super) async fn models(c: &Context) -> Result<Value> {
    let site = site(&c.provider)?;
    let v = json_request(
        c.client
            .get(format!("{}/mavis/api/v1/models", site.llm))
            .query(&[("region", site.region), ("buildEnv", "prod")])
            .bearer_auth(c.token()?),
    )
    .await?;
    parse_models(&v, &format!("{}/mavis/api/v1/llm/v1", site.llm))
}
fn parse_models(v: &Value, base: &str) -> Result<Value> {
    let config = v["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["providerId"] == "minimax")
        .ok_or("MiniMax returned no provider catalog")?;
    let rows = config["config"]["models"]
        .as_object()
        .ok_or("MiniMax returned no models")?;
    let mut out = json!({});
    for (id, row) in rows.iter().take(4096) {
        let mut m = model(
            id,
            row["name"].as_str().unwrap_or(id),
            "anthropic",
            base,
            row["limit"]["context"].as_u64().unwrap_or(0),
            row["limit"]["output"].as_u64().unwrap_or(0),
        );
        m["reasoning"] = json!(row["reasoning"] != false);
        m["attachment"] = json!(row["attachment"] == true);
        m["thinkingMode"] = row["thinking_config"]["mode"].clone();
        m["variants"] = json!({});
        for effort in row["thinking"]["effortOptions"]
            .as_array()
            .or(row["thinking"]["effort_options"].as_array())
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|e| !matches!(*e, "none" | "default"))
        {
            m["variants"][effort] = json!({"reasoningEffort":effort});
        }
        out[id] = m;
    }
    if rows.is_empty() {
        return Err("MiniMax returned no models".into());
    }
    Ok(out)
}
fn encode_uri(text: &str) -> String {
    let mut out = String::new();
    use std::fmt::Write;
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}
fn account_request(
    c: &Context,
    site: &Site,
    auth: &Value,
    path: &str,
    body: Option<&Value>,
    millis: u64,
) -> Result<RequestBuilder> {
    let mut url = url::Url::parse(&format!("{}{path}", site.agent))
        .map_err(|_| "Invalid MiniMax account endpoint")?;
    let ts = millis.to_string();
    let uid = auth["accountId"].as_str().unwrap_or("0");
    url.query_pairs_mut().extend_pairs([
        ("device_platform", "mcode"),
        ("biz_id", "3"),
        ("app_id", "3001"),
        ("version_code", "22201"),
        ("unix", &ts),
        ("timezone_offset", "0"),
        ("sys_language", site.lang),
        ("lang", site.lang),
        ("device_id", "0"),
        ("os_name", vendor_platform()),
        ("browser_name", "mcode"),
        ("user_id", uid),
        ("client", "mcode"),
    ]);
    let payload = body.map(Value::to_string);
    let at = format!("{}?{}", url.path(), url.query().unwrap_or(""));
    let yy = format!(
        "{:x}",
        md5::compute(format!(
            "{}_{}{:x}ooui",
            encode_uri(&at),
            payload.as_deref().unwrap_or("{}"),
            md5::compute(&ts)
        ))
    );
    let seconds = millis / 1000;
    let signature = format!(
        "{:x}",
        md5::compute(format!(
            "{seconds}I*7Cf%WZ#S&%1RlZJ&C2{}",
            payload.as_deref().unwrap_or("")
        ))
    );
    let request = if let Some(payload) = payload {
        c.client.post(url).body(payload)
    } else {
        c.client.get(url)
    };
    Ok(request
        .bearer_auth(s(auth, "access"))
        .header("User-Agent", "MiniMaxCode")
        .header("Content-Type", "application/json")
        .header("yy", yy)
        .header("x-timestamp", seconds)
        .header("x-signature", signature))
}
async fn account_call(
    c: &Context,
    site: &Site,
    auth: &Value,
    path: &str,
    body: Option<&Value>,
) -> Result<Value> {
    let v = json_request(account_request(c, site, auth, path, body, now())?).await?;
    if v["base_resp"]["status_code"]
        .as_i64()
        .is_some_and(|n| n != 0)
    {
        return Err("MiniMax account request was refused".into());
    }
    Ok(v)
}
pub(super) async fn usage(c: &Context) -> Result<Value> {
    let site = site(&c.provider)?;
    let workspace = account_call(
        c,
        &site,
        &c.auth,
        "/matrix/api/v1/user/get_user_extra_info",
        Some(&json!({})),
    )
    .await?;
    let rows = workspace["workspaces"]
        .as_array()
        .or(workspace["data"]["workspaces"].as_array())
        .ok_or("MiniMax returned no account workspace")?;
    let ws = rows
        .iter()
        .find(|w| device::percent(&w["workspace_type"]) == Some(0.))
        .ok_or("MiniMax personal workspace is unavailable")?;
    let v = account_call(
        c,
        &site,
        &c.auth,
        "/matrix/api/v1/commerce/get_membership_info",
        Some(&json!({"workspace_id":ws["workspace_id"]})),
    )
    .await?;
    let data = if v["data"].is_object() {
        &v["data"]
    } else {
        &v
    };
    let has_plan = data["has_token_plan"]
        .as_bool()
        .or(ws["has_token_plan"].as_bool());
    let mut result = json!({"windows":[]});
    if let Some(plan) = has_plan {
        result["plan"] = json!(if plan { "M Plan" } else { "Free" });
    }
    let balance = &data["op_credit_summary"]["total_remaining_amount"];
    if let Some(n) = device::percent(balance)
        .or_else(|| device::percent(&data["opcredit_balance"]))
        .filter(|n| *n >= 0.)
    {
        result["balance"] = json!(format!("{n} credits"));
    }
    result["until"] = json!(device::date(&data["token_plan_expires_at"]));
    if has_plan == Some(true) {
        let group = data["op_group_id"]
            .as_str()
            .or(ws["op_group_id"].as_str())
            .unwrap_or("");
        if group.is_empty() {
            result["error"] = json!("MiniMax M Plan billing group is unavailable");
            return Ok(result);
        }
        let v = json_request(
            c.client
                .get(format!(
                    "{}/v1/api/openplatform/coding_plan/remains",
                    site.platform
                ))
                .bearer_auth(c.token()?)
                .header("X-Group-Id", group),
        )
        .await?;
        result["windows"] = json!(super::plan_quota::minimax(&v)?);
    }
    if has_plan.is_none() && result["balance"].is_null() {
        return Err("MiniMax returned no membership allowance".into());
    }
    Ok(result)
}
pub(super) async fn generate(c: &mut Context, m: &Value, mut body: Value) -> Result<()> {
    match s(m, "thinkingMode") {
        "forced_off" => {
            body.as_object_mut()
                .ok_or("Invalid MiniMax request")?
                .remove("thinking");
        }
        "forced_on" if body["thinking"]["type"] == "disabled" => {
            return Err("This MiniMax model requires reasoning".into())
        }
        _ => {}
    }
    let site = site(&c.provider)?;
    c.pipe(
        send(
            c.client
                .post(format!("{}/mavis/api/v1/llm/v1/messages", site.llm))
                .bearer_auth(c.token()?)
                .header("x-api-key", "sk-xxx")
                .header("anthropic-version", "2023-06-01")
                .header("User-Agent", "MiniMaxAgent")
                .header("X-Mavis-Agent-Id", "main")
                .header("X-Mavis-Timezone-Offset", "0")
                .header("X-Mavis-Session-Id", &c.session)
                .json(&body),
        )
        .await?,
    )
    .await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn regions_are_separate_and_unknown_quota_is_not_exhausted() {
        assert_ne!(
            site("minimax-code").unwrap().account,
            site("minimax-code-global").unwrap().account
        );
        let ws = super::super::plan_quota::minimax(
            &json!({"model_remains":[{"model_name":"general","current_interval_remaining_percent":0,"current_weekly_status":3},{"model_name":"other","current_interval_status":2},{"model_name":"unknown"},{"model_name":"invalid","current_interval_remaining_percent":-1}]}),
        ).unwrap();
        assert_eq!(ws.len(), 2);
        assert_eq!(ws[0]["used"], 100.);
        assert_eq!(ws[1]["aside"], true);
    }
    #[test]
    fn signing_encodes_the_path_and_models_keep_native_protocol() {
        assert_eq!(encode_uri("/path?a=b&c=1"), "%2Fpath%3Fa%3Db%26c%3D1");
        let m=parse_models(&json!({"providers":[{"providerId":"minimax","config":{"models":{"one":{"thinking_config":{"mode":"forced_on"},"thinking":{"effortOptions":["default","high"]}}}}}]}),"https://agent.minimax.io/mavis/api/v1/llm/v1").unwrap();
        assert_eq!(m["one"]["wire"], "anthropic");
        assert_eq!(m["one"]["variants"]["high"]["reasoningEffort"], "high");
    }
}
