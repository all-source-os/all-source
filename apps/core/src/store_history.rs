use super::{EventStore, strict_read::EncodedBudget};
use crate::{
    application::dto::EventDto,
    domain::{
        entities::Event,
        value_objects::{EntityId, TenantId},
    },
    error::{AllSourceError, Result},
    infrastructure::persistence::archive_budget::ArchiveReadBudget,
};
use chrono::{DateTime, Utc};
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicBool},
};
use uuid::Uuid;

pub(crate) enum HistorySelection {
    Timeline {
        entity_id: String,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    },
    Evidence(Uuid),
}

pub(crate) struct HistoryPage {
    pub read_point: DateTime<Utc>,
    pub certified: bool,
    pub truncated: bool,
    pub events: Vec<EventDto>,
}

impl EventStore {
    pub(crate) fn retained_history(
        &self,
        tenant: &str,
        selection: &HistorySelection,
        cancel: Arc<AtomicBool>,
    ) -> Result<HistoryPage> {
        TenantId::new(tenant.to_owned())?;
        if let HistorySelection::Timeline {
            entity_id,
            since,
            until,
        } = selection
        {
            EntityId::new(entity_id.clone())?;
            if until <= since || *until - *since > chrono::Duration::days(31) {
                return Err(AllSourceError::InvalidInput(
                    "Invalid history interval".into(),
                ));
            }
        }
        if self.read_only || !self.history_recovery_healthy {
            return Err(AllSourceError::StorageError(
                "History requires fully recovered primary storage".into(),
            ));
        }
        let mut budget = ArchiveReadBudget::new(self.strict_archive_limits.clone())
            .with_cancellation(Some(cancel));
        let busy = || AllSourceError::StorageError("History snapshot lock unavailable".into());
        let _residency = self
            .cache_residency_gate
            .try_read_for(budget.remaining()?)
            .ok_or_else(busy)?;
        // Ingest appends to Parquet while holding this write lock. Keep both
        // halves of the snapshot on the same side of every append.
        let resident = self
            .events
            .try_read_for(budget.remaining()?)
            .ok_or_else(busy)?;
        let read_point = Utc::now();
        let mut newest: BTreeMap<(DateTime<Utc>, Uuid), EventDto> = BTreeMap::new();
        let mut truncated = false;
        let mut precision_known = true;
        let mut uncertain_ids = std::collections::HashSet::new();
        let mut collect = |event: &Event, work: &ArchiveReadBudget, exact_time: bool| {
            work.check()?;
            let timestamp = event.timestamp;
            if !exact_time && let HistorySelection::Timeline { since, until, .. } = selection {
                // A legacy row can lie on either side of a cut within its stored microsecond.
                let crosses = [since, until].iter().any(|cut| {
                    cut.timestamp_micros() == timestamp.timestamp_micros()
                        && cut.timestamp_subsec_nanos() % 1000 != 0
                });
                if crosses {
                    uncertain_ids.insert(event.id);
                    precision_known = false;
                    return Ok(());
                }
            }
            if uncertain_ids.contains(&event.id) {
                return Ok(());
            }
            if event.tenant_id_str() != tenant
                || !match selection {
                    HistorySelection::Evidence(id) => event.id == *id,
                    HistorySelection::Timeline {
                        entity_id,
                        since,
                        until,
                    } => {
                        event.entity_id_str() == entity_id
                            && timestamp >= *since
                            && timestamp < *until
                    }
                }
            {
                return Ok(());
            }
            precision_known &= exact_time;
            let key = (timestamp, event.id);
            if let Some(old) = newest.keys().find(|(_, id)| *id == event.id).copied() {
                if old == key || !exact_time {
                    return Ok(());
                }
                newest.remove(&old);
            }
            if newest.len() == 21 {
                truncated = true;
                if newest
                    .first_key_value()
                    .is_some_and(|(oldest, _)| key < *oldest)
                {
                    return Ok(());
                }
                newest.pop_first();
            }
            let mut encoded = EncodedBudget {
                remaining: 2 * 1024 * 1024 - 8192,
                work,
            };
            serde_json::to_writer(&mut encoded, event).map_err(|_| {
                AllSourceError::StorageError("History event exceeds response budget".into())
            })?;
            newest.insert(key, EventDto::from(event));
            Ok(())
        };
        let certified = if let Some(storage) = &self.storage {
            storage
                .try_read_for(budget.remaining()?)
                .ok_or_else(busy)?
                .visit_event_history(tenant, selection, &mut budget, &mut collect)?
        } else {
            true
        };
        for event in resident.iter() {
            collect(event, &budget, true)?;
        }
        let events: Vec<EventDto> = newest.into_values().rev().collect();
        let mut encoded = EncodedBudget {
            remaining: 2 * 1024 * 1024 - 8192,
            work: &budget,
        };
        serde_json::to_writer(&mut encoded, &events).map_err(|_| {
            AllSourceError::StorageError("History page exceeds response budget".into())
        })?;
        budget.check()?;
        Ok(HistoryPage {
            read_point,
            certified: certified && precision_known,
            truncated,
            events,
        })
    }
}
