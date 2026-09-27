//! Integration tests for leader-follower WAL replication.
//!
//! These tests spin up a leader EventStore with a WAL shipper and a follower
//! EventStore with a WAL receiver, ingest events into the leader, and verify
//! that the follower receives and replays them via the TCP replication protocol.
//!
//! The real `WalShipper` / `WalReceiver` types only exist when the
//! `replication` feature is enabled — community builds compile in stub structs
//! with no constructors. Gate the whole file on the feature so default-feature
//! builds (and `quality-rust` CI) don't fail to compile against the stubs. Run
//! with: `cargo test -p allsource-core --features replication -- --ignored`.
#![cfg(feature = "replication")]

use allsource_core::{
    QueryEventsRequest,
    domain::entities::Event,
    infrastructure::persistence::WALConfig,
    replication::{WalReceiver, WalShipper},
    store::{EventStore, EventStoreConfig},
};
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create a test event.
fn test_event(entity_id: &str, event_type: &str, payload: serde_json::Value) -> Event {
    Event::from_strings(
        event_type.to_string(),
        entity_id.to_string(),
        "default".to_string(),
        payload,
        None,
    )
    .unwrap()
}

/// Find a random available TCP port by binding to port 0.
fn random_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Create an EventStore with WAL enabled in the given directory.
fn store_with_wal(dir: &std::path::Path) -> EventStore {
    let wal_dir = dir.join("wal");
    let config = EventStoreConfig::with_wal(&wal_dir, WALConfig::default());
    EventStore::with_config(config)
}

#[tokio::test]
#[ignore = "requires running replication infrastructure — run with: cargo test -- --ignored"]
async fn test_leader_follower_wal_replication() {
    // ---------------------------------------------------------------
    // 1. Create temporary directories for leader and follower data.
    // ---------------------------------------------------------------
    let leader_dir = TempDir::new().unwrap();
    let follower_dir = TempDir::new().unwrap();

    // ---------------------------------------------------------------
    // 2. Create the leader EventStore with WAL and start the shipper.
    // ---------------------------------------------------------------
    let leader_store = Arc::new(store_with_wal(leader_dir.path()));

    let (shipper, replication_tx) = WalShipper::new();
    leader_store.enable_wal_replication(replication_tx);

    let shipper = Arc::new(shipper);
    let replication_port = random_port();

    // Start the shipper TCP server in the background.
    let shipper_handle = {
        let shipper = Arc::clone(&shipper);
        tokio::spawn(async move {
            let _ = shipper.serve(replication_port).await;
        })
    };

    // Give the TCP listener a moment to bind.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // ---------------------------------------------------------------
    // 3. Create the follower EventStore and start the WAL receiver.
    // ---------------------------------------------------------------
    let follower_store = Arc::new(EventStore::with_config(EventStoreConfig::with_persistence(
        follower_dir.path().join("archive"),
    )));

    let leader_addr = format!("127.0.0.1:{replication_port}");
    let follower_wal_dir = follower_dir.path().join("follower-wal");

    let receiver = WalReceiver::new(leader_addr, &follower_wal_dir, Arc::clone(&follower_store))
        .expect("WalReceiver should initialize successfully");

    let receiver = Arc::new(receiver);

    // Start the receiver loop in the background.
    let receiver_clone = Arc::clone(&receiver);
    let receiver_handle = tokio::spawn(async move {
        receiver_clone.run().await;
    });

    // Wait for the follower to connect and subscribe.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // ---------------------------------------------------------------
    // 4. Ingest events into the leader's EventStore.
    // ---------------------------------------------------------------
    let num_events: usize = 5;
    for i in 0..num_events {
        let event = test_event(
            &format!("entity-{i}"),
            "test.replicated",
            serde_json::json!({"index": i, "source": "leader"}),
        );
        leader_store
            .ingest(&event)
            .expect("leader ingest should succeed");
    }

    // ---------------------------------------------------------------
    // 5. Wait for replication to propagate.
    // ---------------------------------------------------------------
    // Poll the follower's event count with a timeout instead of a fixed sleep,
    // to keep the test fast while being resilient to scheduling jitter.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let follower_stats = follower_store.stats();
        if follower_stats.total_events >= num_events {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "Timed out waiting for follower replication. Expected {} events, got {}",
            num_events,
            follower_stats.total_events,
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // ---------------------------------------------------------------
    // 6. Verify the follower has the same events as the leader.
    // ---------------------------------------------------------------
    let leader_stats = leader_store.stats();
    let follower_stats = follower_store.stats();

    assert_eq!(
        leader_stats.total_events, follower_stats.total_events,
        "Follower should have the same event count as the leader",
    );

    // Query each entity on both leader and follower and compare.
    for i in 0..num_events {
        let entity_id = format!("entity-{i}");

        let leader_query = QueryEventsRequest {
            entity_id: Some(entity_id.clone()),
            event_type: None,
            tenant_id: None,
            as_of: None,
            since: None,
            until: None,
            limit: None,
            ..Default::default()
        };
        let follower_query = QueryEventsRequest {
            entity_id: Some(entity_id.clone()),
            event_type: None,
            tenant_id: None,
            as_of: None,
            since: None,
            until: None,
            limit: None,
            ..Default::default()
        };

        let leader_events = leader_store.query(&leader_query).unwrap();
        let follower_events = follower_store.query(&follower_query).unwrap();

        assert_eq!(
            leader_events.len(),
            follower_events.len(),
            "Event count mismatch for entity {entity_id}",
        );

        // Verify event content matches.
        for (le, fe) in leader_events.iter().zip(follower_events.iter()) {
            assert_eq!(le.entity_id, fe.entity_id, "Entity ID mismatch");
            assert_eq!(le.event_type, fe.event_type, "Event type mismatch");
            assert_eq!(le.payload, fe.payload, "Event payload mismatch");
        }
    }

    // Verify replication status on the shipper side shows 1 follower.
    let replication_status = shipper.status();
    assert_eq!(
        replication_status.followers, 1,
        "Shipper should report exactly 1 connected follower",
    );

    // Verify receiver status shows connected.
    let receiver_status = receiver.status();
    assert!(
        receiver_status.connected,
        "Receiver should report as connected",
    );
    assert!(
        receiver_status.total_replayed >= num_events as u64,
        "Receiver should have replayed at least {} events, got {}",
        num_events,
        receiver_status.total_replayed,
    );

    // The follower must keep buffered copies resident, then reload the same
    // network-replicated events after their archive flush and cache eviction.
    follower_store.evict_tenant("default");
    assert_eq!(follower_store.total_events(), num_events);
    follower_store.flush_storage().unwrap();
    follower_store.evict_tenant("default");
    assert_eq!(follower_store.total_events(), 0);
    let query = QueryEventsRequest {
        tenant_id: Some("default".into()),
        ..Default::default()
    };
    let leader_ids: Vec<_> = leader_store
        .query(&query)
        .unwrap()
        .iter()
        .map(|event| event.id)
        .collect();
    let reloaded_ids: Vec<_> = follower_store
        .query(&query)
        .unwrap()
        .iter()
        .map(|event| event.id)
        .collect();
    assert_eq!(reloaded_ids, leader_ids);

    // ---------------------------------------------------------------
    // 7. Clean up: shut down receiver and abort shipper.
    // ---------------------------------------------------------------
    receiver.shutdown();
    receiver_handle.abort();
    shipper_handle.abort();
    // test-hang-allow: owned loopback tasks are aborted and joined with a deadline.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), receiver_handle)
        .await
        .expect("owned receiver must exit after abort");
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), shipper_handle)
        .await
        .expect("owned shipper must exit after abort");
}

#[tokio::test]
#[ignore = "requires running replication infrastructure — run with: cargo test -- --ignored"]
async fn test_late_follower_catches_up_from_live_wal() {
    // This test verifies that a follower connecting AFTER events have already
    // been ingested can still catch up via the live WAL broadcast, provided
    // the events are within the broadcast channel buffer.
    let leader_dir = TempDir::new().unwrap();
    let follower_dir = TempDir::new().unwrap();

    let leader_store = Arc::new(store_with_wal(leader_dir.path()));

    let (shipper, replication_tx) = WalShipper::new();
    leader_store.enable_wal_replication(replication_tx);

    let shipper = Arc::new(shipper);
    let replication_port = random_port();

    let shipper_handle = {
        let shipper = Arc::clone(&shipper);
        tokio::spawn(async move {
            let _ = shipper.serve(replication_port).await;
        })
    };

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Ingest events BEFORE the follower connects.
    let num_events = 3;
    for i in 0..num_events {
        let event = test_event("late-entity", "test.late", serde_json::json!({"seq": i}));
        leader_store.ingest(&event).unwrap();
    }

    // Now connect the follower.
    let follower_store = Arc::new(EventStore::new());
    let leader_addr = format!("127.0.0.1:{replication_port}");
    let follower_wal_dir = follower_dir.path().join("follower-wal");

    let receiver =
        WalReceiver::new(leader_addr, &follower_wal_dir, Arc::clone(&follower_store)).unwrap();

    let receiver = Arc::new(receiver);
    let receiver_clone = Arc::clone(&receiver);
    let receiver_handle = tokio::spawn(async move {
        receiver_clone.run().await;
    });

    // Wait for catch-up.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Ingest more events after the follower is connected.
    let additional = 2;
    for i in 0..additional {
        let event = test_event(
            "late-entity",
            "test.late",
            serde_json::json!({"seq": num_events + i}),
        );
        leader_store.ingest(&event).unwrap();
    }

    // Wait for the additional events to replicate.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        // The follower may not have the pre-connect events (they were sent before
        // it subscribed) but should have the post-connect events via live WAL.
        let stats = follower_store.stats();
        if stats.total_events >= additional {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "Timed out waiting for late follower. Expected at least {} events, got {}",
            additional,
            stats.total_events,
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // The follower should have at least the events sent after it connected.
    let follower_stats = follower_store.stats();
    assert!(
        follower_stats.total_events >= additional,
        "Late follower should have received at least {} live events, got {}",
        additional,
        follower_stats.total_events,
    );

    // Clean up.
    receiver.shutdown();
    shipper_handle.abort();
    let _ = receiver_handle.await;
}

#[tokio::test]
#[ignore = "requires running replication infrastructure — run with: cargo test -- --ignored"]
async fn test_replication_preserves_event_data_fidelity() {
    // Verify that event payloads, entity IDs, and event types are preserved
    // exactly through the replication pipeline.
    let leader_dir = TempDir::new().unwrap();
    let follower_dir = TempDir::new().unwrap();

    let leader_store = Arc::new(store_with_wal(leader_dir.path()));
    let (shipper, replication_tx) = WalShipper::new();
    leader_store.enable_wal_replication(replication_tx);

    let shipper = Arc::new(shipper);
    let replication_port = random_port();

    let shipper_handle = {
        let s = Arc::clone(&shipper);
        tokio::spawn(async move {
            let _ = s.serve(replication_port).await;
        })
    };

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let follower_store = Arc::new(EventStore::new());
    let leader_addr = format!("127.0.0.1:{replication_port}");
    let follower_wal_dir = follower_dir.path().join("follower-wal");

    let receiver =
        WalReceiver::new(leader_addr, &follower_wal_dir, Arc::clone(&follower_store)).unwrap();
    let receiver = Arc::new(receiver);
    let rc = Arc::clone(&receiver);
    let receiver_handle = tokio::spawn(async move {
        rc.run().await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // Ingest events with diverse payloads.
    let payloads = vec![
        (
            "user-42",
            "user.registered",
            serde_json::json!({"email": "test@example.com", "plan": "pro"}),
        ),
        (
            "order-99",
            "order.placed",
            serde_json::json!({"items": [{"sku": "A1", "qty": 3}], "total": 59.97}),
        ),
        (
            "metric-1",
            "metric.recorded",
            serde_json::json!({"cpu": 0.87, "memory_mb": 2048, "tags": ["prod", "us-east"]}),
        ),
    ];

    for (entity_id, event_type, payload) in &payloads {
        let event = test_event(entity_id, event_type, payload.clone());
        leader_store.ingest(&event).unwrap();
    }

    // Wait for replication.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if follower_store.stats().total_events >= payloads.len() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "Timed out waiting for replication. Follower has {} of {} events",
            follower_store.stats().total_events,
            payloads.len(),
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // Verify each event's data is identical on the follower.
    for (entity_id, event_type, expected_payload) in &payloads {
        let query = QueryEventsRequest {
            entity_id: Some((*entity_id).to_string()),
            event_type: None,
            tenant_id: None,
            as_of: None,
            since: None,
            until: None,
            limit: None,
            ..Default::default()
        };

        let follower_events = follower_store.query(&query).unwrap();
        assert_eq!(
            follower_events.len(),
            1,
            "Expected exactly 1 event for entity {entity_id} on follower",
        );

        let fe = &follower_events[0];
        assert_eq!(fe.event_type.as_str(), *event_type);
        assert_eq!(fe.entity_id.as_str(), *entity_id);
        assert_eq!(
            fe.payload, *expected_payload,
            "Payload mismatch for entity {entity_id} through replication",
        );
    }

    // Clean up.
    receiver.shutdown();
    shipper_handle.abort();
    let _ = receiver_handle.await;
}
