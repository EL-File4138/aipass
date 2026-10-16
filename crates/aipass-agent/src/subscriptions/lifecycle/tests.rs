use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::oneshot,
};

#[tokio::test(start_paused = true)]
async fn setup_is_bounded_but_submitted_generation_is_not() {
    for generating in [false, true] {
        let (cancel, cancellation) = watch::channel(false);
        let (_refresh, refreshing) = watch::channel(false);
        let (_stage, generation) = watch::channel(generating);
        let task = tokio::spawn(run_cancellable(
            std::future::pending(),
            cancellation,
            refreshing,
            generation,
        ));
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(1201)).await;
        tokio::task::yield_now().await;
        if generating {
            assert!(
                !task.is_finished(),
                "submitted generation must survive the setup deadline"
            );
            cancel.send_replace(true);
            assert!(task.await.unwrap().unwrap_err().contains("cancelled"));
        } else {
            assert!(task.await.unwrap().unwrap_err().contains("timed out"));
        }
    }
}

#[tokio::test(start_paused = true)]
async fn dispatch_transition_and_rotation_ack_survive_setup_deadline() {
    let (_cancel, cancellation) = watch::channel(false);
    let (refresh, refreshing) = watch::channel(true);
    let (stage, generation) = watch::channel(false);
    let task = tokio::spawn(run_cancellable(
        std::future::pending(),
        cancellation,
        refreshing,
        generation,
    ));
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(1201)).await;
    tokio::task::yield_now().await;
    assert!(
        !task.is_finished(),
        "rotated credential must receive its durable ACK"
    );
    stage.send_replace(true);
    refresh.send_replace(false);
    tokio::task::yield_now().await;
    assert!(
        !task.is_finished(),
        "dispatch must retire the expired setup timer"
    );
    drop(_cancel);
    assert!(task.await.unwrap().unwrap_err().contains("cancelled"));
}

#[tokio::test]
async fn transport_waits_for_slow_headers_and_body() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (arrived, request_seen) = oneshot::channel();
    let (headers, send_headers) = oneshot::channel();
    let (body, send_body) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            request.push(socket.read_u8().await.unwrap());
        }
        arrived.send(()).unwrap();
        send_headers.await.unwrap();
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        send_body.await.unwrap();
        socket.write_all(b"ok").await.unwrap();
    });
    let client = transport(Some(&UpstreamProxyConfig {
        mode: aipass_proxy::UpstreamProxyMode::Direct,
        custom_url: None,
    }))
    .unwrap();
    let (received, headers_seen) = oneshot::channel();
    let request = tokio::spawn(async move {
        let response = client
            .get(format!("http://{address}/generate"))
            .send()
            .await
            .unwrap();
        received.send(()).unwrap();
        response.text().await.unwrap()
    });
    request_seen.await.unwrap();
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(91)).await;
    tokio::task::yield_now().await;
    assert!(!request.is_finished());
    headers.send(()).unwrap();
    headers_seen.await.unwrap();
    tokio::time::advance(Duration::from_secs(91)).await;
    tokio::task::yield_now().await;
    assert!(!request.is_finished());
    body.send(()).unwrap();
    assert_eq!(request.await.unwrap(), "ok");
    server.await.unwrap();
}
