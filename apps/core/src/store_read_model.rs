use super::{
    EventStore,
    feed::{FeedCursor, MODEL_EVENT},
};
use crate::{
    domain::{
        entities::Event,
        value_objects::{EntityId, EventType, TenantId},
    },
    error::{AllSourceError, Result},
    infrastructure::persistence::archive_budget::ArchiveReadBudget,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    sync::LazyLock,
};
use tokio::sync::watch;

const MAX_MODEL_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_PAGE_BYTES: usize = 128 * 1024;

/// Commit signals for long-polling readers, keyed by store address, tenant
/// and model. Lives outside the model cache so a waiter holds no cache entry
/// and no request slot.
static WATCHERS: LazyLock<DashMap<String, watch::Sender<u64>>> = LazyLock::new(DashMap::new);

fn watch_key(store: &EventStore, tenant: &str, name: &str) -> String {
    format!("{store:p}/{tenant}/{name}")
}

/// Wakes when a commit lands on one read model.
pub struct ModelWatch {
    key: String,
    receiver: watch::Receiver<u64>,
}

impl ModelWatch {
    /// `false` when no commit can arrive any more.
    pub async fn changed(&mut self) -> bool {
        self.receiver.changed().await.is_ok()
    }
}

impl Drop for ModelWatch {
    fn drop(&mut self) {
        // The count still includes this receiver.
        WATCHERS.remove_if(&self.key, |_, sender| sender.receiver_count() <= 1);
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ModelMeta {
    pub revision: u64,
    pub checkpoint: Option<FeedCursor>,
    pub ready: bool,
    pub reset_revision: u64,
}

/// `state: None` is a deleted row whose version is retained.
#[derive(Debug, Clone, Serialize)]
pub struct ModelRow {
    pub key: String,
    pub state: Option<Value>,
    pub version: u64,
}

#[derive(Debug)]
pub struct RowsPage {
    pub meta: ModelMeta,
    pub content_revision: u64,
    pub rows: Vec<ModelRow>,
    pub next: Option<String>,
}

#[derive(Debug)]
pub struct ChangesPage {
    pub meta: ModelMeta,
    pub changes: Vec<ModelRow>,
    pub revision: u64,
    pub caught_up: bool,
}

#[derive(Debug)]
pub enum ModelChanges {
    /// The cursor predates the last reset; the reader must re-read the rows.
    Reset {
        reset_revision: u64,
    },
    Page(ChangesPage),
}

#[derive(Debug, Default, Clone)]
pub struct ReadModel {
    pub revision: u64,
    pub checkpoint: Option<FeedCursor>,
    pub ready: bool,
    pub rows: BTreeMap<String, Value>,
    bytes: usize,
    versions: BTreeMap<String, u64>,
    reset_revision: u64,
    accessed: Option<std::time::Instant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCommit {
    pub expected_revision: u64,
    pub checkpoint: FeedCursor,
    pub ready: bool,
    #[serde(default)]
    pub reset: bool,
    pub changes: BTreeMap<String, Option<Value>>,
}

#[derive(Debug, Serialize)]
pub struct ModelPage {
    pub revision: u64,
    pub content_revision: u64,
    pub checkpoint: Option<FeedCursor>,
    pub ready: bool,
    pub unchanged: bool,
    pub rows: BTreeMap<String, Value>,
    pub next: Option<String>,
}

fn model_entity(tenant: &str, name: &str) -> Result<String> {
    TenantId::new(tenant.into())?;
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(AllSourceError::InvalidInput(
            "read model name must contain 1..64 ASCII letters, digits, - or _".into(),
        ));
    }
    // Core's OCC marks use entity IDs; tenant identity must participate.
    let mut identity = Sha256::new();
    identity.update(tenant.as_bytes());
    identity.update([0]);
    identity.update(name.as_bytes());
    Ok(format!("read-model:{:x}", identity.finalize()))
}

fn evict_model(models: &mut HashMap<String, ReadModel>, keep: &str) {
    let victim = models
        .iter()
        .filter(|(key, _)| key.as_str() != keep)
        .min_by_key(|(_, model)| model.accessed)
        .map(|(key, _)| key.clone());
    if let Some(key) = victim {
        models.remove(&key);
    }
}

fn bound_cache(models: &mut HashMap<String, ReadModel>, keep: &str) {
    while models.len() > 1
        && (models.len() > 32
            || models.values().map(|model| model.bytes).sum::<usize>() > 64 * 1024 * 1024)
    {
        evict_model(models, keep);
    }
}

impl ReadModel {
    fn apply(&mut self, commit: &ModelCommit, revision: u64) -> Result<()> {
        if commit.reset {
            self.rows.clear();
            self.versions.clear();
            self.bytes = 0;
            self.reset_revision = revision;
        }
        for (key, value) in &commit.changes {
            if !self.versions.contains_key(key) {
                self.bytes += key.len() + 128;
            }
            self.versions.insert(key.clone(), revision);
            if key.len() > 512 {
                return Err(AllSourceError::InvalidInput(
                    "read model key exceeds 512 bytes".into(),
                ));
            }
            if let Some(old) = self.rows.remove(key) {
                self.bytes -= key.len() + serde_json::to_vec(&old)?.len();
            }
            if let Some(value) = value {
                let size = serde_json::to_vec(value)?.len();
                if size + key.len() > MAX_PAGE_BYTES - 4096 {
                    return Err(AllSourceError::InvalidInput(
                        "read model row exceeds bounded page".into(),
                    ));
                }
                self.bytes += key.len() + size;
                self.rows.insert(key.clone(), value.clone());
            }
        }
        if self.bytes > MAX_MODEL_BYTES || self.versions.len() > 100_000 {
            return Err(AllSourceError::InvalidInput(
                "read model exceeds 32 MiB or 100000 keys; shard model".into(),
            ));
        }
        self.revision = revision;
        self.checkpoint = Some(commit.checkpoint);
        self.ready = commit.ready;
        self.accessed = Some(std::time::Instant::now());
        Ok(())
    }

    fn meta(&self) -> ModelMeta {
        ModelMeta {
            revision: self.revision,
            checkpoint: self.checkpoint,
            ready: self.ready,
            reset_revision: self.reset_revision,
        }
    }

    fn content_revision(&self, prefix: &str) -> u64 {
        self.versions
            .range(prefix.to_string()..)
            .take_while(|(key, _)| key.starts_with(prefix))
            .map(|(_, revision)| *revision)
            .max()
            .unwrap_or(0)
            .max(self.reset_revision)
    }
}

fn check_bounds(limit: usize, prefix: &str, after: Option<&str>) -> Result<()> {
    if !(1..=500).contains(&limit) || prefix.len() > 512 || after.is_some_and(|s| s.len() > 512) {
        return Err(AllSourceError::InvalidInput(
            "bounded read requires limit 1..500 and keys <=512 bytes".into(),
        ));
    }
    Ok(())
}

fn row_bytes(key: &str, state: Option<&Value>) -> Result<usize> {
    Ok(serde_json::to_vec(key)?.len()
        + state.map_or(Ok(4), |state| {
            serde_json::to_vec(state).map(|bytes| bytes.len())
        })?
        + 32)
}

impl EventStore {
    /// The model's journal is one entity, so only that entity's archived rows
    /// are decoded; the rest of the tenant's history never counts against the
    /// strict budget.
    fn read_model_journal(&self, tenant: &str, entity: &str) -> Result<Vec<Event>> {
        let mut budget = ArchiveReadBudget::new(self.strict_archive_limits.clone());
        // Held across both reads so eviction cannot drop a flushed commit
        // from memory between the archive scan and the memory scan.
        let _resident = self.cache_residency_gate.read();
        let mut journal = HashMap::new();
        if let Some(storage) = &self.storage {
            storage
                .read()
                .visit_retained_entity(tenant, entity, &mut budget, |event, work| {
                    work.check()?;
                    if event.event_type_str() == MODEL_EVENT {
                        journal.insert(event.id, event.clone());
                    }
                    Ok(())
                })?;
        }
        let events = self.events.read();
        for event in events.iter().filter(|e| {
            e.tenant_id_str() == tenant
                && e.entity_id_str() == entity
                && e.event_type_str() == MODEL_EVENT
        }) {
            journal.entry(event.id).or_insert_with(|| event.clone());
        }
        let mut commits: Vec<_> = journal.into_values().collect();
        commits.sort_by_key(Event::version);
        Ok(commits)
    }

    fn restore_read_model(&self, tenant: &str, entity: &str) -> Result<ReadModel> {
        let commits = self.read_model_journal(tenant, entity)?;
        let mut model = ReadModel::default();
        for event in commits {
            let commit: ModelCommit = serde_json::from_value(event.payload().clone())?;
            if commit.expected_revision != model.revision {
                return Err(AllSourceError::StorageError(
                    "read model journal is incomplete".into(),
                ));
            }
            model.apply(&commit, model.revision + 1)?;
        }
        Ok(model)
    }

    pub fn commit_read_model(&self, tenant: &str, name: &str, commit: ModelCommit) -> Result<u64> {
        self.commit_model(tenant, name, commit, true)
    }

    /// For models Core writes itself (view definitions and their index). Their
    /// checkpoint records when they were written, not how far a fold has read,
    /// so it is neither bound to the current feed epoch nor required to advance.
    pub(super) fn commit_core_read_model(
        &self,
        tenant: &str,
        name: &str,
        commit: ModelCommit,
    ) -> Result<u64> {
        self.commit_model(tenant, name, commit, false)
    }

    fn commit_model(
        &self,
        tenant: &str,
        name: &str,
        commit: ModelCommit,
        fold_checkpoint: bool,
    ) -> Result<u64> {
        if serde_json::to_vec(&commit)?.len() > 240 * 1024 || commit.changes.len() > 500 {
            return Err(AllSourceError::InvalidInput(
                "read model commit exceeds 500 changes or 240 KiB".into(),
            ));
        }
        let entity = model_entity(tenant, name)?;
        if fold_checkpoint {
            let head = self.tenant_feed(tenant, None, 0)?.head;
            if commit.checkpoint.epoch != head.epoch || commit.checkpoint.position > head.position {
                return Err(AllSourceError::InvalidInput(
                    "read model checkpoint is outside current feed generation".into(),
                ));
            }
        }
        let key = format!("{tenant}/{name}");
        let mut models = self.read_models.lock();
        if !models.contains_key(&key) {
            if models.len() >= 32 {
                evict_model(&mut models, &key);
            }
            models.insert(key.clone(), self.restore_read_model(tenant, &entity)?);
            bound_cache(&mut models, &key);
        }
        let model = models.get(&key).expect("inserted read model");
        if model.revision != commit.expected_revision {
            return Err(AllSourceError::VersionConflict {
                expected: commit.expected_revision,
                current: model.revision,
            });
        }
        if fold_checkpoint
            && !commit.reset
            && model.checkpoint.is_some_and(|old| {
                old.epoch != commit.checkpoint.epoch || old.position > commit.checkpoint.position
            })
        {
            return Err(AllSourceError::InvalidInput(
                "read model checkpoint cannot regress; rebuild with reset".into(),
            ));
        }
        let mut next = model.clone();
        let revision = model.revision + 1;
        next.apply(&commit, revision)?;
        let event = Event::new(
            EventType::new(MODEL_EVENT.into())?,
            EntityId::new(entity)?,
            TenantId::new(tenant.into())?,
            serde_json::to_value(commit)?,
        );
        self.ingest_with_expected_version(&event, Some(model.revision))?;
        models.insert(key, next);
        bound_cache(&mut models, &format!("{tenant}/{name}"));
        drop(models);
        if let Some(sender) = WATCHERS.get(&watch_key(self, tenant, name)) {
            sender.send_replace(revision);
        }
        Ok(revision)
    }

    /// Subscribe before reading, so a commit landing between the read and the
    /// wait still wakes the waiter.
    pub fn read_model_watch(&self, tenant: &str, name: &str) -> ModelWatch {
        let key = watch_key(self, tenant, name);
        let receiver = WATCHERS
            .entry(key.clone())
            .or_insert_with(|| watch::channel(0).0)
            .subscribe();
        ModelWatch { key, receiver }
    }

    fn with_model<R>(
        &self,
        tenant: &str,
        name: &str,
        read: impl FnOnce(&ReadModel) -> Result<R>,
    ) -> Result<R> {
        let entity = model_entity(tenant, name)?;
        let key = format!("{tenant}/{name}");
        let mut models = self.read_models.lock();
        if !models.contains_key(&key) {
            if models.len() >= 32 {
                evict_model(&mut models, &key);
            }
            models.insert(key.clone(), self.restore_read_model(tenant, &entity)?);
            bound_cache(&mut models, &key);
        }
        let model = models.get_mut(&key).expect("inserted read model");
        model.accessed = Some(std::time::Instant::now());
        read(model)
    }

    pub fn read_model_meta(&self, tenant: &str, name: &str) -> Result<ModelMeta> {
        self.with_model(tenant, name, |model| Ok(model.meta()))
    }

    /// One row and the revision that last wrote it.
    pub fn read_model_row(
        &self,
        tenant: &str,
        name: &str,
        key: &str,
    ) -> Result<(ModelMeta, Option<(Value, u64)>)> {
        if key.len() > 512 {
            return Err(AllSourceError::InvalidInput(
                "read model key exceeds 512 bytes".into(),
            ));
        }
        self.with_model(tenant, name, |model| {
            let row = model.rows.get(key).map(|state| {
                let version = model.versions.get(key).copied().unwrap_or(model.revision);
                (state.clone(), version)
            });
            Ok((model.meta(), row))
        })
    }

    /// Like [`Self::read_model_page`], with each row's version.
    pub fn read_model_rows(
        &self,
        tenant: &str,
        name: &str,
        prefix: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<RowsPage> {
        check_bounds(limit, prefix, after)?;
        self.with_model(tenant, name, |model| {
            let mut page = RowsPage {
                meta: model.meta(),
                content_revision: model.content_revision(prefix),
                rows: Vec::new(),
                next: None,
            };
            let mut bytes = 4096;
            for (key, value) in model.rows.range(prefix.to_string()..) {
                if !key.starts_with(prefix) {
                    break;
                }
                if after.is_some_and(|after| key.as_str() <= after) {
                    continue;
                }
                let size = row_bytes(key, Some(value))?;
                if page.rows.len() == limit || bytes + size > MAX_PAGE_BYTES {
                    page.next = page.rows.last().map(|row| row.key.clone());
                    break;
                }
                bytes += size;
                page.rows.push(ModelRow {
                    key: key.clone(),
                    state: Some(value.clone()),
                    version: model.versions.get(key).copied().unwrap_or(model.revision),
                });
            }
            Ok(page)
        })
    }

    /// Rows written after revision `after`, ordered by version then key.
    /// Deleted rows keep their version and come back with `state: None`.
    /// One commit is never split across pages, so a page may exceed `limit`
    /// or the page size by the rows of a single commit (at most 500 rows and
    /// 240 KiB).
    pub fn read_model_changes(
        &self,
        tenant: &str,
        name: &str,
        after: u64,
        limit: usize,
    ) -> Result<ModelChanges> {
        check_bounds(limit, "", None)?;
        self.with_model(tenant, name, |model| {
            if after < model.reset_revision {
                return Ok(ModelChanges::Reset {
                    reset_revision: model.reset_revision,
                });
            }
            let mut changed: Vec<(u64, &str)> = model
                .versions
                .iter()
                .filter(|(_, version)| **version > after)
                .map(|(key, version)| (*version, key.as_str()))
                .collect();
            changed.sort_unstable();
            let mut changes = Vec::new();
            let mut bytes = 4096;
            let mut caught_up = true;
            let mut start = 0;
            while start < changed.len() {
                let version = changed[start].0;
                let end = changed[start..]
                    .iter()
                    .position(|(other, _)| *other != version)
                    .map_or(changed.len(), |offset| start + offset);
                let mut group = Vec::with_capacity(end - start);
                let mut group_bytes = 0;
                for (_, key) in &changed[start..end] {
                    let state = model.rows.get(*key).cloned();
                    group_bytes += row_bytes(key, state.as_ref())?;
                    group.push(ModelRow {
                        key: (*key).to_owned(),
                        state,
                        version,
                    });
                }
                if !changes.is_empty()
                    && (changes.len() + group.len() > limit || bytes + group_bytes > MAX_PAGE_BYTES)
                {
                    caught_up = false;
                    break;
                }
                bytes += group_bytes;
                changes.extend(group);
                start = end;
            }
            let revision = if caught_up {
                model.revision
            } else {
                changes.last().map_or(after, |row: &ModelRow| row.version)
            };
            Ok(ModelChanges::Page(ChangesPage {
                meta: model.meta(),
                changes,
                revision,
                caught_up,
            }))
        })
    }

    pub fn read_model_page(
        &self,
        tenant: &str,
        name: &str,
        prefix: &str,
        after: Option<&str>,
        limit: usize,
        revision: Option<u64>,
    ) -> Result<ModelPage> {
        check_bounds(limit, prefix, after)?;
        self.with_model(tenant, name, |model| {
            Self::model_page(model, prefix, after, limit, revision)
        })
    }

    fn model_page(
        model: &ReadModel,
        prefix: &str,
        after: Option<&str>,
        limit: usize,
        revision: Option<u64>,
    ) -> Result<ModelPage> {
        let content_revision = model.content_revision(prefix);
        let unchanged = revision == Some(content_revision);
        let mut page = ModelPage {
            revision: model.revision,
            content_revision,
            checkpoint: model.checkpoint,
            ready: model.ready,
            unchanged,
            rows: BTreeMap::new(),
            next: None,
        };
        if unchanged {
            return Ok(page);
        }
        let mut bytes = 4096;
        for (key, value) in model.rows.range(prefix.to_string()..) {
            if !key.starts_with(prefix) {
                break;
            }
            if after.is_some_and(|after| key.as_str() <= after) {
                continue;
            }
            let size = serde_json::to_vec(key)?.len() + serde_json::to_vec(value)?.len() + 8;
            if page.rows.len() == limit || bytes + size > MAX_PAGE_BYTES {
                page.next = page.rows.keys().next_back().cloned();
                break;
            }
            bytes += size;
            page.rows.insert(key.clone(), value.clone());
        }
        Ok(page)
    }
}
