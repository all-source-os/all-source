use super::EventStore;
use crate::{
    application::services::projection::Projection, domain::entities::Event, error::Result,
};
use chrono::{DateTime, Utc};
use std::{cmp::Ordering, sync::Arc};
use uuid::Uuid;

/// Where a projection's applied history ends, in backfill replay order.
///
/// Replay order is `(timestamp, version, id)`. The id breaks ties between
/// events of different entities that share a timestamp and version, so a
/// position names exactly one point in the history across restarts, even
/// though the in-memory pile order is not stable across them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct BackfillPosition {
    pub timestamp: DateTime<Utc>,
    pub version: i64,
    pub event_id: Uuid,
}

impl BackfillPosition {
    /// The position of `event`: a backfill resumed from it replays only the
    /// events ordered after it.
    pub fn of(event: &Event) -> Self {
        Self {
            timestamp: event.timestamp,
            version: event.version,
            event_id: event.id,
        }
    }

    fn cmp_event(&self, event: &Event) -> Ordering {
        self.timestamp
            .cmp(&event.timestamp)
            .then_with(|| self.version.cmp(&event.version))
            .then_with(|| self.event_id.cmp(&event.id))
    }
}

fn replay_order(a: &Event, b: &Event) -> Ordering {
    a.timestamp
        .cmp(&b.timestamp)
        .then_with(|| a.version.cmp(&b.version))
        .then_with(|| a.id.cmp(&b.id))
}

impl EventStore {
    /// Register a custom projection and replay all existing events through it.
    ///
    /// After registration, the projection will also receive all future events.
    ///
    /// Replay is ordered by `(timestamp, version, id)`. The in-memory pile can
    /// be physically out of order when Parquet is hydrated after WAL recovery —
    /// the WAL tail holds the newest events while Parquet holds the older
    /// history — so the backfill must sort before replaying. Projections with
    /// last-write-wins merge semantics produce wrong state otherwise.
    ///
    /// Ingest is held off from registration until the replay ends, so an event
    /// that arrives meanwhile is delivered once, after the history.
    pub fn register_projection_with_backfill(
        &self,
        projection: &Arc<dyn Projection>,
    ) -> Result<()> {
        self.backfill_projection(projection, None, None)
    }

    /// Like [`register_projection_with_backfill`](Self::register_projection_with_backfill),
    /// but replays only the events ordered after `after` — the position of the
    /// last event the projection applied before it was persisted. `None`
    /// replays the whole history.
    ///
    /// The caller persists the position together with the projection's state;
    /// Core does not store either.
    pub fn register_projection_with_backfill_from(
        &self,
        projection: &Arc<dyn Projection>,
        after: Option<BackfillPosition>,
    ) -> Result<()> {
        self.backfill_projection(projection, after, None)
    }

    /// Like [`register_projection_with_backfill`](Self::register_projection_with_backfill),
    /// but sorts and replays only events whose type starts with one of
    /// `event_type_prefixes`. An exact type such as `"activity.logged"` also
    /// matches `"activity.logged_v2"`. Live events are not filtered.
    pub fn register_projection_with_backfill_for(
        &self,
        projection: &Arc<dyn Projection>,
        event_type_prefixes: &[&str],
    ) -> Result<()> {
        self.backfill_projection(projection, None, Some(event_type_prefixes))
    }

    /// [`register_projection_with_backfill_from`](Self::register_projection_with_backfill_from)
    /// and [`register_projection_with_backfill_for`](Self::register_projection_with_backfill_for)
    /// combined: only matching events after `after` are replayed.
    pub fn register_projection_with_backfill_from_for(
        &self,
        projection: &Arc<dyn Projection>,
        after: Option<BackfillPosition>,
        event_type_prefixes: &[&str],
    ) -> Result<()> {
        self.backfill_projection(projection, after, Some(event_type_prefixes))
    }

    fn backfill_projection(
        &self,
        projection: &Arc<dyn Projection>,
        after: Option<BackfillPosition>,
        event_type_prefixes: Option<&[&str]>,
    ) -> Result<()> {
        // Taken before the projections lock, the order every ingest path uses.
        // Ingest needs `events.write()`, so nothing is ingested between
        // registering and finishing the replay.
        let events = self.events.read();
        self.projections.write().register(Arc::clone(projection));

        let mut ordered: Vec<&Event> = events
            .iter()
            .filter(|event| after.is_none_or(|p| p.cmp_event(event) == Ordering::Less))
            .filter(|event| {
                event_type_prefixes.is_none_or(|prefixes| {
                    let event_type = event.event_type_str();
                    prefixes.iter().any(|p| event_type.starts_with(p))
                })
            })
            .collect();
        ordered.sort_unstable_by(|a, b| replay_order(a, b));
        for event in ordered {
            projection.process(event)?;
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "store_backfill_tests.rs"]
mod tests;
