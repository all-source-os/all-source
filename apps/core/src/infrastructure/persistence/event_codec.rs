//! The event row's Arrow schema and its encoder. Every byte an archive file
//! holds for an event is decided here, so this file is protected from heap
//! replay candidates: changing it changes the on-disk format, and a volume
//! must still boot under the previous release.

use crate::{domain::entities::Event, error::Result};
use arrow::{
    array::{ArrayRef, StringBuilder, TimestampMicrosecondBuilder, UInt64Builder},
    datatypes::{DataType, Field, Schema, TimeUnit},
    record_batch::RecordBatch,
};
use std::sync::Arc;

/// Position of `entity_id` in the event schema. All fields are flat
/// primitives, so this is both the field index and the Parquet leaf index.
pub(super) const ENTITY_ID_COLUMN: usize = 2;

/// Position of `payload` in the event schema. See [`ENTITY_ID_COLUMN`].
pub(super) const PAYLOAD_COLUMN: usize = 3;

/// Position of `metadata` in the event schema. See [`ENTITY_ID_COLUMN`].
pub(super) const METADATA_COLUMN: usize = 5;

/// Position of `version` in the event schema. See [`ENTITY_ID_COLUMN`].
pub(super) const VERSION_COLUMN: usize = 6;

/// Nullable valid-time columns, resolved by name on read. Null or absent means
/// the event's valid time is its record time (`timestamp`) and open-ended.
pub(super) const VALID_FROM_COLUMN: &str = "valid_from";
pub(super) const VALID_TO_COLUMN: &str = "valid_to";

/// Arrow schema for events.
/// ENTITY_ID_COLUMN / VERSION_COLUMN index into this field order.
/// valid_from / valid_to are appended last and read by name, so files
/// written before they existed still load.
pub(super) fn event_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("event_id", DataType::Utf8, false),
        Field::new("event_type", DataType::Utf8, false),
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("payload", DataType::Utf8, false),
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
        Field::new("metadata", DataType::Utf8, true),
        Field::new("version", DataType::UInt64, false),
        Field::new(
            VALID_FROM_COLUMN,
            DataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        ),
        Field::new(
            VALID_TO_COLUMN,
            DataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        ),
    ]))
}

/// Convert events to Arrow RecordBatch
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub(super) fn events_to_record_batch(
    schema: &Arc<Schema>,
    events: &[Event],
) -> Result<RecordBatch> {
    let mut event_id_builder = StringBuilder::new();
    let mut event_type_builder = StringBuilder::new();
    let mut entity_id_builder = StringBuilder::new();
    let mut payload_builder = StringBuilder::new();
    let mut timestamp_builder = TimestampMicrosecondBuilder::new();
    let mut metadata_builder = StringBuilder::new();
    let mut version_builder = UInt64Builder::new();
    let mut valid_from_builder = TimestampMicrosecondBuilder::new();
    let mut valid_to_builder = TimestampMicrosecondBuilder::new();

    for event in events {
        event_id_builder.append_value(event.id.to_string());
        event_type_builder.append_value(event.event_type_str());
        entity_id_builder.append_value(event.entity_id_str());
        payload_builder.append_value(serde_json::to_string(&event.payload)?);

        let timestamp_micros = event.timestamp.timestamp_micros();
        timestamp_builder.append_value(timestamp_micros);

        if let Some(ref metadata) = event.metadata {
            metadata_builder.append_value(serde_json::to_string(metadata)?);
        } else {
            metadata_builder.append_null();
        }

        version_builder.append_value(event.version as u64);
        valid_from_builder.append_option(event.valid_from.map(|t| t.timestamp_micros()));
        valid_to_builder.append_option(event.valid_to.map(|t| t.timestamp_micros()));
    }

    let arrays: Vec<ArrayRef> = vec![
        Arc::new(event_id_builder.finish()),
        Arc::new(event_type_builder.finish()),
        Arc::new(entity_id_builder.finish()),
        Arc::new(payload_builder.finish()),
        Arc::new(timestamp_builder.finish()),
        Arc::new(metadata_builder.finish()),
        Arc::new(version_builder.finish()),
        Arc::new(valid_from_builder.finish()),
        Arc::new(valid_to_builder.finish()),
    ];

    let record_batch = RecordBatch::try_new(schema.clone(), arrays)?;

    Ok(record_batch)
}
