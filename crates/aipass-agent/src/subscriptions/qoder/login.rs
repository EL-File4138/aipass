//! Qoder device login completes through polling, without a desktop callback.
use super::*;

fn login_url(web: &str, client: &str, challenge: &str, nonce: &str, machine: &str) -> url::Url {
    let mut url = url::Url::parse(&format!("{web}/device/selectAccounts")).unwrap();
    url.query_pairs_mut().extend_pairs([
        ("challenge", challenge),
        ("challenge_method", "S256"),
        ("nonce", nonce),
        ("machine_id", machine),
        ("client_id", client),
    ]);
    url
}

pub(crate) async fn login(c: &mut Context, _: usize) -> Result<Value> {
    let (web, api, _, client) = site(c);
    let verifier = format!("{}{}", hex_id(), hex_id());
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let nonce = uuid::Uuid::new_v4().to_string();
    let machine = uuid::Uuid::new_v4().to_string();
    let url = login_url(web, client, &challenge, &nonce, &machine);
    c.challenge(
        url.as_str(),
        "Sign in to Qoder and authorize this device.",
        false,
    )
    .await?;
    let deadline = now() + 900000;
    while now() < deadline {
        let result = raw_json(
            c.client
                .get(format!("{api}/api/v1/deviceToken/poll"))
                .query(&[
                    ("nonce", nonce.as_str()),
                    ("verifier", &verifier),
                    ("challenge_method", "S256"),
                ]),
        )
        .await;
        if let Ok((200, dt)) = result {
            if !s(&dt, "token").is_empty() {
                let (status, jt) = raw_json(
                    c.client
                        .post(format!("{api}/api/v1/me/jobToken"))
                        .bearer_auth(s(&dt, "token"))
                        .json(&json!({"clientId":client})),
                )
                .await?;
                let device =
                    status != 200 && c.provider == "qoder-cn" && (400..500).contains(&status);
                if status != 200 && !device {
                    return Err(format!("Qoder job token returned HTTP {status}"));
                }
                let chat = if device { &dt } else { &jt };
                if s(chat, "token").is_empty() {
                    return Err("Qoder returned no chat token".into());
                }
                let who = json_request(
                    c.client
                        .get(format!("{api}/api/v1/userinfo"))
                        .bearer_auth(s(&dt, "token")),
                )
                .await
                .unwrap_or(json!({}));
                return Ok(
                    json!({"type":"oauth","access":chat["token"],"refresh":chat["refresh_token"],"expires":if device{device_expiry(chat)}else{expires_after(&chat["expires_in"],86400000,1)},"deviceChat":device,"accountId":dt["user_id"],"uid":dt["user_id"],"email":who["email"],"name":who["name"].as_str().or(dt["user_name"].as_str()).unwrap_or(""),"machineId":machine,"deviceToken":dt["token"],"deviceRefresh":dt["refresh_token"]}),
                );
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    Err("Qoder sign-in timed out".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_regions_keep_device_correlation_without_desktop_redirect() {
        let (mut c, _, _) = super::super::super::tests::context(Client::new());
        for (provider, host) in [("qoder", "qoder.com"), ("qoder-cn", "qoder.cn")] {
            c.provider = provider.into();
            let (web, _, _, client) = site(&c);
            let url = login_url(
                web,
                client,
                "fixture-challenge",
                "fixture-nonce",
                "fixture-machine",
            );
            assert_eq!(url.host_str(), Some(host));
            assert_eq!(url.path(), "/device/selectAccounts");
            let q: std::collections::HashMap<_, _> = url.query_pairs().collect();
            assert_eq!(q["challenge"], "fixture-challenge");
            assert_eq!(q["challenge_method"], "S256");
            assert_eq!(q["nonce"], "fixture-nonce");
            assert_eq!(q["machine_id"], "fixture-machine");
            assert_eq!(q["client_id"], client);
            assert!(!q.contains_key("redirect_uri"));
        }
    }
}
