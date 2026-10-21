use super::*;

/// Runs a mock upstream that captures one request and replies with the
/// given status, content type, and body.
pub(super) fn mock_upstream(
    status: &str,
    content_type: &str,
    body: String,
) -> (SocketAddr, std::thread::JoinHandle<(String, Vec<u8>)>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let status = status.to_string();
    let content_type = content_type.to_string();
    let handle = std::thread::spawn(move || {
        let (mut stream, captured) = accept_api_request(&listener);
        write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        captured
    });
    (addr, handle)
}

#[test]
fn discovery_probe_does_not_consume_the_expected_api_request() {
    let (address, upstream) = mock_upstream("200 OK", "application/json", "{}".into());
    let mut probe = std::net::TcpStream::connect(address).unwrap();
    probe
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    probe
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    probe.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 204"));
    let response = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .post(format!("http://{address}/v1/responses"))
        .body("expected-body")
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (headers, body) = upstream.join().unwrap();
    assert!(headers.starts_with("POST /v1/responses "));
    assert_eq!(body, b"expected-body");
}

#[test]
fn authenticated_root_requests_are_not_ignored() {
    let (address, upstream) = mock_upstream("200 OK", "application/json", "{}".into());
    let response = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("http://{address}/"))
        .bearer_auth("test-only")
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(upstream
        .join()
        .unwrap()
        .0
        .to_ascii_lowercase()
        .contains("authorization: bearer test-only"));
}

fn discovery_probe(headers: &str) -> bool {
    headers
        .lines()
        .next()
        .is_some_and(|line| line.starts_with("GET / HTTP/"))
        && !headers.lines().any(|line| {
            line.split_once(':').is_some_and(|(name, _)| {
                name.eq_ignore_ascii_case("authorization") || name.eq_ignore_ascii_case("x-api-key")
            })
        })
}
fn answer_probe(stream: &mut std::net::TcpStream) {
    let _ = stream
        .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
}
pub(super) fn accept_api_request(
    listener: &std::net::TcpListener,
) -> (std::net::TcpStream, (String, Vec<u8>)) {
    loop {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let request = read_http_request(&mut stream);
        if discovery_probe(&request.0) {
            answer_probe(&mut stream);
            continue;
        }
        return (stream, request);
    }
}
pub(super) fn assert_no_api_request(listener: &std::net::TcpListener) {
    listener.set_nonblocking(true).unwrap();
    loop {
        let mut stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return,
            Err(error) => panic!("unexpected listener failure: {error}"),
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let request = read_http_request(&mut stream);
        assert!(
            discovery_probe(&request.0),
            "unexpected upstream request: {}",
            request.0.lines().next().unwrap_or("")
        );
        answer_probe(&mut stream);
    }
}
#[test]
fn no_replay_guard_ignores_only_discovery_and_rejects_real_requests() {
    for (line, auth, allowed) in [
        ("GET / HTTP/1.1", "", true),
        ("POST /v1/responses HTTP/1.1", "", false),
        ("GET / HTTP/1.1", "Authorization: Bearer fixture\r\n", false),
        ("GET /token HTTP/1.1", "", false),
    ] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        write!(
            client,
            "{line}\r\nHost: localhost\r\n{auth}Content-Length: 0\r\n\r\n"
        )
        .unwrap();
        // A completed connect/write is not a synchronization barrier for a
        // nonblocking accept on every OS. Keep the actual guard running until
        // this client observes its response/EOF, so an empty accept queue can
        // never turn a real request into a false success.
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_guard = stop.clone();
        let guard = std::thread::spawn(move || {
            while !stop_guard.load(Ordering::Relaxed) {
                assert_no_api_request(&listener);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reply = Vec::new();
        let read = client.read_to_end(&mut reply);
        stop.store(true, Ordering::Relaxed);
        let result = guard.join();
        read.unwrap();
        assert_eq!(result.is_ok(), allowed, "{line}");
        if allowed {
            assert!(reply.starts_with(b"HTTP/1.1 204"));
        }
    }
}
