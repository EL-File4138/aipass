use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn account(root: &std::path::Path) -> (Context, mpsc::Sender<Value>, mpsc::Receiver<Value>) {
    std::fs::write(root.join("auth.json"),json!({"https://auth.x.ai":{"key":"new","email":"alice","expires_at":"2040-01-01T00:00:00Z"}}).to_string()).unwrap();
    let (mut c, tx, rx) =
        super::super::tests::context(Client::builder().no_proxy().build().unwrap());
    c.provider = "grok".into();
    c.auth = cli_accounts::reference("grok", root).unwrap();
    c.native_token = Some(aipass_agent_protocol::SensitiveString::new("old"));
    (c, tx, rx)
}

async fn server(statuses: Vec<u16>) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/fixture", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        let mut requests = vec![];
        for status in statuses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let n = socket.read(&mut bytes).await.unwrap();
            requests.push(String::from_utf8_lossy(&bytes[..n]).to_string());
            let body = if status == 200 {
                r#"{"ok":true}"#
            } else {
                r#"{"error":"unauthorized"}"#
            };
            socket.write_all(format!("HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err(),
            "unexpected extra request"
        );
        requests
    });
    (url, handle)
}

#[tokio::test]
async fn read_only_401_retries_once_and_other_failures_do_not_refresh() {
    for statuses in [
        vec![401, 200],
        vec![401, 401],
        vec![403],
        vec![429],
        vec![500],
    ] {
        let root = tempfile::tempdir().unwrap();
        let (mut c, _tx, _rx) = account(root.path());
        let (url, handle) = server(statuses.clone()).await;
        let result = get(&mut c, &url).await;
        assert_eq!(result.is_ok(), statuses.last() == Some(&200));
        let requests = handle.await.unwrap();
        assert!(requests[0].contains("Bearer old"));
        if requests.len() == 2 {
            assert!(requests[1].contains("Bearer new"));
        } else {
            assert_eq!(c.token().unwrap(), "old");
        }
    }
}

#[tokio::test]
async fn generation_401_recovers_future_access_without_replaying() {
    let root = tempfile::tempdir().unwrap();
    let (mut c, tx, mut rx) = account(root.path());
    let (url, handle) = server(vec![401]).await;
    let response = send(sign(&c, c.client.post(url)).unwrap().body("fixture"))
        .await
        .unwrap();
    let peer = tokio::spawn(async move {
        let head = rx.recv().await.unwrap();
        assert_eq!(head["value"]["status"], 401);
        let data = rx.recv().await.unwrap();
        assert_eq!(data["type"], "data");
        tx.send(json!({"type":"ack","id":data["id"],"ok":true}))
            .await
            .unwrap();
        assert_eq!(rx.recv().await.unwrap()["type"], "end");
    });
    c.pipe(response).await.unwrap();
    peer.await.unwrap();
    assert_eq!(handle.await.unwrap().len(), 1);
    assert_eq!(c.token().unwrap(), "new");
    assert!(c.feedback.revoked.is_none());
}
