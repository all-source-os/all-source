use super::*;
use crate::infrastructure::web::api;
use axum::{
    Router,
    routing::{get, post},
};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tempfile::TempDir;

const TENANT: &str = "synthetic-archive-worker";

fn event() -> Event {
    Event::from_strings(
        "synthetic.updated".into(),
        "synthetic-entity".into(),
        TENANT.into(),
        serde_json::json!({"synthetic": true}),
        None,
    )
    .unwrap()
}

#[test]
fn cancelled_conditional_append_does_not_reach_wal_or_cache() {
    let directory = TempDir::new().unwrap();
    let config = EventStoreConfig {
        storage_dir: Some(directory.path().join("storage")),
        wal_dir: Some(directory.path().join("wal")),
        ..Default::default()
    };
    let store = EventStore::with_config(config.clone());
    let mut subscriber = store.subscribe_events();
    let cancellation = Arc::new(AtomicBool::new(true));
    assert!(
        store
            .ingest_with_expected_version_cancellable(&event(), Some(0), Some(&cancellation))
            .is_err()
    );
    assert_eq!(store.total_events(), 0);
    assert!(subscriber.try_recv().is_err());
    drop(store);
    let reopened = EventStore::with_config(config);
    assert_eq!(reopened.total_events(), 0);
    assert_eq!(
        reopened
            .ingest_with_expected_version(&event(), Some(0))
            .unwrap(),
        1
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_is_checked_after_waiting_for_durability_gate() {
    let store = Arc::new(EventStore::new());
    let gate_store = Arc::clone(&store);
    let (held_tx, held_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _gate = gate_store.durability_gate.write();
        held_tx.send(()).unwrap();
        // test-hang-allow: bounded synthetic checkpoint contention.
        release_rx.recv_timeout(Duration::from_secs(2))
    });
    held_rx.await.unwrap();
    let cancellation = Arc::new(AtomicBool::new(false));
    let worker_store = Arc::clone(&store);
    let worker_cancel = Arc::clone(&cancellation);
    let worker = tokio::task::spawn_blocking(move || {
        worker_store.ingest_with_expected_version_cancellable(
            &event(),
            Some(0),
            Some(&worker_cancel),
        )
    });
    // Loading happens after the entry cancellation check and before the held durability gate.
    // test-hang-allow: bounded observation of the real store reaching that boundary.
    tokio::time::timeout(Duration::from_secs(1), async {
        while !store.is_tenant_loaded(TENANT) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    cancellation.store(true, Ordering::Release);
    release_tx.send(()).unwrap();
    assert!(worker.await.unwrap().is_err());
    holder.join().unwrap().unwrap();
    assert_eq!(store.total_events(), 0);
    assert_eq!(store.get_entity_version("synthetic-entity"), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn http_health_and_ordinary_writes_survive_a_waiting_conditional_load() {
    let directory = TempDir::new().unwrap();
    let store = Arc::new(EventStore::with_config(EventStoreConfig::with_persistence(
        directory.path(),
    )));
    let lock = store.tenant_loader.lock_for(TENANT);
    let (held_tx, held_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _guard = lock.lock();
        held_tx.send(()).unwrap();
        // test-hang-allow: synthetic cold-load contention, always releases within three seconds.
        release_rx.recv_timeout(Duration::from_secs(3))
    });
    held_rx.await.unwrap();

    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let entered = Arc::new(parking_lot::Mutex::new(Some(entered_tx)));
    let app = Router::new()
        .route("/health", get(api::health))
        .route("/events", post(api::ingest_event))
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let entered = Arc::clone(&entered);
                async move {
                    if request.uri().path() == "/events"
                        && let Some(signal) = entered.lock().take()
                    {
                        let _ = signal.send(());
                    }
                    next.run(request).await
                }
            },
        ))
        .with_state(Arc::clone(&store));
    // test-hang-allow: owned ephemeral loopback listener; server aborted and joined below.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .no_proxy()
        .build()
        .unwrap();
    let request = client
        .post(format!("{url}/events"))
        .json(&serde_json::json!({
            "tenant_id": TENANT, "entity_id": "conditional", "event_type": "synthetic.updated",
            "payload": {}, "expected_version": 0
        }));
    let pending = tokio::spawn(async move { request.send().await.unwrap() });
    tokio::time::timeout(Duration::from_secs(1), entered_rx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        client
            .get(format!("{url}/health"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(client.post(format!("{url}/events")).json(&serde_json::json!({
        "tenant_id": "synthetic-other", "entity_id": "ordinary", "event_type": "synthetic.updated", "payload": {}
    })).send().await.unwrap().status(), 200);
    assert!(
        !pending.is_finished(),
        "conditional write must wait for its archive load lock"
    );
    release_tx.send(()).unwrap();
    assert_eq!(pending.await.unwrap().status(), 200);
    holder.join().unwrap().unwrap();
    server.abort();
    assert!(server.await.unwrap_err().is_cancelled());
    assert_eq!(store.total_events(), 2);
}
