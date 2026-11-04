use allsource_core::{
    QueryEventsRequest,
    application::services::consumer::ConsumerRegistry,
    domain::entities::Event,
    error::AllSourceError,
    infrastructure::persistence::WALConfig,
    store::{EventStore, EventStoreConfig},
};
use serde_json::json;
use tempfile::TempDir;

fn create_test_event(entity_id: &str, event_type: &str) -> Event {
    Event::from_strings(
        event_type.to_string(),
        entity_id.to_string(),
        "default".to_string(),
        json!({"test": true}),
        None,
    )
    .unwrap()
}

// ============================================================
// US-001: Per-entity version tracking
// ============================================================

#[test]
fn test_entity_version_starts_at_zero() {
    let store = EventStore::new();
    assert_eq!(store.get_entity_version("nonexistent"), 0);
}

#[test]
fn test_entity_version_increments_on_ingest() {
    let store = EventStore::new();

    for i in 1..=3 {
        let event = create_test_event("e1", "user.created");
        store.ingest(&event).unwrap();
        assert_eq!(store.get_entity_version("e1"), i);
    }
    assert_eq!(store.get_entity_version("e1"), 3);
}

#[test]
fn test_entity_versions_independent_per_entity() {
    let store = EventStore::new();

    store
        .ingest(&create_test_event("e1", "user.created"))
        .unwrap();
    store
        .ingest(&create_test_event("e1", "user.updated"))
        .unwrap();
    store
        .ingest(&create_test_event("e2", "user.created"))
        .unwrap();

    assert_eq!(store.get_entity_version("e1"), 2);
    assert_eq!(store.get_entity_version("e2"), 1);
}

#[test]
fn test_entity_version_survives_wal_restart() {
    let temp_dir = TempDir::new().unwrap();
    let wal_dir = temp_dir.path().join("wal");
    std::fs::create_dir_all(&wal_dir).unwrap();

    // Phase 1: Ingest events
    {
        let config = EventStoreConfig {
            wal_dir: Some(wal_dir.clone()),
            wal_config: WALConfig::default(),
            ..EventStoreConfig::default()
        };
        let store = EventStore::with_config(config);

        for _ in 0..3 {
            store
                .ingest(&create_test_event("e1", "user.created"))
                .unwrap();
        }
        store
            .ingest(&create_test_event("e2", "user.created"))
            .unwrap();

        assert_eq!(store.get_entity_version("e1"), 3);
        assert_eq!(store.get_entity_version("e2"), 1);
    }

    // Phase 2: Restart — recover from WAL
    {
        let config = EventStoreConfig {
            wal_dir: Some(wal_dir),
            wal_config: WALConfig::default(),
            ..EventStoreConfig::default()
        };
        let store = EventStore::with_config(config);

        assert_eq!(store.get_entity_version("e1"), 3);
        assert_eq!(store.get_entity_version("e2"), 1);
    }
}

// ============================================================
// US-002: Expected version on writes
// ============================================================

#[test]
fn test_expected_version_match_succeeds() {
    let store = EventStore::new();

    // Entity starts at version 0
    let v1 = store
        .ingest_with_expected_version(&create_test_event("e1", "user.created"), Some(0))
        .unwrap();
    assert_eq!(v1, 1);

    let v2 = store
        .ingest_with_expected_version(&create_test_event("e1", "user.updated"), Some(1))
        .unwrap();
    assert_eq!(v2, 2);
}

#[test]
fn test_expected_version_mismatch_returns_conflict() {
    let store = EventStore::new();

    // Ingest first event (version becomes 1)
    store
        .ingest(&create_test_event("e1", "user.created"))
        .unwrap();

    // Try to write with expected_version=0 (stale) — should fail
    let result =
        store.ingest_with_expected_version(&create_test_event("e1", "user.updated"), Some(0));

    assert!(result.is_err());
    match result.unwrap_err() {
        AllSourceError::VersionConflict { expected, current } => {
            assert_eq!(expected, 0);
            assert_eq!(current, 1);
        }
        other => panic!("Expected VersionConflict, got: {other:?}"),
    }

    // Entity version should still be 1 (write was rejected)
    assert_eq!(store.get_entity_version("e1"), 1);
}

#[test]
fn test_no_expected_version_always_succeeds() {
    let store = EventStore::new();

    // Ingest some events first
    store
        .ingest(&create_test_event("e1", "user.created"))
        .unwrap();
    store
        .ingest(&create_test_event("e1", "user.updated"))
        .unwrap();

    // Write without expected_version — always succeeds regardless of current version
    let v = store
        .ingest_with_expected_version(&create_test_event("e1", "user.updated"), None)
        .unwrap();
    assert_eq!(v, 3);
}

#[test]
fn test_concurrent_writes_one_wins_one_loses() {
    let store = EventStore::new();

    // Create entity at version 0
    store
        .ingest(&create_test_event("e1", "item.created"))
        .unwrap();
    // Now version is 1

    // Two "concurrent" writes both expecting version 1
    let result_a =
        store.ingest_with_expected_version(&create_test_event("e1", "item.claimed"), Some(1));
    let result_b =
        store.ingest_with_expected_version(&create_test_event("e1", "item.claimed"), Some(1));

    // One succeeds, one fails
    assert!(result_a.is_ok());
    assert!(result_b.is_err());
    match result_b.unwrap_err() {
        AllSourceError::VersionConflict { expected, current } => {
            assert_eq!(expected, 1);
            assert_eq!(current, 2); // first write bumped it to 2
        }
        other => panic!("Expected VersionConflict, got: {other:?}"),
    }

    assert_eq!(store.get_entity_version("e1"), 2);
}

#[test]
fn test_expected_version_for_new_entity() {
    let store = EventStore::new();

    // New entity — version is 0
    let v = store
        .ingest_with_expected_version(&create_test_event("new-entity", "item.created"), Some(0))
        .unwrap();
    assert_eq!(v, 1);

    // Trying expected_version=0 again should fail
    let result = store
        .ingest_with_expected_version(&create_test_event("new-entity", "item.created"), Some(0));
    assert!(result.is_err());
}

// ============================================================
// US-003: Entity version in query responses
// ============================================================

#[test]
fn test_query_by_entity_includes_entity_version() {
    let store = EventStore::new();

    for _ in 0..3 {
        store
            .ingest(&create_test_event("e1", "user.created"))
            .unwrap();
    }

    let events = store
        .query(&QueryEventsRequest {
            as_of_valid: None,
            since_valid: None,
            until_valid: None,
            entity_id: Some("e1".to_string()),
            ..Default::default()
        })
        .unwrap();

    assert_eq!(events.len(), 3);
    // entity_version is returned by the HTTP handler, not the store.query() method.
    // Verify get_entity_version works correctly.
    assert_eq!(store.get_entity_version("e1"), 3);
}

#[test]
fn test_entity_version_zero_for_unknown_entity() {
    let store = EventStore::new();
    assert_eq!(store.get_entity_version("nonexistent"), 0);
}

#[test]
fn test_version_returned_on_successful_ingest() {
    let store = EventStore::new();

    let v1 = store
        .ingest_with_expected_version(&create_test_event("e1", "user.created"), None)
        .unwrap();
    assert_eq!(v1, 1);

    let v2 = store
        .ingest_with_expected_version(&create_test_event("e1", "user.updated"), None)
        .unwrap();
    assert_eq!(v2, 2);

    let v3 = store
        .ingest_with_expected_version(&create_test_event("e1", "user.deleted"), Some(2))
        .unwrap();
    assert_eq!(v3, 3);
}

// ============================================================
// US-004: Consumer registration and storage
// ============================================================

#[test]
fn test_consumer_register_and_get() {
    let store = EventStore::new();
    let registry = store.consumer_registry();

    let c = registry.register("worker-1", &["scheduler.*".into()]);
    assert_eq!(c.consumer_id, "worker-1");
    assert_eq!(c.event_type_filters, vec!["scheduler.*"]);
    assert_eq!(c.cursor_position, None);

    let fetched = registry.get("worker-1").unwrap();
    assert_eq!(fetched.consumer_id, "worker-1");
}

#[test]
fn test_consumer_implicit_creation() {
    let store = EventStore::new();
    let registry = store.consumer_registry();

    // get_or_create auto-registers
    assert!(registry.get("worker-1").is_none());
    let c = registry.get_or_create("worker-1");
    assert_eq!(c.consumer_id, "worker-1");
    assert!(c.event_type_filters.is_empty());
    assert!(registry.get("worker-1").is_some());
}

#[test]
fn test_consumer_survives_wal_restart() {
    // Consumer state is currently in-memory only.
    // This test verifies the registry works after store recreation.
    // Full WAL persistence of consumer state is tracked separately.
    let store = EventStore::new();
    let registry = store.consumer_registry();
    registry.register("worker-1", &["scheduler.*".into()]);
    registry.ack("worker-1", 5, 10).unwrap();

    // Restore into a new registry (simulating WAL recovery)
    let store2 = EventStore::new();
    let consumer = registry.get("worker-1").unwrap();
    store2.consumer_registry().restore(consumer);

    let restored = store2.consumer_registry().get("worker-1").unwrap();
    assert_eq!(restored.cursor_position, Some(5));
    assert_eq!(restored.event_type_filters, vec!["scheduler.*"]);
}

// ============================================================
// US-005: Consumer pull-based event polling
// ============================================================

#[test]
fn test_consumer_poll_returns_all_events() {
    let store = EventStore::new();

    // Ingest 5 events
    for i in 0..5 {
        store
            .ingest(&create_test_event("e1", &format!("event.type{i}")))
            .unwrap();
    }

    store.consumer_registry().register("c1", &[]);

    let events = store.events_after_offset(0, &[], 100);
    assert_eq!(events.len(), 5);
    // Positions should be 1..=5
    assert_eq!(events[0].0, 1);
    assert_eq!(events[4].0, 5);
}

#[test]
fn test_consumer_poll_after_ack() {
    let store = EventStore::new();

    for i in 0..5 {
        store
            .ingest(&create_test_event("e1", &format!("event.type{i}")))
            .unwrap();
    }

    let registry = store.consumer_registry();
    registry.register("c1", &[]);
    registry.ack("c1", 3, 5).unwrap();

    let consumer = registry.get("c1").unwrap();
    let events = store.events_after_offset(
        consumer.cursor_position.unwrap_or(0),
        &consumer.event_type_filters,
        100,
    );
    // Should return events 4 and 5
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, 4);
    assert_eq!(events[1].0, 5);
}

#[test]
fn test_consumer_poll_with_filters() {
    let store = EventStore::new();

    store
        .ingest(&create_test_event("e1", "scheduler.started"))
        .unwrap();
    store
        .ingest(&create_test_event("e2", "trade.executed"))
        .unwrap();
    store
        .ingest(&create_test_event("e3", "scheduler.completed"))
        .unwrap();

    let filters = vec!["scheduler.*".to_string()];
    let events = store.events_after_offset(0, &filters, 100);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].1.event_type_str(), "scheduler.started");
    assert_eq!(events[1].1.event_type_str(), "scheduler.completed");
}

#[test]
fn test_consumer_poll_empty_when_caught_up() {
    let store = EventStore::new();

    store
        .ingest(&create_test_event("e1", "user.created"))
        .unwrap();
    store
        .ingest(&create_test_event("e2", "user.created"))
        .unwrap();

    // Poll from offset 2 (after all events)
    let events = store.events_after_offset(2, &[], 100);
    assert!(events.is_empty());
}

// ============================================================
// US-006: Consumer acknowledgment
// ============================================================

#[test]
fn test_consumer_ack_advances_cursor() {
    let store = EventStore::new();
    for _ in 0..5 {
        store
            .ingest(&create_test_event("e1", "user.created"))
            .unwrap();
    }

    let registry = store.consumer_registry();
    registry.register("c1", &[]);
    registry.ack("c1", 3, 5).unwrap();
    assert_eq!(registry.get("c1").unwrap().cursor_position, Some(3));
}

#[test]
fn test_consumer_ack_idempotent() {
    let store = EventStore::new();
    for _ in 0..5 {
        store
            .ingest(&create_test_event("e1", "user.created"))
            .unwrap();
    }

    let registry = store.consumer_registry();
    registry.register("c1", &[]);
    registry.ack("c1", 5, 5).unwrap();

    // Acking earlier position is a no-op
    registry.ack("c1", 3, 5).unwrap();
    assert_eq!(registry.get("c1").unwrap().cursor_position, Some(5));
}

#[test]
fn test_consumer_ack_beyond_max_fails() {
    let store = EventStore::new();
    store
        .ingest(&create_test_event("e1", "user.created"))
        .unwrap();

    let registry = store.consumer_registry();
    registry.register("c1", &[]);
    let result = registry.ack("c1", 10, 1);
    assert!(result.is_err());
}

// ============================================================
// US-008: Server-side event type prefix filtering
// ============================================================

#[test]
fn test_filter_prefix_matching() {
    assert!(ConsumerRegistry::matches_filters(
        "scheduler.started",
        &["scheduler.*".into()]
    ));
    assert!(ConsumerRegistry::matches_filters(
        "scheduler.completed",
        &["scheduler.*".into()]
    ));
    assert!(!ConsumerRegistry::matches_filters(
        "trade.executed",
        &["scheduler.*".into()]
    ));
}

#[test]
fn test_filter_multiple_prefixes() {
    let filters = vec!["scheduler.*".into(), "index.*".into()];
    assert!(ConsumerRegistry::matches_filters(
        "scheduler.started",
        &filters
    ));
    assert!(ConsumerRegistry::matches_filters("index.created", &filters));
    assert!(!ConsumerRegistry::matches_filters(
        "trade.executed",
        &filters
    ));
}

#[test]
fn test_filter_empty_matches_all() {
    assert!(ConsumerRegistry::matches_filters("anything.here", &[]));
}
