use super::{EventStore, ReadScope};
use crate::{
    domain::{
        entities::Event,
        value_objects::{EntityId, TenantId},
    },
    error::{AllSourceError, Result},
    infrastructure::persistence::archive_budget::ArchiveReadBudget,
};
use chrono::SubsecRound;
use std::{
    collections::HashSet,
    io,
    sync::{Arc, atomic::AtomicBool},
};

const MAX_ENTITY_EVENTS: usize = 1_001;
// Leave room inside the consumer's 2 MiB wire cap for the response envelope.
const MAX_ENCODED_EVENTS: usize = 2 * 1024 * 1024 - 16 * 1024;

impl EventStore {
    /// Read one complete retained entity snapshot under an explicit read scope.
    /// Archive errors, oversized input and cancellation refuse the
    /// read; this is not evidence that retention never removed older history.
    /// Timestamp precision and ordering match Parquet's microseconds so the
    /// same retained evidence stays stable across cache eviction and restart.
    pub fn query_retained_entity(
        &self,
        tenant_id: &str,
        entity_id: &str,
        limit: usize,
        scope: &ReadScope,
    ) -> Result<(Vec<Event>, usize)> {
        self.query_retained_entity_cancellable(tenant_id, entity_id, limit, scope, None)
    }

    pub(crate) fn query_retained_entity_cancellable(
        &self,
        tenant_id: &str,
        entity_id: &str,
        limit: usize,
        scope: &ReadScope,
        cancellation: Option<&Arc<AtomicBool>>,
    ) -> Result<(Vec<Event>, usize)> {
        TenantId::new(tenant_id.to_string())?;
        EntityId::new(entity_id.to_string())?;
        if !(1..=MAX_ENTITY_EVENTS).contains(&limit) || !scope.permits(entity_id) {
            return Err(AllSourceError::InvalidInput(
                "Strict retained read target or limit refused".into(),
            ));
        }
        let mut budget = ArchiveReadBudget::new(self.strict_archive_limits.clone())
            .with_cancellation(cancellation.cloned());
        budget.check()?;
        let _resident = self
            .cache_residency_gate
            .try_read_for(budget.remaining()?)
            .ok_or_else(|| {
                AllSourceError::StorageError("Strict retained read residency lock timed out".into())
            })?;
        let mut selected = Vec::new();
        let mut seen = HashSet::new();
        let mut remaining = MAX_ENCODED_EVENTS;
        let mut version = 0;
        let mut retain = |event: &Event, work: &ArchiveReadBudget| -> Result<()> {
            work.check()?;
            let observed = u64::try_from(event.version).map_err(|_| {
                AllSourceError::StorageError("Archived entity version cannot be represented".into())
            })?;
            version = version.max(observed);
            if seen.contains(&event.id) {
                return Ok(());
            }
            if selected.len() == MAX_ENTITY_EVENTS {
                return Err(AllSourceError::StorageError(
                    "Strict retained read event budget exceeded".into(),
                ));
            }
            let mut encoded = EncodedBudget { remaining, work };
            serde_json::to_writer(&mut encoded, event).map_err(|_| {
                AllSourceError::StorageError(
                    "Strict retained read encoded data budget exceeded".into(),
                )
            })?;
            remaining = encoded.remaining;
            seen.insert(event.id);
            selected.push(event.clone());
            Ok(())
        };
        if let Some(storage) = &self.storage {
            storage
                .try_read_for(budget.remaining()?)
                .ok_or_else(|| {
                    AllSourceError::StorageError(
                        "Strict retained read storage lock timed out".into(),
                    )
                })?
                .visit_retained_entity(tenant_id, entity_id, &mut budget, &mut retain)?;
        }
        let events = self
            .events
            .try_read_for(budget.remaining()?)
            .ok_or_else(|| {
                AllSourceError::StorageError("Strict retained read event lock timed out".into())
            })?;
        let entries = self
            .index
            .get_by_entity_bounded(entity_id, MAX_ENTITY_EVENTS)?;
        for entry in entries {
            budget.check()?;
            let event = events
                .get(entry.offset)
                .filter(|event| event.id == entry.event_id && event.entity_id_str() == entity_id)
                .ok_or_else(|| {
                    AllSourceError::StorageError(
                        "Strict retained read encountered an inconsistent index".into(),
                    )
                })?;
            if event.tenant_id_str() != tenant_id {
                continue;
            }
            retain(event, &budget)?;
        }
        selected.sort_by(|left, right| {
            left.timestamp
                .timestamp_micros()
                .cmp(&right.timestamp.timestamp_micros())
                .then_with(|| left.version.cmp(&right.version))
        });
        let total = selected.len();
        let result = selected
            .into_iter()
            .take(limit)
            .map(|mut event| {
                event.timestamp = event.timestamp.trunc_subsecs(6);
                event
            })
            .collect();
        budget.check()?;
        self.entity_versions
            .entry(entity_id.to_string())
            .and_modify(|current| *current = (*current).max(version))
            .or_insert(version);
        self.version_index_entities
            .insert(entity_id.to_string(), ());
        Ok((result, total))
    }
}

struct EncodedBudget<'a> {
    remaining: usize,
    work: &'a ArchiveReadBudget,
}

impl io::Write for EncodedBudget<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.work
            .check()
            .map_err(|error| io::Error::other(error.to_string()))?;
        self.remaining = self
            .remaining
            .checked_sub(bytes.len())
            .ok_or_else(|| io::Error::other("encoded event budget exceeded"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "store_strict_read_tests.rs"]
mod tests;
