//! Dedicated Coding/Token Plan namespaces. Never fall back to another billing product.
use super::*;

pub(super) const PROVIDERS: &[&str] = &[
    "kimi-code-cn",
    "kimi-code",
    "minimax-m-plan-cn",
    "minimax-m-plan",
    "bailian-coding-plan",
    "mimo-plan",
    "stepfun-plan-cn",
    "stepfun-plan",
    "qwen-token-plan",
    "bailian-token-plan",
    "volcengine-coding-plan",
    "volcengine-agent-plan",
    "tencent-token-plan",
    "tencent-token-plan-enterprise",
];
pub(super) struct Spec {
    pub name: &'static str,
    pub chat: &'static str,
    pub messages: &'static str,
    pub responses: Option<&'static str>,
    pub models: &'static [&'static str],
    pub live_models: bool,
}
pub(super) fn spec(provider: &str, region: &str) -> Result<Spec> {
    let qwen = &[
        "qwen3.8-max",
        "qwen3.8-flash",
        "qwen3.7-max",
        "qwen3.7-plus",
        "qwen3.6-flash",
        "glm-5.3",
        "glm-5.2",
        "deepseek-v4-pro",
        "deepseek-v4.1-flash",
    ] as &[_];
    Ok(match provider {
        "kimi-code-cn" | "kimi-code" => {
            let (chat, messages) = if provider == "kimi-code-cn" {
                (
                    "https://api.kimi.com/coding/v1",
                    "https://api.kimi.com/coding",
                )
            } else {
                (
                    "https://api.kimi.ai/coding/v1",
                    "https://api.kimi.ai/coding",
                )
            };
            Spec {
                name: "Kimi Code",
                chat,
                messages,
                responses: Some(chat),
                models: &[
                    "k3",
                    "k3-256k",
                    "kimi-for-coding",
                    "kimi-for-coding-highspeed",
                ],
                live_models: true,
            }
        }
        "minimax-m-plan-cn" | "minimax-m-plan" => {
            let (chat, messages) = if provider == "minimax-m-plan-cn" {
                (
                    "https://api.minimaxi.com/v1",
                    "https://api.minimaxi.com/anthropic",
                )
            } else {
                (
                    "https://api.minimax.io/v1",
                    "https://api.minimax.io/anthropic",
                )
            };
            Spec {
                name: "MiniMax M Plan",
                chat,
                messages,
                responses: None,
                models: &[
                    "MiniMax-M3.1-Flash-Preview",
                    "MiniMax-M3",
                    "MiniMax-M2.7",
                    "MiniMax-M2.5",
                ],
                live_models: true,
            }
        }
        "bailian-coding-plan" => {
            let (chat, messages) = match region {
                "cn" => (
                    "https://coding.dashscope.aliyuncs.com/v1",
                    "https://coding.dashscope.aliyuncs.com/apps/anthropic",
                ),
                "intl" => (
                    "https://coding-intl.dashscope.aliyuncs.com/v1",
                    "https://coding-intl.dashscope.aliyuncs.com/apps/anthropic",
                ),
                _ => return Err("Choose a valid Bailian Coding Plan region".into()),
            };
            Spec {
                name: "Bailian Coding Plan",
                chat,
                messages,
                responses: None,
                models: qwen,
                live_models: true,
            }
        }
        "mimo-plan" => {
            let (chat, messages) = match region {
                "cn" => (
                    "https://token-plan-cn.xiaomimimo.com/v1",
                    "https://token-plan-cn.xiaomimimo.com/anthropic",
                ),
                "sgp" => (
                    "https://token-plan-sgp.xiaomimimo.com/v1",
                    "https://token-plan-sgp.xiaomimimo.com/anthropic",
                ),
                "ams" => (
                    "https://token-plan-ams.xiaomimimo.com/v1",
                    "https://token-plan-ams.xiaomimimo.com/anthropic",
                ),
                _ => return Err("Choose a valid MiMo Token Plan region".into()),
            };
            Spec {
                name: "MiMo Token Plan",
                chat,
                messages,
                responses: Some(chat),
                models: &[
                    "mimo-v2.6-pro",
                    "mimo-v2.6-flash",
                    "mimo-v2.5-pro",
                    "mimo-v2.5",
                ],
                live_models: true,
            }
        }
        "stepfun-plan-cn" => Spec {
            name: "StepFun Step Plan (China)",
            chat: "https://api.stepfun.com/step_plan/v1",
            messages: "https://api.stepfun.com/step_plan",
            responses: None,
            models: &["step-5-preview", "step-3.5-flash"],
            live_models: true,
        },
        "stepfun-plan" => Spec {
            name: "StepFun Step Plan (International)",
            chat: "https://api.stepfun.ai/step_plan/v1",
            messages: "https://api.stepfun.ai/step_plan",
            responses: None,
            models: &["step-5-preview", "step-3.5-flash"],
            live_models: true,
        },
        "qwen-token-plan" => Spec {
            name: "Qwen Token Plan",
            chat: "https://token-plan.maas.qianwenaiapi.com/compatible-mode/v1",
            messages: "https://token-plan.maas.qianwenaiapi.com/apps/anthropic",
            responses: None,
            models: qwen,
            live_models: true,
        },
        "bailian-token-plan" => {
            let (chat, messages) = match region {
                "cn" => (
                    "https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1",
                    "https://token-plan.cn-beijing.maas.aliyuncs.com/apps/anthropic",
                ),
                "intl" => (
                    "https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1",
                    "https://token-plan.ap-southeast-1.maas.aliyuncs.com/apps/anthropic",
                ),
                _ => return Err("Choose a valid Bailian Token Plan region".into()),
            };
            Spec {
                name: "Bailian Token Plan",
                chat,
                messages,
                responses: None,
                models: qwen,
                live_models: true,
            }
        }
        "volcengine-coding-plan" => Spec {
            name: "Volcengine Coding Plan",
            chat: "https://ark.cn-beijing.volces.com/api/coding/v3",
            messages: "https://ark.cn-beijing.volces.com/api/coding",
            responses: Some("https://ark.cn-beijing.volces.com/api/coding/v3"),
            models: &[
                "ark-code-latest",
                "doubao-seed-2.1-pro",
                "glm-5.3",
                "deepseek-v4-pro",
                "deepseek-v4.1-flash",
                "kimi-k3",
                "minimax-m3",
            ],
            live_models: false,
        },
        "volcengine-agent-plan" => Spec {
            name: "Volcengine Agent Plan",
            chat: "https://ark.cn-beijing.volces.com/api/plan/v3",
            messages: "https://ark.cn-beijing.volces.com/api/plan",
            responses: Some("https://ark.cn-beijing.volces.com/api/plan/v3"),
            models: &[
                "ark-code-latest",
                "doubao-seed-2.1-pro",
                "glm-5.3",
                "deepseek-v4-pro",
                "deepseek-v4.1-flash",
                "kimi-k3",
                "minimax-m3",
            ],
            live_models: false,
        },
        "tencent-token-plan" => Spec {
            name: "Tencent Cloud Token Plan",
            chat: "https://api.lkeap.cloud.tencent.com/plan/v3",
            messages: "https://api.lkeap.cloud.tencent.com/plan/anthropic",
            responses: None,
            models: &[
                "tc-code-latest",
                "glm-5.3",
                "deepseek-v4-pro-202606",
                "kimi-k3",
                "minimax-m3",
                "hy3",
            ],
            live_models: false,
        },
        "tencent-token-plan-enterprise" => {
            let (chat, messages) = match region {
                "cn" => (
                    "https://tokenhub.tencentmaas.com/plan/v3",
                    "https://tokenhub.tencentmaas.com/plan/anthropic",
                ),
                "intl" => (
                    "https://tokenhub-intl.tencentmaas.com/plan/v3",
                    "https://tokenhub-intl.tencentmaas.com/plan/anthropic",
                ),
                _ => return Err("Choose a valid Tencent Cloud plan region".into()),
            };
            Spec {
                name: "Tencent Cloud Token Plan Enterprise",
                chat,
                messages,
                responses: Some(chat),
                models: &[
                    "auto",
                    "glm-5.3",
                    "kimi-k3",
                    "deepseek-v4.1-flash",
                    "minimax-m3",
                    "hy3",
                ],
                live_models: false,
            }
        }
        _ => return Err("Unknown Coding/Token Plan".into()),
    })
}
fn current(c: &Context) -> Result<Spec> {
    spec(
        &c.provider,
        c.auth["metadata"]["region"].as_str().unwrap_or("cn"),
    )
}
pub(super) async fn fresh(c: &mut Context) -> Result<()> {
    current(c)?;
    if c.auth["type"] != "api" || c.token()?.trim().is_empty() {
        return Err("A plan API key is required".into());
    }
    // Only enforce a documented prefix. Token Plan and Coding Plan keys
    // are distinct products; a fixed plan endpoint owns the billing scope.
    let key = c.token()?;
    if c.provider == "mimo-plan" && !key.starts_with("tp-") {
        return Err("Use the MiMo Token Plan's tp- API key".into());
    }
    if c.provider.starts_with("minimax-m-plan") && !key.starts_with("sk-cp-") {
        return Err("Use the MiniMax subscription key (sk-cp-), not a pay-as-you-go key".into());
    }
    if c.provider == "bailian-coding-plan" && !key.starts_with("sk-sp-") {
        return Err("Use the Bailian Coding Plan subscription key (sk-sp-)".into());
    }
    let region = c.auth["metadata"]["region"].as_str().unwrap_or("cn");
    let scope = format!(
        "{}:{region}:key-{:x}",
        c.provider,
        sha2::Sha256::digest(key.as_bytes())
    );
    if let Some(identity) = c.auth["accountId"].as_str() {
        if identity != scope {
            return Err("The plan key or billing region changed; reconnect explicitly".into());
        }
    } else {
        let mut next = c.auth.clone();
        next["accountId"] = json!(scope);
        c.save(next).await?;
    }
    Ok(())
}
pub(super) async fn models(c: &Context) -> Result<Value> {
    let spec = current(c)?;
    let (wire, base) = if c.provider.starts_with("minimax-m-plan") {
        ("anthropic", spec.messages)
    } else {
        ("chat", spec.chat)
    };
    let mut out = c
        .models
        .as_object()
        .filter(|m| !m.is_empty())
        .map(|_| c.models.clone())
        .unwrap_or_else(|| json!({}));
    if out.as_object().is_none_or(|m| m.is_empty()) {
        for id in spec.models {
            out[*id] = model(id, id, wire, base, 0, 0);
        }
    }
    if spec.live_models {
        if let Ok(v) = json_request(
            c.client
                .get(format!("{}/models", spec.chat))
                .bearer_auth(c.token()?),
        )
        .await
        {
            let live = parse_models(&v, wire, base);

            if live.as_object().is_some_and(|o| !o.is_empty()) {
                out = live;
            }
        }
    }
    Ok(out)
}
fn parse_models(v: &Value, wire: &str, base: &str) -> Value {
    let mut live = json!({});
    if v["has_more"] == true || v["data"].as_array().is_some_and(|a| a.len() > 4096) {
        return live;
    }
    for row in v["data"].as_array().into_iter().flatten().take(4096) {
        let id = s(row, "id");
        if id.is_empty()
            || id.len() > 256
            || id.chars().any(char::is_control)
            || row.get("type").is_some_and(|v| v != "model")
            || row.get("object").is_some_and(|v| v != "model")
        {
            continue;
        }
        live[id] = model(
            id,
            row["name"].as_str().unwrap_or(id),
            wire,
            base,
            row["context_length"].as_u64().unwrap_or(0),
            0,
        );
    }
    live
}
pub(super) async fn usage(c: &Context) -> Result<Value> {
    let spec = current(c)?;
    let url = if c.provider.starts_with("kimi-code") {
        format!("{}/usages", spec.chat)
    } else if c.provider.starts_with("minimax-m-plan") {
        format!("{}/token_plan/remains", spec.chat)
    } else {
        return Ok(json!({"plan":spec.name,"windows":[],"quotaUnavailable":true}));
    };
    let value = json_request(
        c.client
            .get(url)
            .bearer_auth(c.token()?)
            .header("User-Agent", concat!("AIPass/", env!("CARGO_PKG_VERSION"))),
    )
    .await?;
    let windows = if c.provider.starts_with("kimi-code") {
        super::plan_quota::kimi(&value)?
    } else {
        super::plan_quota::minimax(&value)?
    };
    Ok(json!({"plan":spec.name,"windows":windows}))
}

fn endpoint(spec: &Spec, wire: &str) -> Result<String> {
    Ok(match wire {
        "chat" => format!("{}/chat/completions", spec.chat),
        "anthropic" => format!("{}/v1/messages", spec.messages),
        "responses" => format!(
            "{}/responses",
            spec.responses
                .ok_or("This plan has no native Responses endpoint")?
        ),
        _ => return Err("Unsupported plan API".into()),
    })
}
pub(super) async fn generate(c: &mut Context, m: &Value, body: Value) -> Result<()> {
    let spec = current(c)?;
    let wire = s(m, "wire");
    let mut request = c
        .client
        .post(endpoint(&spec, wire)?)
        .bearer_auth(c.token()?)
        .header("User-Agent", concat!("AIPass/", env!("CARGO_PKG_VERSION")));
    if wire == "anthropic" {
        request = request
            .header("x-api-key", c.token()?)
            .header("anthropic-version", "2023-06-01");
    }
    c.pipe(send(request.json(&body)).await?).await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn token_plan_keys_without_coding_plan_prefix_keep_their_region_and_identity() {
        for (provider, region) in [
            ("qwen-token-plan", "cn"),
            ("bailian-token-plan", "cn"),
            ("bailian-token-plan", "intl"),
        ] {
            let (mut c, input, mut output) = super::super::tests::context(Client::new());
            c.provider = provider.into();
            c.auth =
                json!({"type":"api","key":"synthetic-token-plan-key","metadata":{"region":region}});
            let ack = tokio::spawn(async move {
                let frame = tokio::time::timeout(Duration::from_secs(2), output.recv())
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(frame["value"]["metadata"]["region"], region);
                input
                    .send(json!({"type":"ack","id":frame["id"],"ok":true}))
                    .await
                    .unwrap();
            });
            let result = fresh(&mut c).await;
            if result.is_err() {
                ack.abort();
            }
            result.unwrap();
            ack.await.unwrap();
            assert!(s(&c.auth, "accountId").starts_with(&format!("{provider}:{region}:key-")));
            fresh(&mut c).await.unwrap();
            c.auth["key"] = json!("different-synthetic-token-plan-key");
            assert!(fresh(&mut c)
                .await
                .unwrap_err()
                .contains("reconnect explicitly"));
            assert!(current(&c).unwrap().chat.contains("token-plan"));
        }
    }
    #[tokio::test]
    async fn shared_minimax_endpoint_never_accepts_a_payg_key_as_a_plan() {
        for provider in ["minimax-m-plan", "minimax-m-plan-cn"] {
            let (mut c, _, _) = super::super::tests::context(Client::new());
            c.provider = provider.into();
            c.auth = json!({"type":"api","key":"sk-api-synthetic-payg"});
            assert!(fresh(&mut c)
                .await
                .unwrap_err()
                .contains("subscription key"));
            assert!(c.auth["accountId"].is_null());
        }
    }
    #[test]
    fn subscription_keys_keep_their_billing_product_and_native_wire() {
        assert_eq!(
            endpoint(&spec("kimi-code-cn", "cn").unwrap(), "responses").unwrap(),
            "https://api.kimi.com/coding/v1/responses"
        );
        assert_eq!(
            endpoint(&spec("kimi-code", "cn").unwrap(), "anthropic").unwrap(),
            "https://api.kimi.ai/coding/v1/messages"
        );
        let coding = spec("bailian-coding-plan", "intl").unwrap();
        assert_eq!(
            endpoint(&coding, "anthropic").unwrap(),
            "https://coding-intl.dashscope.aliyuncs.com/apps/anthropic/v1/messages"
        );
        assert_ne!(
            coding.chat,
            spec("bailian-token-plan", "intl").unwrap().chat
        );
        let minimax = spec("minimax-m-plan", "cn").unwrap();
        assert_eq!(
            endpoint(&minimax, "anthropic").unwrap(),
            "https://api.minimax.io/anthropic/v1/messages"
        );
        let catalog = parse_models(
            &json!({"data":[{"id":"future","object":"model"},{"id":"file","object":"file"}]}),
            "anthropic",
            minimax.messages,
        );
        assert_eq!(catalog["future"]["wire"], "anthropic");
        assert!(catalog.get("file").is_none());
    }
    #[test]
    fn every_plan_uses_its_dedicated_namespace_without_appending_v1() {
        for id in PROVIDERS {
            let spec = spec(id, "cn").unwrap();
            let url = endpoint(&spec, "chat").unwrap();
            assert!(
                url.contains("plan") || url.contains("coding") || id.starts_with("minimax-m-plan")
            );
            assert!(!url.contains("/v3/v1"));
            assert!(!spec.models.is_empty());
            assert!(endpoint(&spec, "anthropic")
                .unwrap()
                .ends_with("/v1/messages"));
            assert!(!endpoint(&spec, "anthropic").unwrap().contains("/v1/v1"));
        }
        assert!(
            endpoint(&spec("volcengine-coding-plan", "cn").unwrap(), "responses")
                .unwrap()
                .contains("/api/coding/v3/responses")
        );
        assert!(
            endpoint(&spec("volcengine-agent-plan", "cn").unwrap(), "responses")
                .unwrap()
                .contains("/api/plan/v3/responses")
        );
        assert!(endpoint(&spec("tencent-token-plan", "cn").unwrap(), "responses").is_err());
        assert_ne!(
            spec("mimo-plan", "cn").unwrap().chat,
            spec("mimo-plan", "sgp").unwrap().chat
        );
        assert!(spec("mimo-plan", "untrusted").is_err());
        assert_eq!(
            endpoint(
                &spec("tencent-token-plan-enterprise", "intl").unwrap(),
                "anthropic"
            )
            .unwrap(),
            "https://tokenhub-intl.tencentmaas.com/plan/anthropic/v1/messages"
        );
        assert_eq!(
            endpoint(&spec("mimo-plan", "sgp").unwrap(), "anthropic").unwrap(),
            "https://token-plan-sgp.xiaomimimo.com/anthropic/v1/messages"
        );
        assert_eq!(
            endpoint(&spec("stepfun-plan-cn", "cn").unwrap(), "anthropic").unwrap(),
            "https://api.stepfun.com/step_plan/v1/messages"
        );
    }
}
