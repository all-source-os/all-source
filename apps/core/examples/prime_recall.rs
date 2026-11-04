//! Prime Recall — agent memory with compressed index and cross-domain retrieval.
//!
//! Demonstrates the full Recall pipeline: domain-tagged knowledge → auto-generated
//! compressed index → context retrieval with cross-domain reasoning.
//!
//! This is the AllSource answer to zer0dex's dual-layer memory system,
//! with temporal reasoning and full event provenance added.
//!
//! Run with:
//!   cargo run --no-default-features --features prime-recall --example prime_recall

use allsource_core::{
    application::services::projection::Projection,
    prime::{
        EntityId, Prime,
        projections::{CrossDomainProjection, DomainIndexProjection},
        recall::{
            IndexConfig, RecallContextQuery, RecallEngine, build_heuristic_index, build_raw_summary,
        },
    },
};
use serde_json::json;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let prime = Prime::open_in_memory().await?;

    // ─── 1. Build multi-domain knowledge ────────────────────────────────
    println!("=== Seeding knowledge across 3 domains ===\n");

    // Revenue domain
    let rev_nodes = seed_domain(
        &prime,
        "revenue",
        &[
            (
                "metric",
                json!({"name": "Q3 Revenue", "value": "$4.2M", "trend": "up 15%"}),
            ),
            (
                "metric",
                json!({"name": "Churn Rate", "value": "8.5%", "segment": "SMB"}),
            ),
            (
                "decision",
                json!({"name": "June Pricing Change", "impact": "enterprise growth"}),
            ),
        ],
    )
    .await?;

    // Engineering domain
    let eng_nodes = seed_domain(
        &prime,
        "engineering",
        &[
            (
                "service",
                json!({"name": "Core API", "throughput": "469K events/sec"}),
            ),
            (
                "feature",
                json!({"name": "Replication", "type": "leader-follower"}),
            ),
            (
                "metric",
                json!({"name": "Build Time", "value": "2m05s", "trend": "improved"}),
            ),
        ],
    )
    .await?;

    // Product domain
    let prod_nodes = seed_domain(
        &prime,
        "product",
        &[
            ("feature", json!({"name": "Dark Mode", "version": "v3.2"})),
            (
                "metric",
                json!({"name": "NPS Score", "value": "58", "trend": "up from 42"}),
            ),
            (
                "insight",
                json!({"name": "User Research", "finding": "70% want better search"}),
            ),
        ],
    )
    .await?;

    // ─── 2. Create cross-domain relationships ───────────────────────────
    println!("Creating cross-domain relationships...\n");

    // Pricing change → Core API (revenue depends on engineering reliability)
    prime
        .add_edge(&rev_nodes[2], &eng_nodes[0], "requires", None)
        .await?;
    // Build time improvement → reduced sales cycle (eng supports revenue)
    prime
        .add_edge(&eng_nodes[2], &rev_nodes[0], "improves", None)
        .await?;
    // NPS improvement → revenue retention
    prime
        .add_edge(&prod_nodes[1], &rev_nodes[0], "drives", None)
        .await?;
    // User research → product search feature → engineering work
    prime
        .add_edge(&prod_nodes[2], &eng_nodes[0], "depends_on", None)
        .await?;

    let stats = prime.stats();
    println!(
        "Knowledge base: {} nodes, {} edges\n",
        stats.total_nodes, stats.total_edges
    );

    // ─── 3. Build Recall engine and generate compressed index ───────────
    println!("=== Generating compressed index ===\n");

    // Create projections and process events through them
    let domain_index = Arc::new(DomainIndexProjection::new());
    let cross_domain = Arc::new(CrossDomainProjection::new());

    // Replay events through domain projections
    let events = prime
        .core()
        .query(allsource_core::embedded::Query::new().event_type_prefix("prime."))
        .await?;

    for event in &events {
        // Convert EventView to Event for projection processing
        let domain_event = allsource_core::domain::entities::Event::reconstruct_from_strings(
            event.id,
            event.event_type.clone(),
            event.entity_id.clone(),
            event.tenant_id.clone(),
            event.payload.clone(),
            event.timestamp,
            event.metadata.clone(),
            1,
        );
        let _ = domain_index.process(&domain_event);
        let _ = cross_domain.process(&domain_event);
    }

    // Build the raw summary from projections
    let summary = build_raw_summary(&domain_index, &cross_domain);
    println!(
        "Domains found: {:?}",
        summary
            .domains
            .iter()
            .map(|d| &d.domain)
            .collect::<Vec<_>>()
    );
    println!("Cross-domain links: {}", summary.cross_domain_links.len());

    // Generate heuristic index (no LLM needed)
    let index = build_heuristic_index(&summary);
    println!(
        "\n--- Compressed Index ({} tokens) ---",
        allsource_core::prime::recall::estimate_tokens(&index)
    );
    println!("{index}");

    // ─── 4. Use RecallEngine for context retrieval ──────────────────────
    println!("=== Context retrieval ===\n");

    let _config = IndexConfig::default();
    let recall = RecallEngine::with_dependencies(
        domain_index,
        cross_domain,
        allsource_core::prime::recall::IndexCompressor::new(None, 100, 300),
    );

    // Query with compressed index included
    let ctx = recall
        .context(RecallContextQuery {
            query: "How does pricing relate to engineering?".to_string(),
            include_index: true,
            max_tokens: Some(500),
            ..RecallContextQuery::default()
        })
        .await?;

    println!("Context retrieved ({} tokens):", ctx.token_count);
    if !ctx.index.is_empty() {
        println!(
            "  Index excerpt: {}...",
            &ctx.index[..ctx.index.len().min(200)]
        );
    }

    // ─── 5. Time-travel queries ─────────────────────────────────────────
    println!("\n=== Time-travel ===\n");

    let t_before_update = chrono::Utc::now();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Update a node
    prime
        .update_node(&eng_nodes[0], json!({"throughput": "500K events/sec"}))
        .await?;

    // Query as of before the update
    if let Some(node_then) = prime.get_node_as_of(&eng_nodes[0], t_before_update).await? {
        println!(
            "Core API throughput BEFORE update: {}",
            node_then.properties["throughput"]
        );
    }
    if let Some(node_now) = prime.get_node(&eng_nodes[0]) {
        println!(
            "Core API throughput AFTER update:  {}",
            node_now.properties["throughput"]
        );
    }

    // ─── 6. Graph diff ──────────────────────────────────────────────────
    println!("\n=== What changed? ===\n");
    let diff = prime.diff(t_before_update, chrono::Utc::now()).await?;
    println!("Nodes updated: {}", diff.nodes_updated.len());

    prime.shutdown().await?;
    println!("\nDone.");
    Ok(())
}

/// Seed nodes for a domain, returning their entity_ids.
async fn seed_domain(
    prime: &Prime,
    domain: &str,
    entries: &[(&str, serde_json::Value)],
) -> anyhow::Result<Vec<String>> {
    let mut entity_ids = Vec::new();
    for (node_type, mut props) in entries.iter().cloned() {
        props["domain"] = json!(domain);
        let id = prime.add_node(node_type, props).await?;
        let entity_id = EntityId::node(node_type, id.as_str()).to_string();
        println!("  [{domain}] {node_type}: {}", entity_ids.len());
        entity_ids.push(entity_id);
    }
    Ok(entity_ids)
}
