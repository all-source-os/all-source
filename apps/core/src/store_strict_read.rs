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
    io,
    sync::{Arc, atomic::AtomicBool},
};

const MAX_ENTITY_EVENTS: usize = 1_001;
// Leave room inside the consumer's 2 MiB wire cap for the response envelope.
const MAX_ENCODED_EVENTS: usize = 2 * 1024 * 1024 - 16 * 1024;

impl EventStore {
    /// Read one complete retained entity snapshot under an explicit read scope.
    /// Archive errors, oversized input and an evicted certification refuse the
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
        let budget = ArchiveReadBudget::new(self.strict_archive_limits.clone())
            .with_cancellation(cancellation.cloned());
        budget.check()?;
        self.ensure_tenant_loaded_budgeted(tenant_id, true, cancellation.cloned())?;
        let _resident = self
            .cache_residency_gate
            .try_read_for(budget.remaining()?)
            .ok_or_else(|| {
                AllSourceError::StorageError("Strict retained read residency lock timed out".into())
            })?;
        if !self.tenant_loader.is_complete(tenant_id) {
            return Err(AllSourceError::StorageError(
                "Verified archive was evicted before the retained read".into(),
            ));
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
        let mut selected = Vec::with_capacity(entries.len());
        let mut encoded = EncodedBudget {
            remaining: MAX_ENCODED_EVENTS,
            work: &budget,
        };
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
            // Count serialized bytes without allocating another payload-sized
            // buffer. Refuse before cloning any selected events.
            serde_json::to_writer(&mut encoded, event).map_err(|_| {
                AllSourceError::StorageError(
                    "Strict retained read encoded data budget exceeded".into(),
                )
            })?;
            selected.push(event);
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
            .cloned()
            .map(|mut event| {
                event.timestamp = event.timestamp.trunc_subsecs(6);
                event
            })
            .collect();
        budget.check()?;
        self.tenant_loader.touch(tenant_id);
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
mod tests {
    use super::*;
    use crate::store::EventStoreConfig;
    use std::{sync::atomic::Ordering, time::Duration};

    #[tokio::test(flavor = "current_thread")]
    async fn snapshot_lease_blocks_eviction_and_observes_cancellation_before_return() {
        for cancel in [false, true] {
            let directory = tempfile::TempDir::new().unwrap();
            let store = Arc::new(EventStore::with_config(EventStoreConfig::with_persistence(
                directory.path(),
            )));
            let event = Event::from_strings(
                "synthetic.updated".into(),
                "run".into(),
                "synthetic".into(),
                serde_json::json!({}),
                None,
            )
            .unwrap();
            store.ingest_with_expected_version(&event, Some(0)).unwrap();
            store.flush_storage().unwrap();
            // This test is about the residency lease, so the cache must be
            // warm before the reader starts.
            store
                .ensure_tenant_loaded_with_integrity("synthetic", true)
                .unwrap();
            let cancellation = Arc::new(AtomicBool::new(false));
            // Hold only the materialization lock: the reader can first acquire
            // its residency lease, then wait here with its original deadline.
            let lock_store = Arc::clone(&store);
            let (held_tx, held_rx) = tokio::sync::oneshot::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let holder = std::thread::spawn(move || {
                let _events = lock_store.events.write();
                held_tx.send(()).unwrap();
                // test-hang-allow: bounded synthetic contention, including test failures.
                release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            });
            held_rx.await.unwrap();
            let read_store = Arc::clone(&store);
            let flag = Arc::clone(&cancellation);
            let reader = tokio::task::spawn_blocking(move || {
                read_store.query_retained_entity_cancellable(
                    "synthetic",
                    "run",
                    1001,
                    &ReadScope::unrestricted(),
                    Some(&flag),
                )
            });
            // test-hang-allow: bounded observation of the real reader pinning cache residency.
            tokio::time::timeout(Duration::from_secs(1), async {
                while store.cache_residency_gate.try_write().is_some() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let evict_store = Arc::clone(&store);
            let (eviction_tx, eviction_rx) = tokio::sync::oneshot::channel();
            let eviction = tokio::task::spawn_blocking(move || {
                eviction_tx.send(()).unwrap();
                evict_store.evict_tenant("synthetic");
            });
            eviction_rx.await.unwrap();
            if cancel {
                cancellation.store(true, Ordering::Release);
            }
            assert!(!eviction.is_finished());
            release_tx.send(()).unwrap();
            let result = reader.await.unwrap();
            eviction.await.unwrap();
            holder.join().unwrap();
            if cancel {
                assert!(result.unwrap_err().to_string().contains("cancelled"));
            } else {
                let (snapshot, total) = result.unwrap();
                assert_eq!(total, 1);
                assert_eq!(snapshot[0].id, event.id);
            }
            assert_eq!(store.total_events(), 0);
        }
    }
}
