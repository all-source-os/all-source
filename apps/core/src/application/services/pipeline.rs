use crate::{
    domain::entities::{Event, event::value_heap_bytes},
    error::{AllSourceError, Result},
    infrastructure::{
        memory::ledger::{MapCharge, Owner, Tally},
        observability::{
            heap::{hash_entry_bytes, json_heap_bytes},
            metrics::MetricsRegistry,
        },
    },
};
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::{
    collections::{HashMap, VecDeque},
    mem::size_of,
    sync::Arc,
};
use uuid::Uuid;

/// Window type for time-based aggregations
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WindowType {
    /// Tumbling window (non-overlapping)
    Tumbling,
    /// Sliding window (overlapping)
    Sliding,
    /// Session window (activity-based)
    Session,
}

/// Window configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    /// Type of window
    pub window_type: WindowType,

    /// Window size in seconds
    pub size_seconds: i64,

    /// Slide interval in seconds (for sliding windows)
    pub slide_seconds: Option<i64>,

    /// Session timeout in seconds (for session windows)
    pub session_timeout_seconds: Option<i64>,
}

/// Pipeline operator types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PipelineOperator {
    /// Filter events based on a condition
    Filter {
        /// Field path to check (e.g., "payload.status")
        field: String,
        /// Expected value
        value: JsonValue,
        /// Operator: eq, ne, gt, lt, contains
        op: String,
    },

    /// Transform event payload
    Map {
        /// Field to transform
        field: String,
        /// Transformation expression (simple for now)
        transform: String,
    },

    /// Aggregate events
    Reduce {
        /// Field to aggregate
        field: String,
        /// Aggregation function: sum, count, avg, min, max
        function: String,
        /// Group by field (optional)
        group_by: Option<String>,
    },

    /// Window-based aggregation
    Window {
        /// Window configuration
        config: WindowConfig,
        /// Aggregation to apply within window
        aggregation: Box<PipelineOperator>,
    },

    /// Enrich event with external data
    Enrich {
        /// Source to enrich from
        source: String,
        /// Fields to add
        fields: Vec<String>,
    },

    /// Split stream based on condition
    Branch {
        /// Condition field
        field: String,
        /// Branch mapping
        branches: HashMap<String, String>,
    },
}

/// Pipeline configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Pipeline ID
    pub id: Uuid,

    /// Pipeline name
    pub name: String,

    /// Description
    pub description: Option<String>,

    /// Source event types to process
    pub source_event_types: Vec<String>,

    /// Pipeline operators in order
    pub operators: Vec<PipelineOperator>,

    /// Whether pipeline is enabled
    pub enabled: bool,

    /// Output destination (projection name or topic)
    pub output: String,
}

/// Pipeline execution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStats {
    pub pipeline_id: Uuid,
    pub events_processed: u64,
    pub events_filtered: u64,
    pub events_failed: u64,
    pub last_processed: Option<DateTime<Utc>>,
}

/// Stateful operator for maintaining state across events
pub struct StatefulOperator {
    /// Operator state storage
    state: Arc<RwLock<HashMap<String, JsonValue>>>,
    state_charge: Tally,

    /// Window buffers for time-based operations
    windows: Arc<RwLock<HashMap<String, VecDeque<(DateTime<Utc>, Event)>>>>,
    windows_charge: Tally,
}

/// Heap one state entry holds: its table slot, its key, and the value's owned heap.
fn state_entry_bytes(key_len: usize, value: &JsonValue) -> u64 {
    (hash_entry_bytes::<String, JsonValue>() + key_len + json_heap_bytes(value)) as u64
}

impl StatefulOperator {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(HashMap::new())),
            state_charge: Tally::new(Owner::PipelineState),
            windows: Arc::new(RwLock::new(HashMap::new())),
            windows_charge: Tally::new(Owner::PipelineWindows),
        }
    }

    /// Store state value
    pub fn set_state(&self, key: String, value: JsonValue) {
        let key_len = key.len();
        let added = state_entry_bytes(key_len, &value);
        // Every charge and release of state and windows happens under that map's
        // write guard; otherwise a concurrent replace or clear releases bytes
        // before they are added, and the clamped sub loses them.
        let mut state = self.state.write();
        self.state_charge.add(added);
        if let Some(old) = state.insert(key, value) {
            self.state_charge.sub(state_entry_bytes(key_len, &old));
        }
    }

    /// Get state value
    pub fn get_state(&self, key: &str) -> Option<JsonValue> {
        self.state.read().get(key).cloned()
    }

    /// Add event to window
    pub fn add_to_window(&self, window_key: &str, event: Event, timestamp: DateTime<Utc>) {
        let bytes = event.resident_bytes();
        let mut windows = self.windows.write();
        self.windows_charge.add(bytes);
        windows
            .entry(window_key.to_string())
            .or_default()
            .push_back((timestamp, event));
    }

    /// Get events in window
    pub fn get_window(&self, window_key: &str) -> Vec<Event> {
        self.windows
            .read()
            .get(window_key)
            .map(|w| w.iter().map(|(_, e)| e.clone()).collect())
            .unwrap_or_default()
    }

    /// Evict expired events from window
    pub fn evict_window(&self, window_key: &str, cutoff: DateTime<Utc>) {
        if let Some(window) = self.windows.write().get_mut(window_key) {
            let released: u64 = window
                .iter()
                .filter(|(ts, _)| *ts <= cutoff)
                .map(|(_, event)| event.resident_bytes())
                .sum();
            window.retain(|(ts, _)| *ts > cutoff);
            self.windows_charge.sub(released);
        }
    }

    /// Clear all state
    pub fn clear(&self) {
        let mut state = self.state.write();
        state.clear();
        self.state_charge.take();
        drop(state);
        let mut windows = self.windows.write();
        windows.clear();
        self.windows_charge.take();
    }
}

impl Default for StatefulOperator {
    fn default() -> Self {
        Self::new()
    }
}

/// Pipeline execution engine
pub struct Pipeline {
    config: PipelineConfig,
    state: StatefulOperator,
    stats: Arc<RwLock<PipelineStats>>,
}

impl Pipeline {
    pub fn new(config: PipelineConfig) -> Self {
        let stats = PipelineStats {
            pipeline_id: config.id,
            events_processed: 0,
            events_filtered: 0,
            events_failed: 0,
            last_processed: None,
        };

        Self {
            config,
            state: StatefulOperator::new(),
            stats: Arc::new(RwLock::new(stats)),
        }
    }

    /// Process an event through the pipeline
    pub fn process(&self, event: &Event) -> Result<Option<JsonValue>> {
        // Check if event type matches source filter
        if !self.config.source_event_types.is_empty()
            && !self
                .config
                .source_event_types
                .iter()
                .any(|t| t == event.event_type_str())
        {
            return Ok(None);
        }

        if !self.config.enabled {
            return Ok(None);
        }

        let mut current_value = event.payload.clone();
        let mut filtered = false;

        // Apply operators in sequence
        for operator in &self.config.operators {
            match self.apply_operator(operator, &current_value, event) {
                Ok(Some(result)) => {
                    current_value = result;
                }
                Ok(None) => {
                    // Event was filtered out
                    filtered = true;
                    self.stats.write().events_filtered += 1;
                    break;
                }
                Err(e) => {
                    self.stats.write().events_failed += 1;
                    tracing::error!("Pipeline {} operator failed: {}", self.config.name, e);
                    return Err(e);
                }
            }
        }

        // Update stats
        let mut stats = self.stats.write();
        stats.events_processed += 1;
        stats.last_processed = Some(Utc::now());

        if filtered {
            Ok(None)
        } else {
            Ok(Some(current_value))
        }
    }

    /// Apply a single operator
    fn apply_operator(
        &self,
        operator: &PipelineOperator,
        value: &JsonValue,
        event: &Event,
    ) -> Result<Option<JsonValue>> {
        match operator {
            PipelineOperator::Filter {
                field,
                value: expected,
                op,
            } => self.apply_filter(field, expected, op, value),

            PipelineOperator::Map { field, transform } => self.apply_map(field, transform, value),

            PipelineOperator::Reduce {
                field,
                function,
                group_by,
            } => self.apply_reduce(field, function, group_by.as_deref(), value, event),

            PipelineOperator::Window {
                config,
                aggregation,
            } => self.apply_window(config, aggregation, event),

            PipelineOperator::Enrich { source, fields } => self.apply_enrich(source, fields, value),

            PipelineOperator::Branch { field, branches } => {
                self.apply_branch(field, branches, value)
            }
        }
    }

    /// Apply filter operator
    fn apply_filter(
        &self,
        field: &str,
        expected: &JsonValue,
        op: &str,
        value: &JsonValue,
    ) -> Result<Option<JsonValue>> {
        let field_value = self.get_field(value, field);

        let matches = match op {
            "eq" => field_value == Some(expected),
            "ne" => field_value != Some(expected),
            "gt" => {
                if let (Some(JsonValue::Number(a)), JsonValue::Number(b)) =
                    (field_value.as_ref(), expected)
                {
                    a.as_f64().unwrap_or(0.0) > b.as_f64().unwrap_or(0.0)
                } else {
                    false
                }
            }
            "lt" => {
                if let (Some(JsonValue::Number(a)), JsonValue::Number(b)) =
                    (field_value.as_ref(), expected)
                {
                    a.as_f64().unwrap_or(0.0) < b.as_f64().unwrap_or(0.0)
                } else {
                    false
                }
            }
            "contains" => {
                if let (Some(JsonValue::String(a)), JsonValue::String(b)) =
                    (field_value.as_ref(), expected)
                {
                    a.contains(b)
                } else {
                    false
                }
            }
            _ => {
                return Err(AllSourceError::ValidationError(format!(
                    "Unknown filter operator: {op}"
                )));
            }
        };

        if matches {
            Ok(Some(value.clone()))
        } else {
            Ok(None) // Filtered out
        }
    }

    /// Apply map transformation
    fn apply_map(
        &self,
        field: &str,
        transform: &str,
        value: &JsonValue,
    ) -> Result<Option<JsonValue>> {
        let mut result = value.clone();

        // Simple transformations
        let field_value = self.get_field(value, field);

        let transformed = match transform {
            "uppercase" => field_value
                .and_then(|v| v.as_str())
                .map(|s| JsonValue::String(s.to_uppercase())),
            "lowercase" => field_value
                .and_then(|v| v.as_str())
                .map(|s| JsonValue::String(s.to_lowercase())),
            "trim" => field_value
                .and_then(|v| v.as_str())
                .map(|s| JsonValue::String(s.trim().to_string())),
            _ => {
                // Try to parse as number operation
                if let Some(stripped) = transform.strip_prefix("multiply:") {
                    if let Ok(multiplier) = stripped.parse::<f64>() {
                        field_value.and_then(serde_json::Value::as_f64).map(|n| {
                            JsonValue::Number(serde_json::Number::from_f64(n * multiplier).unwrap())
                        })
                    } else {
                        None
                    }
                } else if let Some(stripped) = transform.strip_prefix("add:") {
                    if let Ok(addend) = stripped.parse::<f64>() {
                        field_value.and_then(serde_json::Value::as_f64).map(|n| {
                            JsonValue::Number(serde_json::Number::from_f64(n + addend).unwrap())
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        };

        if let Some(new_value) = transformed {
            self.set_field(&mut result, field, new_value);
        }

        Ok(Some(result))
    }

    /// Apply reduce aggregation
    fn apply_reduce(
        &self,
        field: &str,
        function: &str,
        group_by: Option<&str>,
        value: &JsonValue,
        event: &Event,
    ) -> Result<Option<JsonValue>> {
        // Get group key
        let group_key = if let Some(group_field) = group_by {
            self.get_field(value, group_field)
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .to_string()
        } else {
            "default".to_string()
        };

        let state_key = format!("reduce_{function}_{group_key}");

        // Get current aggregate value
        let current = self.state.get_state(&state_key);

        // Get field value to aggregate
        let field_value = self.get_field(value, field);

        let new_value = match function {
            "count" => {
                let count = current.and_then(|v| v.as_u64()).unwrap_or(0) + 1;
                JsonValue::Number(count.into())
            }
            "sum" => {
                let current_sum = current.and_then(|v| v.as_f64()).unwrap_or(0.0);
                let value_to_add = field_value
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                JsonValue::Number(serde_json::Number::from_f64(current_sum + value_to_add).unwrap())
            }
            "avg" => {
                // Store sum and count separately
                let sum_key = format!("{state_key}_sum");
                let count_key = format!("{state_key}_count");

                let current_sum = self
                    .state
                    .get_state(&sum_key)
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let current_count = self
                    .state
                    .get_state(&count_key)
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                let value_to_add = field_value
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);

                let new_sum = current_sum + value_to_add;
                let new_count = current_count + 1;

                self.state.set_state(
                    sum_key,
                    JsonValue::Number(serde_json::Number::from_f64(new_sum).unwrap()),
                );
                self.state
                    .set_state(count_key, JsonValue::Number(new_count.into()));

                let avg = new_sum / new_count as f64;
                JsonValue::Number(serde_json::Number::from_f64(avg).unwrap())
            }
            "min" => {
                let current_min = current.and_then(|v| v.as_f64());
                let new_val = field_value.and_then(serde_json::Value::as_f64);

                match (current_min, new_val) {
                    (Some(curr), Some(new)) => {
                        JsonValue::Number(serde_json::Number::from_f64(curr.min(new)).unwrap())
                    }
                    (None, Some(new)) => {
                        JsonValue::Number(serde_json::Number::from_f64(new).unwrap())
                    }
                    (Some(curr), None) => {
                        JsonValue::Number(serde_json::Number::from_f64(curr).unwrap())
                    }
                    (None, None) => JsonValue::Null,
                }
            }
            "max" => {
                let current_max = current.and_then(|v| v.as_f64());
                let new_val = field_value.and_then(serde_json::Value::as_f64);

                match (current_max, new_val) {
                    (Some(curr), Some(new)) => {
                        JsonValue::Number(serde_json::Number::from_f64(curr.max(new)).unwrap())
                    }
                    (None, Some(new)) => {
                        JsonValue::Number(serde_json::Number::from_f64(new).unwrap())
                    }
                    (Some(curr), None) => {
                        JsonValue::Number(serde_json::Number::from_f64(curr).unwrap())
                    }
                    (None, None) => JsonValue::Null,
                }
            }
            _ => {
                return Err(AllSourceError::ValidationError(format!(
                    "Unknown reduce function: {function}"
                )));
            }
        };

        // Update state
        self.state.set_state(state_key.clone(), new_value.clone());

        // Return aggregated result
        let result = serde_json::json!({
            "group": group_key,
            "function": function,
            "value": new_value
        });

        Ok(Some(result))
    }

    /// Apply window aggregation
    fn apply_window(
        &self,
        config: &WindowConfig,
        aggregation: &PipelineOperator,
        event: &Event,
    ) -> Result<Option<JsonValue>> {
        let window_key = format!("window_{}", self.config.id);
        let now = Utc::now();

        // Add event to window
        self.state
            .add_to_window(&window_key, event.clone(), event.timestamp);

        // Evict expired events based on window type
        let cutoff = match config.window_type {
            WindowType::Tumbling => now - Duration::seconds(config.size_seconds),
            WindowType::Sliding => {
                let slide = config.slide_seconds.unwrap_or(config.size_seconds);
                now - Duration::seconds(slide)
            }
            WindowType::Session => {
                let timeout = config.session_timeout_seconds.unwrap_or(300);
                now - Duration::seconds(timeout)
            }
        };

        self.state.evict_window(&window_key, cutoff);

        // Get events in current window
        let window_events = self.state.get_window(&window_key);

        // Apply aggregation to window
        let mut aggregate_value = JsonValue::Null;
        for window_event in &window_events {
            if let Ok(Some(result)) =
                self.apply_operator(aggregation, &window_event.payload, window_event)
            {
                aggregate_value = result;
            }
        }

        Ok(Some(serde_json::json!({
            "window_type": config.window_type,
            "window_size_seconds": config.size_seconds,
            "events_in_window": window_events.len(),
            "aggregation": aggregate_value
        })))
    }

    /// Apply enrichment
    fn apply_enrich(
        &self,
        _source: &str,
        fields: &[String],
        value: &JsonValue,
    ) -> Result<Option<JsonValue>> {
        // Placeholder for enrichment logic
        // In production, this would fetch data from external sources
        let mut result = value.clone();

        for field in fields {
            let enriched_value = JsonValue::String(format!("enriched_{field}"));
            self.set_field(&mut result, field, enriched_value);
        }

        Ok(Some(result))
    }

    /// Apply branch routing
    fn apply_branch(
        &self,
        field: &str,
        branches: &HashMap<String, String>,
        value: &JsonValue,
    ) -> Result<Option<JsonValue>> {
        let field_value = self.get_field(value, field);

        if let Some(JsonValue::String(val)) = field_value
            && let Some(route) = branches.get(val)
        {
            let mut result = value.clone();
            if let JsonValue::Object(ref mut obj) = result {
                obj.insert("_route".to_string(), JsonValue::String(route.clone()));
            }
            return Ok(Some(result));
        }

        Ok(Some(value.clone()))
    }

    /// Helper: Get nested field from JSON
    fn get_field<'a>(&self, value: &'a JsonValue, field: &str) -> Option<&'a JsonValue> {
        let parts: Vec<&str> = field.split('.').collect();
        let mut current = value;

        for part in parts {
            current = current.get(part)?;
        }

        Some(current)
    }

    /// Helper: Set nested field in JSON
    fn set_field(&self, value: &mut JsonValue, field: &str, new_value: JsonValue) {
        let parts: Vec<&str> = field.split('.').collect();

        if parts.len() == 1 {
            if let JsonValue::Object(obj) = value {
                obj.insert(field.to_string(), new_value);
            }
            return;
        }

        // Navigate to parent
        let mut current = value;
        for part in &parts[..parts.len() - 1] {
            if let JsonValue::Object(obj) = current {
                current = obj
                    .entry((*part).to_string())
                    .or_insert(JsonValue::Object(Default::default()));
            }
        }

        // Set final value
        if let JsonValue::Object(obj) = current {
            obj.insert((*parts.last().unwrap()).to_string(), new_value);
        }
    }

    /// Get pipeline statistics
    pub fn stats(&self) -> PipelineStats {
        self.stats.read().clone()
    }

    /// Get pipeline configuration
    pub fn config(&self) -> &PipelineConfig {
        &self.config
    }

    /// Reset pipeline state
    pub fn reset(&self) {
        self.state.clear();
        let mut stats = self.stats.write();
        stats.events_processed = 0;
        stats.events_filtered = 0;
        stats.events_failed = 0;
        stats.last_processed = None;
    }
}

/// Manages multiple pipelines
pub struct PipelineManager {
    // Using DashMap for lock-free concurrent access
    pipelines: Arc<DashMap<Uuid, Arc<Pipeline>>>,
    pipelines_charge: MapCharge,
    metrics: Arc<MetricsRegistry>,
}

/// Owned heap of a pipeline's configuration, measured on its JSON form.
fn config_heap(config: &PipelineConfig) -> u64 {
    serde_json::to_value(config).map_or(0, |value| value_heap_bytes(&value) as u64)
}

impl PipelineManager {
    pub fn new() -> Self {
        Self::with_metrics(MetricsRegistry::new())
    }

    pub fn with_metrics(metrics: Arc<MetricsRegistry>) -> Self {
        Self {
            pipelines: Arc::new(DashMap::new()),
            pipelines_charge: MapCharge::new(Owner::Misc, size_of::<(Uuid, Arc<Pipeline>)>()),
            metrics,
        }
    }

    /// Register a new pipeline
    pub fn register(&self, config: PipelineConfig) -> Uuid {
        let id = config.id;
        let name = config.name.clone();
        let pipeline = Arc::new(Pipeline::new(config));
        self.pipelines_charge
            .insert(&self.pipelines, id, pipeline, |_, p| config_heap(&p.config));

        let count = self.pipelines.len();
        self.metrics.pipelines_registered_total.set(count as i64);

        tracing::info!("📊 Registered pipeline: {} ({})", name, id);
        id
    }

    /// Get a pipeline by ID
    pub fn get(&self, id: Uuid) -> Option<Arc<Pipeline>> {
        self.pipelines.get(&id).map(|entry| entry.value().clone())
    }

    /// Process event through all matching pipelines
    pub fn process_event(&self, event: &Event) -> Vec<(Uuid, JsonValue)> {
        let timer = self.metrics.pipeline_duration_seconds.start_timer();

        let mut results = Vec::new();

        for entry in self.pipelines.iter() {
            let id = entry.key();
            let pipeline = entry.value();
            let pipeline_name = &pipeline.config().name;
            let pipeline_id = id.to_string();

            match pipeline.process(event) {
                Ok(Some(result)) => {
                    self.metrics
                        .pipeline_events_processed
                        .with_label_values(&[&pipeline_id, pipeline_name])
                        .inc();
                    results.push((*id, result));
                }
                Ok(None) => {
                    // Event filtered out or didn't match - not an error
                }
                Err(e) => {
                    self.metrics
                        .pipeline_errors_total
                        .with_label_values(&[pipeline_name])
                        .inc();
                    tracing::error!(
                        "Pipeline '{}' ({}) failed to process event: {}",
                        pipeline_name,
                        id,
                        e
                    );
                }
            }
        }

        timer.observe_duration();
        results
    }

    /// List all pipelines
    pub fn list(&self) -> Vec<PipelineConfig> {
        self.pipelines
            .iter()
            .map(|entry| entry.value().config().clone())
            .collect()
    }

    /// Remove a pipeline
    pub fn remove(&self, id: Uuid) -> bool {
        let removed = self
            .pipelines_charge
            .remove_entry(&self.pipelines, &id, |_, p| config_heap(&p.config))
            .is_some();

        if removed {
            let count = self.pipelines.len();
            self.metrics.pipelines_registered_total.set(count as i64);
        }

        removed
    }

    /// Get statistics for all pipelines
    pub fn all_stats(&self) -> Vec<PipelineStats> {
        self.pipelines
            .iter()
            .map(|entry| entry.value().stats())
            .collect()
    }
}

impl Default for PipelineManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_filter_operator() {
        let config = PipelineConfig {
            id: Uuid::new_v4(),
            name: "test_filter".to_string(),
            description: None,
            source_event_types: vec!["test".to_string()],
            operators: vec![PipelineOperator::Filter {
                field: "status".to_string(),
                value: json!("active"),
                op: "eq".to_string(),
            }],
            enabled: true,
            output: "test_output".to_string(),
        };

        let pipeline = Pipeline::new(config);
        let event = Event::from_strings(
            "test".to_string(),
            "entity1".to_string(),
            "default".to_string(),
            json!({"status": "active"}),
            None,
        )
        .unwrap();

        let result = pipeline.process(&event).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_map_operator() {
        let config = PipelineConfig {
            id: Uuid::new_v4(),
            name: "test_map".to_string(),
            description: None,
            source_event_types: vec!["test".to_string()],
            operators: vec![PipelineOperator::Map {
                field: "name".to_string(),
                transform: "uppercase".to_string(),
            }],
            enabled: true,
            output: "test_output".to_string(),
        };

        let pipeline = Pipeline::new(config);
        let event = Event::from_strings(
            "test".to_string(),
            "entity1".to_string(),
            "default".to_string(),
            json!({"name": "hello"}),
            None,
        )
        .unwrap();

        let result = pipeline.process(&event).unwrap().unwrap();
        assert_eq!(result["name"], "HELLO");
    }

    #[test]
    fn test_reduce_count() {
        let config = PipelineConfig {
            id: Uuid::new_v4(),
            name: "test_reduce".to_string(),
            description: None,
            source_event_types: vec!["test".to_string()],
            operators: vec![PipelineOperator::Reduce {
                field: "value".to_string(),
                function: "count".to_string(),
                group_by: None,
            }],
            enabled: true,
            output: "test_output".to_string(),
        };

        let pipeline = Pipeline::new(config);

        for i in 0..5 {
            let event = Event::from_strings(
                "test".to_string(),
                "entity1".to_string(),
                "default".to_string(),
                json!({"value": i}),
                None,
            )
            .unwrap();
            pipeline.process(&event).unwrap();
        }

        let result = pipeline
            .process(
                &Event::from_strings(
                    "test".to_string(),
                    "entity1".to_string(),
                    "default".to_string(),
                    json!({"value": 5}),
                    None,
                )
                .unwrap(),
            )
            .unwrap()
            .unwrap();

        assert_eq!(result["value"], 6);
    }
}
