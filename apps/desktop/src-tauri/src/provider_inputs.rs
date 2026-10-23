//! Normalize desktop provider inputs before passing them to the Agent.
use crate::models::{ProviderAddRequest, ProviderUpdateRequest};
use aipass_provider_registry::{provider_kind_for_id, ProviderEndpoint};
use aipass_vault::{ProviderEntryInput, ProviderEntryUpdateInput};

pub(crate) fn provider_add_input(request: ProviderAddRequest) -> ProviderEntryInput {
    let provider_kind = provider_kind_for_id(request.provider_id.as_deref());
    let endpoints = endpoints_from(
        request.endpoint,
        request.endpoints,
        request.console_endpoints,
    );
    let domains = domains_from(request.domain, &endpoints);
    ProviderEntryInput {
        max_concurrent_requests: request.max_concurrent_requests,
        supports_websockets: request.supports_websockets,
        title: non_empty(request.title).unwrap_or_else(|| "Custom Provider".to_string()),
        provider_kind,
        provider_id: request.provider_id,
        credential_kind: request.credential_kind,
        account_identity: request.account_identity,
        domains,
        favicon_url: request.favicon_url.and_then(non_empty),
        endpoints,
        interface_type: request.interface_type,
        auth_scheme: request.auth_scheme,
        api_key: request.api_key.into_inner(),
        secret_label: request.secret_label.and_then(non_empty),
        default_model: request.default_model.and_then(non_empty),
        model_aliases: clean_pairs(request.model_aliases),
        headers: request.headers,
        quota: request.quota,
        subscription: None,
        gateway: request.gateway,
        tags: clean_strings(request.tags),
        notes: request.notes.and_then(non_empty),
        secret_metadata: request.secret_metadata,
    }
}

pub(crate) fn provider_update_input(request: ProviderUpdateRequest) -> ProviderEntryUpdateInput {
    let provider_kind = provider_kind_for_id(request.provider_id.as_deref());
    let endpoints = endpoints_from(
        request.endpoint,
        request.endpoints,
        request.console_endpoints,
    );
    let domains = domains_from(request.domain, &endpoints);
    ProviderEntryUpdateInput {
        max_concurrent_requests: request.max_concurrent_requests,
        supports_websockets: request.supports_websockets,
        title: non_empty(request.title).unwrap_or_else(|| "Custom Provider".to_string()),
        provider_kind,
        provider_id: request.provider_id,
        credential_kind: request.credential_kind,
        account_identity: request.account_identity,
        domains,
        favicon_url: request.favicon_url.and_then(non_empty),
        endpoints,
        interface_type: request.interface_type,
        auth_scheme: request.auth_scheme,
        api_key: request
            .api_key
            .map(|value| value.into_inner())
            .and_then(non_empty),
        secret_label: request.secret_label.and_then(non_empty),
        default_model: request.default_model.and_then(non_empty),
        model_aliases: clean_pairs(request.model_aliases),
        headers: request.headers,
        quota: request.quota,
        subscription: None,
        gateway: request.gateway,
        tags: clean_strings(request.tags),
        notes: request.notes.and_then(non_empty),
        secret_metadata: request.secret_metadata,
    }
}

pub(crate) fn endpoints_from(
    endpoint: Option<String>,
    endpoints: Vec<String>,
    console_endpoints: Vec<String>,
) -> Vec<ProviderEndpoint> {
    let mut api_endpoints = endpoints
        .into_iter()
        .chain(endpoint)
        .filter_map(non_empty)
        .map(ProviderEndpoint::api)
        .collect::<Vec<_>>();
    api_endpoints.extend(
        console_endpoints
            .into_iter()
            .filter_map(non_empty)
            .map(ProviderEndpoint::console),
    );
    api_endpoints
}

fn clean_strings(values: Vec<String>) -> Vec<String> {
    values.into_iter().filter_map(non_empty).collect()
}

fn clean_pairs(values: Vec<(String, String)>) -> Vec<(String, String)> {
    values
        .into_iter()
        .filter_map(|(left, right)| Some((non_empty(left)?, non_empty(right)?)))
        .collect()
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

// An explicit domain list is an override. Otherwise use only API hosts;
// console URLs must not change credential matching or provider identity.
fn domains_from(domains: Vec<String>, endpoints: &[ProviderEndpoint]) -> Vec<String> {
    let explicit = clean_strings(domains);
    if !explicit.is_empty() {
        return explicit;
    }
    let mut hosts = Vec::new();
    for endpoint in endpoints
        .iter()
        .filter(|endpoint| endpoint.kind == aipass_provider_registry::EndpointKind::Api)
    {
        if let Some(url) = endpoint
            .url
            .as_deref()
            .and_then(|value| url::Url::parse(value).ok())
        {
            if matches!(url.scheme(), "http" | "https") {
                if let Some(host) = url.host_str() {
                    if !hosts.iter().any(|existing| existing == host) {
                        hosts.push(host.to_owned());
                    }
                }
            }
        }
    }
    hosts
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_domains_follow_api_hosts_and_preserve_explicit_overrides() {
        let endpoints = endpoints_from(
            None,
            vec![
                "https://api.example.test:443/v1".into(),
                "https://api.example.test/v2".into(),
                "http://localhost:8080/v1".into(),
            ],
            vec!["https://console.example.test".into()],
        );
        assert_eq!(
            domains_from(vec!["  ".into()], &endpoints),
            vec!["api.example.test", "localhost"]
        );
        assert_eq!(
            domains_from(vec![" custom.example.test ".into()], &endpoints),
            vec!["custom.example.test"]
        );
        assert!(domains_from(
            vec![],
            &endpoints_from(None, vec!["invalid".into()], vec![])
        )
        .is_empty());
    }
}
