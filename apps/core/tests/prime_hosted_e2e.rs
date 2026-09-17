//! End-to-end: stateless `HostedPrime` over a REAL Core (no wiremock).
//!
//! Closes the "everything mocks Core" gap for bead t-10f876 (slice 5). A real
//! multi-tenant `EmbeddedCore` (real WAL/DashMap store + real tenant-filtered
//! query) is wrapped in a thin HTTP server speaking Core's events API; two
//! `HostedPrime` instances talk to it over real HTTP via reqwest. The second
//! instance has a COLD cache, so its reads MUST come back through Core —
//! proving writes really persist to Core and that Core enforces cross-tenant
//! isolation (tenant A's query never returns tenant B's events).
//!
//! Run with: `cargo test -p allsource --features server,prime-recall,prime-vectors,multi-tenant --test prime_hosted_e2e`
//! (compiles to nothing without those features, so default test builds are unaffected.)

#![cfg(all(feature = "prime-recall", feature = "server", feature = "multi-tenant"))]

use std::{sync::Arc, time::Duration};

use allsource_core::{
    embedded::{Config, EmbeddedCore, IngestEvent, Query},
    prime::{hosted::HostedPrime, types::EntityId},
};

/// `node:{type}:{id}` wire id (node_entity_id is deprecated in favor of this).
fn node_eid(node_type: &str, id: &str) -> String {
    EntityId::node(node_type, id).to_wire()
}
use axum::{
    Json, Router,
    extract::{Query as AxQuery, State},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct IngestBody {
    event_type: String,
    entity_id: String,
    payload: Value,
    metadata: Option<Value>,
    tenant_id: Option<String>,
}

#[derive(Deserialize)]
struct QueryParams {
    entity_id: Option<String>,
    event_type: Option<String>,
    event_type_prefix: Option<String>,
    tenant_id: Option<String>,
    limit: Option<usize>,
}

/// Thin HTTP shim over a REAL `EmbeddedCore` — just enough of Core's events API
/// for `HttpCore`/`HostedPrime` to talk to. The store, tenant stamping, and
/// tenant-filtered query are all the real Core implementation.
async fn ingest(State(core): State<Arc<EmbeddedCore>>, Json(b): Json<IngestBody>) -> Json<Value> {
    core.ingest(IngestEvent {
        entity_id: &b.entity_id,
        event_type: &b.event_type,
        payload: b.payload,
        metadata: b.metadata,
        tenant_id: b.tenant_id.as_deref(),
    })
    .await
    .expect("ingest");
    Json(json!({ "ok": true }))
}

async fn query(
    State(core): State<Arc<EmbeddedCore>>,
    AxQuery(p): AxQuery<QueryParams>,
) -> Json<Value> {
    let mut q = Query::new();
    if let Some(v) = p.entity_id {
        q = q.entity_id(v);
    }
    if let Some(v) = p.event_type {
        q = q.event_type(v);
    }
    if let Some(v) = p.event_type_prefix {
        q = q.event_type_prefix(v);
    }
    if let Some(v) = p.tenant_id {
        q = q.tenant_id(v);
    }
    if let Some(v) = p.limit {
        q = q.limit(v);
    }
    let events = core.query(q).await.expect("query");
    Json(json!({ "events": events, "count": events.len() }))
}

/// Boot the shim over a real multi-tenant EmbeddedCore; return the base URL.
async fn spawn_real_core() -> String {
    let core = Arc::new(
        EmbeddedCore::open(Config::builder().single_tenant(false).build().unwrap())
            .await
            .unwrap(),
    );
    let app = Router::new()
        .route("/api/v1/events", post(ingest))
        .route("/api/v1/events/query", get(query))
        .with_state(core);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn hosted_prime_round_trips_and_isolates_tenants_through_real_core() {
    let base = spawn_real_core().await;

    // Writer: add nodes for two tenants against the real Core.
    let writer = HostedPrime::connect(base.clone(), None, 8, Duration::from_mins(1));
    let alice = writer
        .add_node("tenant-a", "contact", json!({"name": "Alice"}))
        .await
        .unwrap();
    let bob = writer
        .add_node("tenant-b", "contact", json!({"name": "Bob"}))
        .await
        .unwrap();
    let alice_eid = node_eid("contact", &alice.0);
    let bob_eid = node_eid("contact", &bob.0);

    // Reader: a SECOND HostedPrime with a cold cache. Its reads must round-trip
    // through Core, so this proves real persistence — not just the writer's
    // warm cache.
    let reader = HostedPrime::connect(base, None, 8, Duration::from_mins(1));

    // Tenant A sees its own node …
    let a = reader.get_node("tenant-a", &alice_eid).await.unwrap();
    assert!(a.is_some(), "tenant-a should read its own node from Core");
    assert_eq!(a.unwrap().properties["name"], "Alice");

    // … and CANNOT see tenant B's node (Core's tenant-filtered query enforces it).
    let leak = reader.get_node("tenant-a", &bob_eid).await.unwrap();
    assert!(
        leak.is_none(),
        "tenant-a must NOT see tenant-b's node — cross-tenant leak!"
    );

    // Symmetric: tenant B sees Bob, not Alice.
    assert!(
        reader
            .get_node("tenant-b", &bob_eid)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        reader
            .get_node("tenant-b", &alice_eid)
            .await
            .unwrap()
            .is_none()
    );

    // search() is also tenant-scoped through Core.
    let a_contacts = reader.search("tenant-a", "contact").await.unwrap();
    assert_eq!(a_contacts.len(), 1, "tenant-a sees only its own contact");
}
