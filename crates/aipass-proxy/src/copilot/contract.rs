//! Copilot endpoint and Enterprise Cloud authority checks. An account's host
//! is part of its identity; tokens never follow arbitrary discovered URLs.
use serde_json::Value;
type Result<T> = std::result::Result<T, String>;
#[cfg(test)]
use serde_json::json;

pub fn host(raw: &str) -> Result<String> {
    let raw = raw.trim().trim_end_matches('/').to_ascii_lowercase();
    if raw.is_empty() || matches!(raw.as_str(), "github.com" | "https://github.com") {
        return Ok(String::new());
    }
    let url = reqwest::Url::parse(
        if raw.contains("://") {
            raw.clone()
        } else {
            format!("https://{raw}")
        }
        .as_str(),
    )
    .map_err(|_| "Invalid Copilot account host")?;
    let hostname = url.host_str().ok_or("Invalid Copilot account host")?;
    let label = hostname
        .strip_suffix(".ghe.com")
        .ok_or("Copilot requires github.com or an Enterprise Cloud host")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || label.is_empty()
        || label.len() > 63
        || label.starts_with('-')
        || label.ends_with('-')
        || !label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("Invalid Copilot Enterprise Cloud host".into());
    }
    Ok(hostname.into())
}
pub fn user_url(host: &str) -> String {
    format!(
        "https://{}/copilot_internal/user",
        if host.is_empty() {
            "api.github.com".into()
        } else {
            format!("api.{host}")
        }
    )
}
pub fn api(value: Option<&str>, host: &str) -> Result<String> {
    let fallback = if host.is_empty() {
        "https://api.githubcopilot.com".into()
    } else {
        format!("https://copilot-api.{host}")
    };
    let u =
        reqwest::Url::parse(value.unwrap_or(&fallback)).map_err(|_| "Invalid Copilot endpoint")?;
    let allowed = u.host_str().is_some_and(|name| {
        if host.is_empty() {
            name == "githubcopilot.com" || name.ends_with(".githubcopilot.com")
        } else {
            name == format!("copilot-api.{host}")
        }
    });
    if u.scheme() != "https"
        || !u.username().is_empty()
        || u.password().is_some()
        || u.port().is_some_and(|p| p != 443)
        || !allowed
        || u.path() != "/"
        || u.query().is_some()
        || u.fragment().is_some()
    {
        return Err("Copilot returned an untrusted endpoint".into());
    }
    Ok(u.as_str().trim_end_matches('/').into())
}
pub fn wire(model: &Value) -> Option<&'static str> {
    // Copilot Auto needs a separate vendor session/pick contract. Do not
    // advertise it as an ordinary /chat/completions model.
    if model["id"] == "auto" {
        return None;
    }
    if model
        .pointer("/capabilities/type")
        .and_then(Value::as_str)
        .is_some_and(|v| v != "chat")
        || model
            .pointer("/policy/state")
            .and_then(Value::as_str)
            .is_some_and(|v| v != "enabled")
        || model["model_picker_enabled"] == false
    {
        return None;
    }
    let Some(endpoints) = model.get("supported_endpoints") else {
        return Some("chat");
    };
    let endpoints = endpoints.as_array()?;
    if endpoints.is_empty() {
        return Some("chat");
    }
    [
        ("chat", ["/chat/completions", "/v1/chat/completions"]),
        ("responses", ["/responses", "/v1/responses"]),
        ("anthropic", ["/messages", "/v1/messages"]),
    ]
    .into_iter()
    .find(|(_, paths)| {
        endpoints
            .iter()
            .any(|v| v.as_str().is_some_and(|s| paths.contains(&s)))
    })
    .map(|(wire, _)| wire)
}

/// Both editor and CLI transports must distinguish a new user turn from a
/// tool continuation. This header participates in vendor request accounting.
pub fn request_headers(body: &Value) -> reqwest::header::HeaderMap {
    let user = body["messages"]
        .as_array()
        .and_then(|m| m.last())
        .is_some_and(|m| {
            m["role"] == "user"
                && !m["content"]
                    .as_array()
                    .is_some_and(|a| a.iter().all(|b| b["type"] == "tool_result"))
        })
        || body["input"].is_string()
        || body["input"]
            .as_array()
            .and_then(|v| v.last())
            .is_some_and(|v| v["role"] == "user");
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "x-initiator",
        reqwest::header::HeaderValue::from_static(if user { "user" } else { "agent" }),
    );
    headers.insert(
        "openai-intent",
        reqwest::header::HeaderValue::from_static("conversation-panel"),
    );
    fn has_image(v: &Value) -> bool {
        match v {
            Value::Array(a) => a.iter().any(has_image),
            Value::Object(o) => {
                matches!(
                    o.get("type").and_then(Value::as_str),
                    Some("image" | "image_url" | "input_image")
                ) || o.values().any(has_image)
            }
            _ => false,
        }
    }
    if has_image(body) {
        headers.insert(
            "copilot-vision-request",
            reqwest::header::HeaderValue::from_static("true"),
        );
    }
    headers
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn user_turns_tool_continuations_and_images_have_the_same_accounting_contract() {
        for body in [
            json!({"messages":[{"role":"user","content":"hello"}]}),
            json!({"input":"hello"}),
        ] {
            assert_eq!(request_headers(&body)["x-initiator"], "user");
        }
        for body in [
            json!({"messages":[{"role":"tool","content":"done"}]}),
            json!({"messages":[{"role":"user","content":[{"type":"tool_result","tool_use_id":"a","content":"done"}]}]}),
            json!({"input":[{"type":"function_call_output","call_id":"a","output":"done"}]}),
        ] {
            assert_eq!(request_headers(&body)["x-initiator"], "agent");
        }
        assert!(
            !request_headers(&json!({"input":"a string mentioning image_url"}))
                .contains_key("copilot-vision-request")
        );
        assert_eq!(
            request_headers(
                &json!({"input":[{"role":"user","content":[{"type":"input_image","image_url":"data:synthetic"}]}]})
            )["copilot-vision-request"],
            "true"
        );
    }
    #[test]
    fn enterprise_tokens_remain_with_their_tenant() {
        assert_eq!(host("https://acme.ghe.com/").unwrap(), "acme.ghe.com");
        for bad in [
            "http://acme.ghe.com",
            "https://user@acme.ghe.com",
            "a.b.ghe.com",
            "acme.ghe.com.evil.test",
            "https://acme.ghe.com/?q=x",
        ] {
            assert!(host(bad).is_err(), "{bad}");
        }
        assert_eq!(
            user_url("acme.ghe.com"),
            "https://api.acme.ghe.com/copilot_internal/user"
        );
        assert_eq!(
            api(None, "acme.ghe.com").unwrap(),
            "https://copilot-api.acme.ghe.com"
        );
        assert!(api(Some("https://copilot-api.other.ghe.com"), "acme.ghe.com").is_err());
        assert!(api(Some("https://api.githubcopilot.com"), "acme.ghe.com").is_err());
        assert!(api(Some("https://api.githubcopilot.com/unexpected"), "").is_err());
    }
    #[test]
    fn catalog_endpoints_and_entitlements_control_the_wire() {
        assert_eq!(wire(&json!({})), Some("chat"));
        assert_eq!(
            wire(&json!({"supported_endpoints":["/responses"]})),
            Some("responses")
        );
        assert_eq!(
            wire(&json!({"supported_endpoints":["/v1/messages"]})),
            Some("anthropic")
        );
        assert_eq!(
            wire(&json!({"supported_endpoints":["/responses/ws"]})),
            None
        );
        assert_eq!(
            wire(&json!({"supported_endpoints":["/responses"],"policy":{"state":"disabled"}})),
            None
        );
        assert_eq!(wire(&json!({"capabilities":{"type":"embedding"}})), None);
    }
}
