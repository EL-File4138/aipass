use super::{AuthArg, CredentialKindArg, InterfaceArg};
use clap::Args;
use uuid::Uuid;

// Keep each large argument builder in its own frame. A single derived root
// command can otherwise exceed the Windows executable main-thread stack.
#[derive(Args)]
pub struct AddArgs {
    #[arg(long)]
    pub title: String,
    #[arg(long)]
    pub provider: Option<String>,
    #[arg(long)]
    pub domain: Vec<String>,
    #[arg(long)]
    pub endpoint: Vec<String>,
    #[arg(long = "console-url")]
    pub console_url: Vec<String>,
    #[arg(long)]
    pub favicon_url: Option<String>,
    #[arg(long, value_enum, default_value = "api")]
    pub credential_kind: CredentialKindArg,
    #[arg(long)]
    pub account_identity: Option<String>,
    #[arg(long, value_enum)]
    pub interface: InterfaceArg,
    #[arg(long, value_enum)]
    pub auth: AuthArg,
    #[arg(long, env = "AIPASS_INPUT_API_KEY")]
    pub api_key: String,
    #[arg(long)]
    pub secret_label: Option<String>,
    #[arg(long)]
    pub default_model: Option<String>,
    #[arg(long = "model-alias")]
    pub model_alias: Vec<String>,
    #[arg(long)]
    pub header: Vec<String>,
    #[arg(long)]
    pub quota_label: Option<String>,
    #[arg(long)]
    pub quota_limit: Option<String>,
    #[arg(long)]
    pub quota_remaining: Option<String>,
    #[arg(long)]
    pub quota_reset_at: Option<String>,
    #[arg(long)]
    pub group: Option<String>,
    #[arg(long)]
    pub billing_rate: Option<String>,
    #[arg(long)]
    pub billing_currency: Option<String>,
    #[arg(long)]
    pub billing_unit_price: Option<String>,
    #[arg(long)]
    pub notes: Option<String>,
    #[arg(long)]
    pub tag: Vec<String>,
}

#[derive(Args)]
pub struct UpdateArgs {
    pub id: Uuid,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub provider: Option<String>,
    #[arg(long)]
    pub domain: Vec<String>,
    #[arg(long)]
    pub endpoint: Vec<String>,
    #[arg(long = "console-url")]
    pub console_url: Vec<String>,
    #[arg(long)]
    pub favicon_url: Option<String>,
    #[arg(long, value_enum)]
    pub credential_kind: Option<CredentialKindArg>,
    #[arg(long)]
    pub account_identity: Option<String>,
    #[arg(long, value_enum)]
    pub interface: Option<InterfaceArg>,
    #[arg(long, value_enum)]
    pub auth: Option<AuthArg>,
    #[arg(long, env = "AIPASS_INPUT_API_KEY")]
    pub api_key: Option<String>,
    #[arg(long)]
    pub secret_label: Option<String>,
    #[arg(long)]
    pub default_model: Option<String>,
    #[arg(long = "model-alias")]
    pub model_alias: Vec<String>,
    #[arg(long)]
    pub header: Vec<String>,
    #[arg(long)]
    pub quota_label: Option<String>,
    #[arg(long)]
    pub quota_limit: Option<String>,
    #[arg(long)]
    pub quota_remaining: Option<String>,
    #[arg(long)]
    pub quota_reset_at: Option<String>,
    #[arg(long)]
    pub group: Option<String>,
    #[arg(long)]
    pub billing_rate: Option<String>,
    #[arg(long)]
    pub billing_currency: Option<String>,
    #[arg(long)]
    pub billing_unit_price: Option<String>,
    #[arg(long)]
    pub notes: Option<String>,
    #[arg(long)]
    pub tag: Vec<String>,
    #[arg(long)]
    pub max_concurrent_requests: Option<u32>,
    #[arg(long)]
    pub supports_websockets: Option<bool>,
}
