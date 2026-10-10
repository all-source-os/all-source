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

#[test]
fn server_warmup_policy_is_explicit_and_preserves_strict_input_limits() {
    assert!(
        EventStoreConfig::default()
            .http_archive_warmup_timeout
            .is_none()
    );
    for data_dir in [None, Some("synthetic-data".to_string())] {
        let (config, _) = EventStoreConfig::from_env_vars(
            data_dir.clone(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(
            config.http_archive_warmup_timeout,
            data_dir.map(|_| Duration::from_secs(30))
        );
        assert_eq!(config.strict_archive_limits.timeout, Duration::from_secs(4));
        assert_eq!(config.strict_archive_limits.max_files, 50_000);
    }
}

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

#[test]
fn http_append_rejects_corrupt_over_budget_or_read_only_archives() {
    for failure in ["file cap", "corrupt", "read only"] {
        let directory = TempDir::new().unwrap();
        let seed = EventStore::with_config(EventStoreConfig::with_persistence(directory.path()));
        seed.ingest_with_expected_version(&event(), Some(0))
            .unwrap();
        seed.flush_storage().unwrap();
        drop(seed);
        let mut config = EventStoreConfig {
            http_archive_warmup_timeout: Some(Duration::from_secs(30)),
            ..EventStoreConfig::with_persistence(directory.path())
        };
        match failure {
            "file cap" => config.strict_archive_limits.max_files = 0,
            "corrupt" => std::fs::write(
                directory.path().join(TENANT).join("events-corrupt.parquet"),
                "synthetic invalid parquet",
            )
            .unwrap(),
            "read only" => config.read_only = true,
            _ => unreachable!(),
        }
        let store = EventStore::with_config(config);
        let mut subscriber = store.subscribe_events();
        let cancellation = Arc::new(AtomicBool::new(false));
        // Mirrors archive_work::append: the admission check no longer warms
        // the archive, so a damaged or over-budget one is refused by the
        // conditional write that reads it.
        let error = store
            .prepare_http_append(&event(), &cancellation)
            .and_then(|()| {
                store.ingest_with_expected_version_cancellable(
                    &event(),
                    Some(1),
                    Some(&cancellation),
                )
            })
            .unwrap_err();
        if failure == "file cap" {
            assert!(error.to_string().contains("budget exceeded: files"));
        }
        assert!(!store.tenant_loader.is_complete(TENANT), "{failure}");
        assert_eq!(store.total_events(), 0, "{failure}");
        assert!(subscriber.try_recv().is_err(), "{failure}");
    }
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
    // The version index resolves after the entry cancellation check and before
    // the held durability gate.
    // test-hang-allow: bounded observation of the real store reaching that boundary.
    tokio::time::timeout(Duration::from_secs(1), async {
        while !store
            .version_index_entities
            .contains_key("synthetic-entity")
        {
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
    // A conditional write resolves its version straight from the archive and
    // never takes the tenant load lock, so holding that lock cannot stall it.
    assert_eq!(pending.await.unwrap().status(), 200);
    release_tx.send(()).unwrap();
    holder.join().unwrap().unwrap();
    server.abort();
    assert!(server.await.unwrap_err().is_cancelled());
    assert_eq!(store.total_events(), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn timed_out_http_append_leaves_no_trace_of_the_cancelled_command() {
    let directory = TempDir::new().unwrap();
    let storage_dir = directory.path().join("storage");
    let seed = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
    seed.ingest_with_expected_version(&event(), Some(0))
        .unwrap();
    seed.flush_storage().unwrap();
    drop(seed);
    let config = EventStoreConfig {
        storage_dir: Some(storage_dir),
        wal_dir: Some(directory.path().join("wal")),
        http_archive_warmup_timeout: Some(Duration::from_secs(10)),
        ..Default::default()
    };
    let store = Arc::new(EventStore::with_config(config.clone()));
    let mut subscriber = store.subscribe_events();
    // Hold the storage lock the version resolve needs, so the conditional
    // append stalls on the archive read rather than on a cache load.
    let lock_store = Arc::clone(&store);
    let (held_tx, held_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _guard = lock_store.storage.as_ref().unwrap().write();
        held_tx.send(()).unwrap();
        // test-hang-allow: controlled cold-load contention outlives the five-second response.
        release_rx.recv_timeout(Duration::from_secs(8))
    });
    held_rx.await.unwrap();
    let app = Router::new()
        .route("/health", get(api::health))
        .route("/events", post(api::ingest_event))
        .with_state(Arc::clone(&store));
    // test-hang-allow: owned ephemeral listener, aborted and joined before reopening WAL.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(7))
        .no_proxy()
        .build()
        .unwrap();
    let command = serde_json::json!({
        "tenant_id": TENANT, "entity_id": "synthetic-entity",
        "event_type": "synthetic.updated", "payload": {}, "expected_version": 1
    });
    let response = client
        .post(format!("{url}/events"))
        .json(&command)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    assert!(
        response
            .text()
            .await
            .unwrap()
            .contains("response deadline exceeded")
    );
    assert!(!store.tenant_loader.is_complete(TENANT));
    assert_eq!(
        client
            .get(format!("{url}/health"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    release_tx.send(()).unwrap();
    holder.join().unwrap().unwrap();
    // Cancellation reaches the archive read, so the abandoned command leaves
    // nothing behind: no event, no broadcast, no half-built version index.
    // Under memory pressure, abandoning the read is the point.
    assert_eq!(store.total_events(), 0);
    assert!(
        !store
            .version_index_entities
            .contains_key("synthetic-entity")
    );
    assert!(subscriber.try_recv().is_err());
    let response = client
        .post(format!("{url}/events"))
        .json(&command)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(store.get_entity_version("synthetic-entity"), 2);
    server.abort();
    assert!(server.await.unwrap_err().is_cancelled());
    // test-hang-allow: prove all request/worker store leases ended before reopening WAL.
    tokio::time::timeout(Duration::from_secs(2), async {
        while Arc::strong_count(&store) != 1 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    drop(store);
    let reopened = EventStore::with_config(config);
    let (events, count) = reopened
        .query_retained_entity(TENANT, "synthetic-entity", 10, &ReadScope::unrestricted())
        .unwrap();
    assert_eq!(count, 2);
    assert_eq!(
        events.iter().map(|event| event.version).collect::<Vec<_>>(),
        vec![1, 2]
    );
}
