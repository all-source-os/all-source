use allsource_core::{
    QueryEventsRequest,
    domain::entities::Event,
    store::{EventStore, EventStoreConfig, ReadScope},
};
use serde_json::json;
use tempfile::TempDir;

const TENANT: &str = "synthetic-strict-read";
const ENTITY: &str = "synthetic-run";

fn event(tenant: &str, entity: &str, payload: serde_json::Value) -> Event {
    Event::from_strings(
        "synthetic.updated".into(),
        entity.into(),
        tenant.into(),
        payload,
        None,
    )
    .unwrap()
}

#[test]
fn strict_reads_refuse_partial_history_even_after_tolerant_queries() {
    let directory = TempDir::new().unwrap();
    {
        let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
        store
            .ingest(&event(TENANT, ENTITY, json!({"synthetic": true})))
            .unwrap();
        store.flush_storage().unwrap();
    }
    let corrupt = directory.path().join(TENANT).join("events-unknown.parquet");
    std::fs::write(&corrupt, "synthetic unreadable retained file").unwrap();
    let store = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
    assert!(
        store
            .query_retained_entity(TENANT, ENTITY, 1001, &ReadScope::unrestricted())
            .is_err()
    );
    assert_eq!(
        store
            .query(&QueryEventsRequest {
                tenant_id: Some(TENANT.into()),
                ..Default::default()
            })
            .unwrap()
            .len(),
        1
    );
    assert!(
        store
            .query_retained_entity(TENANT, ENTITY, 1001, &ReadScope::unrestricted())
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(corrupt).unwrap(),
        "synthetic unreadable retained file"
    );
}

#[test]
fn complete_archive_and_pending_tail_survive_flush_eviction_and_reopen() {
    let directory = TempDir::new().unwrap();
    let config = EventStoreConfig::with_persistence(directory.path());
    let store = EventStore::with_config(config.clone());
    for previous in 0..3 {
        let mut input = event(TENANT, ENTITY, json!({"synthetic": true}));
        // Deliberately reverse submicrosecond time within one persisted tick.
        // Linux exposes this precision naturally; macOS must force it here.
        input.timestamp = "2026-01-01T00:00:00.123456999Z".parse().unwrap();
        input.timestamp -= chrono::TimeDelta::nanoseconds(previous as i64 * 100);
        store
            .ingest_with_expected_version(&input, Some(previous))
            .unwrap();
    }
    let (before, total) = store
        .query_retained_entity(TENANT, ENTITY, 1001, &ReadScope::unrestricted())
        .unwrap();
    assert_eq!(total, 3);
    assert_eq!(
        before.iter().map(|event| event.version).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert!(
        before
            .iter()
            .all(|event| event.timestamp.timestamp_subsec_nanos() == 123_456_000)
    );
    let generic = store
        .query(&QueryEventsRequest {
            tenant_id: Some(TENANT.into()),
            entity_id: Some(ENTITY.into()),
            ..Default::default()
        })
        .unwrap();
    assert!(
        generic
            .iter()
            .all(|event| event.timestamp.timestamp_subsec_nanos() % 1000 != 0)
    );
    store.flush_storage().unwrap();
    store.evict_tenant(TENANT);
    let (after, total) = store
        .query_retained_entity(TENANT, ENTITY, 2, &ReadScope::unrestricted())
        .unwrap();
    assert_eq!(total, 3);
    assert_eq!(after, before[..2]);
    drop(store);
    let reopened = EventStore::with_config(config);
    assert_eq!(
        reopened
            .query_retained_entity(TENANT, ENTITY, 1001, &ReadScope::unrestricted())
            .unwrap()
            .0,
        before
    );
}

#[test]
fn strict_reads_enforce_tenant_entity_and_authoritative_scope() {
    let store = EventStore::new();
    for (tenant, entity) in [
        (TENANT, ENTITY),
        ("synthetic-other", ENTITY),
        (TENANT, "synthetic-other-run"),
    ] {
        store
            .ingest(&event(tenant, entity, json!({"synthetic": true})))
            .unwrap();
    }
    assert!(
        store
            .query_retained_entity(
                TENANT,
                ENTITY,
                10,
                &ReadScope::allow_entity_prefixes(["not-allowed"])
            )
            .is_err()
    );
    let (events, total) = store
        .query_retained_entity(
            TENANT,
            ENTITY,
            10,
            &ReadScope::allow_entity_prefixes(["synthetic-run"]),
        )
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].tenant_id_str(), TENANT);
    assert_eq!(events[0].entity_id_str(), ENTITY);
}

#[test]
fn oversized_entities_are_refused_before_materializing_their_history() {
    let store = EventStore::new();
    let events = (0..1002)
        .map(|_| event(TENANT, ENTITY, json!({})))
        .collect();
    store.ingest_batch(events).unwrap();
    assert!(
        store
            .query_retained_entity(TENANT, ENTITY, 1, &ReadScope::unrestricted())
            .is_err()
    );
}

#[test]
fn encoded_budget_refuses_large_payloads_and_metadata() {
    for metadata in [false, true] {
        let store = EventStore::new();
        let mut large = event(TENANT, ENTITY, json!({}));
        let value = json!({"synthetic": "x".repeat(2 * 1024 * 1024)});
        if metadata {
            large.metadata = Some(value);
        } else {
            large.payload = value;
        }
        store.ingest(&large).unwrap();
        assert!(
            store
                .query_retained_entity(TENANT, ENTITY, 1001, &ReadScope::unrestricted())
                .is_err()
        );
    }
}

#[test]
fn invalid_scope_and_limits_do_not_become_certified_empty_history() {
    let store = EventStore::new();
    for limit in [0, 1002, usize::MAX] {
        assert!(
            store
                .query_retained_entity(TENANT, ENTITY, limit, &ReadScope::unrestricted())
                .is_err()
        );
    }
    assert!(
        store
            .query_retained_entity("../outside", ENTITY, 10, &ReadScope::unrestricted())
            .is_err()
    );
    assert!(
        store
            .query_retained_entity(TENANT, "", 10, &ReadScope::unrestricted())
            .is_err()
    );
    assert_eq!(
        store
            .query_retained_entity(TENANT, ENTITY, 10, &ReadScope::unrestricted())
            .unwrap(),
        (Vec::new(), 0)
    );
}
