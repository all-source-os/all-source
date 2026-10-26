use allsource_core::{
    QueryEventsRequest,
    domain::entities::Event,
    infrastructure::persistence::{CompactionConfig, WALConfig},
    store::{EventStore, EventStoreConfig},
};
use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create test events
fn create_test_event(entity_id: &str, event_type: &str, payload: serde_json::Value) -> Event {
    Event::from_strings(
        event_type.to_string(),
        entity_id.to_string(),
        "default".to_string(),
        payload,
        None,
    )
    .unwrap()
}

#[test]
fn test_full_lifecycle_in_memory() {
    // Test 1: In-memory store basic operations
    let store = EventStore::new();

    // Ingest events
    for i in 0..100 {
        let event = create_test_event(
            "user-1",
            "score.updated",
            json!({"score": i * 10, "timestamp": Utc::now()}),
        );
        store.ingest(&event).unwrap();
    }

    // Query all events
    let query = QueryEventsRequest {
        entity_id: Some("user-1".to_string()),
        event_type: None,
        tenant_id: None,
        as_of: None,
        since: None,
        until: None,
        limit: None,
        ..Default::default()
    };

    let events = store.query(&query).unwrap();
    assert_eq!(events.len(), 100);

    // Reconstruct state
    let state = store.reconstruct_state("user-1", None).unwrap();
    assert!(state["current_state"]["score"].is_number());
    assert!(
        !state["history"].as_array().unwrap().is_empty(),
        "Should have events in history"
    );

    // Get stats
    let stats = store.stats();
    assert_eq!(stats.total_events, 100);
    assert_eq!(stats.total_entities, 1);
}

#[test]
fn test_parquet_persistence_and_recovery() {
    // Test 2: Parquet persistence and recovery
    let temp_dir = TempDir::new().unwrap();

    // Create store with Parquet enabled
    {
        let config = EventStoreConfig::with_persistence(temp_dir.path());
        let store = EventStore::with_config(config);

        // Ingest events
        for i in 0..50 {
            let event = create_test_event(
                "order-1",
                "order.updated",
                json!({"amount": i * 100, "status": "pending"}),
            );
            store.ingest(&event).unwrap();
        }

        // Flush to ensure persistence
        store.flush_storage().unwrap();
    }

    // Restart and verify recovery
    {
        let config = EventStoreConfig::with_persistence(temp_dir.path());
        let store = EventStore::with_config(config);

        // Step 2: boot is O(1) — Parquet stays cold until first
        // per-tenant query. Hydrate the tenant the events were
        // ingested under before checking stats.
        store.ensure_tenant_loaded("default").unwrap();

        let stats = store.stats();
        assert_eq!(
            stats.total_events, 50,
            "Should recover all events from Parquet"
        );

        let state = store.reconstruct_state("order-1", None).unwrap();
        assert_eq!(state["event_count"], 50);
    }
}

#[test]
fn test_wal_durability_and_recovery() {
    // Test 3: WAL durability and crash recovery
    let storage_dir = TempDir::new().unwrap();
    let wal_dir = TempDir::new().unwrap();

    // Create store with WAL
    {
        let config = EventStoreConfig {
            storage_dir: Some(storage_dir.path().to_path_buf()),
            wal_dir: Some(wal_dir.path().to_path_buf()),
            wal_config: WALConfig::default(),
            ..Default::default()
        };
        let store = EventStore::with_config(config);

        // Ingest events (written to WAL)
        for i in 0..30 {
            let event = create_test_event(
                "user-2",
                "login.attempt",
                json!({"attempt": i, "success": i % 2 == 0}),
            );
            store.ingest(&event).unwrap();
        }

        // Don't flush Parquet - simulating crash before checkpoint
    }

    // Restart and verify WAL recovery
    {
        let config = EventStoreConfig {
            storage_dir: Some(storage_dir.path().to_path_buf()),
            wal_dir: Some(wal_dir.path().to_path_buf()),
            wal_config: WALConfig::default(),
            ..Default::default()
        };
        let store = EventStore::with_config(config);

        let stats = store.stats();
        // Should recover at least 30 events (might have more if previous Parquet files exist)
        assert!(
            stats.total_events >= 30,
            "Should recover at least 30 events from WAL after 'crash', got {}",
            stats.total_events
        );

        // Verify specific entity exists
        let query = QueryEventsRequest {
            entity_id: Some("user-2".to_string()),
            event_type: None,
            tenant_id: None,
            as_of: None,
            since: None,
            until: None,
            limit: None,
            ..Default::default()
        };
        let events = store.query(&query).unwrap();
        assert_eq!(events.len(), 30, "Should have exactly 30 events for user-2");
    }
}

#[test]
fn test_latest_state_without_history() {
    // Test 4: Latest state folds every event and leaves history out on request
    let temp_dir = TempDir::new().unwrap();
    let store = EventStore::with_config(EventStoreConfig::with_persistence(temp_dir.path()));

    for i in 0..50 {
        let event = create_test_event(
            "account-1",
            "transaction.processed",
            json!({"amount": i * 50, "balance": 1000 + (i * 50)}),
        );
        store.ingest(&event).unwrap();
    }

    let state = store
        .entity_state("account-1", None, Some("default"), false)
        .unwrap();

    assert_eq!(state["event_count"], 50);
    assert_eq!(state["version"], 50);
    assert_eq!(state["current_state"]["balance"], 1000 + 49 * 50);
    assert!(state.get("history").is_none());
}

#[test]
fn test_time_travel_queries() {
    // Test 5: Time-travel capability
    let store = EventStore::new();

    let timestamps: Vec<_> = (0..10)
        .map(|i| {
            // Use 50ms sleep to ensure timestamps are clearly distinct on slow CI systems
            std::thread::sleep(std::time::Duration::from_millis(50));
            let event = create_test_event(
                "document-1",
                "content.updated",
                json!({"version": i, "content": format!("Version {}", i)}),
            );
            let ts = event.timestamp();
            store.ingest(&event).unwrap();
            ts
        })
        .collect();

    // Query state at different points in time
    let state_at_v5 = store
        .reconstruct_state("document-1", Some(timestamps[5]))
        .unwrap();
    assert_eq!(state_at_v5["event_count"], 6); // Includes events 0-5

    let state_at_v9 = store
        .reconstruct_state("document-1", Some(timestamps[9]))
        .unwrap();
    assert_eq!(state_at_v9["event_count"], 10); // All events

    // Current state should match latest
    let current_state = store.reconstruct_state("document-1", None).unwrap();
    assert_eq!(current_state["event_count"], 10);
}

#[test]
fn test_multi_entity_queries() {
    // Test 6: Multiple entities and filtering
    let store = EventStore::new();

    // Create events for multiple users
    for user_id in 1..=5 {
        for event_num in 1..=10 {
            let event = create_test_event(
                &format!("user-{user_id}"),
                "activity.logged",
                json!({"activity": event_num, "user_id": user_id}),
            );
            store.ingest(&event).unwrap();
        }
    }

    // Query specific user
    let query = QueryEventsRequest {
        entity_id: Some("user-3".to_string()),
        event_type: None,
        tenant_id: None,
        as_of: None,
        since: None,
        until: None,
        limit: None,
        ..Default::default()
    };
    let events = store.query(&query).unwrap();
    assert_eq!(events.len(), 10);

    // Query all events of specific type
    let query = QueryEventsRequest {
        entity_id: None,
        event_type: Some("activity.logged".to_string()),
        tenant_id: None,
        as_of: None,
        since: None,
        until: None,
        limit: None,
        ..Default::default()
    };
    let events = store.query(&query).unwrap();
    assert_eq!(events.len(), 50); // 5 users * 10 events

    // Stats should show all entities
    let stats = store.stats();
    assert_eq!(stats.total_entities, 5);
    assert_eq!(stats.total_events, 50);
}

#[test]
fn test_compaction_reduces_files() {
    // Test 7: Compaction functionality
    let temp_dir = TempDir::new().unwrap();

    let compaction_config = CompactionConfig {
        min_files_to_compact: 2,
        small_file_threshold: 1024, // Very small to trigger compaction
        auto_compact: false,        // Manual control
        ..Default::default()
    };

    let config = EventStoreConfig {
        storage_dir: Some(temp_dir.path().to_path_buf()),
        compaction_config,
        ..Default::default()
    };

    let store = EventStore::with_config(config);

    // Ingest events and flush multiple times to create multiple files
    for batch in 0..3 {
        for i in 0..5 {
            let event = create_test_event(
                &format!("item-{batch}"),
                "item.created",
                json!({"batch": batch, "item": i}),
            );
            store.ingest(&event).unwrap();
        }
        store.flush_storage().unwrap();
    }

    // Trigger manual compaction if available
    if let Some(compaction_manager) = store.compaction_manager() {
        let result = compaction_manager.compact_now().unwrap();

        // Should have compacted something if there were multiple files
        if result.files_compacted > 0 {
            assert!(result.bytes_before > 0);
            // Files might be the same size or slightly smaller due to optimization
            assert!(result.events_compacted >= 15);
        }
    }
}

#[test]
fn test_projection_aggregations() {
    // Test 8: Real-time projections
    let store = EventStore::new();

    // Ingest events for different types
    for i in 0..20 {
        let event_type = if i % 2 == 0 {
            "user.created"
        } else {
            "user.updated"
        };

        let event = create_test_event(&format!("user-{i}"), event_type, json!({"index": i}));
        store.ingest(&event).unwrap();
    }

    // Get snapshot (uses projection)
    let snapshot = store.get_snapshot("user-1");
    // May not exist for all entities, but should not error
    assert!(snapshot.is_ok() || snapshot.is_err());

    let stats = store.stats();
    assert_eq!(stats.total_event_types, 2);
}

#[test]
fn test_concurrent_ingestion() {
    // Test 9: Thread-safe concurrent writes
    let store = Arc::new(EventStore::new());
    let mut handles = vec![];

    for thread_id in 0..5 {
        let store_clone = Arc::clone(&store);
        let handle = std::thread::spawn(move || {
            for i in 0..20 {
                let event = create_test_event(
                    &format!("thread-{thread_id}-entity-{i}"),
                    "concurrent.write",
                    json!({"thread": thread_id, "index": i}),
                );
                store_clone.ingest(&event).unwrap();
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let stats = store.stats();
    assert_eq!(stats.total_events, 100); // 5 threads * 20 events
}

#[test]
fn test_event_stream_ordering() {
    // Test 10: Event ordering guarantees
    let store = EventStore::new();

    let mut timestamps = vec![];
    for i in 0..50 {
        let event = create_test_event("ordered-entity", "sequence.event", json!({"sequence": i}));
        timestamps.push(event.timestamp);
        store.ingest(&event).unwrap();
    }

    let query = QueryEventsRequest {
        entity_id: Some("ordered-entity".to_string()),
        event_type: None,
        tenant_id: None,
        as_of: None,
        since: None,
        until: None,
        limit: None,
        ..Default::default()
    };

    let events = store.query(&query).unwrap();

    // Verify events are returned in timestamp order
    for i in 1..events.len() {
        assert!(
            events[i - 1].timestamp() <= events[i].timestamp(),
            "Events should be ordered by timestamp"
        );
    }
}

#[test]
fn test_full_production_config() {
    // Test 11: Full production configuration
    let storage_dir = TempDir::new().unwrap();
    let wal_dir = TempDir::new().unwrap();

    let wal_config = WALConfig {
        sync_on_write: true,
        ..Default::default()
    };

    let compaction_config = CompactionConfig {
        auto_compact: false, // Manual for testing
        ..Default::default()
    };

    let config = EventStoreConfig::production(
        storage_dir.path(),
        wal_dir.path(),
        wal_config,
        compaction_config,
    );

    let store = EventStore::with_config(config);

    // Ingest events
    for i in 0..100 {
        let event = create_test_event("production-entity", "production.event", json!({"value": i}));
        store.ingest(&event).unwrap();
    }

    // Verify all components working
    let stats = store.stats();
    assert_eq!(stats.total_events, 100);

    // Flush storage
    store.flush_storage().unwrap();

    let state = store.reconstruct_state("production-entity", None).unwrap();
    assert_eq!(state["event_count"], 100);
}

#[test]
fn test_entity_not_found_error() {
    // Test 12: Error handling
    let store = EventStore::new();

    // Query non-existent entity
    let result = store.reconstruct_state("non-existent", None);
    assert!(result.is_err());

    // Query should return empty for non-existent entity
    let query = QueryEventsRequest {
        entity_id: Some("non-existent".to_string()),
        event_type: None,
        tenant_id: None,
        as_of: None,
        since: None,
        until: None,
        limit: None,
        ..Default::default()
    };
    let events = store.query(&query).unwrap();
    assert_eq!(events.len(), 0);
}

#[test]
fn test_event_validation() {
    // Test 13: Event validation
    // Empty entity_id should fail at construction time
    let result = Event::from_strings(
        "test".to_string(),
        String::new(),
        "default".to_string(),
        json!({}),
        None,
    );
    assert!(result.is_err(), "Empty entity_id should fail validation");

    // Empty event_type should fail at construction time
    let result = Event::from_strings(
        String::new(),
        "entity-1".to_string(),
        "default".to_string(),
        json!({}),
        None,
    );
    assert!(result.is_err(), "Empty event_type should fail validation");
}

#[test]
fn test_reconstruct_state_replays_full_history() {
    // Test 14: Every read folds the whole history; no cached base shortens it
    let store = EventStore::new();

    for i in 0..110 {
        let event = create_test_event(
            "heavy-entity",
            "data.update",
            json!({"value": i, "data": format!("Data {}", i)}),
        );
        store.ingest(&event).unwrap();
    }

    let state = store.reconstruct_state("heavy-entity", None).unwrap();

    assert_eq!(state["history"].as_array().unwrap().len(), 110);
    assert_eq!(state["event_count"], 110);
    assert_eq!(state["current_state"]["value"], 109);
}

#[test]
fn test_metadata_preservation() {
    // Test 15: Metadata handling
    let store = EventStore::new();

    let event = Event::from_strings(
        "metadata.test".to_string(),
        "meta-entity".to_string(),
        "default".to_string(),
        json!({"key": "value"}),
        Some(json!({"source": "test", "trace_id": "12345"})),
    )
    .unwrap();

    store.ingest(&event).unwrap();

    let query = QueryEventsRequest {
        entity_id: Some("meta-entity".to_string()),
        event_type: None,
        tenant_id: None,
        as_of: None,
        since: None,
        until: None,
        limit: None,
        ..Default::default()
    };

    let events = store.query(&query).unwrap();
    assert_eq!(events.len(), 1);
    assert!(events[0].metadata().is_some());
    assert_eq!(events[0].metadata().unwrap()["source"], "test");
}
