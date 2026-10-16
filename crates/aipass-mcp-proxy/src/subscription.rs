//! Subscription operations use the same typed Agent authority as the desktop.
use super::*;
use aipass_agent_protocol::{AgentRequest, ProviderRuntimeOptions, SubscriptionPolicy};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    entry_id: Uuid,
    #[serde(default)]
    policy: Option<SubscriptionPolicy>,
    #[serde(default)]
    request_id: Option<Uuid>,
    #[serde(default)]
    credit_id: Option<String>,
}
impl ProxyMcpServer {
    pub(super) fn subscription(&self, name: &str, arguments: Value) -> Result<Value> {
        let params: Parameters = serde_json::from_value(arguments)?;
        let agent = self.agent_client()?;
        if name == "subscription_reset" {
            return Ok(agent.request(&AgentRequest::SubscriptionReset {
                entry_id: params.entry_id,
                request_id: params.request_id,
                credit_id: params.credit_id,
            })?);
        }
        let mut options: ProviderRuntimeOptions =
            agent.request(&AgentRequest::ProviderRuntimeGet {
                id: params.entry_id,
            })?;
        if name == "subscription_policy_set" {
            options.subscription = params.policy.context("policy is required")?;
            options = agent.request(&AgentRequest::ProviderRuntimeSet {
                id: params.entry_id,
                options,
            })?;
        }
        Ok(serde_json::to_value(options.subscription)?)
    }
}
pub(super) fn tools() -> Vec<Value> {
    vec![
        json!({"name":"subscription_policy_get","description":"Read subscription spending and native CLI switching policy.",
            "inputSchema":{"type":"object","properties":{"entry_id":{"type":"string","format":"uuid"}},"required":["entry_id"],"additionalProperties":false}}),
        json!({"name":"subscription_policy_set","description":"Set explicit subscription policy. Paid credits and automatic reset spend the account's credits when enabled; native switching changes the official CLI login. All are disabled by default.",
            "inputSchema":{"type":"object","properties":{"entry_id":{"type":"string","format":"uuid"},"policy":{"type":"object","properties":{
                "allowPaidCredits":{"type":"boolean"},"autoResetCredits":{"type":"boolean"},"nativeSwitch":{"anyOf":[{"type":"null"},{"type":"object","properties":{"routeId":{"type":"string","format":"uuid"},"returnPreferred":{"type":"boolean"}},"required":["routeId","returnPreferred"],"additionalProperties":false}]}},"required":["allowPaidCredits","autoResetCredits","nativeSwitch"],"additionalProperties":false}},"required":["entry_id","policy"],"additionalProperties":false}}),
        json!({"name":"subscription_reset","description":"Spend one available Codex allowance reset credit now. An unfinished attempt is retried with the same idempotency key. This is a consequential account action.",
            "inputSchema":{"type":"object","properties":{"entry_id":{"type":"string","format":"uuid"},"request_id":{"type":"string","format":"uuid"},"credit_id":{"type":"string","maxLength":512}},"required":["entry_id"],"additionalProperties":false}}),
    ]
}
