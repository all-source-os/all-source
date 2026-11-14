//! Charges for the state Prime projections hold. Every projection holds one
//! `Tally` on `projection.permanent`. A new map entry costs its hash-table slot
//! plus the heap its key and value own; an in-place change costs the difference.
//! `clear` takes the tally, so a cleared projection releases what it held.

use crate::infrastructure::{
    memory::ledger::{Owner, Tally},
    observability::heap::{hash_entry_bytes, json_heap_bytes},
};
use serde_json::Value;

/// The tally every Prime projection charges. Dropping the projection releases it.
pub(crate) fn tally() -> Tally {
    Tally::new(Owner::ProjectionPermanent)
}

/// Bytes one new map entry holds: its table slot and the heap it owns.
pub(crate) fn entry<K, V>(owned: u64) -> u64 {
    hash_entry_bytes::<K, V>() as u64 + owned
}

pub(crate) fn text(value: &str) -> u64 {
    value.len() as u64
}

pub(crate) fn opt_text(value: Option<&str>) -> u64 {
    value.map_or(0, text)
}

pub(crate) fn texts(values: &[String]) -> u64 {
    values.iter().map(|value| text(value)).sum()
}

pub(crate) fn json(value: &Value) -> u64 {
    json_heap_bytes(value) as u64
}
