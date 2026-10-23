//! Credential-bound response observations persisted by the Agent's existing
//! encrypted, generation/revision checked account writer. No response text or
//! credentials are retained here.
use super::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct Feedback {
    pub(crate) revoked: Option<String>,
    pub(crate) windows: Vec<Window>,
    pub(crate) reset: Option<ResetAttempt>,
    pub(crate) history: Vec<HistoryItem>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct HistoryItem {
    pub(crate) digest: String,
    pub(crate) owner: uuid::Uuid,
    pub(crate) observed_at: u64,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ResetAttempt {
    pub(crate) request_id: uuid::Uuid,
    pub(crate) credit_id: Option<String>,
    pub(crate) outcome: Option<String>,
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct Window {
    pub(crate) name: String,
    pub(crate) used: f64,
    pub(crate) span: u64,
    pub(crate) resets_at: Option<i64>,
    pub(crate) observed_at: u64,
}
impl Feedback {
    pub(crate) fn valid(&self) -> bool {
        if self.history.len() > 4096
            || self.history.iter().any(|h| {
                h.digest.len() != 64
                    || !h.digest.bytes().all(|b| b.is_ascii_hexdigit())
                    || h.owner.is_nil()
                    || h.observed_at > now() / 1000 + 60
            })
        {
            return false;
        }
        if self.reset.as_ref().is_some_and(|r| {
            r.credit_id
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.len() > 512)
                || r.outcome.as_deref().is_some_and(|s| {
                    !matches!(
                        s,
                        "reset" | "nothingToReset" | "noCredit" | "alreadyRedeemed"
                    )
                })
        }) {
            return false;
        }
        self.revoked
            .as_ref()
            .is_none_or(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            && self.windows.len() <= 16
            && self.windows.iter().all(|w| {
                matches!(w.name.as_str(), "primary" | "secondary")
                    && w.used.is_finite()
                    && (0.0..=100.0).contains(&w.used)
                    && w.span > 0
                    && w.span <= 366 * 86400
                    && w.observed_at <= now() / 1000 + 60
            })
    }
    fn merge(&mut self, windows: Vec<Window>) -> bool {
        let before = self.clone();
        for w in windows {
            if let Some(old) = self.windows.iter_mut().find(|old| old.name == w.name) {
                if w.observed_at >= old.observed_at {
                    *old = w;
                }
            } else {
                self.windows.push(w);
            }
        }
        *self != before
    }
}

impl Context {
    fn credential_mark(&self) -> Result<String> {
        let mut hash = Sha256::new();
        hash.update(self.provider.as_bytes());
        hash.update([0]);
        hash.update(self.native_workspace.as_bytes());
        hash.update([0]);
        hash.update(self.token()?.as_bytes());
        Ok(format!("{:x}", hash.finalize()))
    }
    pub(super) fn check_revoked(&self) -> Result<()> {
        if self.feedback.revoked.as_deref() == Some(self.credential_mark()?.as_str()) {
            Err("Subscription sign-in was revoked; sign in again".into())
        } else {
            Ok(())
        }
    }
    pub(super) async fn save_feedback(&mut self) -> Result<()> {
        self.acknowledged(json!({"type":"feedback","value":self.feedback}))
            .await
    }
    pub(super) async fn served(&mut self) -> Result<()> {
        if self.feedback.revoked.take().is_some() {
            self.save_feedback().await?;
        }
        Ok(())
    }
    pub(super) async fn observe_headers(
        &mut self,
        headers: &reqwest::header::HeaderMap,
    ) -> Result<()> {
        if self.provider != "codex" {
            return Ok(());
        }
        let mut windows = Vec::new();
        for name in ["primary", "secondary"] {
            let number = |suffix: &str| {
                headers
                    .get(format!("x-codex-{name}-{suffix}"))
                    .and_then(|v| v.to_str().ok())
            };
            let Some(used) = number("used-percent")
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|n| n.is_finite() && *n >= 0.0)
                .map(|n| n.min(100.))
            else {
                continue;
            };
            let Some(span) = number("window-minutes")
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|n| *n > 0 && *n <= 366 * 1440)
                .map(|n| n * 60)
            else {
                continue;
            };
            windows.push(Window {
                name: name.into(),
                used,
                span,
                resets_at: number("reset-at").and_then(|v| v.parse().ok()),
                observed_at: now() / 1000,
            });
        }
        if self.feedback.merge(windows) {
            self.save_feedback().await?;
        }
        Ok(())
    }
    pub(super) async fn observe_event(&mut self, value: &Value) -> Result<()> {
        if self.provider != "codex" {
            return Ok(());
        }
        if let Some(owner) = self.history_owner {
            let items = if value["type"] == "response.output_item.done" {
                vec![&value["item"]]
            } else if matches!(
                value["type"].as_str(),
                Some("response.completed" | "response.incomplete")
            ) {
                value["response"]["output"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .collect()
            } else {
                Vec::new()
            };
            let at = now() / 1000;
            let mut changed = false;
            self.feedback
                .history
                .retain(|h| h.observed_at >= at.saturating_sub(86400));
            for encrypted in items
                .into_iter()
                .flat_map(aipass_proxy_conversion::response_item_ciphertexts)
            {
                let digest = format!("{:x}", Sha256::digest(encrypted.as_bytes()));
                if !self
                    .feedback
                    .history
                    .iter()
                    .any(|h| h.digest == digest && h.owner == owner)
                {
                    self.feedback.history.push(HistoryItem {
                        digest,
                        owner,
                        observed_at: at,
                    });
                    changed = true;
                }
            }
            if self.feedback.history.len() > 4096 {
                self.feedback
                    .history
                    .drain(..self.feedback.history.len() - 4096);
            }
            if changed {
                self.save_feedback().await?;
            }
        }
        if value["type"] != "codex.rate_limits" {
            return Ok(());
        }
        let limits = &value["rate_limits"];
        // Named model-specific buckets must not exhaust the entire account.
        if limits["limit_id"].as_str().is_some_and(|v| v != "codex") {
            return Ok(());
        }
        let mut windows = Vec::new();
        for name in ["primary", "secondary"] {
            let w = &limits[name];
            let Some(used) = w["used_percent"]
                .as_f64()
                .filter(|n| n.is_finite() && (0.0..=100.0).contains(n))
            else {
                continue;
            };
            let Some(span) = w["window_minutes"]
                .as_u64()
                .filter(|n| *n > 0 && *n <= 366 * 1440)
                .map(|n| n * 60)
            else {
                continue;
            };
            windows.push(Window {
                name: name.into(),
                used,
                span,
                resets_at: w["reset_at"].as_i64(),
                observed_at: now() / 1000,
            });
        }
        if self.feedback.merge(windows) {
            self.save_feedback().await?;
        }
        Ok(())
    }
    pub(super) async fn refused(&mut self, body: &[u8]) -> Result<()> {
        if self.provider == "grok" {
            let refused = self.credential_mark()?;
            // A 401 may precede the CLI's advertised expiry. Recover future
            // requests, without replaying the current generation.
            if cli_accounts::recover_grok(self).await.is_ok() {
                return Ok(());
            }
            if explicit_revocation(body) {
                self.feedback.revoked = Some(refused);
                self.save_feedback().await?;
            }
            return Ok(());
        }
        if !explicit_revocation(body) {
            return Ok(());
        }
        let refused = self.credential_mark()?;
        if self.provider == "codex" {
            // Recover only for future requests. This generation is never replayed.
            let _ =
                tokio::time::timeout(Duration::from_secs(20), cli_accounts::force_refresh(self))
                    .await;
            if self.credential_mark().is_ok_and(|mark| mark != refused) {
                return Ok(());
            }
        }
        self.feedback.revoked = Some(refused);
        self.save_feedback().await
    }
}

fn explicit_revocation(body: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return false;
    };
    let codes = [
        &value["error"]["code"],
        &value["error"]["type"],
        &value["code"],
        &value["error"],
    ];
    codes.iter().filter_map(|v| v.as_str()).any(|s| {
        matches!(
            s,
            "token_revoked"
                | "token_invalidated"
                | "refresh_token_reused"
                | "refresh_token_invalidated"
                | "refresh_token_expired"
                | "invalid_grant"
        )
    })
}

/// A bounded SSE observer; oversized/malformed events remain transport data
/// and cannot make the proxy buffer an unbounded model response.
#[derive(Default)]
pub(super) struct Events {
    pending: Vec<u8>,
    skipping: bool,
}
const MAX_EVENT: usize = 8 * 1024 * 1024;
impl Events {
    pub(super) fn push(&mut self, bytes: &[u8]) -> Vec<Value> {
        let mut events = Vec::new();
        for &byte in bytes {
            if self.pending.len() >= MAX_EVENT {
                self.pending.clear();
                self.skipping = true;
            }
            self.pending.push(byte);
            if byte != b'\n' {
                continue;
            }
            let blank = self.pending == b"\n" || self.pending == b"\r\n";
            if self.skipping {
                self.pending.clear();
                if blank {
                    self.skipping = false;
                }
                continue;
            }
            if self.pending.ends_with(b"\n\n") || self.pending.ends_with(b"\n\r\n") {
                let mut data = Vec::new();
                for line in self.pending.split(|b| *b == b'\n') {
                    let line = line.strip_suffix(b"\r").unwrap_or(line);
                    if let Some(line) = line.strip_prefix(b"data:") {
                        if !data.is_empty() {
                            data.push(b'\n');
                        }
                        data.extend_from_slice(line.strip_prefix(b" ").unwrap_or(line));
                    }
                }
                if let Ok(value) = serde_json::from_slice(&data) {
                    events.push(value);
                }
                self.pending.clear();
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn reasoning_and_compaction_record_only_account_bound_hashes() {
        for item in [
            json!({"type":"compaction","encrypted_content":"synthetic-private-ciphertext"}),
            json!({"type":"agent_message","encrypted_content":"synthetic-private-ciphertext"}),
            json!({"type":"message","content":[{"type":"encrypted_content","data":"synthetic-private-ciphertext"}]}),
        ] {
            let (mut c, input, mut output) = super::super::tests::context(Client::new());
            let owner = uuid::Uuid::new_v4();
            c.provider = "codex".into();
            c.history_owner = Some(owner);
            let task = tokio::spawn(async move {
                c.observe_event(&json!({"type":"response.output_item.done","item":item}))
                    .await
                    .unwrap();
                c.observe_event(&json!({"type":"response.completed","response":{"output":[item]}}))
                    .await
                    .unwrap();
                c.feedback.history.len()
            });
            let frame = output.recv().await.unwrap();
            assert_eq!(frame["value"]["history"][0]["owner"], owner.to_string());
            assert_eq!(
                frame["value"]["history"][0]["digest"]
                    .as_str()
                    .unwrap()
                    .len(),
                64
            );
            assert!(!frame.to_string().contains("synthetic-private-ciphertext"));
            input
                .send(json!({"type":"ack","id":frame["id"],"ok":true}))
                .await
                .unwrap();
            assert_eq!(task.await.unwrap(), 1);
        }
    }
    #[tokio::test]
    async fn passive_storage_failure_keeps_the_original_generation_and_never_replays_it() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let body = "data: {\"type\":\"response.completed\",\"response\":{\"output\":[]}}\n\n";
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                socket.read_exact(&mut byte).await.unwrap();
                request.push(byte[0]);
            }
            socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nx-codex-primary-used-percent: 100\r\nx-codex-primary-window-minutes: 300\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        });
        let client = Client::builder().no_proxy().build().unwrap();
        let response = client
            .get(format!("http://{address}"))
            .send()
            .await
            .unwrap();
        let (mut c, input, mut output) = super::super::tests::context(client);
        c.provider = "codex".into();
        let task = tokio::spawn(async move { c.pipe(response).await });
        let observed = output.recv().await.unwrap();
        assert_eq!(observed["type"], "feedback");
        input
            .send(json!({"type":"ack","id":observed["id"],"ok":false}))
            .await
            .unwrap();
        let headers = output.recv().await.unwrap();
        assert_eq!(headers["value"]["status"], 200);
        let mut received = Vec::new();
        loop {
            let frame = output.recv().await.unwrap();
            if frame["type"] == "end" {
                break;
            }
            assert_eq!(frame["type"], "data");
            received.extend(STANDARD.decode(frame["value"].as_str().unwrap()).unwrap());
            input
                .send(json!({"type":"ack","id":frame["id"],"ok":true}))
                .await
                .unwrap();
        }
        assert_eq!(received, body.as_bytes());
        task.await.unwrap().unwrap();
        server.await.unwrap();
    }
    #[tokio::test]
    async fn revocation_is_bound_to_the_exact_credential_and_durably_acknowledged() {
        let (mut c, tx, mut rx) = super::super::tests::context(Client::new());
        let task = tokio::spawn(async move {
            c.refused(br#"{"error":{"code":"token_revoked"}}"#)
                .await
                .unwrap();
            assert!(c.check_revoked().is_err());
            c.auth["access"] = json!("replacement-access");
            assert!(c.check_revoked().is_ok());
            c.served().await.unwrap();
            c.feedback.clone()
        });
        let frame = rx.recv().await.unwrap();
        assert_eq!(frame["type"], "feedback");
        assert_eq!(frame["value"]["revoked"].as_str().unwrap().len(), 64);
        assert!(!frame.to_string().contains("test-access"));
        tx.send(json!({"type":"ack","id":frame["id"],"ok":true}))
            .await
            .unwrap();
        let clear = rx.recv().await.unwrap();
        assert!(clear["value"]["revoked"].is_null());
        tx.send(json!({"type":"ack","id":clear["id"],"ok":true}))
            .await
            .unwrap();
        assert!(task.await.unwrap().revoked.is_none());
    }
    #[tokio::test]
    async fn codex_partial_response_observations_preserve_other_windows_and_scopes() {
        let (mut c, tx, mut rx) = super::super::tests::context(Client::new());
        c.provider = "codex".into();
        c.feedback.windows.push(Window {
            name: "secondary".into(),
            used: 20.,
            span: 604800,
            observed_at: 1,
            resets_at: None,
        });
        let task = tokio::spawn(async move {
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert("x-codex-primary-used-percent", "99.5".parse().unwrap());
            headers.insert("x-codex-primary-window-minutes", "300".parse().unwrap());
            c.observe_headers(&headers).await.unwrap();
            c.observe_event(&json!({"type":"codex.rate_limits","rate_limits":{"limit_id":"model-only","primary":{"used_percent":100,"window_minutes":300}}})).await.unwrap();
            c.feedback.clone()
        });
        let frame = rx.recv().await.unwrap();
        tx.send(json!({"type":"ack","id":frame["id"],"ok":true}))
            .await
            .unwrap();
        let feedback = task.await.unwrap();
        assert_eq!(feedback.windows.len(), 2);
        assert_eq!(feedback.windows[0].observed_at, 1);
        assert_eq!(feedback.windows[1].used, 99.5);
        assert_eq!(feedback.windows[1].span, 18000);
    }
    #[test]
    fn only_structured_revocation_is_a_durable_failure() {
        assert!(explicit_revocation(
            br#"{"error":{"code":"token_revoked"}}"#
        ));
        assert!(!explicit_revocation(
            br#"{"error":{"code":"token_expired"}}"#
        ));
        assert!(!explicit_revocation(
            br#"{"error":{"message":"invalid_grant quoted by a proxy"}}"#
        ));
        assert!(!explicit_revocation(b"invalid_grant"));
    }
    #[test]
    fn sse_observation_handles_chunk_boundaries_crlf_and_recovers_after_oversized_event() {
        let mut events = Events::default();
        assert!(events.push(b"data: {\"type\":").is_empty());
        assert_eq!(
            events.push(b"\"codex.rate_limits\"}\r\n\r\n")[0]["type"],
            "codex.rate_limits"
        );
        assert!(events.push(&vec![b'x'; MAX_EVENT + 100]).is_empty());
        assert!(events.push(b"\n\n").is_empty());
        assert_eq!(events.push(b"data: {}\n\n").len(), 1);
        assert_eq!(events.push(b"data: {}\n\r\n").len(), 1);
        assert_eq!(events.push(b"data: {}\r\n\n").len(), 1);
    }
}
