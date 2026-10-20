//! Resolve search keys only in the trusted, unlocked Agent.
use super::*;
impl ProxyService {
    pub(super) fn search_apis(
        &self,
        vault: &Vault,
        skip_unavailable: bool,
    ) -> ServiceResult<Vec<aipass_proxy::ResolvedSearchApi>> {
        let mut resolved = Vec::new();
        for api in self.config.search.apis.iter().filter(|a| a.enabled) {
            let key = (|| -> ServiceResult<String> {
                let Some(entry) = api.provider_entry_id else {
                    return Ok(String::new());
                };
                let credentials = vault
                    .runtime_provider_credentials(
                        entry,
                        api.secret_id.as_deref().unwrap_or_default(),
                    )
                    .map_err(map_vault_error)?;
                Ok(credentials.secret.expose().to_owned())
            })();
            match key {
                Ok(api_key) => resolved.push(aipass_proxy::ResolvedSearchApi {
                    config: api.clone(),
                    api_key,
                }),
                Err(_) if skip_unavailable => {}
                Err(e) => return Err(e),
            }
        }
        Ok(resolved)
    }
}
