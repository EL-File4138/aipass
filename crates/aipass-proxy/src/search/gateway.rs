//! Bounded internal tool rounds reuse routing, account ownership and accounting.
use super::*;
pub(super) fn native(route: &ResolvedRoute, protocol: ProxyProtocol) -> bool {
    !route.targets.is_empty()
        && route.targets.iter().filter(|t| t.config.enabled).all(|t| {
            t.config.effective_protocol(route.config.upstream_protocol) == protocol
                && match t.upstream_kind {
                    UpstreamKind::ClaudeSubscription => {
                        protocol == ProxyProtocol::AnthropicMessages
                    }
                    UpstreamKind::CodexSubscription => protocol == ProxyProtocol::OpenAiResponses,
                    UpstreamKind::Standard => {
                        reqwest::Url::parse(&t.config.base_url).is_ok_and(|u| match protocol {
                            ProxyProtocol::AnthropicMessages => matches!(
                                u.host_str(),
                                Some("api.anthropic.com" | "api.deepseek.com")
                            ),
                            ProxyProtocol::OpenAiResponses => matches!(
                                u.host_str(),
                                Some(
                                    "api.openai.com"
                                        | "api.x.ai"
                                        | "api.deepseek.com"
                                        | "open.bigmodel.cn"
                                        | "api.z.ai"
                                )
                            ),
                            _ => false,
                        })
                    }
                    _ => false,
                }
        })
}
pub(crate) async fn forward(
    mut request: ForwardRequest,
    state: RuntimeState,
    mut route: ResolvedRoute,
    pricing: Vec<ModelPricing>,
) -> Result<Response<BoxBody>, Infallible> {
    let protocol = route.config.inbound_protocol;
    let Some(mut payload) = request.body.json().await else {
        return forward_request_inner(request, state, route, pricing).await;
    };
    if request.image_operation.is_some() || native(&route, protocol) {
        return forward_request_inner(request, state, route, pricing).await;
    }
    let original = payload.clone();
    let restored = state
        .search_history
        .lock()
        .ok()
        .and_then(|mut h| h.restore(route.config.id, protocol, &original, &mut payload));
    if let Some(target) = restored {
        route.targets.retain(|t| t.config.id == target);
        if route.targets.is_empty() {
            return Ok(error_response(
                StatusCode::CONFLICT,
                "search conversation account is no longer available",
            ));
        }
    }
    // Conversation ownership survives changes to this turn's tool permissions.
    if !wire::asked(protocol, &payload) {
        if restored.is_some() {
            request.body = ReplayableRequestBody::Memory(Bytes::from(payload.to_string()));
        }
        return forward_request_inner(request, state, route, pricing).await;
    }
    let plan = match wire::Plan::prepare(protocol, &mut payload) {
        Ok(plan) => plan,
        Err(e) => return Ok(error_response(StatusCode::BAD_REQUEST, &e)),
    };
    if !plan.enabled {
        request.body = ReplayableRequestBody::Memory(Bytes::from(payload.to_string()));
        return forward_request_inner(request, state, route, pricing).await;
    }
    if !available(&state) {
        return Ok(error_response(StatusCode::BAD_REQUEST,"this upstream cannot search; configure a search API or search model in local proxy settings"));
    }
    let streaming = payload["stream"] == true;
    payload["stream"] = json!(false);
    if let Some(object) = payload.as_object_mut() {
        object.remove("stream_options");
    }
    let mut uses = 0;
    let mut usage = json!({});
    loop {
        let response = raw(&request, &state, route.clone(), &pricing, &payload).await;
        let mut response = match response {
            Ok(v) => v,
            Err(e) => match e {},
        };
        if !response.status().is_success() {
            return Ok(response);
        }
        let identity = response.extensions().get::<UpstreamIdentity>().copied();
        if let Some(identity) = identity {
            route.targets.retain(|t| t.config.id == identity.target_id);
        }
        let bytes = match read_body(response.body_mut(), 16 * 1024 * 1024).await {
            Ok(v) => v,
            Err(e) => return Ok(error_response(StatusCode::BAD_GATEWAY, &e)),
        };
        let mut value: Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => {
                return Ok(error_response(
                    StatusCode::BAD_GATEWAY,
                    "search upstream returned invalid JSON",
                ))
            }
        };
        let calls = match wire::calls(protocol, &value, &plan.name) {
            Ok(v) => v,
            Err(e) => return Ok(error_response(StatusCode::BAD_GATEWAY, &e)),
        };
        if calls.is_empty() {
            value["usage"] = wire::sum_usage(&usage, &value["usage"]);
            if uses > 0 {
                if let (Some(identity), Ok(mut history)) = (identity, state.search_history.lock()) {
                    history.remember(
                        route.config.id,
                        identity.target_id,
                        protocol,
                        &original,
                        &payload,
                        &value,
                    );
                }
            }
            let bytes = if streaming {
                super::render::sse(protocol, &value)
            } else {
                value.to_string()
            };
            let (mut parts, _) = response.into_parts();
            parts.headers.remove(header::CONTENT_LENGTH);
            parts.headers.remove(header::CONTENT_ENCODING);
            parts.headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(if streaming {
                    "text/event-stream"
                } else {
                    "application/json"
                }),
            );
            return Ok(Response::from_parts(
                parts,
                Full::new(Bytes::from(bytes))
                    .map_err(|e| match e {})
                    .boxed_unsync(),
            ));
        }
        usage = wire::sum_usage(&usage, &value["usage"]);
        if !plan.enabled || uses + calls.len() > plan.max_uses {
            return Ok(error_response(
                StatusCode::BAD_GATEWAY,
                "gateway search reached its tool-use limit",
            ));
        }
        let mut results = Vec::new();
        for (id, query) in calls {
            uses += 1;
            let searched = if plan.allowed.is_empty() && plan.blocked.is_empty() {
                execute(&state, &query).await
            } else {
                super::api::search(&state, &query).await
            };
            let value = match searched {
                Ok(v) => plan.filter(v),
                Err(e) => json!({"error":e}),
            };
            if value.to_string().len() > MAX_RESULT {
                return Ok(error_response(
                    StatusCode::BAD_GATEWAY,
                    "search result too large",
                ));
            }
            results.push((id, value));
        }
        if let Err(e) = wire::append(protocol, &mut payload, &value, &results) {
            return Ok(error_response(StatusCode::BAD_GATEWAY, &e));
        }
        if uses >= plan.max_uses {
            if let Some(tools) = payload["tools"].as_array_mut() {
                tools.retain(|t| t["name"] != plan.name && t["function"]["name"] != plan.name);
            }
        }
        // The search was fulfilled; a required/tool-specific choice must not
        // force repeated calls, and later rounds stay pinned to the same target.
    }
}
pub(super) async fn read_body(body: &mut BoxBody, limit: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|_| "search model response interrupted")?;
        if let Ok(bytes) = frame.into_data() {
            if out.len() + bytes.len() > limit {
                return Err("search model response too large".into());
            }
            out.extend_from_slice(&bytes);
        }
    }
    Ok(out)
}
pub(super) async fn raw(
    template: &ForwardRequest,
    state: &RuntimeState,
    route: ResolvedRoute,
    pricing: &[ModelPricing],
    payload: &Value,
) -> Result<Response<BoxBody>, Infallible> {
    let request = ForwardRequest {
        image_operation: None,
        websocket: false,
        upstream_pool: None,
        affinity_fallback: template.affinity_fallback.clone(),
        config_changed: template.config_changed.clone(),
        request_id: Uuid::new_v4(),
        method: http::Method::POST,
        request_query: template.request_query.clone(),
        incoming_headers: template.incoming_headers.clone(),
        body: ReplayableRequestBody::Memory(Bytes::from(payload.to_string())),
        started: Instant::now(),
        started_at: now_unix(),
    };
    forward_request_inner(request, state.clone(), route, pricing.to_vec()).await
}
