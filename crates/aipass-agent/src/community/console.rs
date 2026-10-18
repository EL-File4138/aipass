//! Fixed provider/region destinations, never remote-supplied URLs.
use super::*;
pub(super) fn url(provider: &str, region: &str) -> Option<String> {
    Some(match provider {
        "ollama-cloud" => "https://ollama.com/settings/usage",
        "kilo-gateway" => "https://app.kilo.ai",
        "opencode-go" | "opencode-zen" => "https://opencode.ai/console",
        "kimi-code" | "kimi-code-cn" => "https://www.kimi.com/code/console",
        "minimax-m-plan" => "https://platform.minimax.io/subscribe/token-plan",
        "minimax-m-plan-cn" => "https://platform.minimaxi.com/subscribe/token-plan",
        "mimo-plan" => "https://platform.xiaomimimo.com/token-plan",
        "stepfun-plan-cn" => "https://platform.stepfun.com/interface-key",
        "stepfun-plan" => "https://platform.stepfun.ai/interface-key",
        "qwen-token-plan" => "https://platform.qianwenai.com/home/api-keys",
        "bailian-token-plan" | "bailian-coding-plan" => match region {
            "cn" => "https://bailian.console.aliyun.com",
            "intl" if provider == "bailian-coding-plan" => "https://modelstudio.console.alibabacloud.com/ap-southeast-1/subscription/coding-plan",
            "intl" => "https://modelstudio.console.alibabacloud.com/ap-southeast-1/subscription/token-plan",
            _ => return None,
        },
        "volcengine-coding-plan" | "volcengine-agent-plan" => "https://ark.volcengine.com/region:cn-beijing/apikey",
        "tencent-token-plan" | "tencent-token-plan-enterprise" => "https://console.cloud.tencent.com/tokenhub/tokenplan",
        _ => return None,
    }.to_owned())
}
pub(super) fn account_url(account: &Account) -> Option<String> {
    let auth = crate::subscription_import::reader::PrivateAuth(
        serde_json::from_str(account.auth.expose()).ok()?,
    );
    url(
        &account.provider,
        auth.0["metadata"]["region"].as_str().unwrap_or("cn"),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn region_is_bound_and_unknown_regions_never_open_another_console() {
        assert_ne!(
            url("bailian-coding-plan", "cn"),
            url("bailian-coding-plan", "intl")
        );
        assert!(url("bailian-token-plan", "intl")
            .unwrap()
            .contains("token-plan"));
        assert!(url("bailian-coding-plan", "intl")
            .unwrap()
            .contains("coding-plan"));
        assert!(url("bailian-coding-plan", "arbitrary-host").is_none());
    }
}
