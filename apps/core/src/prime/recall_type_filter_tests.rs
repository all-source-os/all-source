//! Semantic recall honours `node_type` and `tenant` in both `nodes` and
//! `vectors`. Vectors are supplied directly, so no embedder is needed.

use serde_json::json;

use crate::prime::{
    Prime,
    types::{RecallQuery, Retrieval, node_entity_id},
};

const QUERY: [f32; 4] = [1.0, 0.0, 0.0, 0.0];
const NEAR: [f32; 4] = [1.0, 0.01, 0.0, 0.0];
const FAR: [f32; 4] = [0.6, 0.8, 0.0, 0.0];

async fn embedded_node(prime: &Prime, node_type: &str, props: serde_json::Value, v: [f32; 4]) {
    let id = prime.add_node(node_type, props).await.unwrap();
    prime
        .embed(&node_entity_id(node_type, id.as_str()), None, v.to_vec())
        .await
        .unwrap();
}

/// Three `skill` nodes, and five `tool` nodes nearer to the query than any skill.
async fn graph() -> Prime {
    let prime = Prime::open_in_memory().await.unwrap();
    for i in 0..3 {
        embedded_node(
            &prime,
            "skill",
            json!({ "name": format!("skill-{i}") }),
            FAR,
        )
        .await;
    }
    for i in 0..5 {
        embedded_node(&prime, "tool", json!({ "name": format!("tool-{i}") }), NEAR).await;
    }
    prime
}

fn query(node_type: Option<&str>, tenant: Option<&str>) -> RecallQuery {
    RecallQuery {
        vector: Some(QUERY.to_vec()),
        node_type: node_type.map(str::to_string),
        tenant: tenant.map(str::to_string),
        depth: 0,
        top_k: 3,
        ..RecallQuery::default()
    }
}

#[tokio::test]
async fn a_typed_semantic_recall_returns_only_that_type() {
    let prime = graph().await;

    let result = prime.recall(query(Some("skill"), None)).await.unwrap();

    assert_eq!(result.retrieval, Retrieval::Semantic);
    let types: Vec<&str> = result
        .nodes
        .iter()
        .map(|n| n.node.node_type.as_str())
        .collect();
    assert_eq!(
        types, ["skill"; 3],
        "nearer tool vectors leaked into a skill recall"
    );
    assert!(!result.vectors.is_empty());
    assert!(
        result
            .vectors
            .iter()
            .all(|v| v.id.starts_with("vec:node:skill:")),
        "vectors: {:?}",
        result.vectors.iter().map(|v| &v.id).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn an_untyped_semantic_recall_still_ranks_every_type() {
    let prime = graph().await;

    let result = prime.recall(query(None, None)).await.unwrap();

    assert_eq!(result.nodes[0].node.node_type, "tool");
    assert!(
        result
            .vectors
            .iter()
            .any(|v| v.id.starts_with("vec:node:tool:"))
    );
}

#[tokio::test]
async fn a_tenant_scoped_recall_lists_only_that_tenants_vectors() {
    let prime = Prime::open_in_memory().await.unwrap();
    embedded_node(
        &prime,
        "skill",
        json!({ "name": "mine", "tenant_id": "a" }),
        FAR,
    )
    .await;
    embedded_node(
        &prime,
        "skill",
        json!({ "name": "theirs", "tenant_id": "b" }),
        NEAR,
    )
    .await;
    prime
        .embed("loose-note", Some("no node behind it"), NEAR.to_vec())
        .await
        .unwrap();

    let scoped = prime.recall(query(None, Some("a"))).await.unwrap();
    let unscoped = prime.recall(query(None, None)).await.unwrap();

    assert_eq!(scoped.nodes.len(), 1);
    assert_eq!(scoped.nodes[0].node.properties["name"], json!("mine"));
    assert_eq!(scoped.vectors.len(), 1, "vectors: {:?}", scoped.vectors);
    assert!(unscoped.vectors.iter().any(|v| v.id == "vec:loose-note"));
}
