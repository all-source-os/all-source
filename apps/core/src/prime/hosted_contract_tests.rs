use super::*;
use crate::prime::{facade::Prime, schema::EnforcementMode};
use std::sync::Mutex;
use wiremock::{
    Mock, MockServer, Request, Respond, ResponseTemplate,
    matchers::{method, path},
};

/// Posted events, returned by the query endpoint the way Core returns them,
/// so a forced re-hydrate sees what was written.
#[derive(Clone, Default)]
struct CoreEvents(Arc<Mutex<Vec<Value>>>);

impl Respond for CoreEvents {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let mut events = self.0.lock().unwrap();
        if req.method.as_str() == "POST" {
            let body: Value = serde_json::from_slice(&req.body).unwrap_or_default();
            let n = events.len();
            events.push(json!({
                "id": format!("00000000-0000-0000-0000-{:012}", n + 1),
                "event_type": body["event_type"],
                "entity_id": body["entity_id"],
                "tenant_id": body["tenant_id"],
                "payload": body.get("payload").cloned().unwrap_or_else(|| json!({})),
                "metadata": body["metadata"],
                "timestamp": format!("2026-06-06T00:{:02}:{:02}Z", n / 60, n % 60),
                "version": 1,
            }));
            return ResponseTemplate::new(200).set_body_json(json!({"ok": true}));
        }
        let filters: Vec<(String, String)> = req
            .url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let matching: Vec<Value> = events
            .iter()
            .filter(|e| {
                filters.iter().all(|(k, v)| match k.as_str() {
                    "entity_id" | "event_type" | "tenant_id" => e[k.as_str()] == *v,
                    "event_type_prefix" => e["event_type"]
                        .as_str()
                        .is_some_and(|t| t.starts_with(v.as_str())),
                    _ => true,
                })
            })
            .cloned()
            .collect();
        let count = matching.len();
        ResponseTemplate::new(200).set_body_json(json!({"events": matching, "count": count}))
    }
}

async fn empty_core() -> MockServer {
    let server = MockServer::start().await;
    let events = CoreEvents::default();
    Mock::given(method("GET"))
        .and(path("/api/v1/events/query"))
        .respond_with(events.clone())
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/events"))
        .respond_with(events)
        .mount(&server)
        .await;
    server
}

async fn posted_events(server: &MockServer) -> Vec<Value> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.method.as_str() == "POST")
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

fn hosted(server: &MockServer) -> HostedPrime {
    HostedPrime::connect(server.uri(), None, 8, Duration::from_mins(1))
}

const PERSON: &str = r#"{"properties": {"age": {"kind": "integer", "min": 0, "max": 150}}}"#;

#[tokio::test]
async fn hosted_and_embedded_refuse_the_same_invalid_write_identically() {
    let server = empty_core().await;
    let h = hosted(&server);
    let schema: Value = serde_json::from_str(PERSON).unwrap();
    h.register_kind("t", SchemaKind::Node, "person", schema.clone())
        .await
        .unwrap();
    h.set_enforcement("t", EnforcementMode::Strict)
        .await
        .unwrap();

    let embedded = Prime::open_in_memory().await.unwrap();
    embedded
        .register_kind(SchemaKind::Node, "person", schema)
        .await
        .unwrap();
    embedded
        .set_enforcement(EnforcementMode::Strict)
        .await
        .unwrap();

    for props in [
        json!({"age": "old"}),
        json!({"age": 151}),
        json!({"age": 1.5}),
    ] {
        let hosted_err = h.add_node("t", "person", props.clone()).await.unwrap_err();
        let embedded_err = embedded.add_node("person", props).await.unwrap_err();
        let AllSourceError::ValidationError(hosted_msg) = hosted_err else {
            panic!("hosted refusal must be a validation error: {hosted_err}");
        };
        assert_eq!(Some(hosted_msg), embedded_err.validation_message());
    }

    let hosted_unknown = h.add_node("t", "gadget", json!({})).await.unwrap_err();
    let embedded_unknown = embedded.add_node("gadget", json!({})).await.unwrap_err();
    assert_eq!(
        hosted_unknown.to_string(),
        format!(
            "Validation error: {}",
            embedded_unknown.validation_message().unwrap()
        )
    );
}

#[tokio::test]
async fn hosted_tenant_less_reads_and_writes_are_refused_before_any_core_call() {
    let server = empty_core().await;
    let h = hosted(&server);
    for tenant in ["", "  "] {
        assert!(h.tenant_graph(tenant).await.is_err());
        assert!(h.get_node(tenant, "node:a:1").await.is_err());
        assert!(h.search(tenant, "a").await.is_err());
        assert!(h.stats(tenant).await.is_err());
        assert!(h.history(tenant, "node:a:1").await.is_err());
        assert!(h.full_graph(tenant, None, None).await.is_err());
        assert!(h.kinds(tenant).await.is_err());
        assert!(h.observations(tenant, "node:a:1").await.is_err());
        assert!(h.projection_defs(tenant).await.is_err());
        assert!(
            h.neighbors(tenant, "node:a:1", None, Direction::Both)
                .await
                .is_err()
        );
        let err = h.add_node(tenant, "a", json!({})).await.unwrap_err();
        assert!(err.to_string().contains("authenticated tenant"), "{err}");
    }
    let requests = server.received_requests().await.unwrap_or_default();
    assert!(requests.is_empty(), "no request may reach Core unscoped");
}

#[tokio::test]
async fn hosted_invalid_confidence_appends_no_event() {
    let server = empty_core().await;
    let h = hosted(&server);
    for bad in [1.5, -0.1, f64::NAN] {
        let err = h
            .add_node_asserted(
                "t",
                "person",
                json!({}),
                Some(Assertion::agent().with_confidence(bad)),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("assertion.confidence"), "{err}");
    }
    assert!(posted_events(&server).await.is_empty());
}

#[tokio::test]
async fn hosted_write_metadata_matches_the_warm_read() {
    let server = empty_core().await;
    let h = hosted(&server);
    h.add_node_asserted(
        "t",
        "person",
        json!({"name": "A"}),
        Some(Assertion::user().with_actor("ann")),
    )
    .await
    .unwrap();
    let posted = posted_events(&server).await;
    assert_eq!(posted[0]["tenant_id"], "t");
    assert_eq!(posted[0]["metadata"]["assertion"]["source"], "user");
    assert_eq!(posted[0]["metadata"]["assertion"]["actor"], "ann");
    assert_eq!(posted[0]["metadata"]["source_priority"], 1000);
}

#[tokio::test]
async fn hosted_kind_registration_and_enforcement_are_tenant_scoped() {
    let server = empty_core().await;
    let h = hosted(&server);
    h.register_kind(
        "tenant-a",
        SchemaKind::Node,
        "person",
        serde_json::from_str(PERSON).unwrap(),
    )
    .await
    .unwrap();
    h.set_enforcement("tenant-a", EnforcementMode::Strict)
        .await
        .unwrap();

    assert_eq!(h.kinds("tenant-a").await.unwrap().len(), 1);
    assert!(h.kinds("tenant-b").await.unwrap().is_empty());
    assert_eq!(
        h.enforcement("tenant-b").await.unwrap(),
        EnforcementMode::Open
    );
    h.add_node("tenant-b", "person", json!({"age": "old"}))
        .await
        .expect("tenant-b is open and has no kinds");
    assert!(
        h.add_node("tenant-a", "person", json!({"age": "old"}))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn hosted_breaking_reregistration_is_refused() {
    let server = empty_core().await;
    let h = hosted(&server);
    h.register_kind(
        "t",
        SchemaKind::Node,
        "person",
        serde_json::from_str(PERSON).unwrap(),
    )
    .await
    .unwrap();
    let err = h
        .register_kind(
            "t",
            SchemaKind::Node,
            "person",
            json!({"properties": {"age": {"kind": "string"}}}),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("kind changed"), "{err}");
    let ok = h
        .register_kind(
            "t",
            SchemaKind::Node,
            "person",
            json!({"properties": {"age": {"kind": "integer", "min": 0, "max": 200}}}),
        )
        .await
        .unwrap();
    assert_eq!(ok.version, 2);
}

#[tokio::test]
async fn a_second_instance_with_a_warm_bundle_sees_nodes_and_kinds_written_by_the_first() {
    let server = empty_core().await;
    let a = hosted(&server);
    let b = hosted(&server);
    b.stats("t").await.unwrap();

    let id = a.add_node("t", "person", json!({"age": 30})).await.unwrap();
    b.update_node(
        "t",
        &node_entity_id("person", id.as_str()),
        json!({"age": 31}),
    )
    .await
    .unwrap();

    a.register_kind(
        "t",
        SchemaKind::Node,
        "person",
        serde_json::from_str(PERSON).unwrap(),
    )
    .await
    .unwrap();
    let err = b
        .register_kind(
            "t",
            SchemaKind::Node,
            "person",
            json!({"properties": {"age": {"kind": "string"}}}),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("kind changed"), "{err}");
}
