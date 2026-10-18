//! Bounded OAuth device polling shared by account-owned subscriptions.
use super::*;

pub(super) const GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
pub(super) struct Fields(pub Vec<(String, String)>);
impl Drop for Fields {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        for (_, value) in &mut self.0 {
            value.zeroize();
        }
    }
}

pub(super) async fn authorize(c: &Context, value: &Value) -> Result<()> {
    let url = value["verification_uri_complete"]
        .as_str()
        .or(value["verification_uri"].as_str())
        .or(value["verification_url"].as_str())
        .ok_or("Device authorization returned no verification page")?;
    let parsed = url::Url::parse(url).map_err(|_| "Invalid verification page")?;
    if parsed.scheme() != "https" || !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Invalid verification page".into());
    }
    c.emit(json!({"type":"challenge","value":{"url":url,"instructions":"Approve the sign-in in your browser","userCode":value["user_code"],"method":"auto"}})).await
}

pub(super) async fn poll(
    c: &Context,
    endpoint: &str,
    fields: Fields,
    duration: u64,
    interval: u64,
) -> Result<Value> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(duration.clamp(1, 600));
    let mut delay = Duration::from_millis(interval.clamp(1000, 30000));
    loop {
        tokio::time::sleep(delay).await;
        if tokio::time::Instant::now() >= deadline {
            return Err("The sign-in code expired; start again".into());
        }
        let reply = raw_json(c.client.post(endpoint).form(&fields.0)).await;
        let (status, v) = match reply {
            Ok((status, _)) if status >= 500 => continue,
            Ok(reply) => reply,
            Err(_) => continue,
        };
        let state = v["error"].as_str().or(v["status"].as_str()).unwrap_or("");
        match state {
            "authorization_pending" | "pending" => continue,
            "slow_down" => {
                delay = (delay + Duration::from_secs(5)).min(Duration::from_secs(60));
                continue;
            }
            "" | "success" if (200..300).contains(&status) && !s(&v, "access_token").is_empty() => {
                return Ok(v)
            }
            _ => {
                return Err(format!(
                    "Device sign-in refused (HTTP {status}); start again"
                ))
            }
        }
    }
}

pub(super) fn token(v: &Value, previous: &Value) -> Result<Value> {
    let access = s(v, "access_token");
    if access.is_empty() {
        return Err("Sign-in returned no access token".into());
    }
    if v["expires_in"].as_u64().is_none_or(|n| n == 0) {
        return Err("Sign-in returned no token expiry".into());
    }
    let mut next = previous.clone();
    next["type"] = json!("oauth");
    next["access"] = json!(access);
    next["refresh"] = json!(v["refresh_token"]
        .as_str()
        .filter(|v| !v.is_empty())
        .unwrap_or(s(previous, "refresh")));
    if s(&next, "refresh").is_empty() {
        return Err("Sign-in returned no refresh grant".into());
    }
    next["expires"] = json!(expires_after(&v["expires_in"], 3600, 1000));
    Ok(next)
}

pub(super) fn percent(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|v| v.is_finite())
}

pub(super) fn date(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        if let Ok(t) =
            time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339)
        {
            return t
                .format(&time::format_description::well_known::Rfc3339)
                .ok();
        }
    }
    let n = value.as_i64().or_else(|| value.as_str()?.parse().ok())?;
    if n <= 0 {
        return None;
    }
    let seconds = if n > 1_000_000_000_000 { n / 1000 } else { n };
    time::OffsetDateTime::from_unix_timestamp(seconds)
        .ok()?
        .format(&time::format_description::well_known::Rfc3339)
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::{BodyExt, Full};
    use hyper_util::rt::TokioIo;

    #[tokio::test]
    async fn device_poll_obeys_pending_and_posts_the_bound_grant_once_per_poll() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/token", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for body in [
                json!({"error":"authorization_pending"}),
                json!({"access_token":"fake-access","refresh_token":"fake-rotation","expires_in":3600}),
            ] {
                let (socket, _) = listener.accept().await.unwrap();
                let service = hyper::service::service_fn(
                    move |request: hyper::Request<hyper::body::Incoming>| {
                        let reply = body.to_string();
                        async move {
                            assert_eq!(request.method(), hyper::Method::POST);
                            let text = String::from_utf8(
                                request
                                    .into_body()
                                    .collect()
                                    .await
                                    .unwrap()
                                    .to_bytes()
                                    .to_vec(),
                            )
                            .unwrap();
                            assert!(text.contains("device_code=account-bound-code"));
                            assert!(text.contains("code_verifier=bound-verifier"));
                            Ok::<_, std::convert::Infallible>(
                                hyper::Response::builder()
                                    .header("Content-Type", "application/json")
                                    .body(Full::new(bytes::Bytes::from(reply)))
                                    .unwrap(),
                            )
                        }
                    },
                );
                hyper::server::conn::http1::Builder::new()
                    .keep_alive(false)
                    .serve_connection(TokioIo::new(socket), service)
                    .await
                    .unwrap();
            }
        });
        let (c, _, _) = super::super::tests::context(Client::builder().no_proxy().build().unwrap());
        let value = poll(
            &c,
            &endpoint,
            Fields(vec![
                ("device_code".into(), "account-bound-code".into()),
                ("code_verifier".into(), "bound-verifier".into()),
            ]),
            30,
            1000,
        )
        .await
        .unwrap();
        assert_eq!(value["refresh_token"], "fake-rotation");
        server.await.unwrap();
    }
    #[test]
    fn rotation_retains_identity_and_rejects_unknown_expiry() {
        let before =
            json!({"accountId":"alice","refresh":"one-use","metadata":{"project":"project-a"}});
        let next = token(
            &json!({"access_token":"fresh","refresh_token":"rotated","expires_in":3600}),
            &before,
        )
        .unwrap();
        assert_eq!(next["accountId"], "alice");
        assert_eq!(next["metadata"], before["metadata"]);
        assert_eq!(next["refresh"], "rotated");
        assert!(token(&json!({"access_token":"fresh"}), &before).is_err());
        assert!(token(
            &json!({"access_token":"fresh","expires_in":3600}),
            &json!({})
        )
        .is_err());
    }
    #[test]
    fn timestamps_accept_vendor_numeric_strings_without_fabricating_epoch_dates() {
        assert_eq!(date(&json!(1800000000)), date(&json!("1800000000")));
        assert_eq!(
            date(&json!(1800000000000i64)),
            date(&json!("1800000000000"))
        );
        assert!(date(&json!(0)).is_none());
        assert!(date(&json!("invalid")).is_none());
    }
}
