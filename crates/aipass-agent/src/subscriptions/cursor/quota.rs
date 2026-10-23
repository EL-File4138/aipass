//! Cursor's included model pools and optional on-demand spending.
use super::*;

pub(super) fn normalize(
    v: &Value,
    hard: Option<&Value>,
    models: &Value,
    plan: &Value,
) -> Result<Value> {
    let u = &v["planUsage"];
    if !u.is_object() {
        return Ok(json!({"plan":plan,"windows":[]}));
    }
    let bucket: std::collections::BTreeSet<_> = v["autoBucketModels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(pool_base)
        .collect();
    let mut candidates = vec!["auto".to_owned()];
    candidates.extend(
        models
            .as_object()
            .into_iter()
            .flat_map(|m| m.keys())
            .flat_map(|id| [id.clone(), id.split('@').next().unwrap_or(id).to_owned()]),
    );
    candidates.extend(
        v["autoBucketModels"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned),
    );
    let pool: std::collections::BTreeSet<_> = candidates
        .into_iter()
        .filter(|id| {
            let p = pool_base(id);
            p == "auto"
                || p == "default"
                || p.starts_with("composer")
                || ["grok-4.5", "grok-4.6", "grok-4.7"].contains(&p.as_str())
                || bucket.contains(&p)
        })
        .collect();
    let ms = v["billingCycleEnd"]
        .as_str()
        .and_then(|s| s.parse::<i64>().ok())
        .or(v["billingCycleEnd"].as_i64());
    let reset = ms
        .and_then(|ms| time::OffsetDateTime::from_unix_timestamp(ms / 1000).ok())
        .and_then(|t| {
            t.format(&time::format_description::well_known::Rfc3339)
                .ok()
        });
    let demand = on_demand(v, hard);
    let paid = demand.is_some();
    let mut windows = vec![];
    for (name, key, aside) in [
        ("Cursor Models", "autoPercentUsed", paid),
        ("Other Models", "apiPercentUsed", paid),
        ("Total", "totalPercentUsed", true),
    ] {
        if let Some(n) = device::percent(&u[key]).filter(|n| *n >= 0.0) {
            let mut w = json!({"name":name,"used":n,"resetsAt":reset,"aside":aside});
            if name == "Cursor Models" {
                w["models"] = json!(pool);
            }
            if name == "Other Models" {
                w["notModels"] = json!(pool);
            }
            windows.push(w);
        }
    }
    if let Some((used, Some(limit))) = demand {
        let mut w =
            json!({"name":"On-demand","used":(100.0*used/limit).clamp(0.0,100.0),"resetsAt":reset});
        // A spent overage budget cannot block still-included requests.
        let auto = device::percent(&u["autoPercentUsed"]).is_some_and(|n| n >= 100.0);
        let api = device::percent(&u["apiPercentUsed"]).is_some_and(|n| n >= 100.0);
        match (auto, api) {
            (true, false) => w["models"] = json!(pool),
            (false, true) => w["notModels"] = json!(pool),
            (false, false) => w["aside"] = json!(true),
            (true, true) => {}
        }
        windows.push(w);
    }
    Ok(json!({"plan":plan,"windows":windows}))
}

// Amounts within a pair use the same units. Period fields are cents;
// GetHardLimit is dollars. None means disabled or unknown, never permission.
fn on_demand(v: &Value, hard: Option<&Value>) -> Option<(f64, Option<f64>)> {
    let num = |v: &Value| device::percent(v).filter(|n| *n >= 0.0);
    let spend = &v["spendLimitUsage"];
    let used = num(&spend["individualUsed"]);
    let own = num(&spend["individualLimit"]);
    if spend["limitType"] == "team" {
        if let Some(limit) = own {
            return (limit > 0.0).then_some((used?, Some(limit)));
        }
        if hard
            .is_some_and(|h| h["noUsageBasedAllowed"] == true || num(&h["hardLimit"]) == Some(0.0))
        {
            return None;
        }
        let pool = num(&spend["pooledLimit"]);
        if let Some(limit) = pool.filter(|n| *n > 0.0) {
            return Some((num(&spend["pooledUsed"])?, Some(limit)));
        }
        return hard
            .filter(|h| num(&h["hardLimit"]).is_some_and(|n| n > 0.0))
            .map(|_| (0.0, None));
    }
    if let Some(hard) = hard {
        if hard["noUsageBasedAllowed"] == true {
            return None;
        }
        let limit = num(&hard["hardLimit"])?;
        if limit >= 2147483647.0 {
            return Some((0.0, None));
        }
        return (limit > 0.0).then_some((used? / 100.0, Some(limit)));
    }
    own.filter(|n| *n > 0.0)
        .zip(used)
        .map(|(cap, used)| (used, Some(cap)))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn read(spend: Value, hard: Option<Value>) -> Value {
        normalize(
            &json!({"planUsage":{"autoPercentUsed":100,"apiPercentUsed":100},
            "spendLimitUsage":spend}),
            hard.as_ref(),
            &json!({"grok-4.7@256k":{},"claude-opus":{}}),
            &json!("Pro"),
        )
        .unwrap()
    }
    #[test]
    fn team_and_personal_spending_use_the_correct_units_and_model_names() {
        let out = read(
            json!({"limitType":"team","individualLimit":50000,"individualUsed":12345}),
            None,
        );
        assert_eq!(out["windows"][0]["aside"], true);
        assert!(out["windows"][0]["models"]
            .as_array()
            .unwrap()
            .contains(&json!("grok-4.7")));
        assert!((out["windows"][2]["used"].as_f64().unwrap() - 24.69).abs() < 1e-8);
        let out = read(
            json!({"limitType":"user","individualUsed":500}),
            Some(json!({"hardLimit":20})),
        );
        assert_eq!(out["windows"][2]["used"], 25.0);
        let out = read(
            json!({"limitType":"team","pooledLimit":"20000","pooledUsed":15000,"individualUsed":100}),
            Some(json!({"hardLimit":200})),
        );
        assert_eq!(out["windows"][2]["used"], 75.0);
    }
    #[test]
    fn absent_or_disabled_overage_does_not_invent_spending_authority() {
        for (spend, hard) in [
            (json!({}), None),
            (json!({"individualLimit":100}), None),
            (
                json!({"individualUsed":0}),
                Some(json!({"hardLimit":100,"noUsageBasedAllowed":true})),
            ),
        ] {
            let out = read(spend, hard);
            assert_eq!(out["windows"].as_array().unwrap().len(), 2);
            assert_eq!(out["windows"][0]["aside"], false);
        }
    }
    #[test]
    fn exhausted_overage_only_gates_the_pool_that_needs_it() {
        let out = normalize(
            &json!({"planUsage":{"autoPercentUsed":10,"apiPercentUsed":100},
            "spendLimitUsage":{"individualUsed":2000}}),
            Some(&json!({"hardLimit":20})),
            &json!({"grok-4.7":{},"claude-opus":{}}),
            &Value::Null,
        )
        .unwrap();
        assert_eq!(out["windows"][2]["used"], 100.0);
        assert!(out["windows"][2]["notModels"]
            .as_array()
            .unwrap()
            .contains(&json!("grok-4.7")));
    }
}
