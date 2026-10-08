use super::*;

#[tokio::test]
async fn discarded_index_future_preserves_metadata_until_polled() {
    let engine = HybridSearchEngine::new(
        Arc::new(VectorSearchEngine::new().unwrap()),
        Arc::new(KeywordSearchEngine::new().unwrap()),
    );
    let payload = serde_json::json!({});
    let future = engine.index_event(
        Uuid::new_v4(),
        "tenant",
        "created",
        Some("entity"),
        &payload,
        Utc::now(),
    );
    assert_eq!(engine.cached_metadata_count(), 0);
    drop(future);
    assert_eq!(engine.cached_metadata_count(), 0);
    let future = engine.index_event(
        Uuid::new_v4(),
        "tenant",
        "created",
        Some("entity"),
        &payload,
        Utc::now(),
    );
    assert_eq!(engine.cached_metadata_count(), 0);
    tokio::time::timeout(std::time::Duration::from_secs(1), future)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(engine.cached_metadata_count(), 1);
}
