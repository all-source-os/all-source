#![allow(deprecated)] // Uses node_entity_id/edge_entity_id — will migrate to EntityId
//! Integration tests for AllSource Prime — exercises the full stack.
//!
//! Each test phase builds on the primitives, combining multiple features
//! to verify they work together. Run with:
//!
//!   cargo test --test prime_integration --features prime
//!   cargo test --test prime_integration --features prime-full  # includes vectors

#![cfg(feature = "prime")]

use allsource_core::prime::{Direction, Prime, event_types, node_entity_id};
use serde_json::json;

async fn test_prime() -> Prime {
    Prime::open_in_memory().await.unwrap()
}

// =============================================================================
// Phase 1: Graph CRUD Lifecycle
// =============================================================================

#[tokio::test]
async fn test_graph_crud_lifecycle() {
    let prime = test_prime().await;

    // Create 3 node types
    let alice = prime
        .add_node("person", json!({"name": "Alice", "role": "engineer"}))
        .await
        .unwrap();
    let bob = prime
        .add_node("person", json!({"name": "Bob"}))
        .await
        .unwrap();
    let project = prime
        .add_node("project", json!({"name": "Prime", "status": "active"}))
        .await
        .unwrap();

    let alice_e = node_entity_id("person", alice.as_str());
    let bob_e = node_entity_id("person", bob.as_str());
    let proj_e = node_entity_id("project", project.as_str());

    // Create edges
    let _edge1 = prime
        .add_edge(&alice_e, &proj_e, "works_on", None)
        .await
        .unwrap();
    let _edge2 = prime
        .add_edge(&bob_e, &proj_e, "works_on", None)
        .await
        .unwrap();
    let _edge3 = prime
        .add_edge(&alice_e, &bob_e, "mentors", None)
        .await
        .unwrap();

    // Verify stats
    let stats = prime.stats();
    assert_eq!(stats.total_nodes, 3);
    assert_eq!(stats.total_edges, 3);
    assert_eq!(stats.nodes_by_type.get("person"), Some(&2));
    assert_eq!(stats.nodes_by_type.get("project"), Some(&1));

    // Update a node
    prime
        .update_node(&alice_e, json!({"level": "senior"}))
        .await
        .unwrap();
    let alice_node = prime.get_node(&alice_e).unwrap();
    assert_eq!(alice_node.properties["name"], "Alice"); // preserved
    assert_eq!(alice_node.properties["level"], "senior"); // added

    // Delete bob — should cascade edges
    prime.delete_node(&bob_e).await.unwrap();
    assert!(prime.get_node(&bob_e).is_none());

    // Bob's edges should be gone
    let alice_neighbors = prime.neighbors(&alice_e, None, Direction::Outgoing);
    // Only project remains (mentors edge to bob was deleted)
    assert_eq!(alice_neighbors.len(), 1);
    assert_eq!(alice_neighbors[0].node_type, "project");

    // Verify stats updated
    let stats = prime.stats();
    assert_eq!(stats.deleted_nodes, 1);

    prime.shutdown().await.unwrap();
}

// =============================================================================
// Phase 2: Traversal
// =============================================================================

#[tokio::test]
async fn test_traversal_full_stack() {
    let prime = test_prime().await;

    // Build graph: A -> B -> D, A -> C -> D, C -> E
    let node_a = prime.add_node("n", json!({"name": "A"})).await.unwrap();
    let node_b = prime.add_node("n", json!({"name": "B"})).await.unwrap();
    let node_c = prime.add_node("n", json!({"name": "C"})).await.unwrap();
    let node_d = prime.add_node("n", json!({"name": "D"})).await.unwrap();
    let node_e = prime.add_node("n", json!({"name": "E"})).await.unwrap();

    let ae = node_entity_id("n", node_a.as_str());
    let be = node_entity_id("n", node_b.as_str());
    let ce = node_entity_id("n", node_c.as_str());
    let de = node_entity_id("n", node_d.as_str());
    let ee = node_entity_id("n", node_e.as_str());

    prime
        .add_edge_weighted(&ae, &be, "link", 1.0, None)
        .await
        .unwrap();
    prime
        .add_edge_weighted(&ae, &ce, "link", 5.0, None)
        .await
        .unwrap();
    prime
        .add_edge_weighted(&be, &de, "link", 1.0, None)
        .await
        .unwrap();
    prime
        .add_edge_weighted(&ce, &de, "link", 1.0, None)
        .await
        .unwrap();
    prime.add_edge(&ce, &ee, "link", None).await.unwrap();

    // Neighbors
    let a_out = prime.neighbors(&ae, None, Direction::Outgoing);
    assert_eq!(a_out.len(), 2); // B, C

    // BFS depth 2: A(0), B(1), C(1), D(2), E(2 via C->E)
    let bfs = prime.neighbors_within(&ae, 2, None, Direction::Outgoing);
    assert_eq!(bfs.len(), 5); // A, B, C, D, E all within 2 hops

    // BFS depth 1: just A and immediate neighbors
    let bfs1 = prime.neighbors_within(&ae, 1, None, Direction::Outgoing);
    assert_eq!(bfs1.len(), 3); // A(0), B(1), C(1)

    // Shortest path (BFS unweighted)
    let path = prime.shortest_path(&ae, &de, None).unwrap();
    assert_eq!(path.len(), 3); // A -> B -> D (or A -> C -> D)

    // Shortest path (Dijkstra weighted) — should prefer A->B->D (cost 2) over A->C->D (cost 6)
    let (path_w, cost) = prime.shortest_path_weighted(&ae, &de, None).unwrap();
    assert_eq!(path_w.len(), 3);
    assert!((cost - 2.0).abs() < f64::EPSILON);

    // Subgraph
    let sg = prime.subgraph(&ae, 1);
    assert_eq!(sg.nodes.len(), 3); // A, B, C
    assert_eq!(sg.edges.len(), 2); // A->B, A->C

    prime.shutdown().await.unwrap();
}

// =============================================================================
// Phase 3: Temporal Queries
// =============================================================================

#[tokio::test]
async fn test_temporal_queries() {
    let prime = test_prime().await;

    let t_start = chrono::Utc::now();

    let id = prime
        .add_node("person", json!({"name": "Alice"}))
        .await
        .unwrap();
    let entity_id = node_entity_id("person", id.as_str());

    prime
        .update_node(&entity_id, json!({"role": "engineer"}))
        .await
        .unwrap();
    prime
        .update_node(&entity_id, json!({"level": "senior"}))
        .await
        .unwrap();

    let t_mid = chrono::Utc::now();

    prime.delete_node(&entity_id).await.unwrap();

    let t_end = chrono::Utc::now();

    // History: 4 events (created, updated x2, deleted)
    let history = prime.history(&entity_id).await.unwrap();
    assert_eq!(history.len(), 4);
    assert_eq!(history[0].event_type, event_types::NODE_CREATED);
    assert_eq!(history[1].event_type, event_types::NODE_UPDATED);
    assert_eq!(history[2].event_type, event_types::NODE_UPDATED);
    assert_eq!(history[3].event_type, event_types::NODE_DELETED);

    // Diff: between start and mid should show 1 node added, 2 updates
    let diff = prime.diff(t_start, t_mid).await.unwrap();
    assert_eq!(diff.nodes_added.len(), 1);
    assert_eq!(diff.nodes_updated.len(), 2);
    assert!(diff.nodes_deleted.is_empty());

    // Timeline: only events after t_mid should include the delete
    let timeline = prime
        .timeline(&entity_id, Some(t_mid), Some(t_end))
        .await
        .unwrap();
    assert_eq!(timeline.len(), 1);
    assert_eq!(timeline[0].event_type, event_types::NODE_DELETED);

    prime.shutdown().await.unwrap();
}

// =============================================================================
// Phase 4: Schema Enforcement
// =============================================================================

#[tokio::test]
async fn test_schema_enforcement() {
    let prime = test_prime().await;

    // Register a schema requiring "name" for person nodes
    prime
        .register_schema(
            "person",
            allsource_core::prime::schema::SchemaKind::Node,
            json!({"required": ["name"]}),
        )
        .await
        .unwrap();

    // Should fail: missing "name"
    let result = prime.add_node("person", json!({"role": "engineer"})).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("name"));

    // Should succeed: has "name"
    let id = prime
        .add_node("person", json!({"name": "Alice"}))
        .await
        .unwrap();
    let entity_id = node_entity_id("person", id.as_str());

    // Update should pass (merged result still has "name")
    prime
        .update_node(&entity_id, json!({"role": "engineer"}))
        .await
        .unwrap();

    // No schema for "project" — anything goes
    prime.add_node("project", json!({})).await.unwrap();

    // Edge schema
    prime
        .register_schema(
            "works_on",
            allsource_core::prime::schema::SchemaKind::Edge,
            json!({"required": ["since"]}),
        )
        .await
        .unwrap();

    let proj = prime
        .add_node("project", json!({"name": "P"}))
        .await
        .unwrap();
    let proj_e = node_entity_id("project", proj.as_str());

    // Should fail: missing "since"
    let result = prime.add_edge(&entity_id, &proj_e, "works_on", None).await;
    assert!(result.is_err());

    // Should succeed with "since"
    let result = prime
        .add_edge(
            &entity_id,
            &proj_e,
            "works_on",
            Some(json!({"since": "2026-01"})),
        )
        .await;
    assert!(result.is_ok());

    prime.shutdown().await.unwrap();
}

// =============================================================================
// Phase 5: Contradiction Detection with Backfill
// =============================================================================

#[tokio::test]
async fn test_contradiction_backfill() {
    let prime = test_prime().await;

    // Create entities
    let alice = prime
        .add_node("person", json!({"name": "Alice"}))
        .await
        .unwrap();
    let co_a = prime
        .add_node("company", json!({"name": "CompanyA"}))
        .await
        .unwrap();
    let co_b = prime
        .add_node("company", json!({"name": "CompanyB"}))
        .await
        .unwrap();

    let alice_e = node_entity_id("person", alice.as_str());
    let co_a_e = node_entity_id("company", co_a.as_str());
    let co_b_e = node_entity_id("company", co_b.as_str());

    // Add contradictory edges BEFORE configuring exclusive
    prime
        .add_edge(&alice_e, &co_a_e, "is_ceo_of", None)
        .await
        .unwrap();
    prime
        .add_edge(&alice_e, &co_b_e, "is_ceo_of", None)
        .await
        .unwrap();

    // No contradictions yet (is_ceo_of not marked exclusive)
    assert!(prime.contradictions().is_empty());

    // Configure exclusive WITH backfill — should find the pre-existing contradiction
    prime.configure_exclusive("is_ceo_of");

    let contradictions = prime.contradictions();
    assert_eq!(contradictions.len(), 1);
    assert_eq!(contradictions[0].entity_id, alice_e);
    assert_eq!(contradictions[0].relation, "is_ceo_of");

    prime.shutdown().await.unwrap();
}

// =============================================================================
// Phase 6: Memory Compaction
// =============================================================================

#[tokio::test]
async fn test_compaction_full() {
    let prime = test_prime().await;

    // Create nodes A, B, C with edges
    let a = prime
        .add_node("person", json!({"name": "Alice", "age": 30}))
        .await
        .unwrap();
    let b = prime
        .add_node(
            "person",
            json!({"name": "Alice Smith", "email": "alice@example.com"}),
        )
        .await
        .unwrap();
    let c = prime
        .add_node("project", json!({"name": "Prime"}))
        .await
        .unwrap();

    let a_e = node_entity_id("person", a.as_str());
    let b_e = node_entity_id("person", b.as_str());
    let c_e = node_entity_id("project", c.as_str());

    // B -> C (works_on)
    prime.add_edge(&b_e, &c_e, "works_on", None).await.unwrap();

    // Compact B into A
    prime.compact(&a_e, &[&b_e]).await.unwrap();

    // A should have merged properties
    let node = prime.get_node(&a_e).unwrap();
    assert_eq!(node.properties["name"], "Alice"); // target wins
    assert_eq!(node.properties["age"], 30);
    assert_eq!(node.properties["email"], "alice@example.com"); // from source

    // B should be deleted
    assert!(prime.get_node(&b_e).is_none());

    // A should have B's edge to C
    let neighbors = prime.neighbors(&a_e, Some("works_on"), Direction::Outgoing);
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].properties["name"], "Prime");

    // Compaction event in history
    let history = prime.history(&a_e).await.unwrap();
    let compacted = history
        .iter()
        .any(|h| h.event_type == "prime.memory.compacted");
    assert!(compacted);

    prime.shutdown().await.unwrap();
}

// =============================================================================
// Phase 7: Persistence Roundtrip
// =============================================================================

#[tokio::test]
async fn test_persistence_roundtrip() {
    let dir = tempfile::tempdir().unwrap();

    // Phase 1: create data and shutdown
    {
        let prime = Prime::open(dir.path()).await.unwrap();
        prime
            .add_node("person", json!({"name": "Alice"}))
            .await
            .unwrap();
        prime
            .add_node("person", json!({"name": "Bob"}))
            .await
            .unwrap();
        prime
            .add_node("project", json!({"name": "Prime"}))
            .await
            .unwrap();
        assert_eq!(prime.stats().total_nodes, 3);
        prime.shutdown().await.unwrap();
    }

    // Phase 2: reopen and verify
    {
        let prime = Prime::open(dir.path()).await.unwrap();

        // Projections should rebuild from WAL
        let stats = prime.stats();
        assert_eq!(stats.total_nodes, 3);
        assert_eq!(stats.nodes_by_type.get("person"), Some(&2));
        assert_eq!(stats.nodes_by_type.get("project"), Some(&1));

        // nodes_by_type should return actual nodes
        let persons = prime.nodes_by_type("person");
        assert_eq!(persons.len(), 2);

        prime.shutdown().await.unwrap();
    }
}

// Reproduces issue #201: a second Prime instance opened on a data-dir that a
// still-running first instance owns must replay the WAL and serve the same data
// (as a read-only replica) instead of returning an empty graph — and it must
// NOT truncate the owner's WAL out from under it.
#[tokio::test]
async fn test_concurrent_instances_share_data_dir() {
    let dir = tempfile::tempdir().unwrap();

    // Instance A: the writer. Add data and KEEP IT OPEN (no shutdown), so it
    // still holds the exclusive data-dir lock when B opens.
    let prime_a = Prime::open(dir.path()).await.unwrap();
    prime_a
        .add_node("person", json!({"name": "Alice"}))
        .await
        .unwrap();
    prime_a
        .add_node("person", json!({"name": "Bob"}))
        .await
        .unwrap();
    prime_a
        .add_node("project", json!({"name": "Prime"}))
        .await
        .unwrap();
    assert_eq!(
        prime_a.stats().total_nodes,
        3,
        "writer should see its own writes"
    );

    // Instance B: opens the SAME data-dir while A is still alive → read-only
    // replica. It must replay A's WAL and see all 3 nodes (issue #201).
    let prime_b = Prime::open(dir.path()).await.unwrap();
    assert_eq!(
        prime_b.stats().total_nodes,
        3,
        "replica must replay the owner's WAL and see 3 nodes (issue #201)"
    );

    // The replica must reject writes rather than corrupt the owner's WAL.
    assert!(
        prime_b
            .add_node("person", json!({"name": "Carol"}))
            .await
            .is_err(),
        "replica must reject writes"
    );

    // The owner keeps working and its WAL was never truncated by B: a fresh
    // reader opened after A shuts down still sees all 3 nodes.
    prime_a.shutdown().await.unwrap();
    drop(prime_b);
    let prime_c = Prime::open(dir.path()).await.unwrap();
    assert_eq!(
        prime_c.stats().total_nodes,
        3,
        "data must survive: no boot orphaned the WAL inode (issue #201)"
    );
    prime_c.shutdown().await.unwrap();
}

// =============================================================================
// Phase 8: Sync Convergence
// =============================================================================

#[tokio::test]
async fn test_sync_convergence() {
    use allsource_core::embedded::{Config, EmbeddedCore, IngestEvent};

    let config_a = Config::builder().node_id(100).build().unwrap();
    let config_b = Config::builder().node_id(200).build().unwrap();

    let core_a = EmbeddedCore::open(config_a).await.unwrap();
    let core_b = EmbeddedCore::open(config_b).await.unwrap();

    // Add different data to each
    core_a
        .ingest(IngestEvent {
            entity_id: "node:person:alice",
            event_type: event_types::NODE_CREATED,
            payload: json!({"node_type": "person", "properties": {"name": "Alice"}}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

    core_b
        .ingest(IngestEvent {
            entity_id: "node:person:bob",
            event_type: event_types::NODE_CREATED,
            payload: json!({"node_type": "person", "properties": {"name": "Bob"}}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

    // Sync
    let report = allsource_core::prime::sync::sync(&core_a, &core_b)
        .await
        .unwrap();

    assert!(report.pushed > 0);
    assert!(report.pulled > 0);

    // Both should have both nodes
    use allsource_core::embedded::Query;
    let a_events = core_a
        .query(Query::new().event_type(event_types::NODE_CREATED))
        .await
        .unwrap();
    assert_eq!(a_events.len(), 2);

    let b_events = core_b
        .query(Query::new().event_type(event_types::NODE_CREATED))
        .await
        .unwrap();
    assert_eq!(b_events.len(), 2);

    core_a.shutdown().await.unwrap();
    core_b.shutdown().await.unwrap();
}

// =============================================================================
// Phase 9: Import/Export Roundtrip
// =============================================================================

#[tokio::test]
async fn test_import_export_roundtrip() {
    let prime = test_prime().await;

    // Create data
    let a = prime
        .add_node("person", json!({"name": "Alice"}))
        .await
        .unwrap();
    let b = prime
        .add_node("project", json!({"name": "Prime"}))
        .await
        .unwrap();
    let a_e = node_entity_id("person", a.as_str());
    let b_e = node_entity_id("project", b.as_str());
    prime.add_edge(&a_e, &b_e, "works_on", None).await.unwrap();

    // Export
    let mut buf = Vec::new();
    let export_stats = allsource_core::prime::import_export::export_json(prime.core(), &mut buf)
        .await
        .unwrap();
    assert_eq!(export_stats.nodes, 2);
    assert_eq!(export_stats.edges, 1);

    prime.shutdown().await.unwrap();

    // Import into fresh Prime
    let prime2 = test_prime().await;
    let reader = std::io::BufReader::new(std::io::Cursor::new(&buf));
    let import_stats = allsource_core::prime::import_export::import_json(prime2.core(), reader)
        .await
        .unwrap();
    assert_eq!(import_stats.nodes, 2);
    assert_eq!(import_stats.edges, 1);

    // Verify data
    assert_eq!(prime2.stats().total_nodes, 2);
    assert_eq!(prime2.stats().total_edges, 1);

    prime2.shutdown().await.unwrap();
}

// =============================================================================
// Vector Tests (feature-gated)
// =============================================================================

#[cfg(feature = "prime-vectors")]
mod vector_tests {
    use super::*;

    #[tokio::test]
    async fn test_embed_search_delete() {
        let prime = test_prime().await;

        // Embed 3 vectors (384-dim for testing)
        let v1: Vec<f32> = (0..384).map(|i| (i as f32) / 384.0).collect();
        let v2: Vec<f32> = (0..384).map(|i| (i as f32 + 0.1) / 384.0).collect();
        let v3: Vec<f32> = (0..384).map(|i| ((384 - i) as f32) / 384.0).collect(); // opposite direction

        prime
            .embed("doc-1", Some("Event sourcing patterns"), v1.clone())
            .await
            .unwrap();
        prime
            .embed("doc-2", Some("CQRS and event stores"), v2.clone())
            .await
            .unwrap();
        prime
            .embed("doc-3", Some("Machine learning basics"), v3.clone())
            .await
            .unwrap();

        // Search similar to doc-1 — doc-2 should rank higher than doc-3
        let results = prime.vector_search(&v1, 3);
        assert!(!results.is_empty());
        // doc-1 itself should be most similar (or doc-2 which is very close)

        // Delete doc-2
        prime.delete_vector("doc-2").await.unwrap();

        // Search again — doc-2 should not appear
        let results = prime.vector_search(&v1, 3);
        assert!(results.iter().all(|r| r.id != "doc-2"));

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_embed_dimension_mismatch_rejected() {
        let prime = test_prime().await;

        // Establish the data-dir at 384 dims.
        let v384: Vec<f32> = (0..384).map(|i| i as f32 / 384.0).collect();
        prime
            .embed("doc-1", Some("first"), v384.clone())
            .await
            .unwrap();

        // A mismatched dimension (e.g. switching to a 1536-dim model) must be
        // rejected, not silently stored — mixing dims corrupts HNSW search.
        let v1536: Vec<f32> = (0..1536).map(|i| i as f32 / 1536.0).collect();
        let err = prime
            .embed("doc-2", Some("wrong dims"), v1536)
            .await
            .expect_err("mismatched dimension must be rejected");
        let msg = err.to_string();
        assert!(msg.contains("dimension mismatch"), "unhelpful error: {msg}");
        assert!(
            msg.contains("384") && msg.contains("1536"),
            "error omits dims: {msg}"
        );

        // The matching dimension still works.
        let v384b: Vec<f32> = (0..384).map(|i| (i as f32 + 1.0) / 384.0).collect();
        prime.embed("doc-3", Some("ok"), v384b).await.unwrap();

        prime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_remember_forget() {
        let prime = test_prime().await;

        let concept = prime
            .add_node("concept", json!({"name": "CRDT"}))
            .await
            .unwrap();
        let concept_e = node_entity_id("concept", concept.as_str());

        // Remember: creates node + embedding + edges
        let vec: Vec<f32> = (0..384).map(|i| (i as f32) / 384.0).collect();
        let node_id = prime
            .remember(
                "CRDTs enable conflict-free replication",
                vec.clone(),
                "paper",
                json!({"title": "CRDT Survey", "year": 2011}),
                &[(concept_e.as_str(), "discusses")],
            )
            .await
            .unwrap();

        // `remember` already returns the full wire entity_id (node:<type>:<uuid>).
        // Re-wrapping it with node_entity_id would double-prefix it (node:paper:node:paper:…)
        // and every lookup below would miss.
        let entity_id = node_id.clone();

        // Verify node exists
        let node = prime.get_node(&entity_id).unwrap();
        assert_eq!(node.properties["title"], "CRDT Survey");

        // Verify edge exists
        let neighbors = prime.neighbors(&entity_id, Some("discusses"), Direction::Outgoing);
        assert_eq!(neighbors.len(), 1);

        // Verify vector exists
        let vec_entry = prime.get_vector(&node_id);
        assert!(vec_entry.is_some());

        // Forget: removes node + edges + vector
        prime.forget(&entity_id).await.unwrap();

        // Everything should be gone
        assert!(prime.get_node(&entity_id).is_none());
        let neighbors = prime.neighbors(&entity_id, None, Direction::Outgoing);
        assert!(neighbors.is_empty());

        prime.shutdown().await.unwrap();
    }
}
