use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn refused_request(c: &Context, status: u16, body: &'static str) -> RequestBuilder {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/token", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\r\n\r\n") {
            let mut chunk = [0; 1024];
            let n = socket.read(&mut chunk).await.unwrap();
            assert!(n > 0 && bytes.len() + n <= 8192);
            bytes.extend_from_slice(&chunk[..n]);
        }
        socket.write_all(format!("HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    });
    c.client.post(url)
}

#[tokio::test]
async fn rejected_grant_rereads_and_never_marks_a_newer_grant_rejected() {
    for rotated in [false, true] {
        let (mut c, input, mut output) =
            super::super::tests::context(Client::builder().no_proxy().build().unwrap());
        c.auth["refresh"] = json!("first-grant");
        let mut stored = c.auth.clone();
        if rotated {
            stored["refresh"] = json!("new-grant");
            stored["access"] = json!("new-access");
        }
        let peer = tokio::spawn(async move {
            let reload = output.recv().await.unwrap();
            assert_eq!(reload["type"], "reload");
            input
                .send(json!({"type":"reloaded","id":reload["id"],"value":stored}))
                .await
                .unwrap();
            if !rotated {
                let save = output.recv().await.unwrap();
                assert_eq!(save["type"], "auth");
                assert_eq!(save["value"]["refresh"], "first-grant");
                assert_eq!(save["value"]["rejectedRefreshFields"], json!(["refresh"]));
                input
                    .send(json!({"type":"ack","id":save["id"],"ok":true}))
                    .await
                    .unwrap();
            }
        });
        let request = refused_request(&c, 400, r#"{"error":"invalid_grant"}"#).await;
        let error = c.refresh_json("refresh", request).await.unwrap_err();
        peer.await.unwrap();
        if rotated {
            assert!(error.contains("changed during renewal"));
            assert!(c.auth["rejectedRefreshFields"].is_null());
            assert_eq!(c.auth["refresh"], "new-grant");
        } else {
            assert_eq!(error, RECONNECT);
            let request = c.client.post("http://127.0.0.1:1/must-not-send");
            assert_eq!(
                c.refresh_json("refresh", request).await.unwrap_err(),
                RECONNECT
            );
        }
    }
}

#[tokio::test]
async fn temporary_failures_do_not_mark_authorization_invalid() {
    let (mut c, _input, mut output) =
        super::super::tests::context(Client::builder().no_proxy().build().unwrap());
    c.auth["refresh"] = json!("grant");
    for (status, body) in [
        (429, r#"{"error":"invalid_grant"}"#),
        (403, r#"{"error":{"message":"invalid_grant"}}"#),
        (500, r#"{}"#),
    ] {
        let request = refused_request(&c, status, body).await;
        assert!(c.refresh_json("refresh", request).await.is_err());
        assert!(c.auth["rejectedRefreshFields"].is_null());
        assert!(output.try_recv().is_err());
    }
}

#[test]
fn rejected_fields_survive_metadata_updates_but_not_grant_replacement() {
    let before = json!({"refresh":"job","deviceRefresh":"device","rejectedRefreshFields":["refresh","deviceRefresh"]});
    let mut after = json!({"refresh":"job","deviceRefresh":"new-device","name":"updated"});
    retain_rejections(&before, &mut after);
    assert_eq!(after["rejectedRefreshFields"], json!(["refresh"]));
    after["refresh"] = json!("new-job");
    retain_rejections(&before, &mut after);
    assert!(after.get("rejectedRefreshFields").is_none());
}
