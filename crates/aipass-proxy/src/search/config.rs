//! Search settings contain vault references, never plaintext credentials.
use super::*;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SearchVendor {
    Tavily,
    Brave,
    Exa,
    Firecrawl,
    You,
    Deepseek,
    Searxng,
}
impl SearchVendor {
    pub fn base(self) -> &'static str {
        match self {
            Self::Tavily => "https://api.tavily.com",
            Self::Brave => "https://api.search.brave.com",
            Self::Exa => "https://api.exa.ai",
            Self::Firecrawl => "https://api.firecrawl.dev",
            Self::You => "https://ydc-index.io",
            Self::Deepseek => "https://api.deepseek.com",
            Self::Searxng => "",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchApiConfig {
    pub id: Uuid,
    pub vendor: SearchVendor,
    pub provider_entry_id: Option<Uuid>,
    pub secret_id: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default = "enabled")]
    pub enabled: bool,
}
fn enabled() -> bool {
    true
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchModelConfig {
    pub route_id: Uuid,
    pub model: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchConfig {
    #[serde(default)]
    pub apis: Vec<SearchApiConfig>,
    #[serde(default)]
    pub model: Option<SearchModelConfig>,
    #[serde(default)]
    pub api_first: bool,
}
impl SearchConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.apis.len() > 32 {
            return Err("at most 32 search API credentials are supported".into());
        }
        let mut ids = HashSet::new();
        for api in &self.apis {
            if !ids.insert(api.id) {
                return Err("search API IDs must be unique".into());
            }
            let url = api.base_url.as_deref().unwrap_or(api.vendor.base());
            let url = reqwest::Url::parse(url).map_err(|_| "search API requires a valid URL")?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(
                    "search API URL must be HTTP(S), without credentials, query or fragment".into(),
                );
            }
            if api.provider_entry_id.is_some() != api.secret_id.is_some()
                || (api.vendor != SearchVendor::Searxng && api.provider_entry_id.is_none())
                || api.secret_id.as_ref().is_some_and(|s| s.trim().is_empty())
            {
                return Err("search API requires a vault credential".into());
            }
        }
        if self.model.as_ref().is_some_and(|m| {
            m.model.trim().is_empty()
                || m.model.len() > 256
                || m.model.chars().any(char::is_control)
        }) {
            return Err("search model must be nonempty and at most 256 bytes".into());
        }
        Ok(())
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct ResolvedSearchApi {
    pub config: SearchApiConfig,
    pub api_key: String,
}
impl std::fmt::Debug for ResolvedSearchApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedSearchApi")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}
impl Drop for ResolvedSearchApi {
    fn drop(&mut self) {
        self.api_key.zeroize();
    }
}
