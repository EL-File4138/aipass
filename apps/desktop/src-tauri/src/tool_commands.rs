use crate::commands::agent_request_async;
use crate::models::{
    from_agent_tool_config_apply, from_agent_tool_config_preview, into_agent_tool_config_request,
    into_tool_id, ToolConfigApplyResponse, ToolConfigPreviewResponse, ToolConfigRequest,
    ToolConfigTool,
};
use crate::models::{from_tool_id, ToolDetection};
use crate::run_blocking;
use aipass_agent_protocol::{
    AgentRequest, ToolConfigApplyResponse as AgentToolConfigApplyResponse,
    ToolConfigPreviewResponse as AgentToolConfigPreviewResponse,
    ToolConfigProxyRequest as AgentToolConfigProxyRequest,
};
use tauri::AppHandle;
use uuid::Uuid;
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolConfigStatusResponse {
    tool: ToolConfigTool,
    state: String,
    entry_title: Option<String>,
    entry_id: Option<Uuid>,
    secret_id: Option<String>,
    mode: Option<aipass_agent_protocol::ToolConfigMode>,
    account_identity: Option<String>,
    operation_id: Option<Uuid>,
    message: Option<String>,
    overrides: Vec<String>,
}
#[tauri::command]
pub(crate) async fn agent_management_apply(
    app: AppHandle,
    request: ToolConfigRequest,
) -> Result<ToolConfigApplyResponse, String> {
    let response: AgentToolConfigApplyResponse = agent_request_async(
        app,
        AgentRequest::ToolConfigManagedApply {
            request: into_agent_tool_config_request(request),
        },
    )
    .await?;
    Ok(from_agent_tool_config_apply(response))
}
#[tauri::command]
pub(crate) async fn tool_config_preview(
    app: AppHandle,
    request: ToolConfigRequest,
) -> Result<ToolConfigPreviewResponse, String> {
    let response: AgentToolConfigPreviewResponse = agent_request_async(
        app,
        AgentRequest::ToolConfigPreview {
            request: into_agent_tool_config_request(request),
        },
    )
    .await?;
    Ok(from_agent_tool_config_preview(response))
}

#[tauri::command]
pub(crate) async fn tool_config_apply(
    app: AppHandle,
    request: ToolConfigRequest,
) -> Result<ToolConfigApplyResponse, String> {
    let response: AgentToolConfigApplyResponse = agent_request_async(
        app,
        AgentRequest::ToolConfigApply {
            request: into_agent_tool_config_request(request),
        },
    )
    .await?;
    Ok(from_agent_tool_config_apply(response))
}

#[tauri::command]
pub(crate) async fn tool_config_proxy_preview(
    app: AppHandle,
    tool: ToolConfigTool,
    route_id: Uuid,
) -> Result<ToolConfigPreviewResponse, String> {
    let response: AgentToolConfigPreviewResponse = agent_request_async(
        app,
        AgentRequest::ToolConfigProxyPreview {
            request: AgentToolConfigProxyRequest {
                tool: into_tool_id(tool),
                route_id,
            },
        },
    )
    .await?;
    Ok(from_agent_tool_config_preview(response))
}

#[tauri::command]
pub(crate) async fn tool_config_proxy_apply(
    app: AppHandle,
    tool: ToolConfigTool,
    route_id: Uuid,
) -> Result<ToolConfigApplyResponse, String> {
    let response: AgentToolConfigApplyResponse = agent_request_async(
        app,
        AgentRequest::ToolConfigProxyApply {
            request: AgentToolConfigProxyRequest {
                tool: into_tool_id(tool),
                route_id,
            },
        },
    )
    .await?;
    Ok(from_agent_tool_config_apply(response))
}

#[tauri::command]
pub(crate) async fn tool_config_status(
    app: AppHandle,
    tool: aipass_agent_protocol::ToolConfigTool,
) -> Result<ToolConfigStatusResponse, String> {
    let status: aipass_agent_protocol::ToolConfigStatus =
        agent_request_async(app, AgentRequest::ToolConfigStatus { tool }).await?;
    Ok(ToolConfigStatusResponse {
        tool: crate::models::from_agent_tool(status.tool),
        state: status.state,
        entry_title: status.entry_title,
        entry_id: status.entry_id,
        secret_id: status.secret_id,
        mode: status.mode,
        account_identity: status.account_identity,
        operation_id: status.operation_id,
        message: status.message,
        overrides: status.overrides,
    })
}
#[tauri::command]
pub(crate) async fn tool_config_rollback(
    app: AppHandle,
    operation_id: uuid::Uuid,
) -> Result<aipass_agent_protocol::ToolConfigApplyResponse, String> {
    agent_request_async(app, AgentRequest::ToolConfigRollback { operation_id }).await
}
#[tauri::command]
pub(crate) async fn tool_config_login_start(
    app: AppHandle,
    request: ToolConfigRequest,
) -> Result<aipass_agent_protocol::ToolConfigLoginStatus, String> {
    agent_request_async(
        app,
        AgentRequest::ToolConfigLoginStart {
            request: into_agent_tool_config_request(request),
        },
    )
    .await
}
#[tauri::command]
pub(crate) async fn tool_config_login_poll(
    app: AppHandle,
    ticket: uuid::Uuid,
) -> Result<aipass_agent_protocol::ToolConfigLoginStatus, String> {
    agent_request_async(app, AgentRequest::ToolConfigLoginPoll { ticket }).await
}
#[tauri::command]
pub(crate) async fn tool_config_login_code(
    app: AppHandle,
    ticket: uuid::Uuid,
    code: String,
) -> Result<(), String> {
    agent_request_async(
        app,
        AgentRequest::ToolConfigLoginCode {
            ticket,
            code: aipass_agent_protocol::SensitiveString::new(code),
        },
    )
    .await
}
#[tauri::command]
pub(crate) async fn tool_config_login_cancel(
    app: AppHandle,
    ticket: uuid::Uuid,
) -> Result<(), String> {
    crate::commands::agent_request_no_unlock_async(
        app,
        AgentRequest::ToolConfigLoginCancel { ticket },
    )
    .await
}

#[tauri::command]
pub(crate) async fn tool_config_detect() -> Result<Vec<ToolDetection>, String> {
    run_blocking(|| {
        Ok(aipass_config_writers::detect_tools()
            .into_iter()
            .map(|detection| ToolDetection {
                tool: from_tool_id(detection.tool),
                binary_found: detection.binary_found,
                local_runtime_found: detection.local_runtime_found,
                config_path: detection.config_path.map(|path| path.display().to_string()),
            })
            .collect())
    })
    .await
}
