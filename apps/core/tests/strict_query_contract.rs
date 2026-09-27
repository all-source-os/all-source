use allsource_core::{
    domain::entities::Event,
    infrastructure::{security::middleware::OptionalAuth, web::api::query_events},
    store::EventStore,
};
use axum::{
    extract::{Query, State},
    http::Uri,
};
use serde_json::{Value, json};
use std::sync::Arc;

async fn query(store: &Arc<EventStore>, parameters: &str) -> allsource_core::error::Result<Value> {
    let uri: Uri = format!("/api/v1/events/query?{parameters}")
        .parse()
        .unwrap();
    let result = query_events(
        OptionalAuth(None),
        Query::try_from_uri(&uri).unwrap(),
        Query::try_from_uri(&uri).unwrap(),
        Query::try_from_uri(&uri).unwrap(),
        Query::try_from_uri(&uri).unwrap(),
        State(Arc::clone(store)),
    )
    .await?;
    Ok(serde_json::to_value(result.0).unwrap())
}

#[tokio::test]
async fn only_explicit_strict_reads_attest_bound_retained_history() {
    let store = Arc::new(EventStore::new());
    for entity in ["run", "run", "other"] {
        store
            .ingest(
                &Event::from_strings(
                    "synthetic.updated".into(),
                    entity.into(),
                    "synthetic".into(),
                    json!({}),
                    None,
                )
                .unwrap(),
            )
            .unwrap();
    }
    let legacy = query(&store, "tenant_id=synthetic&entity_id=run&limit=1")
        .await
        .unwrap();
    assert!(legacy.get("archive_integrity").is_none());
    let strict = query(
        &store,
        "tenant_id=synthetic&entity_id=run&limit=1&integrity=retained-entity-v1",
    )
    .await
    .unwrap();
    assert_eq!(strict["count"], 1);
    assert_eq!(strict["total_count"], 2);
    assert_eq!(strict["has_more"], true);
    assert!(strict.get("entity_version").is_none());
    assert_eq!(
        strict["archive_integrity"],
        json!({
            "protocol": "retained-entity-v1", "tenant_id": "synthetic", "entity_id": "run"
        })
    );
    let empty = query(
        &store,
        "tenant_id=synthetic&entity_id=absent&limit=1001&integrity=retained-entity-v1",
    )
    .await
    .unwrap();
    assert_eq!(empty["events"], json!([]));
    assert_eq!(empty["archive_integrity"]["entity_id"], "absent");
}

#[tokio::test]
async fn filters_missing_targets_and_unknown_protocols_cannot_attest_an_empty_history() {
    let store = Arc::new(EventStore::new());
    let base = "tenant_id=synthetic&entity_id=run&limit=1001&integrity=retained-entity-v1";
    for extra in [
        "offset=1",
        "order=desc",
        "event_type=missing",
        "event_type_prefix=missing",
        "exclude_event_type_prefix=synthetic",
        "payload_filter=%7B%7D",
        "since=2026-01-01T00%3A00%3A00Z",
        "until=2026-01-01T00%3A00%3A00Z",
        "as_of=2026-01-01T00%3A00%3A00Z",
    ] {
        assert!(
            query(&store, &format!("{base}&{extra}")).await.is_err(),
            "accepted {extra}"
        );
    }
    for invalid in [
        "entity_id=run&limit=1001&integrity=retained-entity-v1",
        "tenant_id=synthetic&limit=1001&integrity=retained-entity-v1",
        "tenant_id=synthetic&entity_id=run&integrity=retained-entity-v1",
        "tenant_id=&entity_id=run&limit=1&integrity=retained-entity-v1",
        "tenant_id=synthetic&entity_id=&limit=1&integrity=retained-entity-v1",
        "tenant_id=synthetic&entity_id=run&limit=0&integrity=retained-entity-v1",
        "tenant_id=synthetic&entity_id=run&limit=1002&integrity=retained-entity-v1",
        "tenant_id=synthetic&entity_id=run&limit=1&integrity=retained-entity-v2",
        "tenant_id=synthetic&entity_id=run&limit=1&integrity=",
    ] {
        assert!(query(&store, invalid).await.is_err(), "accepted {invalid}");
    }
    assert!(
        query(&store, &format!("{base}&offset=0&order=asc"))
            .await
            .is_ok()
    );
}
