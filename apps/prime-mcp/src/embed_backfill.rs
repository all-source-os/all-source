//! Converges the store on "every node has a vector".
//!
//! `add_node` deliberately does not embed: embedding needs a model, and a
//! memory engine that refuses writes when the model is missing is worse than
//! one that stores them unembedded. The invariant is held here instead — a
//! node without a vector is invisible to `prime_recall`, so this sweeps the
//! deficit on boot and on an interval rather than at the write.
//!
//! Failing to embed is therefore never fatal. The loop logs, leaves the node
//! unembedded, and retries on the next pass; `prime_stats` reports the
//! outstanding count so the gap is visible while it lasts.

use std::{sync::Arc, time::Duration};

use allsource_core::prime::{
    Prime,
    vectors::{NODE_TEXT_VERSION, NODE_TEXT_VERSION_KEY, node_text},
};
use serde_json::json;

/// Nodes embedded per pass. Bounded so a large deficit cannot monopolise the
/// runtime on a machine that is also serving tool calls.
const BATCH: usize = 128;

pub async fn run_embed_backfill_loop(prime: Arc<Prime>, interval: Duration) {
    tracing::info!(
        interval_ms = interval.as_millis() as u64,
        batch = BATCH,
        "Prime embed back-fill started"
    );

    loop {
        let embedded = run_once(&prime).await;
        if embedded == 0 {
            tokio::time::sleep(interval).await;
        }
    }
}

/// Embed up to [`BATCH`] unembedded nodes. Returns how many were stored.
///
/// Returning the count lets the caller drain a large backlog without waiting
/// a full interval between batches, and park once the store is converged.
pub async fn run_once(prime: &Prime) -> usize {
    let pending = prime.nodes_missing_vectors();
    if pending.is_empty() {
        return 0;
    }

    let outstanding = pending.len();
    let mut embedded = 0usize;

    for node in pending.into_iter().take(BATCH) {
        let wire =
            allsource_core::prime::EntityId::node(&node.node_type, node.id.as_str()).to_wire();
        let text = node_text(&node.node_type, &node.properties);
        if text.trim().is_empty() {
            continue;
        }

        let vector = match prime.embed_text(&text) {
            Ok(v) => v,
            Err(e) => {
                // The model is unavailable, so every node in this pass fails
                // the same way. Stop rather than log once per node.
                tracing::warn!(
                    error = %e,
                    outstanding,
                    "embed back-fill paused — embedding model unavailable"
                );
                return embedded;
            }
        };

        let metadata = json!({ NODE_TEXT_VERSION_KEY: NODE_TEXT_VERSION });
        match prime
            .embed_with_metadata(&wire, Some(&text), vector, Some(metadata))
            .await
        {
            Ok(()) => embedded += 1,
            Err(e) => tracing::warn!(node = %wire, error = %e, "embed back-fill: store failed"),
        }
    }

    if embedded > 0 {
        tracing::info!(
            embedded,
            outstanding = outstanding.saturating_sub(embedded),
            "embed back-fill progress"
        );
    }
    embedded
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The gate that was missing: a node written through the normal path must
    /// end up findable by its own name. Without the back-fill this fails —
    /// `add_node` stores no vector and recall answers empty, which is
    /// indistinguishable from having no such node.
    #[tokio::test]
    async fn a_written_node_becomes_recallable() {
        let prime = Prime::open_in_memory().await.unwrap();
        prime
            .add_node(
                "person",
                json!({"name": "Grace Hopper", "note": "invented the compiler"}),
            )
            .await
            .unwrap();

        assert_eq!(
            prime.count_nodes_missing_vectors(),
            1,
            "add_node is expected NOT to embed; if this fails the write path changed"
        );

        assert_eq!(run_once(&prime).await, 1);
        assert_eq!(prime.count_nodes_missing_vectors(), 0);
    }

    #[tokio::test]
    async fn a_converged_store_does_no_work() {
        let prime = Prime::open_in_memory().await.unwrap();
        prime
            .add_node("tool", json!({"name": "ripgrep"}))
            .await
            .unwrap();

        assert_eq!(run_once(&prime).await, 1);
        assert_eq!(run_once(&prime).await, 0);
    }
}
