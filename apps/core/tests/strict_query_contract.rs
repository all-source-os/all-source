use allsource_core::{
    domain::entities::Event,
    infrastructure::{security::middleware::OptionalAuth, web::api::query_events},
    store::{EventStore, EventStoreConfig},
};
use axum::{
    extract::{Query, State},
    http::Uri,
};
use serde_json::{Value, json};
use std::sync::Arc;

#[tokio::test]
async fn http_retained_read_does_not_warm_unrelated_history() {
    let directory = tempfile::TempDir::new().unwrap();
    {
        let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
        for entity in ["run", "other"] {
            store
                .ingest(
                    &Event::from_strings(
                        "synthetic.updated".into(),
                        entity.into(),
                        "synthetic".into(),
                        if entity == "run" {
                            json!({})
                        } else {
                            json!({"text": "x".repeat(4096)})
                        },
                        None,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        store.flush_storage().unwrap();
    }
    let store = Arc::new(EventStore::with_config(EventStoreConfig {
        http_archive_warmup_timeout: Some(std::time::Duration::from_secs(2)),
        strict_archive_limits: allsource_core::infrastructure::persistence::ArchiveReadLimits {
            max_uncompressed_bytes: 1024,
            ..Default::default()
        },
        ..EventStoreConfig::with_persistence(directory.path())
    }));
    let result = query(
        &store,
        "tenant_id=synthetic&entity_id=run&limit=1001&integrity=retained-entity-v1",
    )
    .await
    .unwrap();
    assert_eq!(result["total_count"], 1);
    assert!(!store.is_tenant_loaded("synthetic"));
}

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
async fn prefix_http_query_filters_before_decoding_and_preserves_pagination() {
    let directory = tempfile::TempDir::new().unwrap();
    {
        let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
        for (tenant, kind, entity, payload) in [
            (
                "synthetic",
                "auth.user.created",
                "user",
                json!({"id":"user"}),
            ),
            (
                "synthetic",
                "auth.user.deleted",
                "user",
                json!({"id":"user","_deleted":true}),
            ),
            (
                "synthetic",
                "unrelated.created",
                "noise",
                json!({"text":"x".repeat(4096)}),
            ),
            (
                "other",
                "auth.user.created",
                "foreign",
                json!({"id":"foreign"}),
            ),
        ] {
            store
                .ingest(
                    &Event::from_strings(kind.into(), entity.into(), tenant.into(), payload, None)
                        .unwrap(),
                )
                .unwrap();
        }
        store.flush_storage().unwrap();
    }
    let store = Arc::new(EventStore::with_config(EventStoreConfig {
        strict_archive_limits: allsource_core::infrastructure::persistence::ArchiveReadLimits {
            max_uncompressed_bytes: 1024,
            ..Default::default()
        },
        ..EventStoreConfig::with_persistence(directory.path())
    }));
    let first = query(
        &store,
        "tenant_id=synthetic&event_type_prefix=auth.user.&integrity=retained-prefix-v1&order=desc&limit=1",
    )
    .await
    .unwrap();
    assert_eq!(first["total_count"], 2);
    assert_eq!(first["has_more"], true);
    assert_eq!(first["events"][0]["payload"]["_deleted"], true);
    let next = query(
        &store,
        "tenant_id=synthetic&event_type_prefix=auth.user.&integrity=retained-prefix-v1&order=desc&limit=1&offset=1",
    )
    .await
    .unwrap();
    assert_eq!(next["has_more"], false);
    assert_eq!(next["events"][0]["entity_id"], "user");
    assert!(!store.is_tenant_loaded("synthetic"));
    for invalid in [
        "integrity=retained-prefix-v1&event_type_prefix=auth.user.",
        "integrity=retained-prefix-v1&tenant_id=synthetic",
        "integrity=retained-prefix-v1&tenant_id=synthetic&event_type_prefix=",
        "integrity=retained-prefix-v1&tenant_id=synthetic&event_type_prefix=auth.user.&entity_id=user",
    ] {
        assert!(query(&store, invalid).await.is_err());
    }
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
    assert_eq!(empty["entity_version"], 0);
    assert_eq!(empty["archive_integrity"]["entity_id"], "absent");
    let complete = query(
        &store,
        "tenant_id=synthetic&entity_id=run&limit=1001&integrity=retained-entity-v1",
    )
    .await
    .unwrap();
    assert_eq!(
        complete["entity_version"],
        complete["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event["version"].as_u64().unwrap())
            .max()
            .unwrap()
    );
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
