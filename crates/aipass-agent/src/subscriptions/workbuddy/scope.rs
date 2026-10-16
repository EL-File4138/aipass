//! WorkBuddy personal and enterprise accounts have distinct routing authority.
use super::*;
pub(super) fn bind(auth: &mut Value, account: &Value) -> Result<()> {
    let uid = identifier(&account["uid"]);
    if uid.is_empty() {
        return Err("WorkBuddy account identity missing".into());
    }
    let enterprise = identifier(&account["enterpriseId"]);
    let enterprise = enterprise.trim();
    if account["type"]
        .as_str()
        .is_some_and(|t| !t.is_empty() && !t.eq_ignore_ascii_case("personal"))
        && enterprise.is_empty()
    {
        return Err(
            "WorkBuddy enterprise identity missing; select the team in WorkBuddy and reconnect"
                .into(),
        );
    }
    auth["uid"] = json!(uid);
    auth["enterpriseId"] = json!(enterprise);
    auth["accountId"] = if enterprise.is_empty() {
        json!(uid)
    } else {
        json!(format!("{uid}::enterprise::{enterprise}"))
    };
    Ok(())
}
pub(super) fn usage(sum: &Value, enterprise: bool, models: &Value) -> Result<Value> {
    let number = |v: &Value| {
        v.as_f64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            .filter(|n| n.is_finite() && *n >= 0.)
    };
    let (used, total) = if enterprise {
        (
            number(&sum["credit"]).ok_or("WorkBuddy returned no enterprise usage")?,
            number(&sum["limitNum"]).ok_or("WorkBuddy returned no enterprise quota")?,
        )
    } else {
        let mut used = 0.;
        let mut total = 0.;
        for package in sum["Packages"].as_array().into_iter().flatten() {
            let capacity = number(&package["CycleTotalCapacity"])
                .ok_or("WorkBuddy returned incomplete package quota")?;
            if capacity > 0. {
                total += capacity;
                used += number(&package["CycleUsedCapacity"])
                    .ok_or("WorkBuddy returned incomplete package usage")?;
            }
        }
        (used, total)
    };
    if !used.is_finite() || !total.is_finite() {
        return Err("WorkBuddy returned invalid quota totals".into());
    }
    let free = models
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, m)| m["free"] == true)
        .map(|(id, _)| id.to_owned())
        .collect::<Vec<_>>();
    let mut windows = Vec::new();
    if total > 0. {
        let mut window = json!({"name":"Credits","used":100. * used / total});
        if !free.is_empty() {
            window["notModels"] = json!(free);
        }
        if enterprise {
            let parse = |field| {
                sum[field].as_str().and_then(|s| {
                    time::OffsetDateTime::parse(
                        &format!("{}+08:00", s.replace(' ', "T")),
                        &time::format_description::well_known::Rfc3339,
                    )
                    .ok()
                })
            };
            if let Some(reset) = parse("cycleResetTime") {
                window["resetsAt"] = json!(reset
                    .format(&time::format_description::well_known::Rfc3339)
                    .ok());
                if let Some(start) = parse("cycleStartTime") {
                    if reset > start {
                        window["span"] = json!((reset - start).whole_seconds());
                    }
                }
            }
        }
        windows.push(window);
    }
    Ok(
        json!({"plan":if enterprise {"Enterprise"} else if sum["IsPaidUser"] == true {"Pro"} else {"Free"},"windows":windows}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enterprise_identity_never_falls_back_to_personal_or_a_different_team() {
        let mut a = json!({"type":"oauth","access":"synthetic"});
        bind(
            &mut a,
            &json!({"uid":"same","enterpriseId":"team-a","type":"enterprise"}),
        )
        .unwrap();
        let mut b = a.clone();
        bind(
            &mut b,
            &json!({"uid":"same","enterpriseId":"team-b","type":"enterprise"}),
        )
        .unwrap();
        assert_ne!(a["accountId"], b["accountId"]);
        assert!(crate::community::ensure_owner(&a, &b).is_err());
        assert!(bind(&mut b, &json!({"uid":"same","type":"enterprise"})).is_err());
        bind(&mut b, &json!({"uid":123,"enterpriseId":456})).unwrap();
        assert_eq!(b["accountId"], "123::enterprise::456");
    }
    #[test]
    fn exhausted_paid_credits_do_not_exhaust_verified_free_models() {
        let out = usage(
            &json!({"Packages":[{"CycleUsedCapacity":"560","CycleTotalCapacity":"560"}]}),
            false,
            &json!({"free-model":{"free":true},"paid-model":{"free":false}}),
        )
        .unwrap();
        assert_eq!(out["windows"][0]["used"], 100.);
        assert_eq!(out["windows"][0]["notModels"], json!(["free-model"]));
        let out = usage(&json!({"limitNum":"1000","credit":"500","cycleResetTime":"2026-11-01 00:00:00","cycleStartTime":"2026-10-01 00:00:00"}),true,&json!({})).unwrap();
        assert_eq!(out["plan"], "Enterprise");
        assert_eq!(out["windows"][0]["used"], 50.);
        assert_eq!(out["windows"][0]["resetsAt"], "2026-11-01T00:00:00+08:00");
    }
    #[test]
    fn missing_usage_is_unknown_rather_than_an_empty_paid_pool() {
        assert!(usage(&json!({"limitNum":100}), true, &json!({})).is_err());
        assert!(usage(
            &json!({"Packages":[{"CycleTotalCapacity":100}]}),
            false,
            &json!({})
        )
        .is_err());
        assert_eq!(
            usage(&json!({"limitNum":100,"credit":0}), true, &json!({})).unwrap()["windows"][0]
                ["used"],
            0.
        );
    }
}
