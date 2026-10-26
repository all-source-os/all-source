use super::EventStore;
use crate::{
    application::dto::EventDto,
    domain::entities::Event,
    error::{AllSourceError, Result},
    infrastructure::persistence::{
        archive_budget::ArchiveReadBudget,
        feed_log::{Appended, FeedLog, FeedLogRecord, FeedLogScope},
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};
use uuid::Uuid;

pub const MODEL_EVENT: &str = "allsource.read_model.committed";
pub const FEED_BYTES: usize = 512 * 1024;
const SCAN_WINDOW: usize = 10_000;
pub const DURABLE_LOG_ENV: &str = "ALLSOURCE_FEED_DURABLE_LOG";
/// Under the WAL directory.
pub const FEED_LOG_DIR: &str = "feed-log";

/// `ALLSOURCE_FEED_DURABLE_LOG` (default off, syntax in
/// [`FeedLogScope::parse`]) keeps the named tenants' feed epoch and positions
/// in a log, so a restart does not renumber them.
pub(super) fn open_feed_log(wal_dir: Option<&Path>, read_only: bool) -> Option<Arc<FeedLog>> {
    let scope = FeedLogScope::parse(std::env::var(DURABLE_LOG_ENV).ok().as_deref())?;
    let Some(wal_dir) = wal_dir.filter(|_| !read_only) else {
        tracing::warn!(
            "{DURABLE_LOG_ENV} needs a writable store with a WAL; feed positions stay per process"
        );
        return None;
    };
    let dir = wal_dir.join(FEED_LOG_DIR);
    match FeedLog::open(dir.clone(), scope) {
        Ok(log) => Some(Arc::new(log)),
        Err(error) => {
            tracing::error!(
                "Durable feed log unavailable at {}: {error}; feed positions stay per process",
                dir.display()
            );
            None
        }
    }
}

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
    /// Mirrors the tenant's [`FeedLog`] record for record: same epoch, same
    /// positions.
    durable: bool,
}

impl Default for FeedIndex {
    fn default() -> Self {
        Self {
            epoch: Uuid::new_v4().as_u64_pair().0 & ((1_u64 << 52) - 1),
            events: Vec::new(),
            ids: HashSet::new(),
            narrow: false,
            durable: false,
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

struct Scan {
    epoch: u64,
    head: usize,
    start: FeedCursor,
    candidates: Vec<Candidate>,
    stop: usize,
}

/// Positions past `visible` are not served yet.
fn scan_feed(
    feed: &FeedIndex,
    visible: usize,
    cursor: Option<FeedCursor>,
    limit: usize,
    prefixes: &[String],
    entity: Option<&str>,
) -> Result<Scan> {
    let epoch = feed.epoch;
    let head = feed.events.len().min(visible);
    let start = cursor.unwrap_or(FeedCursor { epoch, position: 0 });
    if start.epoch != epoch || start.position > head {
        return Err(AllSourceError::VersionConflict {
            expected: start.epoch,
            current: epoch,
        });
    }
    let mut candidates = Vec::new();
    let mut position = start.position;
    if limit > 0 {
        let scan_end = head.min(start.position.saturating_add(SCAN_WINDOW));
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
    Ok(Scan {
        epoch,
        head,
        start,
        candidates,
        stop: position,
    })
}

impl EventStore {
    fn durable_feed_log(&self, tenant: &str) -> Option<&FeedLog> {
        self.feed_log.as_deref().filter(|log| log.includes(tenant))
    }

    pub(super) fn observe_feed_head(&self, event: &Event, _position: usize) {
        if event.event_type_str() == MODEL_EVENT {
            return;
        }
        if let Some(log) = self.durable_feed_log(event.tenant_id_str()) {
            self.observe_durable_feed(log, event);
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

    /// Called under the events write lock, as is every durable index build,
    /// so the index and the log take records in the same order.
    fn observe_durable_feed(&self, log: &FeedLog, event: &Event) {
        let tenant = event.tenant_id_str();
        let record = FeedLogRecord::new(event.id, event.event_type_str(), event.entity_id_str());
        match log.append(tenant, record.clone()) {
            Ok(Appended::New) => {
                if let Some(mut feed) = self.feed_indexes.get_mut(tenant)
                    && feed.durable
                {
                    feed.ids.insert(record.id);
                    feed.events.push(FeedEntry {
                        id: record.id,
                        event_type: record.event_type,
                        entity_id: record.entity_id,
                        at: Locator::Memory,
                    });
                }
            }
            Ok(Appended::Duplicate | Appended::NotOpen) => {}
            Err(error) => {
                tracing::error!("Feed log append failed for event {}: {error}", event.id);
                // The next read rebuilds the index and re-appends what the log lacks.
                self.feed_indexes.remove(tenant);
            }
        }
    }

    /// Make every pending feed-log record durable. Runs before a checkpoint
    /// retires WAL segments and before boot recovery truncates the WAL, so
    /// the order the log gave recovered events is not lost with them.
    pub(super) fn sync_feed_log(&self) -> Result<()> {
        match &self.feed_log {
            Some(log) => self.publish_feed_log(log, None),
            None => Ok(()),
        }
    }

    /// The WAL is fsynced first so that no visible position outlives the
    /// event it names.
    fn publish_feed_log(&self, log: &FeedLog, tenant: Option<&str>) -> Result<()> {
        let pending = match tenant {
            Some(tenant) => log.has_pending(tenant),
            None => log.has_any_pending(),
        };
        if !pending {
            return Ok(());
        }
        if let Some(wal) = &self.wal {
            wal.sync()?;
        }
        match tenant {
            Some(tenant) => log.sync_tenant(tenant),
            None => log.sync_all(),
        }
    }

    fn ensure_durable_feed(&self, tenant: &str, log: &FeedLog) -> Result<()> {
        if !self
            .feed_indexes
            .get(tenant)
            .is_some_and(|feed| feed.durable)
        {
            self.build_durable_feed(tenant, log)?;
        }
        self.publish_feed_log(log, Some(tenant))
    }

    /// Index the tenant's log. A tenant without one is backfilled from the
    /// archive in path order plus memory, as the narrow index numbers it; a
    /// tenant with one gets any event the log lacks appended at the end, so
    /// no served position moves.
    fn build_durable_feed(&self, tenant: &str, log: &FeedLog) -> Result<()> {
        let _resident = self.cache_residency_gate.read();
        let keys = match &self.storage {
            Some(storage) => {
                let mut budget = ArchiveReadBudget::new(self.strict_archive_limits.clone());
                storage.read().scan_feed_keys(tenant, &mut budget)?
            }
            None => Vec::new(),
        };
        let events = self.events.write();
        if self
            .feed_indexes
            .get(tenant)
            .is_some_and(|feed| feed.durable)
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
        let mut located: HashMap<Uuid, Locator> = HashMap::with_capacity(keys.len());
        let mut seen = HashSet::with_capacity(keys.len());
        let mut history = Vec::new();
        for key in keys {
            if key.event_type == MODEL_EVENT {
                continue;
            }
            located.entry(key.id).or_insert(Locator::Archive {
                file: key.file,
                row: key.row,
            });
            if seen.insert(key.id) {
                history.push(FeedLogRecord {
                    id: key.id,
                    event_type: intern(key.event_type),
                    entity_id: intern(key.entity_id),
                });
            }
        }
        for event in events.iter().filter(|event| {
            event.tenant_id_str() == tenant && event.event_type_str() != MODEL_EVENT
        }) {
            if seen.insert(event.id) {
                history.push(FeedLogRecord {
                    id: event.id,
                    event_type: intern(event.event_type_str().to_owned()),
                    entity_id: intern(event.entity_id_str().to_owned()),
                });
            }
        }
        if log.is_open(tenant) {
            let mut appended = 0_usize;
            for record in history {
                if log.append(tenant, record)? == Appended::New {
                    appended += 1;
                }
            }
            if appended > 0 {
                tracing::warn!("Feed log lacked {appended} archived or resident events; appended");
            }
        } else {
            if let Some(wal) = &self.wal {
                wal.sync()?;
            }
            log.create(tenant, &history)?;
        }
        let (epoch, records) = log
            .records(tenant)
            .ok_or_else(|| AllSourceError::StorageError("feed log is not open; retry".into()))?;
        let mut feed = FeedIndex {
            epoch,
            events: Vec::with_capacity(records.len()),
            ids: HashSet::with_capacity(records.len()),
            narrow: true,
            durable: true,
        };
        for record in records {
            feed.ids.insert(record.id);
            feed.events.push(FeedEntry {
                at: located.get(&record.id).cloned().unwrap_or(Locator::Memory),
                id: record.id,
                event_type: record.event_type,
                entity_id: record.entity_id,
            });
        }
        self.feed_indexes.insert(tenant.to_owned(), feed);
        drop(events);
        Ok(())
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
        let durable = self.durable_feed_log(tenant);
        if let Some(log) = durable {
            self.ensure_durable_feed(tenant, log)?;
        } else if narrow_feed_enabled() {
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
        let Scan {
            epoch,
            head: head_position,
            start,
            candidates,
            stop: scan_stop,
        } = match durable {
            Some(log) => {
                let visible = log.durable_len(tenant);
                match self.feed_indexes.get(tenant) {
                    Some(feed) if feed.durable => {
                        scan_feed(&feed, visible, cursor, limit, prefixes, entity)
                    }
                    // A default index here would serve a random epoch.
                    _ => Err(AllSourceError::StorageError(
                        "feed index was evicted; retry".into(),
                    )),
                }
            }
            None => scan_feed(
                &self.feed_indexes.entry(tenant.to_owned()).or_default(),
                usize::MAX,
                cursor,
                limit,
                prefixes,
                entity,
            ),
        }?;
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
