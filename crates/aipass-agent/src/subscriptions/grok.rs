use super::*;
const BASE: &str = "https://cli-chat-proxy.grok.com/v1";
#[cfg(test)]
mod recovery_tests;
pub(super) async fn login(c: &mut Context, method: usize) -> Result<Value> {
    cli_accounts::login(c, method).await
}
pub(super) async fn fresh(c: &mut Context) -> Result<()> {
    if s(&c.auth, "nativeHome").is_empty() {
        // One-time upgrade of existing complete grants into a CLI-owned home.
        let bundle = s(&c.auth, "aipassNativeBundle");
        if bundle.is_empty() {
            return Err("Reconnect Grok with the official Grok Build CLI".into());
        }
        let ambient = cli_accounts::default_home("grok")?;
        if let Ok(reference) = cli_accounts::reference("grok", &ambient) {
            if reference["accountId"] == c.auth["accountId"] {
                c.save(reference).await?;
                return cli_accounts::fresh(c).await;
            }
        }
        let path = cli_accounts::new_home("grok")?;
        crate::claude_bridge::write_private(&path.join("auth.json"), bundle.as_bytes())?;
        let reference = cli_accounts::reference("grok", &path)?;
        c.save(reference).await?;
    }
    cli_accounts::fresh(c).await
}
fn sign(c: &Context, r: RequestBuilder) -> Result<RequestBuilder> {
    Ok(r.bearer_auth(c.token()?)
        .header("x-grok-client-version", "1.0.41")
        .header("x-xai-token-auth", "xai-grok-cli")
        .header("x-grok-client-identifier", "grok-shell")
        .header("x-grok-client-mode", "headless")
        .header(
            "User-Agent",
            format!(
                "grok-shell/1.0.41 ({}; {})",
                std::env::consts::OS,
                std::env::consts::ARCH
            ),
        ))
}
pub(super) async fn models(c: &mut Context) -> Result<Value> {
    let v = get(c, &format!("{BASE}/models")).await?;
    let mut out = json!({});
    for m in v["data"].as_array().into_iter().flatten() {
        let id = s(m, "id");
        if id.is_empty() || (!s(m, "api_backend").is_empty() && m["api_backend"] != "responses") {
            continue;
        }
        let mut item = model(
            id,
            m["name"].as_str().unwrap_or(id),
            "responses",
            BASE,
            m["context_window"].as_u64().unwrap_or(0),
            0,
        );
        item["variants"] = json!({});
        for e in m["reasoning_efforts"].as_array().into_iter().flatten() {
            let e = s(e, "value");
            if !e.is_empty() {
                item["variants"][e] = json!({"reasoningEffort":e});
            }
        }
        item["attachment"] = json!(true);
        out[id] = item;
    }
    Ok(out)
}
pub(super) async fn usage(c: &mut Context) -> Result<Value> {
    let v = get(c, &format!("{BASE}/billing?format=credits")).await?;
    parse_usage(&v)
}
// Only read-only endpoints use this retry. Generation remains on converted/pipe.
async fn get(c: &mut Context, url: &str) -> Result<Value> {
    for attempt in 0..2 {
        let response = send(sign(c, c.client.get(url))?.timeout(Duration::from_secs(30))).await?;
        let status = response.status().as_u16();
        let bytes = read_bytes(response, LIMIT).await?;
        if status == 401 && attempt == 0 {
            cli_accounts::recover_grok(c).await?;
            continue;
        }
        if !(200..300).contains(&status) {
            return Err(format!("Grok API returned HTTP {status}"));
        }
        return serde_json::from_slice(&bytes).map_err(|_| "Grok returned invalid JSON".into());
    }
    unreachable!()
}
fn parse_usage(v: &Value) -> Result<Value> {
    let cfg = &v["config"];
    let (name, span) =
        match s(&cfg["currentPeriod"], "type").trim_start_matches("USAGE_PERIOD_TYPE_") {
            "DAILY" => ("1 day", 86400),
            "WEEKLY" => ("7 days", 604800),
            "MONTHLY" => ("Month", 2592000),
            _ => ("Allowance", 0),
        };
    let reset = cfg["currentPeriod"]["end"]
        .as_str()
        .or(cfg["billingPeriodEnd"].as_str());
    let mut windows = vec![];
    if let Some(used) = cfg["creditUsagePercent"].as_f64() {
        windows.push(json!({"name":name,"span":span,"used":used,"resetsAt":reset}));
    }
    if let Some((cap, used)) = device::percent(&cfg["onDemandCap"]["val"])
        .filter(|n| *n > 0.0)
        .zip(device::percent(&cfg["onDemandUsed"]["val"]).filter(|n| *n >= 0.0))
    {
        windows
            .push(json!({"name":"On-demand","used":100.0*used/cap,"aside":true,"resetsAt":reset}));
    }
    if windows.is_empty() {
        return Err("Grok returned no allowance data".into());
    }
    Ok(json!({"windows":windows}))
}
pub(super) async fn generate(c: &mut Context, _: &Value, mut body: Value) -> Result<()> {
    let mut adapter = aipass_proxy_conversion::providers::grok::Adapter::prepare(&mut body)?;
    let req = sign(c, c.client.post(format!("{BASE}/responses")))?.json(&body);
    c.converted(send(req).await?, &mut adapter).await
}

#[cfg(test)]
mod usage_tests {
    use super::*;
    #[test]
    fn an_on_demand_cap_alone_is_not_a_zero_usage_reading() {
        let out =
            parse_usage(&json!({"config":{"creditUsagePercent":30,"onDemandCap":{"val":100}}}))
                .unwrap();
        assert_eq!(out["windows"].as_array().unwrap().len(), 1);
        let out = parse_usage(&json!({"config":{"creditUsagePercent":0,"onDemandCap":{"val":100},"onDemandUsed":{"val":0}}})).unwrap();
        assert_eq!(out["windows"][1]["used"], 0.0);
        assert!(parse_usage(&json!({})).is_err());
    }
}
