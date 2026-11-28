use allsource_core::{
    domain::entities::Event,
    infrastructure::{
        security::{
            auth::{Claims, Role},
            middleware::AuthContext,
        },
        web::event_history::{HistoryGatewaySubject, history},
    },
    store::{EventStore, EventStoreConfig},
};
use axum::{
    Extension, Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    routing::get,
};
use chrono::Utc;
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};
use tower::ServiceExt;
use uuid::Uuid;

const SINCE: &str = "2026-11-20T00:00:00Z";
const UNTIL: &str = "2026-11-21T00:00:00Z";

fn event(tenant: &str, entity: &str, id: u128, timestamp: &str) -> Event {
    let mut event = Event::from_strings(
        "deployment.released.v1".into(),
        entity.into(),
        tenant.into(),
        json!({"version":"2.4"}),
        None,
    )
    .unwrap();
    event.id = Uuid::from_u128(id);
    event.timestamp = timestamp.parse().unwrap();
    event
}

async fn request(store: &Arc<EventStore>, query: &str, role: Role) -> (StatusCode, Value) {
    let claims = Claims::new(
        "gateway".into(),
        "internal".into(),
        role,
        chrono::Duration::minutes(5),
    );
    let response = Router::new()
        .route("/api/v1/events/history", get(history))
        .with_state(Arc::clone(store))
        .layer(Extension(HistoryGatewaySubject("gateway".into())))
        .layer(Extension(AuthContext { claims }))
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/events/history?{query}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    if status.is_success() {
        assert_eq!(response.headers()["cache-control"], "private, no-store");
    }
    let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn timeline(entity: &str) -> String {
    format!("tenant_id=shop&entity_id={entity}&since={SINCE}&until={UNTIL}")
}

#[tokio::test]
async fn gateway_bearer_is_authenticated_on_history_routes() {
    use allsource_core::infrastructure::security::{
        auth::AuthManager,
        middleware::{AuthState, auth_middleware, should_skip_auth},
    };
    let auth_manager = Arc::new(AuthManager::default());
    let (id, token) =
        auth_manager.create_api_key("fixture".into(), "gateway".into(), Role::Admin, None);
    let app = Router::new()
        .route("/api/v1/events/history", get(history))
        .with_state(Arc::new(EventStore::new()))
        .layer(Extension(HistoryGatewaySubject(id.id.to_string())))
        .layer(axum::middleware::from_fn_with_state(
            AuthState { auth_manager },
            auth_middleware,
        ));
    let request = Request::builder()
        .uri(format!("/api/v1/events/history?{}", timeline("checkout")))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(app.oneshot(request).await.unwrap().status(), StatusCode::OK);
    assert!(!should_skip_auth("/api/v1/events/history/workspaces"));
    assert!(should_skip_auth("/api/v1/events/query"));
}

#[tokio::test]
async fn tenant_admin_cannot_use_gateway_tenant_selector() {
    let claims = Claims::new(
        "customer".into(),
        "shop".into(),
        Role::Admin,
        chrono::Duration::minutes(5),
    );
    let app = Router::new()
        .route("/api/v1/events/history", get(history))
        .with_state(Arc::new(EventStore::new()))
        .layer(Extension(HistoryGatewaySubject("gateway".into())))
        .layer(Extension(AuthContext { claims }));
    let request = Request::builder()
        .uri(format!("/api/v1/events/history?{}", timeline("checkout")))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.oneshot(request).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn persisted_and_hot_history_has_total_order_half_open_bounds_and_exact_citations() {
    let directory = tempfile::tempdir().unwrap();
    let config = || EventStoreConfig::with_persistence(directory.path());
    {
        let writer = EventStore::with_config(config());
        for id in (1..=22).rev() {
            writer
                .ingest(&event("shop", "checkout", id, SINCE))
                .unwrap();
        }
        writer
            .ingest(&event("shop", "checkout", 100, UNTIL))
            .unwrap();
        writer
            .ingest(&event("other", "checkout", 200, SINCE))
            .unwrap();
        writer.flush_storage().unwrap();
    }
    let store = Arc::new(EventStore::with_config(config()));
    store.ingest(&event("shop", "checkout", 23, SINCE)).unwrap();
    let (status, page) = request(&store, &timeline("checkout"), Role::Admin).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["protocol"], "retained-history-v1");
    assert_eq!(page["coverage"], "complete");
    assert_eq!(page["tenant_id"], "shop");
    assert_eq!(page["entity_id"], "checkout");
    assert_eq!(page["since"], SINCE);
    assert_eq!(page["until"], UNTIL);
    assert!(
        page["read_point"]
            .as_str()
            .unwrap()
            .parse::<chrono::DateTime<Utc>>()
            .is_ok()
    );
    assert_eq!(page["truncated"], true);
    let rows = page["events"].as_array().unwrap();
    assert_eq!(rows.len(), 21);
    for (row, id) in rows.iter().zip((3..=23).rev()) {
        assert_eq!(row["id"], Uuid::from_u128(id).to_string());
        assert_eq!(row["tenant_id"], "shop");
        assert_eq!(row["timestamp"], SINCE);
    }
    let archived = &rows[1];
    let lookup = format!(
        "tenant_id=shop&event_id={}",
        archived["id"].as_str().unwrap()
    );
    let (status, page) = request(&store, &lookup, Role::Admin).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["events"], json!([archived]));
    let (_, foreign) = request(&store, &lookup.replace("shop", "other"), Role::Admin).await;
    assert_eq!(foreign["events"], json!([]));
    let (_, empty) = request(&store, &timeline("cart"), Role::Admin).await;
    assert_eq!(empty["coverage"], "complete");
    assert_eq!(empty["events"], json!([]));
    assert_eq!(empty["truncated"], false);
    assert_eq!(
        request(&store, &lookup, Role::ReadOnly).await.0,
        StatusCode::FORBIDDEN
    );
}

fn parquet_file(dir: &Path) -> Option<std::path::PathBuf> {
    std::fs::read_dir(dir).unwrap().flatten().find_map(|entry| {
        let path = entry.path();
        if path.is_dir() {
            parquet_file(&path)
        } else {
            (path.extension().and_then(|value| value.to_str()) == Some("parquet")).then_some(path)
        }
    })
}

#[tokio::test]
async fn submicrosecond_history_survives_flush_and_preserves_citation_time() {
    let directory = tempfile::tempdir().unwrap();
    let config = || EventStoreConfig::with_persistence(directory.path());
    let timestamp = |n: u32| format!("2026-11-20T00:00:00.123456{n:03}Z");
    let query = format!(
        "tenant_id=shop&entity_id=checkout&since={}&until={}",
        timestamp(500),
        timestamp(950)
    );
    let store = Arc::new(EventStore::with_config(config()));
    for (id, nano) in [(999, 100), (123, 900), (234, 800)] {
        store
            .ingest(&event("shop", "checkout", id, &timestamp(nano)))
            .unwrap();
    }
    let (_, hot) = request(&store, &query, Role::Admin).await;
    store.flush_storage().unwrap();
    drop(store);
    let store = Arc::new(EventStore::with_config(config()));
    let (_, cold) = request(&store, &query, Role::Admin).await;
    assert_eq!(cold["coverage"], "complete");
    assert_eq!(hot["events"], cold["events"]);
    let rows = cold["events"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["timestamp"], timestamp(900));
    assert_eq!(rows[1]["timestamp"], timestamp(800));
    let lookup = format!("tenant_id=shop&event_id={}", Uuid::from_u128(123));
    let (_, detail) = request(&store, &lookup, Role::Admin).await;
    assert_eq!(detail["events"], json!([rows[0]]));
}

#[tokio::test]
async fn legacy_archive_cannot_place_a_row_across_a_submicrosecond_boundary() {
    use allsource_core::infrastructure::persistence::ParquetStorage;
    let directory = tempfile::tempdir().unwrap();
    let storage = ParquetStorage::new(directory.path()).unwrap();
    let legacy = event("shop", "checkout", 1, "2026-11-20T00:00:00.123456Z");
    storage
        .write_atomic_parquet("shop", "snapshot.legacy", &[legacy])
        .unwrap();
    drop(storage);
    let store = Arc::new(EventStore::with_config(EventStoreConfig::with_persistence(
        directory.path(),
    )));
    let narrow = format!(
        "tenant_id=shop&entity_id=checkout&since={SINCE}&until=2026-11-20T00:00:00.123456500Z"
    );
    for _ in 0..2 {
        let (status, page) = request(&store, &narrow, Role::Admin).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(page["events"], json!([]));
        assert_eq!(page["coverage"], "partial");
        store.hydrate_all_from_storage().unwrap();
    }
    let (_, page) = request(&store, &timeline("checkout"), Role::Admin).await;
    assert_eq!(page["events"].as_array().unwrap().len(), 1);
    assert_eq!(page["coverage"], "partial");
}

#[tokio::test]
#[cfg(unix)]
async fn failed_configured_storage_or_wal_recovery_refuses_history() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("blocked");
    std::os::unix::fs::symlink(directory.path().join("missing-storage-volume"), &path).unwrap();
    let wal_dir = directory.path().join("wal");
    std::fs::create_dir(&wal_dir).unwrap();
    std::fs::write(wal_dir.join("wal-0000000000000000.log"), b"{corrupt}\n").unwrap();
    for config in [
        EventStoreConfig::with_persistence(&path),
        EventStoreConfig::with_wal(&path, Default::default()),
        EventStoreConfig::with_wal(&wal_dir, Default::default()),
    ] {
        let store = Arc::new(EventStore::with_config(config));
        let (status, page) = request(&store, &timeline("checkout"), Role::Admin).await;
        assert!(status.is_server_error(), "{status} {page}");
        assert!(page.get("coverage").is_none());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_observes_whole_concurrent_batches() {
    let store = Arc::new(EventStore::new());
    let (start, ready) = std::sync::mpsc::channel();
    let writer = {
        let store = Arc::clone(&store);
        std::thread::spawn(move || {
            ready
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap();
            for batch in 0..10 {
                store
                    .ingest_batch(vec![
                        event("shop", "checkout", 2 * batch + 1, SINCE),
                        event("shop", "checkout", 2 * batch + 2, SINCE),
                    ])
                    .unwrap();
            }
        })
    };
    start.send(()).unwrap();
    for _ in 0..8 {
        let (status, page) = request(&store, &timeline("checkout"), Role::Admin).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(page["events"].as_array().unwrap().len() % 2, 0);
    }
    writer.join().unwrap();
    let (_, page) = request(&store, &timeline("checkout"), Role::Admin).await;
    assert_eq!(page["events"].as_array().unwrap().len(), 20);
}

#[tokio::test]
async fn missing_inventory_reports_partial_and_rejects_mixed_selectors() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(EventStore::with_config(EventStoreConfig::with_persistence(
        directory.path(),
    )));
    let (_, page) = request(&store, &timeline("checkout"), Role::Admin).await;
    assert_eq!(page["coverage"], "partial");
    let both = format!("{}&event_id={}", timeline("checkout"), Uuid::new_v4());
    assert_eq!(
        request(&store, &both, Role::Admin).await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn missing_or_unreadable_retained_segment_never_reports_complete() {
    for missing in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(EventStore::with_config(EventStoreConfig::with_persistence(
            directory.path(),
        )));
        store.ingest(&event("shop", "checkout", 1, SINCE)).unwrap();
        store.flush_storage().unwrap();
        let path = parquet_file(directory.path()).unwrap();
        if missing {
            std::fs::remove_file(path).unwrap();
        } else {
            std::fs::write(path, b"unreadable").unwrap();
        }
        let (status, page) = request(&store, &timeline("checkout"), Role::Admin).await;
        assert!(status.is_server_error());
        assert!(page.get("coverage").is_none());
    }
}
