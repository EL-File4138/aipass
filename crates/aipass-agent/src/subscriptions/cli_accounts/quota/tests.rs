use super::*;

fn copilot(plan: &str, quota: Value) -> Value {
    copilot_usage(
        &json!({"copilot_plan":plan,"quota_reset_date":"2026-11-01",
            "quota_snapshots":{"premium_interactions":quota}}),
        &json!({"paid":{"premiumMultiplier":1},"free":{"premiumMultiplier":0}}),
    )
    .unwrap()
}

#[test]
fn copilot_pooled_enterprise_exhaustion_is_scoped_to_premium_models() {
    let out = copilot(
        "enterprise",
        json!({"unlimited":true,"has_quota":false,"entitlement":"0","overage_permitted":true}),
    );
    let w = &out["windows"][0];
    assert_eq!(w["used"], 100.0);
    assert_eq!(w["aside"], false);
    assert_eq!(w["models"], json!(["paid"]));
    assert_eq!(w["resetsAt"], "2026-11-01T00:00:00Z");
    assert_eq!(
        copilot("enterprise", json!({"unlimited":true,"has_quota":true}))["windows"],
        json!([])
    );
    assert_eq!(
        copilot("free", json!({"entitlement":"0","percent_remaining":0}))["windows"],
        json!([])
    );
}

#[test]
fn copilot_counts_override_rounded_percent_and_overage_respects_the_seat() {
    let out = copilot(
        "individual_max",
        json!({"entitlement":"20000","quota_remaining":19999.9,"percent_remaining":99.9}),
    );
    assert!((out["windows"][0]["used"].as_f64().unwrap() - 0.0005).abs() < 1e-8);
    for (plan, aside) in [
        ("individual", true),
        ("business", false),
        ("enterprise", false),
    ] {
        let out = copilot(
            plan,
            json!({"entitlement":300,"quota_remaining":0,"overage_permitted":true}),
        );
        assert_eq!(out["windows"][0]["used"], 100.0);
        assert_eq!(out["windows"][0]["aside"], aside);
    }
}

#[test]
fn copilot_free_chat_can_exhaust_but_completions_do_not_gate_chat_models() {
    let out = copilot_usage(
        &json!({"quota_snapshots":{
            "chat":{"entitlement":"200","quota_remaining":"0","has_quota":false},
            "completions":{"entitlement":4000,"percent_remaining":0}
        }}),
        &json!({}),
    )
    .unwrap();
    assert_eq!(out["windows"][0]["aside"], false);
    assert_eq!(out["windows"][1]["aside"], true);
}

#[test]
fn gemini_accepts_numeric_strings_but_rejects_invalid_or_unscoped_buckets() {
    let out = gemini_usage(&json!({"buckets":[
        {"modelId":"gemini-pro","remainingFraction":"0.25","resetTime":"2026-11-01T00:00:00Z"},
        {"modelId":"negative","remainingFraction":-0.1},
        {"modelId":"overflow","remainingFraction":1.1},
        {"remainingFraction":0}, {"modelId":"missing"}
    ]}))
    .unwrap();
    assert_eq!(out["windows"].as_array().unwrap().len(), 1);
    assert_eq!(out["windows"][0]["used"], 75.0);
    assert_eq!(out["windows"][0]["models"], json!(["gemini-pro"]));
    assert!(gemini_usage(&json!({"buckets":[{"modelId":"m","remainingFraction":"NaN"}]})).is_err());
    assert_eq!(
        gemini_usage(&json!({"buckets":[{"modelId":"m","remainingFraction":0}]})).unwrap()
            ["windows"][0]["used"],
        100.0
    );
}
