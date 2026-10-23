//! Ollama's purchased credits and included allowances are separate pools.
use super::*;

pub(super) fn ollama(v: &Value) -> Result<Value> {
    let number = |v: &Value| device::percent(v).filter(|n| *n >= 0.0);
    let purchased = number(&v["purchased"]["balance_usd"]);
    let extra = purchased.is_some_and(|n| n > 0.0);
    let included = &v["included"];
    let mut windows = Vec::new();
    for (field, name, span) in [
        ("session", "Session limit", 0),
        ("weekly", "7 days", 604800),
    ] {
        let row = &included[field];
        if let Some(left) = number(&row["remaining_percent"]).filter(|n| *n <= 100.0) {
            windows.push(json!({"name":name,"used":100.0-left,"span":span,
                "resetsAt":device::date(&row["resets_at"]),"aside":extra}));
        }
    }
    if let Some((limit, left)) = number(&included["allowance_usd"])
        .filter(|n| *n > 0.0)
        .zip(number(&included["balance_usd"]))
    {
        let reset = device::date(&included["period"]["until"]);
        let span = reset
            .as_deref()
            .zip(device::date(&included["period"]["from"]))
            .and_then(|(end, start)| {
                let parse = |s| {
                    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
                        .ok()
                };
                u64::try_from((parse(end)? - parse(&start)?).whole_seconds())
                    .ok()
                    .filter(|n| *n > 0)
            });
        windows.push(
            json!({"name":"Allowance","used":(100.0*(1.0-left/limit)).clamp(0.0,100.0),
            "span":span,"resetsAt":reset,"aside":extra}),
        );
    }
    if purchased.is_none() && windows.is_empty() {
        return Err("Ollama returned no balance or allowance data".into());
    }
    Ok(json!({"plan":"Ollama Cloud","windows":windows,
        "balance":purchased.map(|n|format!("${n:.2}"))}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn included_allowance_and_purchased_credits_keep_distinct_meanings() {
        let out = ollama(&json!({"included":{"allowance_usd":100,"balance_usd":72.5,
            "period":{"from":"2026-10-01T00:00:00Z","until":"2026-11-01T00:00:00Z"}},
            "purchased":{"balance_usd":"25"}}))
        .unwrap();
        assert!((out["windows"][0]["used"].as_f64().unwrap() - 27.5).abs() < 1e-8);
        assert_eq!(out["windows"][0]["span"], 31 * 86400);
        assert_eq!(out["windows"][0]["aside"], true);
        assert_eq!(out["balance"], "$25.00");
        assert_eq!(
            ollama(&json!({"purchased":{"balance_usd":0}})).unwrap()["balance"],
            "$0.00"
        );
    }
    #[test]
    fn legacy_limits_preserve_zero_and_do_not_invent_missing_balances() {
        let out = ollama(&json!({"included":{"session":{"remaining_percent":0},
            "weekly":{"remaining_percent":40,"resets_at":"2026-11-01T00:00:00Z"}}}))
        .unwrap();
        assert_eq!(out["windows"][0]["used"], 100.0);
        assert_eq!(out["windows"][0]["aside"], false);
        assert_eq!(out["windows"][1]["used"], 60.0);
        assert!(out["balance"].is_null());
        for v in [
            json!({}),
            json!({"included":{"allowance_usd":100}}),
            json!({"included":{"weekly":{"remaining_percent":-1}}}),
        ] {
            assert!(ollama(&v).is_err());
        }
    }
}
