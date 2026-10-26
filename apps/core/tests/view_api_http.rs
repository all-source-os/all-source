#![cfg(feature = "server")]

use allsource_core::{
    domain::entities::Event,
    infrastructure::{
        security::{
            auth::{Claims, Role},
            middleware::AuthContext,
        },
        web::{read_model_api, response_limit, view_api},
    },
    store::{EventStore, view},
};
use axum::{Extension, Router, middleware, routing::get};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn principal(tenant: &str, role: Role) -> AuthContext {
    AuthContext {
        claims: Claims::new(
            format!("{tenant}-user"),
            tenant.into(),
            role,
            chrono::Duration::minutes(5),
        ),
    }
}

/// One server per principal, all on the same store, like the routes in
/// `api_v1.rs` behind the auth layer.
async fn serve(store: &Arc<EventStore>, auth: AuthContext) -> String {
    let app = Router::new()
        .route("/api/v1/event-feed", get(read_model_api::feed))
        .route(
            "/api/v1/read-models/{name}",
            get(read_model_api::read).put(read_model_api::commit),
        )
        .route("/api/v1/views", get(view_api::list_views))
        .route(
            "/api/v1/views/{view}",
            get(view_api::get_view)
                .put(view_api::put_view)
                .delete(view_api::delete_view),
        )
        .route(
            "/api/v1/views/{view}/entities",
            get(view_api::list_entities),
        )
        .route(
            "/api/v1/views/{view}/entities/{key}",
            get(view_api::get_entity),
        )
        .route("/api/v1/views/{view}/changes", get(view_api::changes))
        .route("/api/v1/admin/views", get(view_api::admin_views))
        .with_state(store.clone())
        .layer(Extension(auth))
        .layer(middleware::from_fn(response_limit::bounded));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

fn client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(25))
        .build()
        .unwrap()
}

fn definition() -> Value {
    json!({
        "source": { "prefixes": ["ops.probe."] },
        "key": "{payload.probe}:{day:timestamp}",
        "ops": [
            { "on": "ops.probe.observed", "set": { "probe": "payload.probe", "day": "day:timestamp" } },
            { "on": "ops.probe.observed", "when": { "payload.ok": true }, "increment": { "ok": 1 } },
            { "on": "ops.probe.observed", "when": { "payload.ok": false }, "increment": { "failed": 1 } },
            { "on": "ops.probe.observed", "max": { "last_seen_at": "timestamp" } },
            { "on": "ops.probe.observed", "set": { "status": "payload.status" } }
        ]
    })
}

async fn send(request: reqwest::RequestBuilder) -> (StatusCode, Value) {
    let response = request.send().await.unwrap();
    let status = response.status();
    let body = response.bytes().await.unwrap();
    let value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap()
    };
    (status, value)
}

/// The admin principal folding for `tenant`: reads the feed head and commits
/// one page of rows, as the Query Service does.
async fn admin_commit(
    http: &Client,
    admin: &str,
    tenant: &str,
    model: &str,
    expected_revision: u64,
    reset: bool,
    changes: Value,
) -> u64 {
    let (status, feed) = send(http.get(format!(
        "{admin}/api/v1/event-feed?tenant_id={tenant}&limit=0"
    )))
    .await;
    assert_eq!(status, StatusCode::OK, "{feed}");
    let commit = json!({
        "expected_revision": expected_revision,
        "checkpoint": feed["head"],
        "ready": true,
        "reset": reset,
        "changes": changes,
    });
    let (status, body) = send(
        http.put(format!(
            "{admin}/api/v1/read-models/{model}?tenant_id={tenant}"
        ))
        .json(&commit),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["revision"].as_u64().unwrap()
}

fn seeded_store() -> Arc<EventStore> {
    view::set_views_enabled(true);
    let store = Arc::new(EventStore::new());
    store
        .ingest(
            &Event::from_strings(
                "ops.probe.observed".into(),
                "probe-a".into(),
                "alice".into(),
                json!({"probe": "probe-a", "ok": true}),
                None,
            )
            .unwrap(),
        )
        .unwrap();
    store
}

#[tokio::test]
async fn a_defined_view_serves_rows_the_admin_folder_commits() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let store = seeded_store();
        let tenant = serve(&store, principal("alice", Role::Developer)).await;
        let admin = serve(&store, principal("system", Role::Admin)).await;
        let http = client();

        let (status, defined) = send(
            http.put(format!("{tenant}/api/v1/views/ops_status"))
                .json(&definition()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{defined}");
        assert_eq!(defined["view"], "ops_status");
        assert_eq!(defined["definition"], definition());
        assert_eq!(defined["generation"], 1);
        assert_eq!(defined["revision"], 0);
        assert_eq!(defined["checkpoint"], Value::Null);
        assert_eq!(defined["ready"], false);

        let (_, again) = send(
            http.put(format!("{tenant}/api/v1/views/ops_status"))
                .json(&definition()),
        )
        .await;
        assert_eq!(again["generation"], 1, "an identical definition keeps its generation");

        let mut empty_ops = definition();
        empty_ops["ops"] = json!([]);
        let (status, invalid) = send(
            http.put(format!("{tenant}/api/v1/views/ops_status"))
                .json(&empty_ops),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            invalid,
            json!({"error": "invalid_view", "detail": "ops must contain 1..64 operations"})
        );

        let (status, page) = send(http.get(format!("{tenant}/api/v1/views/ops_status/entities"))).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["rows"], json!([]));
        assert_eq!(page["generation"], 1);
        assert_eq!(page["ready"], false);

        let (_, listed) = send(http.get(format!("{tenant}/api/v1/views"))).await;
        assert_eq!(
            listed,
            json!({"views": [{"view": "ops_status", "generation": 1, "revision": 0, "ready": false}]})
        );

        let (status, refused) = send(http.get(format!("{tenant}/api/v1/admin/views"))).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(refused["error"], "admin_required");
        let (status, index) = send(http.get(format!("{admin}/api/v1/admin/views"))).await;
        assert_eq!(status, StatusCode::OK, "{index}");
        assert_eq!(
            index["views"],
            json!([{"tenant_id": "alice", "view": "ops_status", "generation": 1, "definition": definition()}])
        );

        let first = admin_commit(
            &http,
            &admin,
            "alice",
            "view_ops_status",
            0,
            true,
            json!({"probe-a:2026-10-27": {"ok": 1}, "probe-b:2026-10-27": {"ok": 2}}),
        )
        .await;
        assert_eq!(first, 1);

        let row_url = format!("{tenant}/api/v1/views/ops_status/entities/probe-a%3A2026-10-27");
        let response = http.get(&row_url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let etag = response.headers()["etag"].to_str().unwrap().to_owned();
        let latest: Value = response.json().await.unwrap();
        assert_eq!(
            latest,
            json!({"view": "ops_status", "key": "probe-a:2026-10-27", "state": {"ok": 1}, "version": 1, "revision": 1, "generation": 1, "ready": true})
        );
        let unchanged = http
            .get(&row_url)
            .header("if-none-match", &etag)
            .send()
            .await
            .unwrap();
        assert_eq!(unchanged.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(unchanged.headers()["etag"].to_str().unwrap(), etag);
        assert!(unchanged.bytes().await.unwrap().is_empty());

        let (status, missing) = send(http.get(format!(
            "{tenant}/api/v1/views/ops_status/entities/probe-q%3A2026-10-27"
        )))
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(missing, json!({"error": "not_found", "ready": true}));

        let (_, page_one) = send(http.get(format!(
            "{tenant}/api/v1/views/ops_status/entities?limit=1"
        )))
        .await;
        assert_eq!(
            page_one["rows"],
            json!([{"key": "probe-a:2026-10-27", "state": {"ok": 1}, "version": 1}])
        );
        assert_eq!(page_one["next"], "probe-a:2026-10-27");
        assert_eq!(page_one["revision"], 1);
        let (_, page_two) = send(http.get(format!(
            "{tenant}/api/v1/views/ops_status/entities?limit=1&after=probe-a%3A2026-10-27"
        )))
        .await;
        assert_eq!(page_two["rows"][0]["key"], "probe-b:2026-10-27");
        assert_eq!(page_two["next"], Value::Null);

        let second = admin_commit(
            &http,
            &admin,
            "alice",
            "view_ops_status",
            1,
            false,
            json!({"probe-a:2026-10-27": {"ok": 3}, "probe-b:2026-10-27": null}),
        )
        .await;
        assert_eq!(second, 2);
        let (status, changes) = send(http.get(format!(
            "{tenant}/api/v1/views/ops_status/changes?after=1"
        )))
        .await;
        assert_eq!(status, StatusCode::OK, "{changes}");
        assert_eq!(
            changes,
            json!({
                "view": "ops_status",
                "changes": [
                    {"key": "probe-a:2026-10-27", "version": 2, "state": {"ok": 3}},
                    {"key": "probe-b:2026-10-27", "version": 2, "state": null}
                ],
                "revision": 2,
                "caught_up": true,
                "generation": 1
            })
        );
        let (_, current) = send(http.get(format!(
            "{tenant}/api/v1/views/ops_status/changes?after=2"
        )))
        .await;
        assert_eq!(current["changes"], json!([]));
        assert_eq!(current["revision"], 2);
        assert_eq!(current["caught_up"], true);

        let (status, info) = send(http.get(format!("{tenant}/api/v1/views/ops_status"))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(info["revision"], 2);
        assert_eq!(info["ready"], true);
        assert!(info["checkpoint"]["epoch"].is_u64());
    })
    .await
    .expect("view lifecycle exceeded 60s");
}

#[tokio::test]
async fn a_waiting_changes_request_returns_when_a_commit_lands() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let store = seeded_store();
        let tenant = serve(&store, principal("alice", Role::Developer)).await;
        let admin = serve(&store, principal("system", Role::Admin)).await;
        let http = client();
        send(
            http.put(format!("{tenant}/api/v1/views/ops_status"))
                .json(&definition()),
        )
        .await;
        admin_commit(
            &http,
            &admin,
            "alice",
            "view_ops_status",
            0,
            true,
            json!({"a": {"ok": 1}}),
        )
        .await;

        let started = Instant::now();
        let (status, idle) = send(http.get(format!(
            "{tenant}/api/v1/views/ops_status/changes?after=1&wait=1"
        )))
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            started.elapsed() >= Duration::from_millis(900),
            "wait must hold an empty answer"
        );
        assert_eq!(idle["changes"], json!([]));
        assert_eq!(idle["caught_up"], true);

        let waiter = {
            let http = http.clone();
            let url = format!("{tenant}/api/v1/views/ops_status/changes?after=1&wait=20");
            tokio::spawn(async move {
                let started = Instant::now();
                let body = send(http.get(url)).await;
                (body, started.elapsed())
            })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        admin_commit(
            &http,
            &admin,
            "alice",
            "view_ops_status",
            1,
            false,
            json!({"b": {"ok": 2}}),
        )
        .await;
        let ((status, woke), elapsed) = waiter.await.unwrap();
        assert_eq!(status, StatusCode::OK, "{woke}");
        assert!(
            elapsed < Duration::from_secs(10),
            "commit must release the wait, took {elapsed:?}"
        );
        assert_eq!(
            woke["changes"],
            json!([{"key": "b", "version": 2, "state": {"ok": 2}}])
        );
        assert_eq!(woke["revision"], 2);
    })
    .await
    .expect("long-poll proof exceeded 60s");
}

#[tokio::test]
async fn a_cursor_older_than_a_reset_is_refused_and_delete_resets_rows() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let store = seeded_store();
        let tenant = serve(&store, principal("alice", Role::Developer)).await;
        let admin = serve(&store, principal("system", Role::Admin)).await;
        let http = client();
        send(
            http.put(format!("{tenant}/api/v1/views/ops_status"))
                .json(&definition()),
        )
        .await;
        admin_commit(
            &http,
            &admin,
            "alice",
            "view_ops_status",
            0,
            false,
            json!({"a": {"ok": 1}}),
        )
        .await;
        admin_commit(
            &http,
            &admin,
            "alice",
            "view_ops_status",
            1,
            true,
            json!({"c": {"ok": 9}}),
        )
        .await;

        let (status, reset) =
            send(http.get(format!("{tenant}/api/v1/views/ops_status/changes?after=1"))).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(reset, json!({"error": "view_reset", "generation": 1}));
        let (status, after_reset) =
            send(http.get(format!("{tenant}/api/v1/views/ops_status/changes?after=2"))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(after_reset["changes"], json!([]));

        let (status, deleted) =
            send(http.delete(format!("{tenant}/api/v1/views/ops_status"))).await;
        assert_eq!(status, StatusCode::OK, "{deleted}");
        let (status, gone) = send(http.get(format!("{tenant}/api/v1/views/ops_status"))).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(gone["error"], "view_not_found");
        let (_, index) = send(http.get(format!("{admin}/api/v1/admin/views"))).await;
        assert_eq!(index["views"], json!([]));

        let (_, redefined) = send(
            http.put(format!("{tenant}/api/v1/views/ops_status"))
                .json(&definition()),
        )
        .await;
        assert_eq!(
            redefined["generation"], 2,
            "a redefinition never reuses a generation"
        );
        let (_, rows) = send(http.get(format!("{tenant}/api/v1/views/ops_status/entities"))).await;
        assert_eq!(rows["rows"], json!([]), "delete resets the old rows");
        let (_, index) = send(http.get(format!("{admin}/api/v1/admin/views"))).await;
        assert_eq!(index["views"][0]["generation"], 2);
    })
    .await
    .expect("reset proof exceeded 60s");
}

#[tokio::test]
async fn only_the_admin_principal_acts_for_a_tenant_or_writes_reserved_models() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let store = seeded_store();
        let tenant = serve(&store, principal("alice", Role::Developer)).await;
        let admin = serve(&store, principal("system", Role::Admin)).await;
        let http = client();

        let (status, refused) = send(http.get(format!(
            "{tenant}/api/v1/event-feed?tenant_id=bob&limit=0"
        )))
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(refused["error"], "tenant_forbidden");
        let (status, own) = send(http.get(format!(
            "{tenant}/api/v1/event-feed?tenant_id=alice&limit=0"
        )))
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(own["tenant_id"], "alice");
        let (status, _) = send(http.get(format!(
            "{tenant}/api/v1/read-models/board?tenant_id=bob"
        )))
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let commit = json!({"expected_revision": 0, "checkpoint": own["head"], "ready": true, "changes": {"k": {"v": 1}}});
        let (status, _) = send(
            http.put(format!("{tenant}/api/v1/read-models/board?tenant_id=bob"))
                .json(&commit),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let tenant_admin = serve(&store, principal("alice", Role::Admin)).await;
        let (status, refused) = send(http.get(format!(
            "{tenant_admin}/api/v1/event-feed?tenant_id=bob&limit=0"
        )))
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "a tenant's admin is not the service principal");
        assert_eq!(refused["error"], "tenant_forbidden");

        for reserved in ["view_ops_status", "views_registry", "views_index"] {
            let (status, body) = send(
                http.put(format!("{tenant}/api/v1/read-models/{reserved}"))
                    .json(&commit),
            )
            .await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{reserved}");
            assert_eq!(body, json!({"error": "reserved_read_model"}));
        }

        let revision = admin_commit(&http, &admin, "alice", "view_ops_status", 0, false, json!({"k": {"v": 1}})).await;
        assert_eq!(revision, 1);
        let (status, rows) = send(http.get(format!("{tenant}/api/v1/read-models/view_ops_status"))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(rows["rows"], json!({"k": {"v": 1}}), "the admin commit landed in alice's model");
    })
    .await
    .expect("act-as proof exceeded 60s");
}
