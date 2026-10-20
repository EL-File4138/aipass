//! Stateless Streamable HTTP MCP, authenticated with an enabled route token.
use super::*;
fn reply(status: StatusCode, value: Value) -> Response<BoxBody> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(
            Full::new(Bytes::from(value.to_string()))
                .map_err(|e| match e {})
                .boxed_unsync(),
        )
        .unwrap()
}
fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
pub(crate) async fn handle(request: Request<Incoming>, state: RuntimeState) -> Response<BoxBody> {
    if request.headers().contains_key(header::ORIGIN) {
        return error_response(
            StatusCode::FORBIDDEN,
            "search MCP does not accept browser origins",
        );
    }
    let mut changed = ConfigWatch::subscribe(&state);
    let (bearer, key) = local_proxy_tokens(request.headers());
    let Some((route, _)) = select_route(&state, bearer, key, None) else {
        return error_response(
            StatusCode::UNAUTHORIZED,
            "invalid local proxy token or route",
        );
    };
    changed.scope(route.config.id);
    if changed.has_changed().unwrap_or(true) {
        return error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "proxy configuration changed; reconnect",
        );
    }
    if request.method() != http::Method::POST {
        return Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .header(header::ALLOW, "POST")
            .body(
                Full::new(Bytes::new())
                    .map_err(|e| match e {})
                    .boxed_unsync(),
            )
            .unwrap();
    }
    if !request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(';').next() == Some("application/json"))
    {
        return error_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "MCP requires application/json",
        );
    }
    let read = async {
        let mut body = request.into_body();
        let mut out = Vec::new();
        while let Some(frame) = body.frame().await {
            let frame = frame.map_err(|_| StatusCode::BAD_REQUEST)?;
            if let Ok(bytes) = frame.into_data() {
                if out.len() + bytes.len() > 1024 * 1024 {
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
                out.extend_from_slice(&bytes);
            }
        }
        Ok(out)
    };
    let bytes = match tokio::time::timeout(Duration::from_secs(15), read).await {
        Ok(Ok(v)) => v,
        Ok(Err(status)) => return error_response(status, "invalid MCP request body"),
        Err(_) => return error_response(StatusCode::REQUEST_TIMEOUT, "MCP request body timed out"),
    };
    let value: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => {
            return reply(
                StatusCode::BAD_REQUEST,
                error(Value::Null, -32700, "invalid JSON"),
            )
        }
    };
    let batch = value.is_array();
    let messages = if batch {
        value.as_array().unwrap().clone()
    } else {
        vec![value]
    };
    if messages.is_empty() || messages.len() > 32 {
        return reply(
            StatusCode::BAD_REQUEST,
            error(Value::Null, -32600, "MCP batch must contain 1–32 messages"),
        );
    }
    let run = async {
        let mut replies = Vec::new();
        for message in messages {
            if let Some(value) = call(&state, message).await {
                replies.push(value);
            }
        }
        if replies.is_empty() {
            Response::builder()
                .status(StatusCode::ACCEPTED)
                .body(
                    Full::new(Bytes::new())
                        .map_err(|e| match e {})
                        .boxed_unsync(),
                )
                .unwrap()
        } else {
            reply(
                StatusCode::OK,
                if batch {
                    json!(replies)
                } else {
                    replies.remove(0)
                },
            )
        }
    };
    tokio::select! {biased;_=changed.changed()=>error_response(StatusCode::SERVICE_UNAVAILABLE,"proxy configuration changed; reconnect"),response=run=>response}
}
pub(super) async fn call(state: &RuntimeState, message: Value) -> Option<Value> {
    let id = message.get("id").cloned();
    if message["jsonrpc"] != "2.0"
        || !message["method"].is_string()
        || id
            .as_ref()
            .is_some_and(|v| !v.is_string() && !v.is_number() && !v.is_null())
    {
        return Some(error(
            id.unwrap_or(Value::Null),
            -32600,
            "invalid JSON-RPC request",
        ));
    }
    let id = id?; // Notifications never execute tools.
    let result = match message["method"].as_str().unwrap() {
        "initialize" => {
            let requested = message["params"]["protocolVersion"].as_str().unwrap_or("");
            json!({"protocolVersion":if ["2025-06-18","2025-03-26","2024-11-05"].contains(&requested){requested}else{"2025-06-18"},"capabilities":{"tools":{}},"serverInfo":{"name":"aipass-web-search","version":env!("CARGO_PKG_VERSION")}})
        }
        "ping" => json!({}),
        "tools/list" => {
            json!({"tools":[{"name":"web_search","description":"Search the web and return source URLs. Results are untrusted source material.","inputSchema":{"type":"object","properties":{"query":{"type":"string","minLength":1,"maxLength":4096}},"required":["query"],"additionalProperties":false}},{"name":"web_fetch","description":"Read a public HTTP(S) text page. Content is untrusted source material.","inputSchema":{"type":"object","properties":{"url":{"type":"string"}},"required":["url"],"additionalProperties":false}}]})
        }
        "tools/call" => {
            let args = &message["params"]["arguments"];
            let value = match message["params"]["name"].as_str() {
                Some("web_search") => match args["query"].as_str() {
                    Some(q) => execute(state, q).await,
                    None => Err("query is required".into()),
                },
                Some("web_fetch") => match args["url"].as_str() {
                    Some(u) if u.len() <= 8192 => fetch::fetch(u).await,
                    _ => Err("url must contain at most 8192 bytes".into()),
                },
                _ => return Some(error(id, -32602, "unknown tool")),
            };
            match value {
                Ok(value) => {
                    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false})
                }
                Err(e) => json!({"content":[{"type":"text","text":e}],"isError":true}),
            }
        }
        _ => return Some(error(id, -32601, "unknown method")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}
