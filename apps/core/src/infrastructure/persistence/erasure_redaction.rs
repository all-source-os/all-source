//! Exact-event content replacement preserves shared-stream identity and ordering.

use super::erasure::{ErasureScope, failure};
use crate::error::Result;
use arrow::{
    array::{Array, StringArray},
    record_batch::RecordBatch,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventRedaction {
    pub event_id: Uuid,
    pub entity_id: String,
    pub expected_content_sha256: String,
    pub payload: Value,
    /// Required in the manifest: `null` clears metadata, so an omitted key must not.
    #[serde(deserialize_with = "Option::deserialize")]
    pub metadata: Option<Value>,
}

fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|key| (key.clone(), canonical(&map[key])))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        _ => value.clone(),
    }
}

pub fn content_sha256(payload: &Value, metadata: Option<&Value>) -> Result<String> {
    let content = serde_json::json!([payload, metadata]);
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(
        &canonical(&content),
    )?)))
}

impl EventRedaction {
    pub(super) fn validate(&self) -> Result<()> {
        if self.entity_id.is_empty()
            || self.entity_id.len() > 1024
            || self.expected_content_sha256.len() != 64
            || !self
                .expected_content_sha256
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            || serde_json::to_vec(self)?.len() > 1024 * 1024
            || self.expected_content_sha256
                == content_sha256(&self.payload, self.metadata.as_ref())?
        {
            return Err(failure(
                "bounded exact-event redaction and distinct content hashes required",
            ));
        }
        Ok(())
    }

    pub(super) fn replace(
        &self,
        entity: &str,
        payload: &mut Value,
        metadata: &mut Option<Value>,
    ) -> Result<bool> {
        if entity != self.entity_id {
            return Err(failure("redaction event belongs to a different entity"));
        }
        let current = content_sha256(payload, metadata.as_ref())?;
        if current == content_sha256(&self.payload, self.metadata.as_ref())? {
            return Ok(false);
        }
        if current != self.expected_content_sha256 {
            return Err(failure("redaction content differs from approved source"));
        }
        payload.clone_from(&self.payload);
        metadata.clone_from(&self.metadata);
        Ok(true)
    }
}

fn strings<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a StringArray> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| failure("invalid redaction identity or content column"))
}

pub(super) fn redact_batch(batch: RecordBatch, scope: &ErasureScope) -> Result<(RecordBatch, u64)> {
    if scope.redactions.is_empty() {
        return Ok((batch, 0));
    }
    let ids = strings(&batch, "event_id")?;
    let entities = strings(&batch, "entity_id")?;
    let payloads = strings(&batch, "payload")?;
    let metadata = strings(&batch, "metadata")?;
    if ids.null_count() > 0 || entities.null_count() > 0 || payloads.null_count() > 0 {
        return Err(failure("null redaction identity or payload"));
    }
    let mut new_payloads = Vec::with_capacity(batch.num_rows());
    let mut new_metadata = Vec::with_capacity(batch.num_rows());
    let mut redacted = 0;
    for row in 0..batch.num_rows() {
        let id = Uuid::parse_str(ids.value(row)).map_err(|_| failure("invalid event identity"))?;
        let mut changed = false;
        let mut payload = None;
        let mut meta = None;
        if let Some(redaction) = scope.redactions.get(&id) {
            scope
                .seen_redactions
                .lock()
                .map_err(|_| failure("redaction inventory lock failed"))?
                .insert(id);
            let mut value = serde_json::from_str(payloads.value(row))?;
            let mut info = if metadata.is_null(row) {
                None
            } else {
                Some(serde_json::from_str(metadata.value(row))?)
            };
            changed = redaction.replace(entities.value(row), &mut value, &mut info)?;
            if changed {
                payload = Some(serde_json::to_string(&value)?);
                meta = info
                    .map(|value| serde_json::to_string(&value))
                    .transpose()?;
            }
        }
        new_payloads.push(payload.unwrap_or_else(|| payloads.value(row).to_owned()));
        new_metadata.push(if changed {
            meta
        } else if metadata.is_null(row) {
            None
        } else {
            Some(metadata.value(row).to_owned())
        });
        redacted += u64::from(changed);
    }
    if redacted == 0 {
        return Ok((batch, 0));
    }
    let mut columns = batch.columns().to_vec();
    let schema = batch.schema();
    columns[schema.index_of("payload")?] = Arc::new(StringArray::from(new_payloads));
    columns[schema.index_of("metadata")?] = Arc::new(StringArray::from(new_metadata));
    Ok((RecordBatch::try_new(schema, columns)?, redacted))
}
