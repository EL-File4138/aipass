//! ProtoJSON quota fields omit default zero values on quota-billed plans.
use super::*;
pub(super) fn scope(
    usage: &mut Value,
    tiers: Option<&std::collections::BTreeMap<String, u64>>,
    models: &Value,
) {
    let mut free = std::collections::BTreeSet::new();
    if let Some(tiers) = tiers {
        free.extend(
            tiers
                .iter()
                .filter(|(_, tier)| **tier == 4)
                .map(|(id, _)| id.clone()),
        );
        for (id, model) in models.as_object().into_iter().flatten() {
            let Some(families) = model.get("devin_families") else {
                continue;
            };
            // A family alias is exempt only if every effort it can select is free.
            if [
                "", "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
            ]
            .iter()
            .all(|effort| {
                let variant =
                    aipass_proxy_conversion::providers::devin::variant(families, id, effort);
                tiers.get(&variant) == Some(&4)
            }) {
                free.insert(id.clone());
            }
        }
    }
    for window in usage["windows"].as_array_mut().into_iter().flatten() {
        if window["aside"] == true {
            continue;
        }
        if tiers.is_none() {
            // No current cost metadata means no verified account-wide scope.
            window["aside"] = json!(true);
        } else {
            window["notModels"] = json!(free);
        }
    }
}
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
    if let Some((limit, used)) = num(&st["acuLimit"])
        .filter(|n| *n > 0.0)
        .zip(num(&st["acuConsumed"]).filter(|n| *n >= 0.0))
    {
        windows.push(
            json!({"name":"ACUs","used":100.0*used/limit,"aside":true,"resetsAt":st["planEnd"]}),
        );
    }
    let balance = num(&st["overageBalanceMicros"])
        .filter(|n| *n >= 0.)
        .map(|n| format!("${:.2}", n / 1_000_000.));
    Ok(
        json!({"plan":st["planInfo"]["planName"],"until":st["planEnd"],"windows":windows,"balance":balance}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhausted_quota_keeps_free_variants_and_all_free_aliases_available() {
        let tiers = std::collections::BTreeMap::from([
            ("swe-low".into(), 4),
            ("swe-high".into(), 4),
            ("mixed-low".into(), 4),
            ("mixed-high".into(), 2),
        ]);
        let models = json!({"swe":{"devin_families":[{"uid":"swe","models":[{"id":"swe-low"},{"id":"swe-high"}]}]},"mixed":{"devin_families":[{"uid":"mixed","models":[{"id":"mixed-low"},{"id":"mixed-high"}]}]}});
        let mut usage = json!({"windows":[{"used":100},{"name":"ACUs","used":20,"aside":true}]});
        scope(&mut usage, Some(&tiers), &models);
        let excluded = usage["windows"][0]["notModels"].as_array().unwrap();
        assert!(excluded.contains(&json!("swe")));
        assert!(excluded.contains(&json!("mixed-low")));
        assert!(!excluded.contains(&json!("mixed")));
        assert!(!excluded.contains(&json!("mixed-high")));
        assert!(usage["windows"][1]["notModels"].is_null());
        scope(&mut usage, None, &models);
        assert_eq!(usage["windows"][0]["aside"], true);
        assert_eq!(usage["windows"][0]["used"], 100);
    }
    #[test]
    fn zero_balance_is_preserved_without_inventing_an_acu_counter() {
        let out = normalize(
            &json!({"userStatus":{"planStatus":{"acuLimit":100,"overageBalanceMicros":0}}}),
        )
        .unwrap();
        assert_eq!(out["balance"], "$0.00");
        assert_eq!(out["windows"], json!([]));
        let out = normalize(&json!({"userStatus":{"planStatus":{"acuLimit":100,"acuConsumed":0}}}))
            .unwrap();
        assert_eq!(out["windows"][0]["used"], 0.0);
        assert!(out["balance"].is_null());
    }
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
