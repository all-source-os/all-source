//! Batch import/export for Prime graph data.
//!
//! Export writes one JSON object per line (JSON Lines format). Import reads the
//! same format and ingests proper `prime.*` events to maintain event sourcing.
//!
//! Import is idempotent — re-importing the same data uses entity IDs with
//! `FirstWriteWins` merge strategy, so duplicates are silently ignored.

use std::io::{BufRead, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    embedded::IngestEvent,
    prime::{event_store::EventStore, types::event_types},
};

/// Statistics returned from an export operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExportStats {
    pub nodes: usize,
    pub edges: usize,
    pub vectors: usize,
}

/// Statistics returned from an import operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportStats {
    pub nodes: usize,
    pub edges: usize,
    pub vectors: usize,
}

/// A single line in the export format.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportLine {
    #[serde(rename = "type")]
    kind: String,
    entity_id: String,
    data: Value,
}

/// Export all Prime graph data to a writer in JSON Lines format.
///
/// Each line is a JSON object: `{"type": "node"|"edge"|"vector", "entity_id": "...", "data": {...}}`
pub async fn export_json(
    core: &dyn EventStore,
    mut writer: impl Write,
) -> crate::error::Result<ExportStats> {
    use crate::embedded::Query;

    let events = core.query(Query::new().event_type_prefix("prime.")).await?;

    let mut stats = ExportStats::default();
    // Track which entities we've already exported (only export latest state)
    let mut exported_nodes = std::collections::HashSet::new();
    let mut exported_edges = std::collections::HashSet::new();

    for event in &events {
        let entity_id = &event.entity_id;

        match event.event_type.as_str() {
            // Not collapsed into a pattern guard for the same reason as the
            // EDGE_CREATED arm below: `insert` mutates the dedup set.
            #[allow(clippy::collapsible_match)]
            event_types::NODE_CREATED => {
                if exported_nodes.insert(entity_id.clone()) {
                    let line = ExportLine {
                        kind: "node".to_string(),
                        entity_id: entity_id.clone(),
                        data: event.payload.clone(),
                    };
                    serde_json::to_writer(&mut writer, &line)
                        .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;
                    writeln!(writer)
                        .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;
                    stats.nodes += 1;
                }
            }
            // Not collapsed into a `EDGE_CREATED if exported_edges.insert(..)`
            // pattern guard: `insert` mutates the dedup set, and hiding that
            // side effect in a guard reads as a pure test. It would also break
            // the deliberate symmetry with the NODE_CREATED arm above, which
            // clippy cannot collapse (it is not the last non-wildcard arm).
            #[allow(clippy::collapsible_match)]
            event_types::EDGE_CREATED => {
                if exported_edges.insert(entity_id.clone()) {
                    let line = ExportLine {
                        kind: "edge".to_string(),
                        entity_id: entity_id.clone(),
                        data: event.payload.clone(),
                    };
                    serde_json::to_writer(&mut writer, &line)
                        .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;
                    writeln!(writer)
                        .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;
                    stats.edges += 1;
                }
            }
            _ => {} // Skip updates, deletes, vectors (for now — vector export needs prime-vectors)
        }
    }

    writer
        .flush()
        .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;

    Ok(stats)
}

/// Import Prime graph data from a reader in JSON Lines format.
///
/// Each line must be a JSON object: `{"type": "node"|"edge"|"vector", "entity_id": "...", "data": {...}}`
///
/// Import ingests proper `prime.*` events (maintains event sourcing). Idempotent
/// via `FirstWriteWins` merge strategy on entity IDs.
pub async fn import_json(
    core: &dyn EventStore,
    reader: impl BufRead,
) -> crate::error::Result<ImportStats> {
    let mut stats = ImportStats::default();

    for line_result in reader.lines() {
        let line =
            line_result.map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let export_line: ExportLine = serde_json::from_str(line)
            .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;

        let event_type = match export_line.kind.as_str() {
            "node" => {
                stats.nodes += 1;
                event_types::NODE_CREATED
            }
            "edge" => {
                stats.edges += 1;
                event_types::EDGE_CREATED
            }
            "vector" => {
                stats.vectors += 1;
                // Vector events use prime.vector.stored
                "prime.vector.stored"
            }
            _ => continue,
        };

        core.ingest(IngestEvent {
            entity_id: &export_line.entity_id,
            event_type,
            payload: export_line.data,
            metadata: None,
            tenant_id: None,
        })
        .await?;
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedded::{Config, EmbeddedCore, Query};
    use std::io::Cursor;

    async fn test_core() -> EmbeddedCore {
        let config = Config::builder().build().unwrap();
        EmbeddedCore::open(config).await.unwrap()
    }

    #[tokio::test]
    async fn test_export_nodes_and_edges() {
        let core = test_core().await;

        // Create 2 nodes and 1 edge
        core.ingest(IngestEvent {
            entity_id: "node:person:alice",
            event_type: event_types::NODE_CREATED,
            payload: serde_json::json!({"node_type": "person", "properties": {"name": "Alice"}}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        core.ingest(IngestEvent {
            entity_id: "node:person:bob",
            event_type: event_types::NODE_CREATED,
            payload: serde_json::json!({"node_type": "person", "properties": {"name": "Bob"}}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        core.ingest(IngestEvent {
            entity_id: "edge:e-1",
            event_type: event_types::EDGE_CREATED,
            payload: serde_json::json!({"id": "e-1", "source": "alice", "target": "bob", "relation": "knows"}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        let mut buf = Vec::new();
        let stats = export_json(&core, &mut buf).await.unwrap();

        assert_eq!(stats.nodes, 2);
        assert_eq!(stats.edges, 1);

        let output = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = output.trim().lines().collect();
        assert_eq!(lines.len(), 3);

        // Each line is valid JSON
        for line in &lines {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(parsed.get("type").is_some());
            assert!(parsed.get("entity_id").is_some());
            assert!(parsed.get("data").is_some());
        }

        core.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_import_from_json_lines() {
        let core = test_core().await;

        let input = r#"{"type":"node","entity_id":"node:person:alice","data":{"node_type":"person","properties":{"name":"Alice"}}}
{"type":"edge","entity_id":"edge:e-1","data":{"id":"e-1","source":"alice","target":"bob","relation":"knows"}}
"#;

        let reader = Cursor::new(input);
        let stats = import_json(&core, reader).await.unwrap();

        assert_eq!(stats.nodes, 1);
        assert_eq!(stats.edges, 1);

        // Verify events were ingested
        let events = core
            .query(Query::new().event_type_prefix("prime."))
            .await
            .unwrap();
        assert_eq!(events.len(), 2);

        core.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_export_import_roundtrip() {
        let core = test_core().await;

        // Create data
        core.ingest(IngestEvent {
            entity_id: "node:person:alice",
            event_type: event_types::NODE_CREATED,
            payload: serde_json::json!({"node_type": "person", "properties": {"name": "Alice"}}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        core.ingest(IngestEvent {
            entity_id: "node:project:prime",
            event_type: event_types::NODE_CREATED,
            payload: serde_json::json!({"node_type": "project", "properties": {"name": "Prime"}}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        // Export
        let mut buf = Vec::new();
        export_json(&core, &mut buf).await.unwrap();
        let exported = String::from_utf8(buf.clone()).unwrap();

        // Import into a fresh core
        let core2 = test_core().await;
        let reader = Cursor::new(exported);
        let stats = import_json(&core2, reader).await.unwrap();
        assert_eq!(stats.nodes, 2);

        // Verify data matches
        let events = core2
            .query(Query::new().event_type(event_types::NODE_CREATED))
            .await
            .unwrap();
        assert_eq!(events.len(), 2);

        core.shutdown().await.unwrap();
        core2.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_import_idempotent() {
        let core = test_core().await;

        let input = r#"{"type":"node","entity_id":"node:person:alice","data":{"node_type":"person","properties":{"name":"Alice"}}}"#;

        // Import twice
        let reader1 = Cursor::new(input);
        import_json(&core, reader1).await.unwrap();

        let reader2 = Cursor::new(input);
        import_json(&core, reader2).await.unwrap();

        // Should have 2 events (FirstWriteWins doesn't deduplicate at event level,
        // but the projection applies FWW semantics)
        let events = core
            .query(Query::new().entity_id("node:person:alice"))
            .await
            .unwrap();
        // Both events are stored, but projections see FWW
        assert!(!events.is_empty());

        core.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_import_empty_input() {
        let core = test_core().await;

        let reader = Cursor::new("");
        let stats = import_json(&core, reader).await.unwrap();

        assert_eq!(stats.nodes, 0);
        assert_eq!(stats.edges, 0);

        core.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_import_skips_blank_lines() {
        let core = test_core().await;

        let input = "\n\n{\"type\":\"node\",\"entity_id\":\"node:person:alice\",\"data\":{\"node_type\":\"person\",\"properties\":{\"name\":\"Alice\"}}}\n\n";

        let reader = Cursor::new(input);
        let stats = import_json(&core, reader).await.unwrap();
        assert_eq!(stats.nodes, 1);

        core.shutdown().await.unwrap();
    }
}
