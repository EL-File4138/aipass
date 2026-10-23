//! Trae account transport. China and international identities never share hosts.
use super::*;
const CLIENT: &str = "ono9krqynydwx5";
struct Site {
    web: &'static str,
    auth: &'static str,
    api: &'static str,
    version: &'static str,
    plugin: &'static str,
}
fn site(provider: &str) -> Result<Site> {
    match provider {
        "trae-cn" => Ok(Site {
            web: "https://www.trae.cn",
            auth: "https://api.trae.cn",
            api: "https://trae-api-cn.mchost.guru",
            version: "3.3.65",
            plugin: "2.3.24254",
        }),
        "trae-global" => Ok(Site {
            web: "https://www.trae.ai",
            auth: "https://growsg-normal.trae.ai",
            api: "https://coresg-normal.trae.ai",
            version: "3.5.51",
            plugin: "2.3.62834",
        }),
        _ => Err("Unknown Trae service".into()),
    }
}
fn object(v: &Value) -> Value {
    v.as_str()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| v.clone())
}
fn epoch(v: &Value) -> u64 {
    if let Some(n) = v.as_u64().or_else(|| v.as_str()?.parse().ok()) {
        if n > 1_000_000_000_000 {
            n
        } else {
            n.saturating_mul(1000)
        }
    } else {
        device::date(v)
            .and_then(|s| {
                time::OffsetDateTime::parse(&s, &time::format_description::well_known::Rfc3339).ok()
            })
            .map(|t| t.unix_timestamp().max(0) as u64 * 1000)
            .unwrap_or(0)
    }
}
fn auth(provider: &str, jwt: &Value, info: &Value, previous: &Value) -> Result<Value> {
    let token = jwt["Token"]
        .as_str()
        .or(jwt["token"].as_str())
        .filter(|s| !s.is_empty())
        .ok_or("Trae returned no access token")?;
    let refresh = jwt["RefreshToken"]
        .as_str()
        .or(jwt["refreshToken"].as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(s(previous, "refresh"));
    if refresh.is_empty() {
        return Err("Trae returned no refresh token".into());
    }
    let uid = identifier(&info["UserID"]);
    let uid = if uid.is_empty() {
        identifier(&info["userId"])
    } else {
        uid
    };
    let uid = if uid.is_empty() {
        previous["metadata"]["userId"]
            .as_str()
            .unwrap_or("")
            .to_owned()
    } else {
        uid
    };
    if uid.is_empty() {
        return Err("Trae returned no account identity".into());
    }
    let mut next = previous.clone();
    next["type"] = json!("oauth");
    next["access"] = json!(token);
    next["refresh"] = json!(refresh);
    next["expires"] = json!([
        epoch(&jwt["TokenExpireAt"]),
        epoch(&jwt["tokenExpireAt"]),
        expires(token)
    ]
    .into_iter()
    .filter(|n| *n > 0)
    .min()
    .unwrap_or(0));
    if next["expires"].as_u64().unwrap_or(0) <= now() {
        return Err("Trae sign-in expired; reconnect".into());
    }
    if !next["metadata"].is_object() {
        next["metadata"] = json!({});
    }
    next["metadata"]["userId"] = json!(uid);
    let scope = ["TenantID", "AccountID", "OrganizationID"]
        .iter()
        .map(|key| identifier(&info[*key]))
        .find(|s| !s.is_empty())
        .unwrap_or_else(|| s(&previous["metadata"], "scope").to_owned());
    next["metadata"]["scope"] = json!(scope);
    next["accountId"] = json!(format!("{provider}:{uid}:{scope}"));
    if let Some(name) = info["ScreenName"].as_str().or(info["screenName"].as_str()) {
        next["metadata"]["displayName"] = json!(name);
    }
    if let Some(region) = info["Region"]
        .as_str()
        .or(info["region"].as_str())
        .or(info["AIRegion"].as_str())
        .or(info["aiRegion"].as_str())
    {
        next["metadata"]["region"] = json!(region);
    }
    Ok(next)
}
pub(super) async fn login(c: &mut Context, _method: usize) -> Result<Value> {
    let site = site(&c.provider)?;
    let listener = loopback::Loopback::bind(&[0]).await?;
    // The callback URI is supplied to the vendor. Bind it to this login so a
    // browser navigation cannot attach another account to a pending ticket.
    let callback_path = format!("/authorize/{}", hex_id());
    let callback = format!("http://127.0.0.1:{}{callback_path}", listener.port);
    let device = hex_id();
    let machine = hex_id();
    let trace = hex_id();
    let mut url = url::Url::parse(&format!("{}/authorization", site.web)).unwrap();
    url.query_pairs_mut().extend_pairs([
        ("client_id", CLIENT),
        ("auth_callback_url", &callback),
        ("login_version", "1"),
        ("auth_from", "solo"),
        ("login_channel", "native_ide"),
        ("auth_type", "local"),
        ("redirect", "0"),
        ("login_trace_id", &trace),
        ("plugin_version", site.plugin),
        ("machine_id", &machine),
        ("device_id", &device),
        ("x_device_id", &device),
        ("x_machine_id", &machine),
        ("x_device_type", vendor_platform()),
        ("x_app_version", site.version),
        ("x_app_type", "stable"),
        ("hide_saas_login", "false"),
    ]);
    c.challenge(url.as_str(), "Sign in to Trae in your browser", false)
        .await?;
    let callback = listener
        .wait(&[site.web], |cb| {
            cb.method == "GET" && cb.path == callback_path && cb.fields["userJwt"].is_string()
        })
        .await?;
    let jwt = crate::subscription_import::reader::PrivateAuth(object(&callback.fields["userJwt"]));
    let info = object(&callback.fields["userInfo"]);
    let mut next = auth(&c.provider, &jwt.0, &info, &json!({}))?;
    next["metadata"]["deviceId"] = json!(device);
    next["metadata"]["machineId"] = json!(machine);
    Ok(next)
}
pub(super) async fn fresh(c: &mut Context) -> Result<()> {
    if c.auth["expires"].as_u64().unwrap_or(0) > now() + 120000 {
        return Ok(());
    }
    let site = site(&c.provider)?;
    let _guard = c.refresh_guard()?;
    let value=crate::subscription_import::reader::PrivateAuth(c.refresh_json("refresh", c.client.post(format!("{}/cloudide/api/v3/trae/oauth/ExchangeToken",site.auth)).json(&json!({"ClientID":CLIENT,"ClientSecret":"-","RefreshToken":c.auth["refresh"],"UserID":c.auth["metadata"]["userId"]}))).await?);
    let jwt = if value.0["Result"].is_object() {
        &value.0["Result"]
    } else if value.0["result"].is_object() {
        &value.0["result"]
    } else {
        &value.0
    };
    let info = object(&jwt["UserInfo"]);
    c.save(auth(&c.provider, jwt, &info, &c.auth)?).await
}
fn us(c: &Context) -> bool {
    c.auth["metadata"]["region"]
        .as_str()
        .is_some_and(|s| s.to_ascii_uppercase().starts_with("US"))
}
fn base(c: &Context) -> Result<&'static str> {
    if c.provider == "trae-global" && us(c) {
        Ok("https://coreva-normal.trae.ai")
    } else {
        Ok(site(&c.provider)?.api)
    }
}
fn signed(c: &Context, request: RequestBuilder) -> Result<RequestBuilder> {
    Ok(request
        .header("Authorization", format!("Cloud-IDE-JWT {}", c.token()?))
        .header("X-Cloudide-Token", c.token()?)
        .header("x-ide-token", c.token()?)
        .header("x-uid", s(&c.auth["metadata"], "userId"))
        .header("x-app-id", "6eefa01c-1036-4c7e-9ca5-d891f63bfcd8")
        .header("x-device-id", s(&c.auth["metadata"], "deviceId"))
        .header("x-machine-id", s(&c.auth["metadata"], "machineId"))
        .header("x-app-version", "0.1.69")
        .header("x-app-version-code", "20260917")
        .header("x-ide-version", "0.1.69")
        .header("x-ide-version-type", "stable")
        .header("x-request-id", hex_id())
        .header("User-Agent", concat!("AIPass/", env!("CARGO_PKG_VERSION"))))
}
async fn post(c: &Context, url: &str, body: Value) -> Result<Value> {
    let value = json_request(signed(c, c.client.post(url))?.json(&body)).await?;
    if value["code"].as_i64().is_some_and(|n| n != 0)
        || value["ResponseMetadata"]["Error"].is_object()
    {
        return Err("Trae refused the account request; check the sign-in".into());
    }
    Ok(value)
}
pub(super) async fn models(c: &Context) -> Result<Value> {
    let value=post(c,&format!("{}/api/ide/v1/batch_get_detail_param",base(c)?),json!({"functions":["chat_v3","solo_work_lite","solo_agent","solo_agent_lite"],"agent_type":"","current_config_info":{"config_name":"","is_custom_model":false},"mode_type":0,"access_type":0,"show_custom_model":false})).await?;
    parse_models(&value, c.provider == "trae-global")
}
fn parse_models(v: &Value, international: bool) -> Result<Value> {
    let groups = v["function_configs"]
        .as_array()
        .or(v["data"]["function_configs"].as_array())
        .ok_or("Trae returned no model catalog")?;
    let mut out = json!({});
    for group in groups {
        let function = s(group, "function");
        if !matches!(
            function,
            "chat_v3" | "solo_work_lite" | "solo_agent" | "solo_agent_lite"
        ) {
            continue;
        }
        for row in group["config_info_list"].as_array().into_iter().flatten() {
            let id = s(row, "config_name");
            if id.is_empty()
                || id.starts_with("custom_model")
                || row["config_switch"] == false
                || row["is_custom_model"] == true
                || (international && row["is_invisible_to_user"] == true)
                || row["usage"]
                    .as_str()
                    .is_some_and(|s| s != "chat_completion")
            {
                continue;
            }
            let native = row["model_detail_list"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|m| s(m, "model_name").ends_with("__dev"));
            if out[id].is_object() && native.is_none() {
                continue;
            }
            let name = row["display_config"]["display_name"]
                .as_str()
                .or(row["display_name"].as_str())
                .unwrap_or(id);
            let mut m = model(
                id,
                name,
                "chat",
                "",
                row["context_window_tokens"]["dev"].as_u64().unwrap_or(0),
                native.and_then(|m| m["max_tokens"].as_u64()).unwrap_or(0),
            );
            m["attachment"] = json!(row["multimodal"]
                .as_bool()
                .or(row["display_config"]["multimodal"].as_bool())
                .unwrap_or(false));
            m["function"] = json!(function);
            m["nativeModel"] = json!(native.map(|v| s(v, "model_name")).unwrap_or(""));
            out[id] = m;
        }
    }
    if out
        .as_object()
        .is_none_or(|m| m.is_empty() || m.len() > 4096)
    {
        return Err("Trae returned no available models".into());
    }
    Ok(out)
}
pub(super) async fn usage(c: &Context) -> Result<Value> {
    let (host, path, body) = if c.provider == "trae-cn" {
        (
            "https://api.trae.cn",
            "/trae/api/v2/pay/ide_user_ent_usage",
            json!({"require_usage":true,"req_source":0}),
        )
    } else {
        (
            if us(c) {
                "https://api-us-east.trae.ai"
            } else {
                "https://api-sg-central.trae.ai"
            },
            "/trae/api/v1/pay/user_current_entitlement_list",
            json!({}),
        )
    };
    let v = post(c, &format!("{host}{path}"), body).await?;
    parse_usage(&v, c.provider == "trae-cn")
}
fn parse_usage(v: &Value, china: bool) -> Result<Value> {
    parse_usage_at(v, china, now())
}
fn parse_usage_at(v: &Value, china: bool, millis: u64) -> Result<Value> {
    let mut total = 0.;
    let mut used = 0.;
    let mut end = None;
    let packs = v["user_entitlement_pack_list"]
        .as_array()
        .or(v["data"]["user_entitlement_pack_list"].as_array())
        .ok_or("Trae returned no entitlement packs")?;
    for pack in packs {
        let base = &pack["entitlement_base_info"];
        let quota = &base["quota"];
        let expires = epoch(&base["end_time"]);
        if (expires > 0 && expires <= millis) || epoch(&base["start_time"]) > millis {
            continue;
        }
        if china {
            if let Some(limit) = device::percent(&quota["credits_limit"]) {
                if limit < 0. {
                    return Ok(json!({"plan":"Trae","balance":"Unlimited","windows":[]}));
                }
                let count = device::percent(&pack["usage"]["credits_amount"])
                    .filter(|n| *n >= 0.)
                    .ok_or("Trae returned incomplete credit usage")?;
                total += limit;
                used += count;
            } else {
                continue;
            }
        } else {
            let limit = device::percent(&quota["basic_usage_limit"])
                .filter(|n| *n >= 0.)
                .ok_or("Trae returned no basic allowance")?;
            let count = device::percent(&pack["usage"]["basic_usage_amount"])
                .filter(|n| *n >= 0.)
                .ok_or("Trae returned incomplete basic usage")?;
            let bonus = if quota.get("bonus_usage_limit").is_some() {
                device::percent(&quota["bonus_usage_limit"])
                    .filter(|n| *n >= 0.)
                    .ok_or("Trae returned no bonus allowance")?
            } else {
                0.
            };
            let bonus_used = if bonus > 0. || pack["usage"].get("bonus_usage_amount").is_some() {
                device::percent(&pack["usage"]["bonus_usage_amount"])
                    .filter(|n| *n >= 0.)
                    .ok_or("Trae returned incomplete bonus usage")?
            } else {
                0.
            };
            total += limit + bonus;
            used += count + bonus_used;
        }
        let at = device::date(&base["end_time"]);
        if at.is_some() && (end.is_none() || at < end) {
            end = at;
        }
    }
    if total <= 0. {
        return Err("Trae returned no measurable allowance".into());
    }
    Ok(
        json!({"plan":"Trae","windows":[{"name":if china{"Credits"}else{"Usage"},"used":(100.*used/total).clamp(0.,100.),"resetsAt":end}]}),
    )
}
pub(super) async fn generate(c: &mut Context, m: &Value, body: Value) -> Result<()> {
    let mut request = aipass_proxy_conversion::providers::trae::request(
        &body,
        s(m, "function"),
        s(m, "id"),
        s(m, "nativeModel"),
        &c.session,
        m["attachment"] == true,
    )?;
    let mut decoder = aipass_proxy_conversion::providers::trae::TraeStream::new(
        s(&body, "model"),
        &format!("chatcmpl-{}", hex_id()),
        &body,
    )?;
    request["request_id"] = json!(hex_id());
    let response = send(
        signed(
            c,
            c.client
                .post(format!("{}/api/agent/v3/llm_utils_chat", base(c)?)),
        )?
        .header("Accept", "text/event-stream")
        .json(&request),
    )
    .await?;
    c.converted(response, &mut decoder).await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allowance_ignores_expired_packs_and_requires_reported_bonus_usage() {
        let now = 1800000000000u64;
        let active = json!({"entitlement_base_info":{"end_time":"1800003600","quota":{"basic_usage_limit":10,"bonus_usage_limit":5}},"usage":{"basic_usage_amount":10,"bonus_usage_amount":5}});
        let expired = json!({"entitlement_base_info":{"end_time":"1799999999","quota":{"basic_usage_limit":100,"bonus_usage_limit":0}},"usage":{"basic_usage_amount":0}});
        let usage = parse_usage_at(
            &json!({"user_entitlement_pack_list":[active.clone(),expired]}),
            false,
            now,
        )
        .unwrap();
        assert_eq!(usage["windows"][0]["used"], 100.);
        assert_eq!(
            usage["windows"][0]["resetsAt"],
            device::date(&json!(1800003600)).unwrap()
        );
        let mut missing = active;
        missing["usage"]
            .as_object_mut()
            .unwrap()
            .remove("bonus_usage_amount");
        assert!(
            parse_usage_at(&json!({"user_entitlement_pack_list":[missing]}), false, now).is_err()
        );
    }
    #[test]
    fn expiry_uses_the_earliest_known_deadline_and_numeric_tenants_remain_scoped() {
        let jwt_token = format!(
            "header.{}.sig",
            URL_SAFE_NO_PAD.encode(br#"{"exp":4102444800}"#)
        );
        let jwt = json!({"Token":jwt_token,"RefreshToken":"refresh","TokenExpireAt":4202444800u64});
        let before = auth(
            "trae-cn",
            &jwt,
            &json!({"UserID":123,"TenantID":456}),
            &json!({}),
        )
        .unwrap();
        assert_eq!(before["accountId"], "trae-cn:123:456");
        assert_eq!(before["expires"], 4102444800000u64);
        let after = auth(
            "trae-cn",
            &jwt,
            &json!({"UserID":123,"TenantID":789}),
            &before,
        )
        .unwrap();
        assert!(crate::community::ensure_owner(&before, &after).is_err());
    }
    #[test]
    fn catalogue_keeps_only_chat_models_and_the_function_that_lists_them() {
        let v = json!({"function_configs":[{"function":"solo_agent","config_info_list":[{"config_name":"one","model_detail_list":[{"model_name":"one__dev"}]},{"config_name":"hidden","is_invisible_to_user":true},{"config_name":"custom_model_x"}]}]});
        let models = parse_models(&v, true).unwrap();
        assert_eq!(models.as_object().unwrap().len(), 1);
        assert_eq!(models["one"]["function"], "solo_agent");
        assert_eq!(models["one"]["nativeModel"], "one__dev");
    }
    #[test]
    fn account_scope_and_realm_cannot_be_swapped_by_a_refresh() {
        let jwt = json!({"Token":"fake","RefreshToken":"refresh","TokenExpireAt":4102444800u64});
        let info = json!({"UserID":"alice","TenantID":"team-a"});
        let a = auth("trae-cn", &jwt, &info, &json!({})).unwrap();
        let b = auth("trae-global", &jwt, &info, &a).unwrap();
        assert!(crate::community::ensure_owner(&a, &b).is_err());
        let b = auth(
            "trae-cn",
            &jwt,
            &json!({"UserID":"alice","TenantID":"team-b"}),
            &a,
        )
        .unwrap();
        assert!(crate::community::ensure_owner(&a, &b).is_err());
    }
}
