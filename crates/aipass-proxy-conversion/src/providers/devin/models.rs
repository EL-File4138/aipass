//! The account's own ClientModelConfig cost tiers, not model-name guesses.
use super::super::connect::{bytes, field_int, field_text, fields, string, Field};
use std::collections::BTreeMap;

pub fn request(key: &str, os: &str) -> Vec<u8> {
    bytes(
        1,
        &[
            string(1, "devin-cli"),
            string(2, "3000.11.3"),
            string(3, key),
            string(4, "en"),
            string(5, os),
            string(7, "3000.11.3"),
            string(12, "chisel"),
            string(28, "chisel"),
        ]
        .concat(),
    )
}

pub fn cost_tiers(data: &[u8]) -> Result<BTreeMap<String, u64>, String> {
    let mut tiers = BTreeMap::new();
    for (number, value) in fields(data)? {
        if let (1, Field::Bytes(config)) = (number, value) {
            let config = fields(config)?;
            let id = field_text(&config, 22);
            if !id.is_empty() {
                let tier = field_int(&config, 24);
                if tiers.insert(id, tier).is_some() {
                    return Err("Devin returned duplicate model configs".into());
                }
            }
        }
    }
    if tiers.is_empty() {
        return Err("Devin returned no model configs".into());
    }
    Ok(tiers)
}

#[cfg(test)]
mod tests {
    use super::super::super::connect::uint;
    use super::*;
    #[test]
    fn only_the_explicit_free_cost_tier_exempts_a_variant() {
        let config = |id: &str, tier: Option<u64>| {
            bytes(
                1,
                &[
                    string(22, id),
                    tier.map(|n| uint(24, n)).unwrap_or_default(),
                ]
                .concat(),
            )
        };
        let out = cost_tiers(
            &[
                config("free", Some(4)),
                config("paid", Some(1)),
                config("unknown", None),
            ]
            .concat(),
        )
        .unwrap();
        assert_eq!(out["free"], 4);
        assert_eq!(out["paid"], 1);
        assert_eq!(out["unknown"], 0);
        assert!(cost_tiers(&[]).is_err());
        assert!(cost_tiers(&[config("same", Some(4)), config("same", Some(1))].concat()).is_err());
        assert!(cost_tiers(&[10, 255]).is_err());
    }
}
