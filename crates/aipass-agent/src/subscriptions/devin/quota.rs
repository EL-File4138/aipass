//! ProtoJSON quota fields omit default zero values on quota-billed plans.
use super::*;
pub(super) fn normalize(v: &Value) -> Result<Value> {
    let st = &v["userStatus"]["planStatus"];
    if !st.is_object() {
        return Err("Devin returned no plan status".into());
    }
    let num = |v: &Value| {
        v.as_f64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            .filter(|n| n.is_finite())
    };
    let mut windows = vec![];
    for (name, span, left, reset, hide) in [
        (
            "1 day",
            86400,
            "dailyQuotaRemainingPercent",
            "dailyQuotaResetAtUnix",
            "hideDailyQuota",
        ),
        (
            "7 days",
            604800,
            "weeklyQuotaRemainingPercent",
            "weeklyQuotaResetAtUnix",
            "hideWeeklyQuota",
        ),
    ] {
        if st["planInfo"][hide] == true {
            continue;
        }
        let omitted = st.get(left).is_none_or(Value::is_null);
        let remaining = num(&st[left]).or_else(|| {
            (omitted
                && st["planInfo"]["billingStrategy"] == "BILLING_STRATEGY_QUOTA"
                && num(&st[reset]).is_some_and(|t| t > 0.))
            .then_some(0.)
        });
        if let Some(n) = remaining.filter(|n| (0.0..=100.0).contains(n)) {
            let at = num(&st[reset])
                .and_then(|n| time::OffsetDateTime::from_unix_timestamp(n as i64).ok())
                .and_then(|t| {
                    t.format(&time::format_description::well_known::Rfc3339)
                        .ok()
                });
            windows.push(
                json!({"name":name,"used":(100.0-n).clamp(0.0,100.0),"span":span,"resetsAt":at}),
            );
        }
    }
    if let Some(limit) = num(&st["acuLimit"]).filter(|n| *n > 0.0) {
        windows.push(json!({"name":"ACUs","used":100.0*num(&st["acuConsumed"]).unwrap_or(0.0)/limit,"aside":true,"resetsAt":st["planEnd"]}));
    }
    let balance = num(&st["overageBalanceMicros"])
        .filter(|n| *n > 0.)
        .map(|n| format!("${:.2}", n / 1_000_000.));
    Ok(
        json!({"plan":st["planInfo"]["planName"],"until":st["planEnd"],"windows":windows,"balance":balance}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn omitted_proto_zero_exhausts_only_a_quota_billed_window_with_a_reset() {
        let usage = normalize(&json!({"userStatus":{"planStatus":{
            "planInfo":{"billingStrategy":"BILLING_STRATEGY_QUOTA"},
            "dailyQuotaRemainingPercent":100,"dailyQuotaResetAtUnix":"1800000000",
            "weeklyQuotaResetAtUnix":"1800000001"}}}))
        .unwrap();
        assert_eq!(usage["windows"].as_array().unwrap().len(), 2);
        assert_eq!(usage["windows"][0]["used"], 0.);
        assert_eq!(usage["windows"][1]["used"], 100.);
        assert_eq!(normalize(&json!({"userStatus":{"planStatus":{"planInfo":{"billingStrategy":"BILLING_STRATEGY_CREDITS"},"weeklyQuotaResetAtUnix":"1800000001"}}})).unwrap()["windows"], json!([]));
        assert_eq!(normalize(&json!({"userStatus":{"planStatus":{"planInfo":{"billingStrategy":"BILLING_STRATEGY_QUOTA","hideWeeklyQuota":true},"weeklyQuotaResetAtUnix":"1800000001"}}})).unwrap()["windows"], json!([]));
        assert!(normalize(&json!({})).is_err());
        for n in [-1., 101.] {
            assert_eq!(
                normalize(&json!({"userStatus":{"planStatus":{"dailyQuotaRemainingPercent":n}}}))
                    .unwrap()["windows"],
                json!([])
            );
        }
    }
}
