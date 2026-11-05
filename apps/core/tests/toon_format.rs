//! TDD RED phase tests for Phase 8: TOON Format in Embedded API.
//!
//! TOON (Token-Oriented Object Notation) is a compact format that uses ~50%
//! fewer tokens than JSON for tabular data — ideal for LLM consumption.
//!
//! Run with: cargo test --features embedded-toon --test toon_format

#[cfg(feature = "embedded-toon")]
mod tests {
    use allsource_core::embedded::{Config, EmbeddedCore, IngestEvent, Query};
    use serde_json::json;

    // =========================================================================
    // query_toon returns compact TOON-encoded string
    // =========================================================================

    #[tokio::test]
    async fn query_toon_returns_valid_toon_string() {
        let core = open_core().await;

        core.ingest(IngestEvent {
            entity_id: "order-1",
            event_type: "order.placed",
            payload: json!({"total": 99.99, "currency": "USD"}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        let toon = core
            .query_toon(Query::new().entity_id("order-1"))
            .await
            .unwrap();

        // TOON output is a non-empty string
        assert!(!toon.is_empty());
        // Should contain the event data
        assert!(toon.contains("order-1") || toon.contains("order.placed"));
    }

    #[tokio::test]
    async fn query_toon_reduces_structural_overhead() {
        let core = open_core().await;

        // Ingest several events
        for i in 0..10 {
            core.ingest(IngestEvent {
                entity_id: &format!("user-{i}"),
                event_type: "user.created",
                payload: json!({"name": format!("User {i}"), "email": format!("user{i}@example.com")}),
                metadata: None,
                tenant_id: None,
            })
            .await
            .unwrap();
        }

        let toon = core
            .query_toon(Query::new().event_type("user.created"))
            .await
            .unwrap();

        let events = core
            .query(Query::new().event_type("user.created"))
            .await
            .unwrap();
        let json_str = serde_json::to_string(&events).unwrap();

        // TOON removes JSON structural overhead: quotes around keys, braces, brackets
        let json_braces = json_str.matches('{').count() + json_str.matches('}').count();
        let toon_braces = toon.matches('{').count() + toon.matches('}').count();

        assert!(
            toon_braces < json_braces,
            "TOON ({toon_braces} braces) should have fewer braces than JSON ({json_braces})"
        );

        // TOON should not have quoted keys (JSON has "key": format)
        let json_quoted_keys = json_str.matches("\"event_type\":").count();
        let toon_quoted_keys = toon.matches("\"event_type\":").count();
        assert_eq!(
            toon_quoted_keys, 0,
            "TOON should not have JSON-style quoted keys"
        );
        assert!(json_quoted_keys > 0, "JSON should have quoted keys");
    }

    #[tokio::test]
    async fn query_toon_empty_result_returns_empty_toon() {
        let core = open_core().await;

        let toon = core
            .query_toon(Query::new().entity_id("nonexistent"))
            .await
            .unwrap();

        // Empty result should still be valid (empty string or minimal TOON)
        // Not an error
        assert!(toon.len() < 50); // Very small output for empty result
    }

    #[tokio::test]
    async fn query_toon_single_event() {
        let core = open_core().await;

        core.ingest(IngestEvent {
            entity_id: "doc-1",
            event_type: "doc.created",
            payload: json!({"title": "Hello World", "author": "Alice"}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        let toon = core
            .query_toon(Query::new().entity_id("doc-1"))
            .await
            .unwrap();

        assert!(!toon.is_empty());
        // Single event should still produce valid TOON
        assert!(toon.contains("doc-1") || toon.contains("doc.created"));
    }

    // =========================================================================
    // TOON round-trip and validation
    // =========================================================================

    #[tokio::test]
    async fn query_toon_round_trips_through_decode() {
        let core = open_core().await;

        core.ingest(IngestEvent {
            entity_id: "rt-1",
            event_type: "roundtrip.test",
            payload: json!({"name": "Alice", "score": 42}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        let toon = core
            .query_toon(Query::new().entity_id("rt-1"))
            .await
            .unwrap();

        // Decode TOON back to JSON
        let opts = toon_format::DecodeOptions::new();
        let decoded: serde_json::Value =
            toon_format::decode(&toon, &opts).expect("TOON should decode back to valid JSON");

        // Should be an array with one event
        let arr = decoded.as_array().expect("Decoded TOON should be an array");
        assert_eq!(arr.len(), 1, "Should contain exactly 1 event");

        let event = &arr[0];
        assert_eq!(event["entity_id"], "rt-1");
        assert_eq!(event["event_type"], "roundtrip.test");
        assert_eq!(event["payload"]["name"], "Alice");
        assert_eq!(event["payload"]["score"], 42);
    }

    #[tokio::test]
    async fn query_toon_preserves_field_values() {
        let core = open_core().await;

        core.ingest(IngestEvent {
            entity_id: "fv-1",
            event_type: "field.values",
            payload: json!({
                "string": "hello world",
                "number": 42.5,
                "boolean": true,
                "null_val": null,
                "nested": {"key": "value"}
            }),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        let toon = core
            .query_toon(Query::new().entity_id("fv-1"))
            .await
            .unwrap();

        let opts = toon_format::DecodeOptions::new();
        let decoded: serde_json::Value = toon_format::decode(&toon, &opts).expect("Should decode");
        let event = &decoded.as_array().unwrap()[0];

        assert_eq!(event["payload"]["string"], "hello world");
        assert_eq!(event["payload"]["boolean"], true);
        // Nested objects should survive round-trip
        assert_eq!(event["payload"]["nested"]["key"], "value");
    }

    #[tokio::test]
    async fn query_toon_handles_special_characters() {
        let core = open_core().await;

        core.ingest(IngestEvent {
            entity_id: "special-1",
            event_type: "special.chars",
            payload: json!({
                "with_quotes": "he said \"hello\"",
                "with_newline": "line1\nline2",
                "with_unicode": "emoji: \u{1F600}",
                "with_tabs": "col1\tcol2"
            }),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        // Should not error during encoding
        let toon = core
            .query_toon(Query::new().entity_id("special-1"))
            .await
            .unwrap();

        assert!(!toon.is_empty(), "TOON output should not be empty");

        // Should be decodable
        let opts = toon_format::DecodeOptions::new();
        let decoded: serde_json::Value =
            toon_format::decode(&toon, &opts).expect("TOON with special chars should decode");
        let event = &decoded.as_array().unwrap()[0];
        // Unicode should survive
        let unicode_val = event["payload"]["with_unicode"]
            .as_str()
            .unwrap_or_default();
        assert!(
            unicode_val.contains('\u{1F600}'),
            "Unicode should survive round-trip"
        );
    }

    // =========================================================================
    // Helper
    // =========================================================================

    async fn open_core() -> EmbeddedCore {
        EmbeddedCore::open(Config::builder().build().unwrap())
            .await
            .unwrap()
    }
}
