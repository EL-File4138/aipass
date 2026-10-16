//! Shared vendor allowance timestamp normalization.
use super::*;
pub(super) fn timestamp(v: &Value) -> Value {
    let time = if let Some(n) = v
        .as_i64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
    {
        time::OffsetDateTime::from_unix_timestamp(n / if n > 100000000000 { 1000 } else { 1 }).ok()
    } else {
        v.as_str().and_then(|s| {
            time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
        })
    };
    time.and_then(|t| {
        t.format(&time::format_description::well_known::Rfc3339)
            .ok()
    })
    .map(Value::String)
    .unwrap_or(Value::Null)
}
