use super::EventStore;
use crate::{
    application::dto::EventDto,
    domain::entities::Event,
    error::{AllSourceError, Result},
    infrastructure::persistence::archive_budget::ArchiveReadBudget,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, OnceLock},
};
use uuid::Uuid;

pub const MODEL_EVENT: &str = "allsource.read_model.committed";
pub const FEED_BYTES: usize = 512 * 1024;
const SCAN_WINDOW: usize = 10_000;

/// `ALLSOURCE_FEED_NARROW_INDEX=0` restores the feed that hydrates the whole
/// tenant on every call.
fn narrow_feed_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("ALLSOURCE_FEED_NARROW_INDEX").map_or(true, |value| value.trim() != "0")
    })
}

#[derive(Clone)]
enum Locator {
    Memory,
    Archive { file: Arc<PathBuf>, row: u64 },
}

struct FeedEntry {
    id: Uuid,
    event_type: Arc<str>,
    entity_id: Arc<str>,
    at: Locator,
}

pub(super) struct FeedIndex {
    epoch: u64,
    events: Vec<FeedEntry>,
    ids: HashSet<Uuid>,
    /// Built from the tenant's whole archive plus memory, so it numbers every
    /// event without the tenant being resident.
    narrow: bool,
}

impl Default for FeedIndex {
    fn default() -> Self {
        Self {
            epoch: Uuid::new_v4().as_u64_pair().0 & ((1_u64 << 52) - 1),
            events: Vec::new(),
            ids: HashSet::new(),
            narrow: false,
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
    pub positions: Vec<usize>,
    pub caught_up: bool,
}

struct Candidate {
    position: usize,
    id: Uuid,
    at: Locator,
}

impl EventStore {
    pub(super) fn observe_feed_head(&self, event: &Event, _position: usize) {
        if event.event_type_str() == MODEL_EVENT {
            return;
        }
        let mut feed = self
            .feed_indexes
            .entry(event.tenant_id_str().to_owned())
            .or_default();
        if feed.ids.insert(event.id) {
            feed.events.push(FeedEntry {
                id: event.id,
                event_type: Arc::from(event.event_type_str()),
                entity_id: Arc::from(event.entity_id_str()),
                at: Locator::Memory,
            });
        }
    }

    pub(super) fn rebuild_tenant_feed(&self, tenant: &str, events: &[Event]) {
        self.feed_indexes.remove(tenant);
        for event in events
            .iter()
            .filter(|event| event.tenant_id_str() == tenant)
        {
            self.observe_feed_head(event, 0);
        }
    }

    /// Number the tenant's whole history from the archive's id, type and
    /// entity columns plus the events still only in memory. The epoch is new:
    /// positions from any earlier index of this tenant do not carry over.
    fn ensure_narrow_feed(&self, tenant: &str) -> Result<()> {
        if self
            .feed_indexes
            .get(tenant)
            .is_some_and(|feed| feed.narrow)
        {
            return Ok(());
        }
        let _resident = self.cache_residency_gate.read();
        let keys = match &self.storage {
            Some(storage) => {
                let mut budget = ArchiveReadBudget::new(self.strict_archive_limits.clone());
                storage.read().scan_feed_keys(tenant, &mut budget)?
            }
            None => Vec::new(),
        };
        // Ingest observes the feed under the events write lock, so holding it
        // makes the swap atomic with every concurrent append.
        let events = self.events.write();
        if self
            .feed_indexes
            .get(tenant)
            .is_some_and(|feed| feed.narrow)
        {
            return Ok(());
        }
        let mut names: HashMap<String, Arc<str>> = HashMap::new();
        let mut intern = |value: String| -> Arc<str> {
            names
                .entry(value)
                .or_insert_with_key(|value| Arc::from(value.as_str()))
                .clone()
        };
        let mut feed = FeedIndex {
            narrow: true,
            ..FeedIndex::default()
        };
        for key in keys {
            if key.event_type == MODEL_EVENT || !feed.ids.insert(key.id) {
                continue;
            }
            feed.events.push(FeedEntry {
                id: key.id,
                event_type: intern(key.event_type),
                entity_id: intern(key.entity_id),
                at: Locator::Archive {
                    file: key.file,
                    row: key.row,
                },
            });
        }
        for event in events.iter().filter(|event| {
            event.tenant_id_str() == tenant && event.event_type_str() != MODEL_EVENT
        }) {
            if feed.ids.insert(event.id) {
                feed.events.push(FeedEntry {
                    id: event.id,
                    event_type: intern(event.event_type_str().to_owned()),
                    entity_id: intern(event.entity_id_str().to_owned()),
                    at: Locator::Memory,
                });
            }
        }
        self.feed_indexes.insert(tenant.to_owned(), feed);
        drop(events);
        Ok(())
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
        if narrow_feed_enabled() {
            self.ensure_narrow_feed(tenant)?;
        } else {
            self.ensure_tenant_loaded_with_integrity(tenant, true)?;
            if self.storage.is_some() && !self.tenant_loader.is_complete(tenant) {
                return Err(AllSourceError::StorageError(
                    "feed archive certification was evicted; retry".into(),
                ));
            }
        }
        let _resident = self.cache_residency_gate.read();
        let (epoch, head_position, start, candidates, scan_stop) = {
            let feed = self.feed_indexes.entry(tenant.to_owned()).or_default();
            let epoch = feed.epoch;
            let head_position = feed.events.len();
            let start = cursor.unwrap_or(FeedCursor { epoch, position: 0 });
            if start.epoch != epoch || start.position > head_position {
                return Err(AllSourceError::VersionConflict {
                    expected: start.epoch,
                    current: epoch,
                });
            }
            let mut candidates = Vec::new();
            let mut position = start.position;
            if limit > 0 {
                let scan_end = head_position.min(start.position.saturating_add(SCAN_WINDOW));
                while position < scan_end {
                    let entry = &feed.events[position];
                    position += 1;
                    if entity.is_none_or(|entity| &*entry.entity_id == entity)
                        && (prefixes.is_empty()
                            || prefixes
                                .iter()
                                .any(|prefix| entry.event_type.starts_with(prefix.as_str())))
                    {
                        candidates.push(Candidate {
                            position: position - 1,
                            id: entry.id,
                            at: entry.at.clone(),
                        });
                        if candidates.len() == limit {
                            break;
                        }
                    }
                }
            }
            (epoch, head_position, start, candidates, position)
        };
        let head = FeedCursor {
            epoch,
            position: head_position,
        };
        if limit == 0 {
            return Ok(FeedPage {
                tenant_id: tenant.to_owned(),
                cursor: head,
                head,
                events: vec![],
                positions: vec![],
                caught_up: true,
            });
        }
        let events = self.feed_payloads(tenant, &candidates)?;
        let mut output = Vec::new();
        let mut positions = Vec::new();
        let mut bytes = 1024;
        let mut cursor_position = scan_stop.max(start.position);
        for (candidate, event) in candidates.iter().zip(events) {
            let dto = EventDto::from(&event);
            let size = serde_json::to_vec(&dto)?.len() + 24;
            if bytes + size > FEED_BYTES {
                if output.is_empty() {
                    return Err(AllSourceError::InvalidInput(
                        "single feed event exceeds bounded feed response".into(),
                    ));
                }
                cursor_position = candidate.position;
                break;
            }
            bytes += size;
            output.push(dto);
            positions.push(candidate.position + 1);
        }
        Ok(FeedPage {
            tenant_id: tenant.to_owned(),
            cursor: FeedCursor {
                epoch,
                position: cursor_position,
            },
            head,
            events: output,
            positions,
            caught_up: cursor_position >= head_position,
        })
    }

    /// Payloads for `candidates`, in order. If an archive file moved under the
    /// index (compaction or repack), locators are remapped from a fresh narrow
    /// scan once; positions do not change.
    fn feed_payloads(&self, tenant: &str, candidates: &[Candidate]) -> Result<Vec<Event>> {
        if let Some(events) = self.read_feed_payloads(tenant, candidates)? {
            return Ok(events);
        }
        let remapped = self.remap_feed_locators(tenant, candidates)?;
        self.read_feed_payloads(tenant, &remapped)?.ok_or_else(|| {
            self.feed_indexes.remove(tenant);
            AllSourceError::StorageError("feed archive changed; retry".into())
        })
    }

    /// `None` when a locator no longer resolves.
    fn read_feed_payloads(
        &self,
        tenant: &str,
        candidates: &[Candidate],
    ) -> Result<Option<Vec<Event>>> {
        let mut found: HashMap<Uuid, Event> = HashMap::with_capacity(candidates.len());
        let mut by_file: HashMap<Arc<PathBuf>, Vec<u64>> = HashMap::new();
        {
            let events = self.events.read();
            for candidate in candidates {
                match &candidate.at {
                    Locator::Memory => {
                        let event = self
                            .index
                            .get_by_id(&candidate.id)
                            .and_then(|offset| events.get(offset))
                            .filter(|event| event.id == candidate.id);
                        match event {
                            Some(event) => {
                                found.insert(candidate.id, event.clone());
                            }
                            None => return Ok(None),
                        }
                    }
                    Locator::Archive { file, row } => {
                        by_file.entry(Arc::clone(file)).or_default().push(*row);
                    }
                }
            }
        }
        if !by_file.is_empty() {
            let Some(storage) = &self.storage else {
                return Ok(None);
            };
            let storage = storage.read();
            let mut budget = ArchiveReadBudget::new(self.strict_archive_limits.clone());
            for (file, mut rows) in by_file {
                rows.sort_unstable();
                rows.dedup();
                match storage.read_feed_rows(&file, &rows, tenant, &mut budget) {
                    Ok(events) => {
                        for event in events {
                            found.insert(event.id, event);
                        }
                    }
                    Err(_) if !file.exists() => return Ok(None),
                    Err(error) => return Err(error),
                }
            }
        }
        let mut ordered = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            match found.remove(&candidate.id) {
                Some(event) => ordered.push(event),
                None => return Ok(None),
            }
        }
        Ok(Some(ordered))
    }

    fn remap_feed_locators(
        &self,
        tenant: &str,
        candidates: &[Candidate],
    ) -> Result<Vec<Candidate>> {
        let keys = match &self.storage {
            Some(storage) => {
                let mut budget = ArchiveReadBudget::new(self.strict_archive_limits.clone());
                storage.read().scan_feed_keys(tenant, &mut budget)?
            }
            None => Vec::new(),
        };
        let mut located: HashMap<Uuid, Locator> = HashMap::with_capacity(keys.len());
        for key in keys {
            located.entry(key.id).or_insert(Locator::Archive {
                file: key.file,
                row: key.row,
            });
        }
        if let Some(mut feed) = self.feed_indexes.get_mut(tenant) {
            for entry in &mut feed.events {
                if let Some(at) = located.get(&entry.id) {
                    entry.at = at.clone();
                }
            }
        }
        Ok(candidates
            .iter()
            .map(|candidate| Candidate {
                position: candidate.position,
                id: candidate.id,
                at: located
                    .get(&candidate.id)
                    .cloned()
                    .unwrap_or(Locator::Memory),
            })
            .collect())
    }
}
