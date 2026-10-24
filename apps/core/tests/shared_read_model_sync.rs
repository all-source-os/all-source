#![cfg(all(feature = "server", feature = "embedded-sync"))]

use allsource_core::{infrastructure::web::api, store::EventStore};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::post,
};
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn sync_push_refuses_reserved_journal_before_any_batch_write() {
    for (event_type, entity_id) in [
        ("allsource.read_model.committed", "ordinary"),
        ("item.changed", "read-model:forged"),
    ] {
        let store = Arc::new(EventStore::new());
        let app = Router::new()
            .route("/sync/push", post(api::sync_push_handler))
            .with_state(store.clone());
        let event = |kind: &str, entity: &str| {
            json!({
                "event_id":"synthetic", "origin_region":"local",
                "hlc_timestamp":{"physical_ms":1,"logical":0,"node_id":1},
                "event_data":{"event_type":kind,"entity_id":entity,"tenant_id":"alice","payload":{}}
            })
        };
        let body = json!({"node_id":"local", "version_vector":{}, "events":[event("item.changed","allowed"),event(event_type,entity_id)]});
        let response = app
            .oneshot(
                Request::post("/sync/push")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            store.stats().total_events,
            0,
            "reserved journal refusal must precede all batch writes"
        );
    }
}
