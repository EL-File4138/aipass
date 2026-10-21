use super::*;
#[test]
fn search_credentials_are_encrypted_and_removed_from_runtime_when_unavailable() {
    for vendor in [
        aipass_proxy::SearchVendor::Tavily,
        aipass_proxy::SearchVendor::Bocha,
        aipass_proxy::SearchVendor::Yuanbao,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let vault = Vault::create(temp.path(), &SecretString::new("test master"))
            .unwrap()
            .vault;
        let entry = vault
            .add_provider(provider_input(
                "search-test-secret",
                "https://api.tavily.com".into(),
                "test",
            ))
            .unwrap();
        let secret = vault.get_provider_summary(entry).unwrap().secret_refs[0]
            .id
            .clone();
        let mut service = ProxyService::new(temp.path()).unwrap();
        let mut config = ProxyConfig::default();
        config.search.apis.push(aipass_proxy::SearchApiConfig {
            id: Uuid::new_v4(),
            vendor,
            provider_entry_id: Some(entry),
            secret_id: Some(secret),
            base_url: None,
            enabled: true,
        });
        service.set_config(&vault, config.clone()).unwrap();
        let runtime = service.runtime_config(&vault).unwrap();
        assert_eq!(runtime.search_apis[0].api_key, "search-test-secret");
        assert!(!format!("{:?}", runtime.search_apis).contains("search-test-secret"));
        let stored = std::fs::read(temp.path().join(CONFIG_FILE)).unwrap();
        let stored = String::from_utf8_lossy(&stored);
        assert!(!stored.contains("search-test-secret"));
        assert!(!stored.contains("tavily"));
        assert!(!stored.contains("bocha"));
        assert!(!stored.contains("yuanbao"));
        assert!(!stored.contains(&entry.to_string()));
        let mut reloaded = ProxyService::new(temp.path()).unwrap();
        reloaded.load_config(&vault).unwrap();
        assert_eq!(reloaded.config.search, config.search);
        vault.trash_provider(entry).unwrap();
        assert!(reloaded.runtime_config(&vault).is_err());
        assert!(reloaded
            .runtime_config_inner(&vault, true)
            .unwrap()
            .search_apis
            .is_empty());
    }
}
