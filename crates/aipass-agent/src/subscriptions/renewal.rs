//! A rejected rotating grant is reread before it is marked unusable.
use super::*;

const RECONNECT: &str = "Subscription refresh authorization was rejected; reconnect this account";

impl Context {
    pub(super) async fn refresh_json(
        &mut self,
        field: &str,
        request: RequestBuilder,
    ) -> Result<Value> {
        if !matches!(field, "refresh" | "deviceRefresh") {
            return Err("invalid refresh credential field".into());
        }
        if self.auth["rejectedRefreshFields"]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v == field))
        {
            return Err(RECONNECT.into());
        }
        let rejected = SecretString::new(s(&self.auth, field));
        let (status, value) = raw_json(request).await?;
        if (200..300).contains(&status) {
            return Ok(value);
        }
        if !matches!(status, 400 | 401 | 403) || !rejected_grant(&value) {
            return Err(format!("subscription API returned HTTP {status}"));
        }
        self.reread_auth().await?;
        if s(&self.auth, field) != rejected.expose() {
            return Err("Subscription credentials changed during renewal; retry".into());
        }
        let mut next = self.auth.clone();
        let mut fields = next["rejectedRefreshFields"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !fields.iter().any(|v| v == field) {
            fields.push(json!(field));
        }
        next["rejectedRefreshFields"] = json!(fields);
        self.save(next).await?;
        Err(RECONNECT.into())
    }

    async fn reread_auth(&mut self) -> Result<()> {
        self.sequence += 1;
        self.emit(json!({"type":"reload","id":self.sequence}))
            .await?;
        let mut reply = crate::subscription_import::reader::PrivateAuth(
            tokio::time::timeout(Duration::from_secs(30), self.read())
                .await
                .map_err(|_| "credential reread timed out")??,
        );
        if reply.0["type"] != "reloaded" || reply.0["id"] != self.sequence {
            return Err("credential reread failed; retry later".into());
        }
        crate::community::ensure_owner(&self.auth, &reply.0["value"])?;
        native_import::erase(&mut self.auth);
        self.auth = reply.0["value"].take();
        Ok(())
    }
}

fn rejected_grant(v: &Value) -> bool {
    [&v["error"], &v["error"]["code"], &v["code"]]
        .into_iter()
        .filter_map(Value::as_str)
        .any(|s| {
            matches!(
                s,
                "invalid_grant"
                    | "refresh_token_reused"
                    | "refresh_token_invalidated"
                    | "refresh_token_expired"
            )
        })
}

pub(super) fn retain_rejections(before: &Value, after: &mut Value) {
    let mut fields = after["rejectedRefreshFields"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    fields.extend(
        before["rejectedRefreshFields"]
            .as_array()
            .into_iter()
            .flatten()
            .cloned(),
    );
    fields.retain(|field| {
        field.as_str().is_some_and(|field| {
            matches!(field, "refresh" | "deviceRefresh")
                && before[field] == after[field]
                && !s(after, field).is_empty()
        })
    });
    fields.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    fields.dedup();
    if fields.is_empty() {
        if let Some(map) = after.as_object_mut() {
            map.remove("rejectedRefreshFields");
        }
    } else {
        after["rejectedRefreshFields"] = json!(fields);
    }
}

#[cfg(test)]
mod tests;
