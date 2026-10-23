//! Read-only references to the ordinary Kimi desktop Work account.
use super::*;
const BASE: &str = "https://agent-gw.kimi.com/coding/v1";
const MEMBER: &str =
    "https://www.kimi.com/apiv2/kimi.gateway.membership.v2.MembershipService/GetSubscriptionStats";
const UA: &str = concat!("AIPass/", env!("CARGO_PKG_VERSION"), " KimiWork");

pub(super) fn directory() -> Result<PathBuf> {
    let h = home()?;
    let parent = if cfg!(target_os = "macos") {
        h.join("Library/Application Support")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or(h.join("AppData/Roaming"))
    } else {
        aipass_config_writers::xdg_config_home(&h)
    };
    Ok(parent.join("kimi-desktop/daimon-share/daimon"))
}

fn read(c: &Context) -> Result<crate::subscription_import::reader::PrivateAuth> {
    let dir = native_import::root(&c.auth)
        .map(Ok)
        .unwrap_or_else(directory)?;
    let v = crate::subscription_import::reader::PrivateAuth(read_json(&dir.join("config.json"))?);
    validate(&v.0)?;
    Ok(v)
}
fn validate(v: &Value) -> Result<&str> {
    let creds = &v["credentials"];
    let work = &creds["kimiCode"];
    let uid = s(&creds["kimiWeb"], "userId");
    if s(work, "baseUrl").trim_end_matches('/') != BASE
        || s(work, "apiKey").is_empty()
        || s(&creds["kimiWeb"], "accessToken").is_empty()
        || uid.is_empty()
    {
        return Err("Open Kimi desktop and sign in to Work, then reconnect Kimi Work".into());
    }
    Ok(uid)
}
fn identity(v: &Value) -> Result<String> {
    Ok(format!(
        "{:x}",
        sha2::Sha256::digest(format!("aipass:kimi-work:{}", validate(v)?))
    ))
}
pub(super) async fn login(c: &mut Context, _method: usize) -> Result<Value> {
    let data = read(c)?;
    let root = native_import::root(&c.auth)
        .map(Ok)
        .unwrap_or_else(directory)?;
    Ok(
        json!({"type":"oauth","accountId":identity(&data.0)?,"nativeSource":{"provider":"kimi-work","root":root,"selector":""},"nativeDevice":cli_accounts::device()?}),
    )
}
pub(super) async fn fresh(c: &mut Context) -> Result<()> {
    let data = read(c)?;
    if c.auth["accountId"] != identity(&data.0)? {
        return Err("Kimi Work account changed; reconnect explicitly".into());
    }
    c.native_token = Some(aipass_agent_protocol::SensitiveString::new(s(
        &data.0["credentials"]["kimiCode"],
        "apiKey",
    )));
    Ok(())
}
pub(super) async fn models(c: &Context) -> Result<Value> {
    let v = json_request(
        c.client
            .get(format!("{BASE}/models"))
            .bearer_auth(c.token()?)
            .header("User-Agent", UA),
    )
    .await?;
    parse_models(&v)
}
fn parse_models(v: &Value) -> Result<Value> {
    let list = v["data"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 100)
        .ok_or("Kimi Work returned no models")?;
    if v["has_more"] == true {
        return Err("Kimi Work returned an incomplete model catalog".into());
    }
    let mut out = json!({});
    for row in list {
        let id = s(row, "id");
        if id.is_empty() || id.len() > 256 || id.chars().any(|c| c.is_whitespace() || c == '/') {
            continue;
        }
        let mut m = model(
            id,
            row["display_name"].as_str().unwrap_or(id),
            "chat",
            BASE,
            row["context_length"].as_u64().unwrap_or(262144).min(262144),
            32768,
        );
        m["reasoning"] = json!(row["supports_reasoning"] == true);
        m["attachment"] = json!(row["supports_image_in"] == true);
        m["variants"] = json!({});
        for option in row["reasoningEffortOptions"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let effort = s(option, "effort")
                .trim_start_matches("REASONING_EFFORT_")
                .to_ascii_lowercase();
            if !effort.is_empty()
                && effort.len() <= 64
                && effort
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                m["variants"][&effort] = json!({"reasoningEffort":effort});
            }
        }
        out[id] = m;
    }
    if out.as_object().is_none_or(|o| o.is_empty()) {
        return Err("Kimi Work returned no supported models".into());
    }
    Ok(out)
}
pub(super) async fn usage(c: &Context) -> Result<Value> {
    let data = read(c)?;
    if c.auth["accountId"] != identity(&data.0)? {
        return Err("Kimi Work account changed; reconnect explicitly".into());
    }
    let v = json_request(
        c.client
            .post(MEMBER)
            .bearer_auth(s(&data.0["credentials"]["kimiWeb"], "accessToken"))
            .header("User-Agent", UA)
            .header("Connect-Protocol-Version", "1")
            .json(&json!({})),
    )
    .await?;
    let used = device::percent(&v["subscriptionBalance"]["amountUsedRatio"])
        .filter(|n| *n >= 0.)
        .ok_or("Kimi Work returned no membership allowance")?;
    Ok(
        json!({"plan":"Kimi Work","windows":[{"name":"Membership","used":(used*100.).clamp(0.,100.)}]}),
    )
}
pub(super) async fn generate(c: &mut Context, _model: &Value, mut body: Value) -> Result<()> {
    aipass_proxy_conversion::providers::kimi::normalize_tools(&mut body)?;
    c.pipe(
        send(
            c.client
                .post(format!("{BASE}/chat/completions"))
                .bearer_auth(c.token()?)
                .header("User-Agent", UA)
                .json(&body),
        )
        .await?,
    )
    .await
}

use std::path::PathBuf;
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn native_reference_never_copies_tokens_and_rereads_rotations() {
        let dir = tempfile::tempdir().unwrap();
        let write = |uid: &str, token: &str| {
            std::fs::write(dir.path().join("config.json"), json!({"credentials":{"kimiCode":{"baseUrl":BASE,"apiKey":token},"kimiWeb":{"userId":uid,"accessToken":"fake-web-secret"}}}).to_string()).unwrap()
        };
        write("alice", "fake-work-secret");
        let (mut c, _, _) = super::super::tests::context(Client::new());
        c.provider = "kimi-work".into();
        c.auth = json!({"nativeSource":{"provider":"kimi-work","root":dir.path(),"selector":""}});
        c.auth = login(&mut c, 0).await.unwrap();
        assert_eq!(c.auth["type"], "oauth");
        assert!(!c.auth.to_string().contains("secret"));
        write("alice", "fake-rotated-token");
        fresh(&mut c).await.unwrap();
        assert_eq!(c.token().unwrap(), "fake-rotated-token");
        write("bob", "foreign-token");
        assert!(fresh(&mut c).await.unwrap_err().contains("account changed"));
        assert_eq!(c.token().unwrap(), "fake-rotated-token");
    }
    #[test]
    fn work_identity_is_bound_to_the_work_gateway_and_user() {
        let mut v = json!({"credentials":{"kimiCode":{"baseUrl":BASE,"apiKey":"fake-work"},"kimiWeb":{"userId":"alice","accessToken":"fake-web"}}});
        let a = identity(&v).unwrap();
        v["credentials"]["kimiWeb"]["userId"] = json!("bob");
        assert_ne!(a, identity(&v).unwrap());
        v["credentials"]["kimiCode"]["baseUrl"] = json!("https://api.kimi.com/coding/v1");
        assert!(identity(&v).is_err());
    }
    #[test]
    fn work_models_keep_standard_context_and_vendor_efforts() {
        let v=parse_models(&json!({"data":[{"id":"k3-agent","context_length":1048576,"reasoningEffortOptions":[{"effort":"REASONING_EFFORT_HIGH"}]}]})).unwrap();
        assert_eq!(v["k3-agent"]["limit"]["context"], 262144);
        assert_eq!(v["k3-agent"]["variants"]["high"]["reasoningEffort"], "high");
        assert!(parse_models(&json!({"data":[],"has_more":true})).is_err());
    }
}
