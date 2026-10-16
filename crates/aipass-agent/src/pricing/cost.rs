use super::*;

/// Recompute the cost of a single usage row at query time. Group rules win
/// when the credential is assigned to a group with a matching version/rule;
/// otherwise per-config overrides win over the official list prices. A
/// credential multiplier applies in every branch.
#[allow(clippy::too_many_arguments)]
pub fn resolve_cost(
    config: &PricingConfig,
    overrides: &[ModelPricing],
    list_prices: &[ModelPriceRule],
    entry_id: Uuid,
    secret_id: &str,
    model: Option<&str>,
    started_at: i64,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
) -> u64 {
    resolve_cost_with_ttl(
        config,
        overrides,
        list_prices,
        entry_id,
        secret_id,
        model,
        started_at,
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_creation_tokens,
        0,
    )
}

pub fn resolve_row_cost(
    config: &PricingConfig,
    overrides: &[ModelPricing],
    list_prices: &[ModelPriceRule],
    row: &aipass_proxy::UsageRow,
) -> u64 {
    resolve_cost_with_ttl(
        config,
        overrides,
        list_prices,
        row.provider_entry_id,
        &row.secret_id,
        row.model.as_deref(),
        row.started_at,
        row.input_tokens,
        row.output_tokens,
        row.cache_read_tokens,
        row.cache_creation_tokens,
        row.cache_creation_1h_tokens,
    )
}

#[allow(clippy::too_many_arguments)]
fn resolve_cost_with_ttl(
    config: &PricingConfig,
    overrides: &[ModelPricing],
    list_prices: &[ModelPriceRule],
    entry_id: Uuid,
    secret_id: &str,
    model: Option<&str>,
    started_at: i64,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    cache_creation_1h_tokens: u64,
) -> u64 {
    let assignment = config
        .assignments
        .iter()
        .find(|item| item.entry_id == entry_id && item.secret_id == secret_id);
    let group_cost = assignment
        .and_then(|item| item.group_id)
        .and_then(|group_id| config.groups.iter().find(|group| group.id == group_id))
        .and_then(|group| {
            group
                .versions
                .iter()
                .rev()
                .find(|version| version.effective_from <= started_at)
        })
        .and_then(|version| model.and_then(|model| find_rule(&version.rules, model)))
        .map(|rule| {
            rule_cost(
                rule,
                started_at,
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_creation_tokens,
                cache_creation_1h_tokens,
            )
        });
    let base = group_cost.unwrap_or_else(|| {
        model
            .and_then(|model| {
                overrides
                    .iter()
                    .filter(|item| item.model == model || model.starts_with(&item.model))
                    .max_by_key(|item| item.model.len())
            })
            .map(|pricing| {
                tokens_cost(
                    pricing.input_micros_per_million,
                    pricing.output_micros_per_million,
                    pricing.cache_read_micros_per_million,
                    pricing.cache_creation_micros_per_million,
                    pricing.cache_creation_1h_micros_per_million,
                    input_tokens,
                    output_tokens,
                    cache_read_tokens,
                    cache_creation_tokens,
                    cache_creation_1h_tokens,
                )
            })
            .or_else(|| {
                model
                    .and_then(|model| find_rule(list_prices, model))
                    .map(|rule| {
                        rule_cost(
                            rule,
                            started_at,
                            input_tokens,
                            output_tokens,
                            cache_read_tokens,
                            cache_creation_tokens,
                            cache_creation_1h_tokens,
                        )
                    })
            })
            .unwrap_or(0)
    });
    let multiplier = assignment.map(|item| item.multiplier).unwrap_or(1.0);
    if multiplier == 1.0 {
        return base;
    }
    let scaled = base as f64 * multiplier;
    if scaled.is_finite() && scaled > 0.0 {
        scaled.round() as u64
    } else {
        0
    }
}

pub(super) fn find_rule<'a>(
    rules: &'a [ModelPriceRule],
    model: &str,
) -> Option<&'a ModelPriceRule> {
    rules
        .iter()
        .filter(|rule| rule.model == model || model.starts_with(&rule.model))
        .max_by_key(|rule| rule.model.len())
}

fn rule_cost(
    rule: &ModelPriceRule,
    started_at: i64,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    cache_creation_1h_tokens: u64,
) -> u64 {
    let (input, output, cache_read, cache_creation, cache_creation_1h) = match &rule.off_peak {
        Some(window) if off_peak_contains(window, started_at) => (
            window.input_micros_per_million,
            window.output_micros_per_million,
            window.cache_read_micros_per_million,
            window.cache_creation_micros_per_million,
            window.cache_creation_1h_micros_per_million,
        ),
        _ => (
            rule.input_micros_per_million,
            rule.output_micros_per_million,
            rule.cache_read_micros_per_million,
            rule.cache_creation_micros_per_million,
            rule.cache_creation_1h_micros_per_million,
        ),
    };
    tokens_cost(
        input,
        output,
        cache_read,
        cache_creation,
        cache_creation_1h,
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_creation_tokens,
        cache_creation_1h_tokens,
    )
}

pub(super) fn off_peak_contains(window: &OffPeakWindow, started_at: i64) -> bool {
    let minute = (started_at.rem_euclid(86_400) / 60) as u16;
    if window.start_minute_utc <= window.end_minute_utc {
        minute >= window.start_minute_utc && minute < window.end_minute_utc
    } else {
        minute >= window.start_minute_utc || minute < window.end_minute_utc
    }
}

#[allow(clippy::too_many_arguments)]
fn tokens_cost(
    input_micros: u64,
    output_micros: u64,
    cache_read_micros: u64,
    cache_creation_micros: u64,
    cache_creation_1h_micros: Option<u64>,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    cache_creation_1h_tokens: u64,
) -> u64 {
    let hour = cache_creation_1h_tokens.min(cache_creation_tokens);
    input_tokens
        .saturating_mul(input_micros)
        .saturating_add(output_tokens.saturating_mul(output_micros))
        .saturating_add(cache_read_tokens.saturating_mul(cache_read_micros))
        .saturating_add((cache_creation_tokens - hour).saturating_mul(cache_creation_micros))
        .saturating_add(
            hour.saturating_mul(cache_creation_1h_micros.unwrap_or(cache_creation_micros)),
        )
        / 1_000_000
}

#[cfg(test)]
mod tests {
    use super::*;
    use aipass_agent_protocol::CredentialAssignment;
    #[test]
    fn mixed_ttl_prices_follow_versions_off_peak_overrides_and_multiplier() {
        let id = Uuid::new_v4();
        let row = aipass_proxy::UsageRow {
            started_at: 3600,
            provider_entry_id: id,
            secret_id: "key".into(),
            model: Some("claude-test".into()),
            input_tokens: 100,
            output_tokens: 10,
            cache_read_tokens: 50,
            cache_creation_tokens: 1000,
            cache_creation_1h_tokens: 400,
            status: 200,
            first_token_ms: None,
        };
        let mut rule = ModelPriceRule {
            model: "claude-".into(),
            input_micros_per_million: 1_000_000,
            output_micros_per_million: 2_000_000,
            cache_read_micros_per_million: 100_000,
            cache_creation_micros_per_million: 1_250_000,
            cache_creation_1h_micros_per_million: Some(2_000_000),
            off_peak: None,
        };
        let mut config = PricingConfig::default();
        assert_eq!(resolve_row_cost(&config, &[], &[rule.clone()], &row), 1675);
        rule.cache_creation_1h_micros_per_million = None;
        assert_eq!(resolve_row_cost(&config, &[], &[rule.clone()], &row), 1375);
        rule.off_peak = Some(OffPeakWindow {
            start_minute_utc: 30,
            end_minute_utc: 90,
            cache_creation_micros_per_million: 1_000_000,
            cache_creation_1h_micros_per_million: Some(3_000_000),
            ..Default::default()
        });
        config.assignments.push(CredentialAssignment {
            entry_id: id,
            secret_id: "key".into(),
            multiplier: 0.5,
            ..Default::default()
        });
        config.groups.push(PricingGroup {
            id: Uuid::new_v4(),
            versions: vec![
                GroupPriceVersion {
                    effective_from: 0,
                    rules: vec![rule.clone()],
                },
                GroupPriceVersion {
                    effective_from: 7200,
                    rules: vec![],
                },
            ],
            ..Default::default()
        });
        config.assignments[0].group_id = Some(config.groups[0].id);
        assert_eq!(resolve_row_cost(&config, &[], &[], &row), 900);
        let overrides = [ModelPricing {
            model: "claude-".into(),
            input_micros_per_million: 0,
            output_micros_per_million: 0,
            cache_read_micros_per_million: 0,
            cache_creation_micros_per_million: 1_000_000,
            cache_creation_1h_micros_per_million: Some(4_000_000),
        }];
        assert_eq!(resolve_row_cost(&config, &overrides, &[], &row), 900);
        let later = aipass_proxy::UsageRow {
            started_at: 8000,
            ..row
        };
        assert_eq!(resolve_row_cost(&config, &overrides, &[], &later), 1100);
    }
}
