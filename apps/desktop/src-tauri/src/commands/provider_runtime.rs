//! Typed Agent bridge for provider runtime preferences and subscription reset.
use super::*;

#[tauri::command]
pub(crate) async fn provider_runtime_get(
    app: AppHandle,
    id: Uuid,
) -> Result<aipass_agent_protocol::ProviderRuntimeOptions, String> {
    agent_request_async(app, AgentRequest::ProviderRuntimeGet { id }).await
}
#[tauri::command]
pub(crate) async fn provider_runtime_set(
    app: AppHandle,
    id: Uuid,
    options: aipass_agent_protocol::ProviderRuntimeOptions,
) -> Result<aipass_agent_protocol::ProviderRuntimeOptions, String> {
    agent_request_async(app, AgentRequest::ProviderRuntimeSet { id, options }).await
}
#[tauri::command]
pub(crate) async fn provider_runtime_probe(
    app: AppHandle,
    id: Uuid,
) -> Result<serde_json::Value, String> {
    agent_request_async(app, AgentRequest::ProviderRuntimeProbe { id }).await
}
#[tauri::command]
pub(crate) async fn provider_webhook_test(
    app: AppHandle,
    id: Uuid,
    webhook_id: Uuid,
) -> Result<(), String> {
    agent_request_async(app, AgentRequest::ProviderWebhookTest { id, webhook_id }).await
}

#[tauri::command]
pub(crate) async fn subscription_reset(
    app: AppHandle,
    id: Uuid,
) -> Result<serde_json::Value, String> {
    agent_request_async(
        app,
        AgentRequest::SubscriptionReset {
            entry_id: id,
            request_id: None,
            credit_id: None,
        },
    )
    .await
}
