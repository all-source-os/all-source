//! MCP tool event emission helper.
//!
//! [`McpToolTracker`] wraps MCP tool calls and automatically emits
//! `mcp.tool.result` and `mcp.tool.error` events that feed the
//! [`ToolCallAuditProjection`](super::ai_projections::ToolCallAuditProjection).
//!
//! # Example
//!
//! ```rust,no_run
//! use allsource_core::embedded::{Config, EmbeddedCore};
//! use allsource_core::embedded::mcp_events::McpToolTracker;
//! use serde_json::json;
//!
//! # #[tokio::main]
//! # async fn main() -> allsource_core::error::Result<()> {
//! let core = EmbeddedCore::open(Config::builder().build()?).await?;
//! let tracker = McpToolTracker::new(&core);
//!
//! // Manual emission
//! tracker.emit_tool_result("wf-1", "read_file", 50).await?;
//! tracker.emit_tool_error("wf-1", "web_search", "timeout").await?;
//!
//! // Automatic tracking with timing
//! let result = tracker.track("wf-1", "read_file", || async {
//!     Ok(json!({"content": "hello"}))
//! }).await?;
//! # Ok(())
//! # }
//! ```

use crate::{
    embedded::{EmbeddedCore, IngestEvent},
    error::Result,
};
use serde_json::Value;
use std::future::Future;

/// Helper that wraps MCP tool calls and auto-emits lifecycle events.
pub struct McpToolTracker<'a> {
    core: &'a EmbeddedCore,
}

impl<'a> McpToolTracker<'a> {
    /// Create a new tracker bound to an `EmbeddedCore` instance.
    pub fn new(core: &'a EmbeddedCore) -> Self {
        Self { core }
    }

    /// Emit a `mcp.tool.result` event for a successful tool call.
    pub async fn emit_tool_result(
        &self,
        workflow_id: &str,
        tool_name: &str,
        duration_ms: u64,
    ) -> Result<()> {
        self.core
            .ingest(IngestEvent {
                entity_id: workflow_id,
                event_type: "mcp.tool.result",
                payload: serde_json::json!({
                    "tool_name": tool_name,
                    "duration_ms": duration_ms,
                }),
                metadata: None,
                tenant_id: None,
            })
            .await
    }

    /// Emit a `mcp.tool.error` event for a failed tool call.
    pub async fn emit_tool_error(
        &self,
        workflow_id: &str,
        tool_name: &str,
        error: &str,
    ) -> Result<()> {
        self.core
            .ingest(IngestEvent {
                entity_id: workflow_id,
                event_type: "mcp.tool.error",
                payload: serde_json::json!({
                    "tool_name": tool_name,
                    "error": error,
                }),
                metadata: None,
                tenant_id: None,
            })
            .await
    }

    /// Track a tool call: execute the future and auto-emit a result or error
    /// event with timing information.
    ///
    /// On success, emits `mcp.tool.result` with `duration_ms`.
    /// On error, emits `mcp.tool.error` with the error message and `duration_ms`.
    pub async fn track<F, Fut>(&self, workflow_id: &str, tool_name: &str, f: F) -> Result<Value>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Value>>,
    {
        let start = std::time::Instant::now();
        let result = f().await;
        let duration_ms = start.elapsed().as_millis() as u64;

        match &result {
            Ok(_) => {
                self.emit_tool_result(workflow_id, tool_name, duration_ms)
                    .await?;
            }
            Err(e) => {
                self.core
                    .ingest(IngestEvent {
                        entity_id: workflow_id,
                        event_type: "mcp.tool.error",
                        payload: serde_json::json!({
                            "tool_name": tool_name,
                            "error": e.to_string(),
                            "duration_ms": duration_ms,
                        }),
                        metadata: None,
                        tenant_id: None,
                    })
                    .await?;
            }
        }

        result
    }
}
