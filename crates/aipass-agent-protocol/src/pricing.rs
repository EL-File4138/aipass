use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct OffPeakWindow {
    pub start_minute_utc: u16,
    pub end_minute_utc: u16,
    pub input_micros_per_million: u64,
    pub output_micros_per_million: u64,
    pub cache_read_micros_per_million: u64,
    pub cache_creation_micros_per_million: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_1h_micros_per_million: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ModelPriceRule {
    pub model: String,
    pub input_micros_per_million: u64,
    pub output_micros_per_million: u64,
    pub cache_read_micros_per_million: u64,
    pub cache_creation_micros_per_million: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_1h_micros_per_million: Option<u64>,
    pub off_peak: Option<OffPeakWindow>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct GroupPriceVersion {
    pub effective_from: i64,
    pub rules: Vec<ModelPriceRule>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct PricingGroup {
    /// Manual rule edits survive gateway price refreshes.
    pub manual: bool,
    pub id: Uuid,
    pub name: String,
    pub versions: Vec<GroupPriceVersion>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CredentialAssignment {
    pub entry_id: Uuid,
    pub secret_id: String,
    pub group_id: Option<Uuid>,
    pub multiplier: f64,
    /// Explicit user settings must survive automatic gateway refreshes.
    #[serde(default)]
    pub manual: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PricingConfig {
    pub groups: Vec<PricingGroup>,
    pub assignments: Vec<CredentialAssignment>,
    pub list_price_updated_at: Option<i64>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PricingApplyScope {
    AllHistory,
    FromNow,
}
