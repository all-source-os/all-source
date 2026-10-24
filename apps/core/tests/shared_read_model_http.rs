#![cfg(feature = "server")]

use allsource_core::{
    domain::entities::Event,
    infrastructure::{
        security::{
            auth::{Claims, Role},
            middleware::AuthContext,
        },
        web::{api, read_model_api, response_limit},
    },
    store::{EventStore, read_model::ModelCommit},
};
use axum::{
    Extension, Router, middleware,
    routing::{get, post},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

#[tokio::test]
async fn one_five_twenty_idle_http_clients_transfer_metadata_only_after_warmup() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let store = Arc::new(EventStore::new());
        store.ingest(&Event::from_strings("item.enqueued".into(), "one".into(), "alice".into(), json!({"id":"one"}), None).unwrap()).unwrap();
        let head = store.tenant_feed("alice", None, 0).unwrap().head;
        store.commit_read_model("alice", "board", ModelCommit { expected_revision: 0, checkpoint: head, ready: true, reset: false, changes: BTreeMap::from([("q/one".into(), Some(json!({"title":"x".repeat(60000)})))]) }).unwrap();
        let auth = AuthContext { claims: Claims::new("owner".into(), "alice".into(), Role::Admin, chrono::Duration::minutes(5)) };
        let app = Router::new().route("/api/v1/event-feed", get(read_model_api::feed)).route("/api/v1/read-models/{name}", get(read_model_api::read).put(read_model_api::commit)).route("/api/v1/events", post(api::ingest_event)).route("/api/v1/events/batch", post(api::ingest_events_batch)).with_state(store.clone()).layer(Extension(auth)).layer(middleware::from_fn(response_limit::bounded));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let mut servers = tokio::task::JoinSet::new();
        servers.spawn(async move { axum::serve(listener, app).await.unwrap(); });
        let client = reqwest::Client::builder().timeout(Duration::from_secs(2)).build().unwrap();
        for (event_type, entity_id) in [("item.changed", "read-model:forged"), ("allsource.read_model.committed", "ordinary")] {
            let body = json!({"event_type":event_type,"entity_id":entity_id,"tenant_id":"alice","payload":{}});
            let refused = client.post(format!("{base}/api/v1/events")).json(&body).send().await.unwrap();
            assert_eq!(refused.status(), reqwest::StatusCode::BAD_REQUEST);
            let refused = client.post(format!("{base}/api/v1/events/batch")).json(&json!({"events":[body]})).send().await.unwrap();
            assert_eq!(refused.status(), reqwest::StatusCode::BAD_REQUEST);
        }
        let warm: Value = client.get(format!("{base}/api/v1/read-models/board?prefix=q/&limit=100")).send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
        assert_eq!(warm["rows"]["q/one"]["title"].as_str().unwrap().len(), 60000);
        let revision = warm["content_revision"].as_u64().unwrap();
        for clients in [1, 5, 20] {
            let started = Instant::now();
            let mut jobs = tokio::task::JoinSet::new();
            for _ in 0..clients {
                let client = client.clone(); let base = base.clone();
                jobs.spawn(async move {
                    let mut bytes = 0;
                    for _ in 0..5 {
                        let feed: Value = client.get(format!("{base}/api/v1/event-feed?limit=0")).send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
                        assert!(feed["events"].as_array().unwrap().is_empty());
                        let body = client.get(format!("{base}/api/v1/read-models/board?prefix=q/&limit=100&revision={revision}")).send().await.unwrap().error_for_status().unwrap().bytes().await.unwrap();
                        assert!(body.len() < 512);
                        let page: Value = serde_json::from_slice(&body).unwrap();
                        assert_eq!(page["unchanged"], true);
                        assert!(page["rows"].as_object().unwrap().is_empty());
                        bytes += body.len();
                    }
                    bytes
                });
            }
            let mut bytes = 0;
            while let Some(result) = jobs.join_next().await { bytes += result.unwrap(); }
            println!("clients={clients} rounds=5 model_body_bytes={bytes} source_payload_bytes=0 elapsed_ms={}", started.elapsed().as_millis());
            assert_eq!(store.total_events(), 2, "idle clients must not append projection or checkpoint events");
        }
        servers.abort_all();
        while servers.join_next().await.is_some() {}
    }).await.expect("HTTP projection proof exceeded 30s");
}
