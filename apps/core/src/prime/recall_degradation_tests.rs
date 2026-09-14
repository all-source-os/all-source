//! Recall without a query vector.
//!
//! An MCP client whose embedder cannot load still sends `text`. These tests pin
//! that such a call returns a ranked answer and says how it was ranked, rather
//! than returning nothing. None of them touch the embedder — every query
//! supplies `vector: None` directly, so they run identically on a machine with
//! no model on disk.

use serde_json::json;

use crate::prime::{
    Prime,
    types::{RecallQuery, Retrieval},
};

async fn graph() -> Prime {
    let prime = Prime::open_in_memory().await.unwrap();
    prime
        .add_node(
            "function",
            json!({ "name": "call_search", "file": "src/tools.rs", "line": 857 }),
        )
        .await
        .unwrap();
    prime
        .add_node(
            "function",
            json!({ "name": "parse_invoice", "file": "src/billing.rs", "line": 12 }),
        )
        .await
        .unwrap();
    prime
        .add_node("person", json!({ "name": "Alice" }))
        .await
        .unwrap();
    prime
}

fn text_query(text: &str) -> RecallQuery {
    RecallQuery {
        text: Some(text.to_string()),
        vector: None,
        ..RecallQuery::default()
    }
}

#[tokio::test]
async fn text_without_a_vector_returns_ranked_nodes_not_nothing() {
    let prime = graph().await;
    let result = prime.recall(text_query("search")).await.unwrap();

    assert_eq!(result.retrieval, Retrieval::Lexical);
    assert!(!result.nodes.is_empty(), "lexical recall returned nothing");
    assert_eq!(
        result.nodes[0].node.properties["name"],
        json!("call_search"),
        "the best lexical match should rank first"
    );
}

#[tokio::test]
async fn a_lexical_answer_is_labelled_as_degraded() {
    let prime = graph().await;
    let result = prime.recall(text_query("invoice")).await.unwrap();
    assert!(result.retrieval.is_degraded());
}

#[tokio::test]
async fn lexical_hits_seed_graph_expansion() {
    // Pins the ordering: a seeding arm that runs after expansion leaves `depth`
    // dead, and the result looks the same as a graph with no edges.
    let prime = Prime::open_in_memory().await.unwrap();
    let hub = prime
        .add_node("service", json!({ "name": "checkout" }))
        .await
        .unwrap();
    let leaf = prime
        .add_node("database", json!({ "name": "ledger" }))
        .await
        .unwrap();
    prime
        .add_edge(
            &format!("node:service:{}", hub.as_str()),
            &format!("node:database:{}", leaf.as_str()),
            "depends_on",
            None,
        )
        .await
        .unwrap();

    let result = prime
        .recall(RecallQuery {
            text: Some("checkout".to_string()),
            vector: None,
            depth: 1,
            ..RecallQuery::default()
        })
        .await
        .unwrap();

    let reached_leaf = result
        .nodes
        .iter()
        .any(|n| n.node.properties["name"] == json!("ledger") && n.depth == 1);
    assert!(
        reached_leaf,
        "a lexical seed must expand through the graph; got {:?}",
        result
            .nodes
            .iter()
            .map(|n| (&n.node.properties["name"], n.depth))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn unmatched_text_with_a_node_type_falls_back_to_that_type() {
    let prime = graph().await;
    let result = prime
        .recall(RecallQuery {
            text: Some("kubernetes scheduler".to_string()),
            vector: None,
            node_type: Some("person".to_string()),
            ..RecallQuery::default()
        })
        .await
        .unwrap();

    assert_eq!(result.retrieval, Retrieval::TypeScan);
    assert_eq!(result.nodes.len(), 1);
    assert_eq!(result.nodes[0].node.properties["name"], json!("Alice"));
}

#[tokio::test]
async fn unmatched_text_with_no_node_type_returns_nothing_rather_than_the_whole_graph() {
    let prime = graph().await;
    let result = prime
        .recall(text_query("kubernetes scheduler"))
        .await
        .unwrap();

    assert!(
        result.nodes.is_empty(),
        "an unmatched query must not degrade into 'here is the entire graph'"
    );
}

#[tokio::test]
async fn a_node_type_filter_still_applies_to_lexical_hits() {
    let prime = graph().await;
    let result = prime
        .recall(RecallQuery {
            text: Some("search".to_string()),
            vector: None,
            node_type: Some("person".to_string()),
            depth: 0,
            ..RecallQuery::default()
        })
        .await
        .unwrap();

    assert!(
        result.nodes.iter().all(|n| n.node.node_type == "person"),
        "node_type must bound the lexical arm"
    );
}

#[tokio::test]
async fn the_lexical_seed_set_is_bounded_by_top_k() {
    let prime = Prime::open_in_memory().await.unwrap();
    for i in 0..200 {
        prime
            .add_node("function", json!({ "name": format!("handle_event_{i}") }))
            .await
            .unwrap();
    }

    let result = prime
        .recall(RecallQuery {
            text: Some("handle event".to_string()),
            vector: None,
            depth: 0,
            top_k: 5,
            ..RecallQuery::default()
        })
        .await
        .unwrap();

    assert!(
        result.nodes.len() <= 5,
        "top_k must bound the result, got {}",
        result.nodes.len()
    );
}
