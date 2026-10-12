//! AI Workflow Projection Templates.
//!
//! Pre-built projections for common AI agent patterns:
//! - Token usage / cost tracking
//! - MCP tool call audit (success rate, latency)
//! - Human-in-the-loop approval queue
//! - Agent utilization (active/idle/capacity)

use crate::{
    application::services::projection::Projection, domain::entities::Event, error::Result,
};
use dashmap::DashMap;
use serde_json::{Value, json};

// =============================================================================
// Token Usage / Cost Tracking Projection
// =============================================================================

/// Tracks LLM token usage and costs per entity, with per-model breakdown.
///
/// Folds `llm.call.completed` events into aggregate stats:
/// `{ total_input_tokens, total_output_tokens, total_cost_usd, calls_count, by_model }`
pub struct TokenUsageProjection {
    states: DashMap<String, Value>,
}

impl Default for TokenUsageProjection {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenUsageProjection {
    pub fn new() -> Self {
        Self {
            states: DashMap::new(),
        }
    }
}

impl Projection for TokenUsageProjection {
    fn name(&self) -> &'static str {
        "token_usage"
    }

    fn process(&self, event: &Event) -> Result<()> {
        if event.event_type_str() != "llm.call.completed" {
            return Ok(());
        }

        let entity_id = event.entity_id_str().to_string();
        let payload = &event.payload;

        let input_tokens = payload
            .get("input_tokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let output_tokens = payload
            .get("output_tokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        // Store costs as integer microdollars internally to avoid f64 drift.
        // Convert from f64 USD on input, accumulate as u64, convert back on read.
        let cost_microdollars = (payload
            .get("cost_usd")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0)
            * 1_000_000.0)
            .round() as u64;
        let model = payload
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        self.states
            .entry(entity_id)
            .and_modify(|state| {
                let ti = state["total_input_tokens"].as_u64().unwrap_or(0) + input_tokens;
                let to = state["total_output_tokens"].as_u64().unwrap_or(0) + output_tokens;
                let tc_micro = state["_cost_microdollars"].as_u64().unwrap_or(0) + cost_microdollars;
                let cc = state["calls_count"].as_u64().unwrap_or(0) + 1;

                state["total_input_tokens"] = json!(ti);
                state["total_output_tokens"] = json!(to);
                state["_cost_microdollars"] = json!(tc_micro);
                state["total_cost_usd"] = json!(tc_micro as f64 / 1_000_000.0);
                state["calls_count"] = json!(cc);

                // Per-model breakdown
                if let Some(by_model) = state.get_mut("by_model") {
                    let model_entry = by_model
                        .as_object_mut()
                        .unwrap()
                        .entry(&model)
                        .or_insert(json!({"calls": 0, "input_tokens": 0, "output_tokens": 0, "cost_usd": 0.0, "_cost_microdollars": 0}));
                    model_entry["calls"] = json!(model_entry["calls"].as_u64().unwrap_or(0) + 1);
                    model_entry["input_tokens"] =
                        json!(model_entry["input_tokens"].as_u64().unwrap_or(0) + input_tokens);
                    model_entry["output_tokens"] =
                        json!(model_entry["output_tokens"].as_u64().unwrap_or(0) + output_tokens);
                    let model_micro = model_entry["_cost_microdollars"].as_u64().unwrap_or(0) + cost_microdollars;
                    model_entry["_cost_microdollars"] = json!(model_micro);
                    model_entry["cost_usd"] = json!(model_micro as f64 / 1_000_000.0);
                }
            })
            .or_insert_with(|| {
                json!({
                    "total_input_tokens": input_tokens,
                    "total_output_tokens": output_tokens,
                    "_cost_microdollars": cost_microdollars,
                    "total_cost_usd": cost_microdollars as f64 / 1_000_000.0,
                    "calls_count": 1,
                    "by_model": {
                        model: {
                            "calls": 1,
                            "input_tokens": input_tokens,
                            "output_tokens": output_tokens,
                            "_cost_microdollars": cost_microdollars,
                            "cost_usd": cost_microdollars as f64 / 1_000_000.0,
                        }
                    }
                })
            });

        Ok(())
    }

    fn get_state(&self, entity_id: &str) -> Option<Value> {
        self.states.get(entity_id).map(|v| v.clone())
    }

    fn clear(&self) {
        self.states.clear();
    }
}

// =============================================================================
// MCP Tool Call Audit Projection
// =============================================================================

/// Maximum number of duration samples to keep per tool.
/// Older samples are dropped when this limit is reached.
const MAX_DURATION_SAMPLES: usize = 1000;

/// Tracks MCP tool call success rates and latency per entity.
///
/// Folds `mcp.tool.result` and `mcp.tool.error` events into per-tool stats.
/// Duration samples are capped at `MAX_DURATION_SAMPLES` per tool to bound
/// memory usage and keep percentile computation O(1) amortized.
pub struct ToolCallAuditProjection {
    states: DashMap<String, Value>,
}

impl Default for ToolCallAuditProjection {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolCallAuditProjection {
    pub fn new() -> Self {
        Self {
            states: DashMap::new(),
        }
    }
}

impl Projection for ToolCallAuditProjection {
    fn name(&self) -> &'static str {
        "tool_call_audit"
    }

    fn process(&self, event: &Event) -> Result<()> {
        let event_type = event.event_type_str();
        if event_type != "mcp.tool.result" && event_type != "mcp.tool.error" {
            return Ok(());
        }

        let entity_id = event.entity_id_str().to_string();
        let payload = &event.payload;
        let tool_name = payload
            .get("tool_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let is_success = event_type == "mcp.tool.result";
        let duration_ms = payload
            .get("duration_ms")
            .and_then(serde_json::Value::as_f64);

        self.states
            .entry(entity_id)
            .and_modify(|state| {
                let tool = state
                    .as_object_mut()
                    .unwrap()
                    .entry(&tool_name)
                    .or_insert(json!({
                        "total_calls": 0,
                        "successes": 0,
                        "failures": 0,
                        "success_rate": 0.0,
                        "durations": [],
                    }));
                let total = tool["total_calls"].as_u64().unwrap_or(0) + 1;
                let successes = tool["successes"].as_u64().unwrap_or(0) + u64::from(is_success);
                let failures = tool["failures"].as_u64().unwrap_or(0) + u64::from(!is_success);

                tool["total_calls"] = json!(total);
                tool["successes"] = json!(successes);
                tool["failures"] = json!(failures);
                tool["success_rate"] = json!(successes as f64 / total as f64);

                if let (Some(d), Some(durations)) = (duration_ms, tool["durations"].as_array_mut())
                {
                    durations.push(json!(d));
                    // Cap the ring buffer: drop oldest samples when over limit
                    if durations.len() > MAX_DURATION_SAMPLES {
                        let excess = durations.len() - MAX_DURATION_SAMPLES;
                        durations.drain(..excess);
                    }
                    let mut sorted: Vec<f64> = durations
                        .iter()
                        .filter_map(serde_json::Value::as_f64)
                        .collect();
                    sorted.sort_by(f64::total_cmp);
                    let len = sorted.len();
                    let p50_idx = len / 2;
                    let p95_idx = ((len as f64 * 0.95).ceil() as usize).min(len - 1);
                    tool["p50_ms"] = json!(sorted[p50_idx]);
                    tool["p95_ms"] = json!(sorted[p95_idx]);
                }
            })
            .or_insert_with(|| {
                let mut tool_state = json!({
                    "total_calls": 1,
                    "successes": i32::from(is_success),
                    "failures": i32::from(!is_success),
                    "success_rate": if is_success { 1.0 } else { 0.0 },
                    "durations": [],
                });
                if let Some(d) = duration_ms {
                    tool_state["durations"] = json!([d]);
                    tool_state["p50_ms"] = json!(d);
                    tool_state["p95_ms"] = json!(d);
                }
                json!({ tool_name: tool_state })
            });

        Ok(())
    }

    fn get_state(&self, entity_id: &str) -> Option<Value> {
        self.states.get(entity_id).map(|v| v.clone())
    }

    fn clear(&self) {
        self.states.clear();
    }
}

// =============================================================================
// Human-in-the-Loop Queue Projection
// =============================================================================

/// Tracks pending approval requests across all workflows.
///
/// Query with entity_id `"__all"` to get the full pending queue (sorted
/// by timestamp, oldest first). Use a specific entity ID for per-workflow state.
pub struct HumanInLoopQueueProjection {
    /// entity_id -> (reason, timestamp) for pending approvals
    pending: DashMap<String, (String, chrono::DateTime<chrono::Utc>)>,
}

impl Default for HumanInLoopQueueProjection {
    fn default() -> Self {
        Self::new()
    }
}

impl HumanInLoopQueueProjection {
    pub fn new() -> Self {
        Self {
            pending: DashMap::new(),
        }
    }
}

impl Projection for HumanInLoopQueueProjection {
    fn name(&self) -> &'static str {
        "human_in_loop_queue"
    }

    fn process(&self, event: &Event) -> Result<()> {
        let event_type = event.event_type_str();
        let entity_id = event.entity_id_str().to_string();

        match event_type {
            "workflow.approval.requested" => {
                let reason = event
                    .payload
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                self.pending.insert(entity_id, (reason, event.timestamp()));
            }
            "workflow.approval.granted" | "workflow.approval.rejected" => {
                self.pending.remove(&entity_id);
            }
            _ => {}
        }

        Ok(())
    }

    fn get_state(&self, entity_id: &str) -> Option<Value> {
        if entity_id == "__all" {
            let mut entries: Vec<(String, String, chrono::DateTime<chrono::Utc>)> = self
                .pending
                .iter()
                .map(|entry| {
                    let (reason, ts) = entry.value();
                    (entry.key().clone(), reason.clone(), *ts)
                })
                .collect();
            // Sort by timestamp (oldest first)
            entries.sort_by_key(|(_, _, ts)| *ts);

            let pending_approvals: Vec<Value> = entries
                .into_iter()
                .map(|(eid, reason, ts)| {
                    json!({
                        "entity_id": eid,
                        "reason": reason,
                        "requested_at": ts.to_rfc3339(),
                    })
                })
                .collect();

            Some(json!({ "pending_approvals": pending_approvals }))
        } else {
            self.pending.get(entity_id).map(|entry| {
                let (reason, ts) = entry.value();
                json!({
                    "entity_id": entity_id,
                    "reason": reason,
                    "requested_at": ts.to_rfc3339(),
                })
            })
        }
    }

    fn clear(&self) {
        self.pending.clear();
    }
}

// =============================================================================
// Agent Utilization Projection
// =============================================================================

/// Per-replicant state, consolidated into a single struct to avoid TOCTOU
/// races across multiple DashMap operations.
#[derive(Clone)]
struct ReplicantState {
    status: String,
    active_workflows: Vec<String>,
}

/// Tracks agent (replicant) utilization: active, idle, total capacity.
///
/// Folds `replicant.*` and `workflow.claimed`/`workflow.output.ready` events.
/// Query with entity_id `__all` to get aggregate utilization.
///
/// All per-replicant state is consolidated into a single DashMap entry to
/// prevent TOCTOU races between status and workflow tracking.
///
/// **Thread safety:** Updates to `replicants` and `workflow_to_replicant` are
/// not atomic across both maps. This is safe because `process()` is called
/// under the `EventStore` events write lock, ensuring single-writer semantics.
/// However, `get_state("__all")` reads are eventually consistent — a
/// concurrent read may see a partially-updated state between the two maps.
pub struct AgentUtilizationProjection {
    /// replicant_id -> consolidated state
    replicants: DashMap<String, ReplicantState>,
    /// workflow_id -> replicant_id (reverse lookup for completions)
    workflow_to_replicant: DashMap<String, String>,
}

impl Default for AgentUtilizationProjection {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentUtilizationProjection {
    pub fn new() -> Self {
        Self {
            replicants: DashMap::new(),
            workflow_to_replicant: DashMap::new(),
        }
    }
}

impl Projection for AgentUtilizationProjection {
    fn name(&self) -> &'static str {
        "agent_utilization"
    }

    fn process(&self, event: &Event) -> Result<()> {
        let event_type = event.event_type_str();
        let entity_id = event.entity_id_str().to_string();

        match event_type {
            "replicant.registered" => {
                self.replicants.insert(
                    entity_id,
                    ReplicantState {
                        status: "idle".to_string(),
                        active_workflows: Vec::new(),
                    },
                );
            }
            "replicant.stale" => {
                self.replicants
                    .entry(entity_id)
                    .and_modify(|s| s.status = "stale".to_string())
                    .or_insert(ReplicantState {
                        status: "stale".to_string(),
                        active_workflows: Vec::new(),
                    });
            }
            "replicant.heartbeat" => {
                self.replicants.entry(entity_id).and_modify(|s| {
                    if s.status != "stale" {
                        s.status = "idle".to_string();
                    }
                });
            }
            "workflow.claimed" => {
                if let Some(rid) = event.payload.get("replicant_id").and_then(|v| v.as_str()) {
                    let rid = rid.to_string();
                    self.workflow_to_replicant
                        .insert(entity_id.clone(), rid.clone());
                    // Single atomic update: add workflow and set status
                    self.replicants.entry(rid).and_modify(|s| {
                        if !s.active_workflows.contains(&entity_id) {
                            s.active_workflows.push(entity_id.clone());
                        }
                        s.status = "active".to_string();
                    });
                }
            }
            "workflow.output.ready" | "workflow.step.failed" => {
                // Workflow completed — free up the replicant
                if let Some((_, rid)) = self.workflow_to_replicant.remove(&entity_id) {
                    self.replicants.entry(rid).and_modify(|s| {
                        s.active_workflows.retain(|w| w != &entity_id);
                        if s.active_workflows.is_empty() {
                            s.status = "idle".to_string();
                        }
                    });
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn get_state(&self, entity_id: &str) -> Option<Value> {
        if entity_id == "__all" {
            let mut active = 0u64;
            let mut idle = 0u64;
            let mut total = 0u64;

            for entry in &self.replicants {
                let state = entry.value();
                if state.status != "stale" {
                    total += 1;
                    match state.status.as_str() {
                        "active" => active += 1,
                        "idle" => idle += 1,
                        _ => {}
                    }
                }
            }

            Some(json!({
                "total_capacity": total,
                "active": active,
                "idle": idle,
            }))
        } else {
            self.replicants
                .get(entity_id)
                .map(|entry| json!({ "status": entry.value().status }))
        }
    }

    fn clear(&self) {
        self.replicants.clear();
        self.workflow_to_replicant.clear();
    }
}
