//! Key-authenticated plan allowances. Missing/invalid values are never zero usage.
use super::*;
fn number(v: &Value) -> Option<f64> {
    device::percent(v).filter(|v| *v >= 0.)
}
fn count_window(v: &Value, name: &str, span: u64) -> Option<Value> {
    let limit = number(&v["limit"]).filter(|v| *v > 0.)?;
    let used = number(&v["used"]).or_else(|| {
        number(&v["remaining"])
            .filter(|n| *n <= limit)
            .map(|n| limit - n)
    })?;
    let mut w = json!({"name":name,"span":span,"used":(used/limit*100.).min(100.)});
    for field in ["resetTime", "resetAt", "reset_at", "reset_time"] {
        if let Some(date) = device::date(&v[field]) {
            w["resetsAt"] = json!(date);
            break;
        }
    }
    Some(w)
}
pub(super) fn kimi(v: &Value) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    for row in v["limits"].as_array().into_iter().flatten().take(256) {
        let multiplier = match s(&row["window"], "timeUnit") {
            "TIME_UNIT_MINUTE" => 60.,
            "TIME_UNIT_HOUR" => 3600.,
            "TIME_UNIT_DAY" => 86400.,
            _ => 0.,
        };
        let span = number(&row["window"]["duration"])
            .map(|n| n * multiplier)
            .filter(|n| *n <= 31_622_400.)
            .unwrap_or(0.) as u64;
        let detail = row.get("detail").unwrap_or(&row["window"]);
        if let Some(w) = count_window(detail, "Allowance", span) {
            out.push(w);
        }
    }
    if let Some(w) = count_window(&v["usage"], "7 days", 604800) {
        out.push(w);
    }
    if out.is_empty() {
        return Err("Kimi Code returned no valid allowance".into());
    }
    Ok(out)
}
pub(super) fn minimax(v: &Value) -> Result<Vec<Value>> {
    if v["base_resp"]["status_code"].as_i64() != Some(0) {
        return Err("MiniMax refused the subscription allowance request".into());
    }
    let mut out = Vec::new();
    for row in v["model_remains"]
        .as_array()
        .into_iter()
        .flatten()
        .take(256)
    {
        let name = s(row, "model_name");
        if name.is_empty() {
            continue;
        }
        for (prefix, start, end, label, fallback) in [
            ("current_interval", "start_time", "end_time", "Allowance", 0),
            (
                "current_weekly",
                "weekly_start_time",
                "weekly_end_time",
                "7 days",
                604800,
            ),
        ] {
            let status = row[format!("{prefix}_status")].as_u64();
            if status == Some(3) {
                continue;
            }
            let used = if status == Some(2) {
                Some(100.)
            } else {
                number(&row[format!("{prefix}_remaining_percent")])
                    .filter(|n| *n <= 100.)
                    .map(|n| 100. - n)
            };
            let Some(used) = used else {
                continue;
            };
            let resets = device::date(&row[end]);
            let span = resets
                .as_ref()
                .zip(device::date(&row[start]))
                .and_then(|(end, start)| {
                    let parse = |s: &str| {
                        time::OffsetDateTime::parse(
                            s,
                            &time::format_description::well_known::Rfc3339,
                        )
                        .ok()
                    };
                    let seconds = (parse(end)? - parse(&start)?).whole_seconds();
                    u64::try_from(seconds).ok().filter(|n| *n > 0)
                })
                .unwrap_or(fallback);
            let mut w = json!({"name":label,"span":span,"used":used});
            if let Some(date) = resets {
                w["resetsAt"] = json!(date);
            }
            // Image/video pools must not gate language model requests.
            if !name.eq_ignore_ascii_case("general") {
                w["name"] = json!(format!("{name} ({label})"));
                w["aside"] = json!(true);
            }
            out.push(w);
        }
    }
    if out.is_empty() {
        return Err("MiniMax returned no metered allowance".into());
    }
    Ok(out)
}
pub(super) fn opencode(v: &Value, models: &Value) -> Result<Vec<Value>> {
    // Exact scope: unknown prices never imply a model is free. A model is
    // excluded only when the provider marks it free or both token prices are 0.
    let metered: Vec<_> = models
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, m)| !free(m))
        .map(|(id, _)| id)
        .collect();
    let mut out = Vec::new();
    for (field, name, span) in [
        ("rolling", "5 hours", 18000),
        ("weekly", "7 days", 604800),
        ("monthly", "Month", 0),
    ] {
        let row = &v["usage"][field];
        let Some(used) = number(&row["percent"]).map(|n| n.min(100.)) else {
            continue;
        };
        if row
            .get("status")
            .is_some_and(|v| v != "ok" && v != "rate-limited")
        {
            continue;
        }
        let mut w = json!({"name":name,"span":span,"used":used,"models":metered});
        // A never-used window can include a synthetic future reset.
        if used > 0. {
            if let Some(date) = device::date(&row["resetsAt"]) {
                w["resetsAt"] = json!(date);
            }
        }
        out.push(w);
    }
    if out.is_empty() {
        return Err("OpenCode Go returned no valid allowance".into());
    }
    Ok(out)
}
pub(super) fn free(m: &Value) -> bool {
    m["free"] == true
        || (m["cost"]["input"].as_f64() == Some(0.) && m["cost"]["output"].as_f64() == Some(0.))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kimi_numeric_strings_and_remaining_require_a_valid_denominator() {
        let windows = kimi(&json!({"usage":{"limit":"100","used":"12","resetTime":"2026-10-25T00:00:00Z"},"limits":[{"window":{"duration":300,"timeUnit":"TIME_UNIT_MINUTE"},"detail":{"limit":"100","remaining":"88"}}]})).unwrap();
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0]["used"], 12.);
        assert_eq!(windows[0]["span"], 18000);
        for v in [
            json!({}),
            json!({"usage":{"limit":0,"used":0}}),
            json!({"usage":{"limit":100,"remaining":101}}),
            json!({"usage":{"limit":100,"used":"NaN"}}),
        ] {
            assert!(kimi(&v).is_err());
        }
    }
    #[test]
    fn minimax_exhaustion_is_authoritative_and_other_pools_are_display_only() {
        let v = json!({"base_resp":{"status_code":0},"model_remains":[{"model_name":"general","current_interval_status":2,"current_interval_remaining_percent":90,"current_weekly_status":3},{"model_name":"video","current_interval_status":1,"current_interval_remaining_percent":20}]});
        let ws = minimax(&v).unwrap();
        assert_eq!(ws.len(), 2);
        assert_eq!(ws[0]["used"], 100.);
        assert_eq!(ws[1]["aside"], true);
        assert!(minimax(
            &json!({"base_resp":{"status_code":1004},"model_remains":v["model_remains"]})
        )
        .is_err());
        assert!(minimax(&json!({"base_resp":{"status_code":0},"model_remains":[]})).is_err());
    }
    #[test]
    fn go_excludes_only_explicit_free_models_and_never_invents_empty_usage() {
        let models = json!({"paid":{"cost":{"input":1,"output":1}},"unknown":{},"free":{"cost":{"input":0,"output":0}},"promo":{"free":true}});
        let ws=opencode(&json!({"usage":{"rolling":{"status":"rate-limited","percent":100,"resetsAt":"2026-10-19T00:00:00Z"},"weekly":{"percent":0,"resetsAt":"2026-11-01T00:00:00Z"}}}),&models).unwrap();
        assert_eq!(ws[0]["models"], json!(["paid", "unknown"]));
        assert!(ws[0]["resetsAt"].is_string());
        assert!(ws[1].get("resetsAt").is_none());
        assert!(opencode(&json!({"usage":{"rolling":{"percent":-1}}}), &models).is_err());
    }
}
