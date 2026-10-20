use super::*;
use futures_util::SinkExt;
use tokio_tungstenite::{
    client_async,
    tungstenite::{client::IntoClientRequest, Message},
    WebSocketStream,
};

#[tokio::test]
async fn websocket_search_uses_the_gateway_even_when_the_target_supports_native_ws() {
    let search = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = source(format!("http://{}", search.local_addr().unwrap()));
    let search_task = tokio::spawn(async move {
        let (mut socket, _) = search.accept().await.unwrap();
        let (_, body) = request(&mut socket).await;
        assert_eq!(body["query"], "current version");
        respond(
            &mut socket,
            "200 OK",
            json!({"results":[{"url":"https://example.com","content":"version two"}]}),
        )
        .await;
    });
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_url = format!("http://{}", upstream.local_addr().unwrap());
    let model_task = tokio::spawn(async move {
        for round in 0..2 {
            let (mut socket, _) = upstream.accept().await.unwrap();
            let (headers, _) = request(&mut socket).await;
            assert!(headers.starts_with("GET /v1/responses "), "{headers}");
            let key = headers
                .lines()
                .find_map(|line| line.strip_prefix("sec-websocket-key: "))
                .unwrap();
            let accept =
                tokio_tungstenite::tungstenite::handshake::derive_accept_key(key.as_bytes());
            socket.write_all(format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n").as_bytes()).await.unwrap();
            let mut ws = WebSocketStream::from_raw_socket(
                socket,
                tokio_tungstenite::tungstenite::protocol::Role::Server,
                None,
            )
            .await;
            let Message::Text(text) = ws.next().await.unwrap().unwrap() else {
                panic!("missing create event")
            };
            let body: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(body["tools"][0]["name"], "aipass_web_search");
            let output = if round == 0 {
                json!([{"type":"function_call","id":"fc_one","call_id":"call_one","name":"aipass_web_search","arguments":"{\"query\":\"current version\"}"}])
            } else {
                assert!(body["input"].to_string().contains("version two"));
                json!([{"type":"message","id":"msg_one","role":"assistant","content":[{"type":"output_text","text":"version two"}]}])
            };
            for event in [
                json!({"type":"response.created","response":{"id":format!("resp_{round}"),"status":"in_progress","output":[]}}),
                json!({"type":"response.completed","response":{"id":format!("resp_{round}"),"status":"completed","output":output,"usage":{"input_tokens":1,"output_tokens":1}}}),
            ] {
                ws.send(Message::Text(event.to_string().into()))
                    .await
                    .unwrap();
            }
        }
    });
    let mut route = single_target_route("ws-search-token", upstream_url, RetryPolicy::default());
    route.targets[0].supports_websockets = true;
    let mut config = RuntimeConfig::from_routes("", vec![route]);
    config.search.apis.push(api.config.clone());
    config.search_apis.push(api);
    let (_temp, _handle, url) = start(config);
    let mut req = url
        .replacen("http://", "ws://", 1)
        .into_client_request()
        .unwrap();
    *req.uri_mut() = format!(
        "{}/v1/responses",
        req.uri().to_string().trim_end_matches('/')
    )
    .parse()
    .unwrap();
    req.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer ws-search-token"),
    );
    let socket = tokio::net::TcpStream::connect(req.uri().authority().unwrap().as_str())
        .await
        .unwrap();
    let (mut ws, _) = client_async(req, socket).await.unwrap();
    ws.send(Message::Text(json!({"type":"response.create","model":"test","input":[{"role":"user","content":"find current version"}],"tools":[{"type":"web_search"}]}).to_string().into())).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let message = ws.next().await.unwrap().unwrap();
            let Message::Text(text) = message else {
                continue;
            };
            let event: Value = serde_json::from_str(&text).unwrap();
            assert_ne!(event["type"], "error", "{event}");
            assert!(!text.contains("aipass_web_search"));
            if event["type"] == "response.completed" {
                assert!(text.contains("version two"));
                break;
            }
        }
    })
    .await
    .unwrap();
    ws.close(None).await.unwrap();
    search_task.await.unwrap();
    model_task.await.unwrap();
}
