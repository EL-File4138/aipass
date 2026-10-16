//! Official CLI reset-credit RPC and explicit paid-credit admission policy.
use super::*;
use crate::subscriptions::feedback::ResetAttempt;

pub(crate) async fn consume_reset(c: &mut Context, attempt: ResetAttempt) -> Result<Value> {
    if attempt
        .credit_id
        .as_ref()
        .is_some_and(|s| s.is_empty() || s.len() > 512)
    {
        return Err("Invalid reset credit".into());
    }
    if let Some(previous) = &c.feedback.reset {
        if previous.request_id == attempt.request_id {
            if previous.credit_id != attempt.credit_id {
                return Err("Reset operation changed".into());
            }
            if let Some(outcome) = &previous.outcome {
                return Ok(json!({"outcome":outcome}));
            }
        } else if previous.outcome.is_none() {
            return Err("A reset outcome is unknown; retry the original operation".into());
        }
    }
    let _guard = c.refresh_guard()?;
    let mut attempt = ResetAttempt {
        outcome: None,
        ..attempt
    };
    c.feedback.reset = Some(attempt.clone());
    c.save_feedback().await?;
    let path = root(&c.auth)?;
    let mut cmd = command(c, "codex", &path)?;
    cmd.arg("app-server");
    let result = rpc(
        cmd,
        c,
        "account/rateLimitResetCredit/consume",
        json!({"creditId":attempt.credit_id,"idempotencyKey":attempt.request_id.to_string()}),
        false,
        false,
    )
    .await?;
    let outcome = result["outcome"]
        .as_str()
        .filter(|s| {
            matches!(
                *s,
                "reset" | "nothingToReset" | "noCredit" | "alreadyRedeemed"
            )
        })
        .ok_or("Codex returned an unknown reset outcome; retry the original operation")?;
    attempt.outcome = Some(outcome.into());
    c.feedback.reset = Some(attempt);
    if matches!(outcome, "reset" | "alreadyRedeemed") {
        c.feedback.windows.clear();
    }
    c.save_feedback().await?;
    Ok(json!({"outcome":outcome}))
}

pub(super) async fn admit(c: &mut Context) -> Result<bool> {
    if c.policy.allow_paid_credits && !c.policy.auto_reset_credits {
        return Ok(true);
    }
    // Check the official account allowance immediately before submitting work.
    // This bounds cache lag; the vendor remains the authority on actual charges.
    let limits = backend::read_limits(c).await?;
    let mut usage = codex_usage(&limits)?;
    if exhausted(&usage) && c.policy.auto_reset_credits {
        if let Some(attempt) = reset_attempt(&c.feedback, &limits) {
            let result = consume_reset(c, attempt).await?;
            if matches!(
                result["outcome"].as_str(),
                Some("reset" | "alreadyRedeemed")
            ) {
                usage = codex_usage(&backend::read_limits(c).await?)?;
            }
        }
    }
    if c.policy.allow_paid_credits || allowance_available(&usage)? {
        return Ok(true);
    }
    c.headers(429, json!([["content-type", "application/json"]]))
        .await?;
    c.data(br#"{"error":{"type":"quota_exhausted","message":"Subscription allowance exhausted; paid credits are disabled"}}"#).await?;
    c.emit(json!({"type":"end"})).await?;
    Ok(false)
}

fn reset_attempt(
    feedback: &crate::subscriptions::feedback::Feedback,
    limits: &Value,
) -> Option<ResetAttempt> {
    // The last available credit may already have been consumed by an attempt
    // whose response was lost. Resolve its key even when the balance is zero.
    if let Some(pending) = feedback.reset.as_ref().filter(|r| r.outcome.is_none()) {
        return Some(pending.clone());
    }
    limits["rateLimitResetCredits"]["availableCount"]
        .as_u64()
        .filter(|n| *n > 0)
        .map(|_| ResetAttempt {
            request_id: uuid::Uuid::new_v4(),
            credit_id: None,
            outcome: None,
        })
}

fn allowance_available(usage: &Value) -> Result<bool> {
    if !usage["windows"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|w| w["aside"] != true && w["used"].as_f64().is_some_and(|n| n.is_finite() && n >= 0.))
    {
        return Err("Codex account allowance is unknown; paid credits are disabled".into());
    }
    Ok(!exhausted(usage))
}

pub(super) fn exhausted(usage: &Value) -> bool {
    usage["windows"].as_array().into_iter().flatten().any(|w| {
        w["aside"] != true
            && w["used"].as_f64().is_some_and(|n| n >= 100.)
            && w["resetsAt"]
                .as_str()
                .and_then(|s| {
                    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
                        .ok()
                })
                .is_none_or(|t| t > time::OffsetDateTime::now_utc())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_unknown_reset_retries_its_key_after_the_last_credit_disappears() {
        let mut feedback = crate::subscriptions::feedback::Feedback::default();
        let limits = json!({"rateLimitResetCredits":{"availableCount":0}});
        assert!(reset_attempt(&feedback, &limits).is_none());
        let id = uuid::Uuid::new_v4();
        feedback.reset = Some(ResetAttempt {
            request_id: id,
            credit_id: Some("last-credit".into()),
            outcome: None,
        });
        let retry = reset_attempt(&feedback, &limits).unwrap();
        assert_eq!(retry.request_id, id);
        assert_eq!(retry.credit_id.as_deref(), Some("last-credit"));
        feedback.reset.as_mut().unwrap().outcome = Some("reset".into());
        assert!(reset_attempt(&feedback, &limits).is_none());
    }
    #[test]
    fn unknown_or_model_only_allowance_cannot_authorize_paid_disabled_generation() {
        for usage in [
            json!({}),
            json!({"windows":[]}),
            json!({"windows":[{"used":0,"aside":true}]}),
        ] {
            assert!(allowance_available(&usage).is_err());
        }
        assert!(allowance_available(&json!({"windows":[{"used":0}]})).unwrap());
        assert!(!allowance_available(&json!({"windows":[{"used":100}]})).unwrap());
    }
    #[tokio::test]
    async fn completed_reset_is_idempotent_and_unknown_outcome_cannot_start_a_new_spend() {
        let (mut c, _input, _output) = super::super::super::tests::context(Client::new());
        let attempt = ResetAttempt {
            request_id: uuid::Uuid::new_v4(),
            credit_id: Some("credit-a".into()),
            outcome: Some("reset".into()),
        };
        c.feedback.reset = Some(attempt.clone());
        // There is deliberately no CLI home. A duplicate must use the receipt.
        assert_eq!(
            consume_reset(&mut c, attempt.clone()).await.unwrap()["outcome"],
            "reset"
        );
        let mut changed = attempt.clone();
        changed.credit_id = Some("credit-b".into());
        assert!(consume_reset(&mut c, changed)
            .await
            .unwrap_err()
            .contains("changed"));
        c.feedback.reset.as_mut().unwrap().outcome = None;
        let mut other = attempt;
        other.request_id = uuid::Uuid::new_v4();
        assert!(consume_reset(&mut c, other)
            .await
            .unwrap_err()
            .contains("unknown"));
    }
    #[tokio::test]
    async fn reset_never_reaches_the_cli_without_a_durable_intent_ack() {
        let (mut c, input, mut output) = super::super::super::tests::context(Client::new());
        let id = uuid::Uuid::new_v4();
        let task = tokio::spawn(async move {
            consume_reset(
                &mut c,
                ResetAttempt {
                    request_id: id,
                    credit_id: None,
                    outcome: None,
                },
            )
            .await
        });
        let frame = output.recv().await.unwrap();
        assert_eq!(frame["type"], "feedback");
        assert_eq!(frame["value"]["reset"]["requestId"], id.to_string());
        input
            .send(json!({"type":"ack","id":frame["id"],"ok":false}))
            .await
            .unwrap();
        assert!(task
            .await
            .unwrap()
            .unwrap_err()
            .contains("refused account update"));
    }
}
