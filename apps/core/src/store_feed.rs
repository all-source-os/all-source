use super::EventStore;
use crate::{
    application::dto::EventDto,
    error::{AllSourceError, Result},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MODEL_EVENT: &str = "allsource.read_model.committed";
pub const FEED_BYTES: usize = 512 * 1024;

pub(super) struct FeedIndex {
    epoch: u64,
    events: Vec<Uuid>,
}

impl Default for FeedIndex {
    fn default() -> Self {
        Self {
            epoch: Uuid::new_v4().as_u64_pair().0 & ((1_u64 << 52) - 1),
            events: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedCursor {
    pub epoch: u64,
    pub position: usize,
}

#[derive(Debug, Serialize)]
pub struct FeedPage {
    pub tenant_id: String,
    pub cursor: FeedCursor,
    pub head: FeedCursor,
    pub events: Vec<EventDto>,
    pub caught_up: bool,
}

impl EventStore {
    pub(super) fn observe_feed_head(
        &self,
        event: &crate::domain::entities::Event,
        _position: usize,
    ) {
        if event.event_type_str() != MODEL_EVENT {
            self.feed_indexes
                .entry(event.tenant_id_str().to_owned())
                .or_default()
                .events
                .push(event.id);
        }
    }

    pub(super) fn rebuild_tenant_feed(
        &self,
        tenant: &str,
        events: &[crate::domain::entities::Event],
    ) {
        self.feed_indexes.remove(tenant);
        for (position, event) in events
            .iter()
            .enumerate()
            .filter(|(_, event)| event.tenant_id_str() == tenant)
        {
            self.observe_feed_head(event, position + 1);
        }
    }

    /// Epoch changes force a rebuild instead of silently skipping reordered history.
    pub fn tenant_feed(
        &self,
        tenant: &str,
        cursor: Option<FeedCursor>,
        limit: usize,
    ) -> Result<FeedPage> {
        self.tenant_feed_filtered(tenant, cursor, limit, &[])
    }

    pub fn tenant_feed_filtered(
        &self,
        tenant: &str,
        cursor: Option<FeedCursor>,
        limit: usize,
        prefixes: &[String],
    ) -> Result<FeedPage> {
        self.tenant_feed_scoped(tenant, cursor, limit, prefixes, None)
    }

    pub fn tenant_feed_scoped(
        &self,
        tenant: &str,
        cursor: Option<FeedCursor>,
        limit: usize,
        prefixes: &[String],
        entity: Option<&str>,
    ) -> Result<FeedPage> {
        if limit > 500 {
            return Err(AllSourceError::InvalidInput(
                "feed limit must be 0..500".into(),
            ));
        }
        self.ensure_tenant_loaded_with_integrity(tenant, true)?;
        let _resident = self.cache_residency_gate.read();
        if self.storage.is_some() && !self.tenant_loader.is_complete(tenant) {
            return Err(AllSourceError::StorageError(
                "feed archive certification was evicted; retry".into(),
            ));
        }
        let events = self.events.read();
        let feed = self.feed_indexes.entry(tenant.to_owned()).or_default();
        let epoch = feed.epoch;
        let head_position = feed.events.len();
        let head = FeedCursor {
            epoch,
            position: head_position,
        };
        let start = cursor.unwrap_or(FeedCursor { epoch, position: 0 });
        if start.epoch != epoch || start.position > head_position {
            return Err(AllSourceError::VersionConflict {
                expected: start.epoch,
                current: epoch,
            });
        }
        if limit == 0 {
            return Ok(FeedPage {
                tenant_id: tenant.to_owned(),
                cursor: head,
                head,
                events: vec![],
                caught_up: true,
            });
        }
        let mut output = Vec::new();
        let mut bytes = 1024;
        let mut position = start.position;
        let scan_end = head_position.min(start.position.saturating_add(10_000));
        while position < scan_end {
            let event_id = feed.events[position];
            let event = self
                .index
                .get_by_id(&event_id)
                .and_then(|offset| events.get(offset))
                .filter(|event| event.id == event_id)
                .ok_or_else(|| {
                    AllSourceError::StorageError(
                        "feed index is inconsistent; rebuild required".into(),
                    )
                })?;
            if event.tenant_id_str() == tenant
                && event.event_type_str() != MODEL_EVENT
                && entity.is_none_or(|entity| event.entity_id_str() == entity)
                && (prefixes.is_empty()
                    || prefixes
                        .iter()
                        .any(|prefix| event.event_type_str().starts_with(prefix)))
            {
                let dto = EventDto::from(event);
                let size = serde_json::to_vec(&dto)?.len();
                if bytes + size > FEED_BYTES {
                    if output.is_empty() {
                        return Err(AllSourceError::InvalidInput(
                            "single feed event exceeds bounded feed response".into(),
                        ));
                    }
                    break;
                }
                bytes += size;
                output.push(dto);
            }
            position += 1;
            if output.len() == limit {
                break;
            }
        }
        Ok(FeedPage {
            tenant_id: tenant.to_owned(),
            cursor: FeedCursor { epoch, position },
            head,
            events: output,
            caught_up: position >= head_position,
        })
    }
}
