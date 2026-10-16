use super::*;
#[test]
fn old_usage_database_keeps_rows_and_defaults_hour_cache_to_zero() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("usage.db");
    let store = UsageStore::open(&path).unwrap();
    let record = UsageRecord {
        id: Uuid::new_v4(),
        started_at: 1,
        duration_ms: 1,
        first_token_ms: None,
        route_id: Uuid::new_v4(),
        provider_entry_id: Uuid::new_v4(),
        secret_id: "key".into(),
        model: Some("claude".into()),
        inbound_protocol: ProxyProtocol::AnthropicMessages,
        upstream_protocol: ProxyProtocol::AnthropicMessages,
        status: 200,
        attempts: 1,
        input_tokens: 5,
        output_tokens: 2,
        cache_read_tokens: 50,
        cache_creation_tokens: 100,
        cache_creation_1h_tokens: 40,
        estimated_cost_micros: 0,
    };
    store.record(&record).unwrap();
    assert_eq!(store.iter_rows().unwrap()[0].cache_creation_1h_tokens, 40);
    store
        .connection
        .lock()
        .unwrap()
        .execute(
            "ALTER TABLE proxy_usage DROP COLUMN cache_creation_1h_tokens",
            [],
        )
        .unwrap();
    drop(store);
    let store = UsageStore::open(&path).unwrap();
    let row = &store.iter_rows().unwrap()[0];
    assert_eq!(row.input_tokens, 5);
    assert_eq!(row.cache_creation_tokens, 100);
    assert_eq!(row.cache_creation_1h_tokens, 0);
    store.record(&record).unwrap();
    assert_eq!(store.iter_rows().unwrap()[0].cache_creation_1h_tokens, 40);
    let summary = store.summary(|_| 0).unwrap();
    assert_eq!(summary.cache_creation_tokens, 100);
}
