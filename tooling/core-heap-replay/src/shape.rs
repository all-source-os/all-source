//! The fixture shape. Numbers only: no tenant names, no payload text.

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Shape {
    pub seed: u64,
    pub tenants: Tenants,
    pub payload: Payload,
    pub ingest: Ingest,
    pub entities: Entities,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tenants {
    /// Event count per hydrated tenant, largest first.
    pub counts: Vec<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Payload {
    /// Mean payload-plus-metadata size from the budget counter.
    pub mean_bytes: u64,
    /// Log-normal sigma times 1000. Assumed: no production sample is taken.
    pub sigma_milli: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Ingest {
    /// Events pushed into the dedicated `replay-ingest` tenant.
    pub events: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Entities {
    /// Events per entity, so each tenant gets `count / events_per_entity` entities.
    pub events_per_entity: u64,
}

pub fn load_shape(path: &Path) -> Result<Shape, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    toml::from_str(&text).map_err(|e| e.to_string())
}

/// Dotted paths of every string value in the document. Empty when numbers only.
pub fn check_numbers_only(value: &toml::Value) -> Vec<String> {
    let mut found = Vec::new();
    walk(value, "", &mut found);
    found
}

fn walk(value: &toml::Value, path: &str, found: &mut Vec<String>) {
    match value {
        toml::Value::String(_) => found.push(path.to_string()),
        toml::Value::Table(table) => {
            for (key, child) in table {
                let next = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                walk(child, &next, found);
            }
        }
        toml::Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                walk(child, &format!("{path}[{index}]"), found);
            }
        }
        _ => {}
    }
}
