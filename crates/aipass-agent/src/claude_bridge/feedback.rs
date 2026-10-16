//! Decode only named vendor allowance windows; never forward CLI diagnostics.
use super::*;
use aipass_agent_protocol::ClaudeAllowanceObservation;
pub(super) fn observe(inner: &Inner, key: &str, info: &Value) {
    let windows = decode(info);
    if windows.is_empty() {
        return;
    }
    let Some((id, marker)) = inner
        .runs
        .lock()
        .ok()
        .and_then(|runs| runs.get(key).map(|run| (run.entry_id, run.marker.clone())))
    else {
        return;
    };
    // Local IPC can reload the runtime. Never hold the runs lock across it.
    if let Ok(client) = crate::AgentClient::for_vault(inner.vault.clone()) {
        let _ = client.request::<()>(&AgentRequest::ClaudeNativeObserve {
            entry_id: id,
            marker,
            windows,
        });
    }
}
fn decode(info: &Value) -> Vec<ClaudeAllowanceObservation> {
    let mut windows = Vec::new();
    let mut append = |scope: &str, value: &Value, rejected: bool| {
        if !matches!(
            scope,
            "five_hour"
                | "seven_day"
                | "seven_day_opus"
                | "seven_day_sonnet"
                | "seven_day_haiku"
                | "seven_day_fable"
                | "seven_day_overage_included"
        ) {
            return;
        }
        let used = value["utilization"]
            .as_f64()
            .or(if rejected { Some(1.0) } else { None });
        let Some(used) = used.filter(|v| v.is_finite() && (0.0..=1.0).contains(v)) else {
            return;
        };
        windows.push(ClaudeAllowanceObservation {
            scope: scope.into(),
            used_percent: used * 100.,
            resets_at: value["resetsAt"].as_i64(),
            observed_at: time::OffsetDateTime::now_utc().unix_timestamp(),
        });
    };
    for (scope, value) in info["unifiedWindows"].as_object().into_iter().flatten() {
        append(scope, value, false);
    }
    if let Some(scope) = info["rateLimitType"].as_str() {
        if info["unifiedWindows"].get(scope).is_none() {
            append(scope, info, info["status"] == "rejected");
        }
    }
    windows
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unknown_scopes_and_infers_exhaustion_only_for_explicit_rejection() {
        assert!(decode(&json!({"rateLimitType":"five_hour","status":"allowed"})).is_empty());
        assert_eq!(
            decode(&json!({"rateLimitType":"five_hour","status":"rejected"}))[0].used_percent,
            100.
        );
        let windows = decode(
            &json!({"unifiedWindows":{"seven_day_opus":{"utilization":0.4,"resetsAt":1800000000},"future_unknown":{"utilization":1},"seven_day":{"utilization":4}}}),
        );
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].scope, "seven_day_opus");
        assert_eq!(windows[0].used_percent, 40.);
    }
}
