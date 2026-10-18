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
        let (mut stream, captured) = loop {
            let (mut stream, _) = listener.accept().unwrap();
            let captured = read_http_request(&mut stream);
            let root_probe = captured
                .0
                .lines()
                .next()
                .is_some_and(|line| line.starts_with("GET / HTTP/"))
                && !captured.0.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, _)| {
                        name.eq_ignore_ascii_case("authorization")
                            || name.eq_ignore_ascii_case("x-api-key")
                    })
                });
            if root_probe {
                let _ = stream.write_all(
                    b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
                continue;
            }
            break (stream, captured);
        };
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
