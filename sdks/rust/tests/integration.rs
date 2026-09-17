//! Integration tests for the AllSource Rust SDK.
//!
//! These tests require a running AllSource Core instance.
//! They are skipped automatically if ALLSOURCE_TEST_CORE_URL is not set.
//!
//! # Running
//!
//! ```bash
//! # Start Core (default port 3900):
//! docker run -p 3900:3900 ghcr.io/all-source-os/chronos-core:latest
//!
//! # Run integration tests:
//! ALLSOURCE_TEST_CORE_URL=http://localhost:3900 \
//! ALLSOURCE_TEST_API_KEY=test-key \
//!   cargo test -p allsource --test integration
//!
//! # Against the Docker stack (leader on port 3280):
//! ALLSOURCE_TEST_CORE_URL=http://localhost:3280 \
//! ALLSOURCE_TEST_API_KEY=test-internal-key-for-integration \
//!   cargo test -p allsource --test integration
//! ```

use allsource::*;
use serde_json::json;
use std::time::Duration;

// ═══════════════════════════════════════════════════════════════════════════
// Test helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Read Core URL from env, or skip the test.
fn core_url() -> Option<String> {
    std::env::var("ALLSOURCE_TEST_CORE_URL").ok()
}

fn api_key() -> String {
    std::env::var("ALLSOURCE_TEST_API_KEY").unwrap_or_else(|_| "test-key".into())
}

/// Generate a unique entity ID for test isolation.
fn unique_entity(prefix: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}-{ts}")
}

/// Macro to skip test if Core is not running.
macro_rules! require_core {
    () => {
        match core_url() {
            Some(url) => url,
            None => {
                eprintln!("SKIPPED: set ALLSOURCE_TEST_CORE_URL to run integration tests");
                return;
            }
        }
    };
}

fn make_core_client(url: &str) -> CoreClient {
    CoreClient::new(url, &api_key()).expect("failed to create CoreClient")
}

fn make_query_client(url: &str) -> QueryClient {
    QueryClient::new(url, &api_key()).expect("failed to create QueryClient")
}

// ═══════════════════════════════════════════════════════════════════════════
// Health
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_core_health() {
    let url = require_core!();
    let client = make_core_client(&url);
    let health = client.health().await.expect("health check failed");
    assert_eq!(health.status, "healthy");
}

#[tokio::test]
async fn test_query_health() {
    let url = require_core!();
    let client = make_query_client(&url);
    let health = client.health().await.expect("health check failed");
    assert_eq!(health.status, "healthy");
}

// ═══════════════════════════════════════════════════════════════════════════
// Single event ingestion + query round-trip
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_ingest_and_query_single_event() {
    let url = require_core!();
    let core = make_core_client(&url);
    let qs = make_query_client(&url);
    let entity = unique_entity("sdk-single");

    // Ingest
    let resp = core
        .ingest_event(IngestEventInput {
            event_type: "sdk.test.created".into(),
            entity_id: entity.clone(),
            payload: json!({"test": true, "value": 42}),
            metadata: Some(json!({"source": "rust-sdk-integration-test"})),
            ..Default::default()
        })
        .await
        .expect("ingest failed");

    assert!(!resp.event_id.is_empty(), "event_id should not be empty");
    assert!(!resp.timestamp.is_empty(), "timestamp should not be empty");

    // Query back
    let events = qs.get_entity_events(&entity).await.expect("query failed");

    assert_eq!(events.count, 1, "should find exactly 1 event");
    assert_eq!(events.events.len(), 1);

    let event = &events.events[0];
    assert_eq!(event.entity_id, entity);
    assert_eq!(event.event_type, "sdk.test.created");
    assert_eq!(event.payload["value"], 42);
    assert_eq!(event.payload["test"], true);
}

#[tokio::test]
async fn test_ingest_with_no_metadata() {
    let url = require_core!();
    let core = make_core_client(&url);
    let entity = unique_entity("sdk-no-meta");

    let resp = core
        .ingest_event(IngestEventInput {
            event_type: "sdk.test.minimal".into(),
            entity_id: entity,
            payload: json!({}),
            metadata: None,
            ..Default::default()
        })
        .await
        .expect("ingest with no metadata failed");

    assert!(!resp.event_id.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// Batch ingestion
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_batch_ingest() {
    let url = require_core!();
    let core = make_core_client(&url);
    let qs = make_query_client(&url);
    let entity = unique_entity("sdk-batch");

    let events: Vec<IngestEventInput> = (0..5)
        .map(|i| IngestEventInput {
            event_type: format!("sdk.test.batch.item{i}"),
            entity_id: entity.clone(),
            payload: json!({"index": i}),
            metadata: None,
            ..Default::default()
        })
        .collect();

    let resp = core
        .ingest_batch(events)
        .await
        .expect("batch ingest failed");

    assert_eq!(resp.total, 5, "total should be 5");
    assert_eq!(resp.ingested, 5, "all 5 should be ingested");
    assert_eq!(resp.events.len(), 5, "should get 5 event responses");

    // Verify all have unique IDs
    let ids: std::collections::HashSet<&str> =
        resp.events.iter().map(|e| e.event_id.as_str()).collect();
    assert_eq!(ids.len(), 5, "all event IDs should be unique");

    // Query back
    let result = qs.get_entity_events(&entity).await.expect("query failed");
    assert_eq!(result.count, 5, "should find all 5 events");
}

// ═══════════════════════════════════════════════════════════════════════════
// Query filters
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_query_by_event_type() {
    let url = require_core!();
    let core = make_core_client(&url);
    let qs = make_query_client(&url);
    let entity = unique_entity("sdk-type-filter");
    let event_type = format!(
        "sdk.test.typed.{}",
        entity.split('-').next_back().unwrap_or("0")
    );

    for i in 0..3 {
        core.ingest_event(IngestEventInput {
            event_type: event_type.clone(),
            entity_id: entity.clone(),
            payload: json!({"seq": i}),
            metadata: None,
            ..Default::default()
        })
        .await
        .expect("ingest failed");
    }

    let result = qs
        .get_events_by_type(&event_type)
        .await
        .expect("query by type failed");

    assert!(
        result.count >= 3,
        "should find at least 3 events of this type"
    );
    for event in &result.events {
        assert_eq!(event.event_type, event_type);
    }
}

#[tokio::test]
async fn test_query_with_limit() {
    let url = require_core!();
    let core = make_core_client(&url);
    let qs = make_query_client(&url);
    let entity = unique_entity("sdk-limit");

    for i in 0..5 {
        core.ingest_event(IngestEventInput {
            event_type: "sdk.test.limit".into(),
            entity_id: entity.clone(),
            payload: json!({"seq": i}),
            metadata: None,
            ..Default::default()
        })
        .await
        .expect("ingest failed");
    }

    let params = QueryEventsParams::new().entity_id(&entity).limit(2);
    let result = qs.query_events(params).await.expect("query failed");

    assert_eq!(
        result.events.len(),
        2,
        "limit=2 should return exactly 2 events"
    );
}

#[tokio::test]
async fn test_query_nonexistent_entity() {
    let url = require_core!();
    let qs = make_query_client(&url);
    let result = qs
        .get_entity_events("nonexistent-entity-99999999")
        .await
        .expect("query should succeed with empty results");

    assert_eq!(result.count, 0);
    assert!(result.events.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// EventFolder integration — full round-trip
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Default)]
struct OrderFolder {
    total: f64,
    item_count: u32,
    status: String,
}

struct OrderState {
    total: f64,
    item_count: u32,
    status: String,
}

impl EventFolder for OrderFolder {
    type State = OrderState;

    fn apply(&mut self, event: &Event) -> bool {
        match event.event_type.as_str() {
            "sdk.test.order.created" => {
                self.status = "created".into();
                true
            }
            "sdk.test.order.item_added" => {
                self.item_count += 1;
                if let Some(price) = event
                    .payload
                    .get("price")
                    .and_then(serde_json::Value::as_f64)
                {
                    self.total += price;
                }
                true
            }
            "sdk.test.order.completed" => {
                self.status = "completed".into();
                true
            }
            _ => false,
        }
    }

    fn finalize(self) -> Option<OrderState> {
        if self.status.is_empty() {
            None
        } else {
            Some(OrderState {
                total: self.total,
                item_count: self.item_count,
                status: self.status,
            })
        }
    }
}

#[tokio::test]
async fn test_query_and_fold() {
    let url = require_core!();
    let core = make_core_client(&url);
    let qs = make_query_client(&url);
    let entity = unique_entity("sdk-fold");

    // Create an order lifecycle
    for (et, payload) in [
        ("sdk.test.order.created", json!({})),
        (
            "sdk.test.order.item_added",
            json!({"item": "Widget", "price": 29.99}),
        ),
        (
            "sdk.test.order.item_added",
            json!({"item": "Gadget", "price": 49.99}),
        ),
        ("sdk.test.order.completed", json!({})),
    ] {
        core.ingest_event(IngestEventInput {
            event_type: et.into(),
            entity_id: entity.clone(),
            payload,
            metadata: None,
            ..Default::default()
        })
        .await
        .unwrap();
    }

    // Fold into domain state
    let params = QueryEventsParams::new().entity_id(&entity);
    let state = qs
        .query_and_fold::<OrderFolder>(params)
        .await
        .expect("query_and_fold failed");

    let order = state.expect("folder should produce state");
    assert_eq!(order.status, "completed");
    assert_eq!(order.item_count, 2);
    assert!(
        (order.total - 79.98).abs() < 0.01,
        "total should be ~79.98, got {}",
        order.total
    );
}

#[tokio::test]
async fn test_fold_empty_entity() {
    let url = require_core!();
    let qs = make_query_client(&url);

    let params = QueryEventsParams::new().entity_id("nonexistent-fold-test");
    let state = qs
        .query_and_fold::<OrderFolder>(params)
        .await
        .expect("query should succeed");

    assert!(state.is_none(), "fold of empty stream should be None");
}

// ═══════════════════════════════════════════════════════════════════════════
// Error handling
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_connection_refused() {
    let mut config = ClientConfig::new("http://127.0.0.1:19999", "key");
    config.retry.max_retries = 0;
    let client = CoreClient::with_config(config).unwrap();

    let err = client.health().await.unwrap_err();
    assert!(
        matches!(err, Error::Http(_)),
        "expected Http error, got: {err}"
    );
    assert!(err.is_retryable(), "connection errors should be retryable");
}

#[tokio::test]
async fn test_circuit_breaker_trips() {
    let mut config = ClientConfig::new("http://127.0.0.1:19999", "key");
    config.retry.max_retries = 0;
    config.circuit_breaker_threshold = 2;
    config.circuit_breaker_recovery = Duration::from_mins(1);
    let client = CoreClient::with_config(config).unwrap();

    // Two failures to trip the breaker
    let _ = client.health().await;
    let _ = client.health().await;

    // Third should be rejected by circuit breaker
    let err = client.health().await.unwrap_err();
    assert!(
        err.is_circuit_open(),
        "expected CircuitOpen error, got: {err}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Client configuration
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_custom_timeout() {
    let url = require_core!();
    let mut config = ClientConfig::new(&url, &api_key());
    config.timeout = Duration::from_secs(5);
    config.retry.max_retries = 1;
    let client = CoreClient::with_config(config).unwrap();

    let health = client.health().await.expect("health check failed");
    assert_eq!(health.status, "healthy");
}

#[tokio::test]
async fn test_clone_clients_share_transport() {
    let url = require_core!();
    let core = make_core_client(&url);
    let core2 = core.clone();

    // Both clones should work
    let (h1, h2) = tokio::join!(core.health(), core2.health());
    h1.expect("original health failed");
    h2.expect("cloned health failed");
}

// ═══════════════════════════════════════════════════════════════════════════
// Concurrent usage
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_concurrent_ingestion() {
    let url = require_core!();
    let core = make_core_client(&url);
    let qs = make_query_client(&url);
    let entity = unique_entity("sdk-concurrent");

    let mut handles = Vec::new();
    for i in 0..10 {
        let core = core.clone();
        let entity = entity.clone();
        handles.push(tokio::spawn(async move {
            core.ingest_event(IngestEventInput {
                event_type: "sdk.test.concurrent".into(),
                entity_id: entity,
                payload: json!({"worker": i}),
                metadata: None,
                ..Default::default()
            })
            .await
        }));
    }

    for handle in handles {
        handle.await.expect("task panicked").expect("ingest failed");
    }

    let result = qs.get_entity_events(&entity).await.expect("query failed");
    assert_eq!(
        result.count, 10,
        "all 10 concurrent events should be stored"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Data integrity
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[allow(clippy::approx_constant)] // test data, not approximating a constant
async fn test_payload_fidelity() {
    let url = require_core!();
    let core = make_core_client(&url);
    let qs = make_query_client(&url);
    let entity = unique_entity("sdk-fidelity");

    let complex_payload = json!({
        "string": "hello world",
        "number": 42,
        "float": 3.14159,
        "boolean": true,
        "null_val": null,
        "nested": {
            "array": [1, 2, 3],
            "deep": {"key": "value"}
        },
        "emoji": "\u{1F980}",
        "unicode": "\u{65E5}\u{672C}\u{8A9E}",
        "special_chars": "line1\nline2\ttab"
    });

    core.ingest_event(IngestEventInput {
        event_type: "sdk.test.fidelity".into(),
        entity_id: entity.clone(),
        payload: complex_payload.clone(),
        metadata: None,
        ..Default::default()
    })
    .await
    .expect("ingest failed");

    let result = qs.get_entity_events(&entity).await.expect("query failed");
    assert_eq!(result.count, 1);

    let stored = &result.events[0].payload;
    assert_eq!(stored["string"], "hello world");
    assert_eq!(stored["number"], 42);
    assert_eq!(stored["boolean"], true);
    assert!(stored["null_val"].is_null());
    assert_eq!(stored["nested"]["array"][1], 2);
    assert_eq!(stored["nested"]["deep"]["key"], "value");
}

#[tokio::test]
async fn test_metadata_preserved() {
    let url = require_core!();
    let core = make_core_client(&url);
    let qs = make_query_client(&url);
    let entity = unique_entity("sdk-metadata");

    let metadata = json!({
        "correlation_id": "corr-123",
        "source": "integration-test",
        "trace_id": "abc-def-ghi"
    });

    core.ingest_event(IngestEventInput {
        event_type: "sdk.test.metadata".into(),
        entity_id: entity.clone(),
        payload: json!({"data": 1}),
        metadata: Some(metadata),
        ..Default::default()
    })
    .await
    .expect("ingest failed");

    let result = qs.get_entity_events(&entity).await.expect("query failed");
    assert_eq!(result.count, 1);

    let stored_meta = &result.events[0].metadata;
    assert_eq!(stored_meta["correlation_id"], "corr-123");
    assert_eq!(stored_meta["source"], "integration-test");
}

// ═══════════════════════════════════════════════════════════════════════════
// Event ordering
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_events_ordered_chronologically() {
    let url = require_core!();
    let core = make_core_client(&url);
    let qs = make_query_client(&url);
    let entity = unique_entity("sdk-order");

    for i in 0..5 {
        core.ingest_event(IngestEventInput {
            event_type: "sdk.test.ordered".into(),
            entity_id: entity.clone(),
            payload: json!({"seq": i}),
            metadata: None,
            ..Default::default()
        })
        .await
        .expect("ingest failed");

        // Ensure different timestamps
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let result = qs.get_entity_events(&entity).await.expect("query failed");
    assert_eq!(result.count, 5);

    // Events should be in chronological order
    for (i, event) in result.events.iter().enumerate() {
        let seq = event.payload["seq"].as_u64().expect("seq should be u64");
        assert_eq!(
            seq, i as u64,
            "events should be in order: expected seq={i}, got seq={seq}"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Projections
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_list_projections() {
    let url = require_core!();
    let qs = make_query_client(&url);

    let result = qs.list_projections().await;
    match result {
        Ok(resp) => {
            // Should return a valid response even if empty
            let _ = resp.total; // validates deserialization
        }
        Err(Error::Api { status: 404, .. }) => {
            // Acceptable — endpoint may not exist on all Core versions
        }
        Err(e) => panic!("unexpected error: {e}"),
    }
}
