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
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

const MAX_MODEL_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_PAGE_BYTES: usize = 128 * 1024;

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
}

impl EventStore {
    fn restore_read_model(&self, tenant: &str, entity: &str) -> Result<ReadModel> {
        self.ensure_tenant_loaded_with_integrity(tenant, true)?;
        let _resident = self.cache_residency_gate.read();
        if self.storage.is_some() && !self.tenant_loader.is_complete(tenant) {
            return Err(AllSourceError::StorageError(
                "read model archive certification was evicted; retry".into(),
            ));
        }
        let events = self.events.read();
        let mut commits: Vec<_> = events
            .iter()
            .filter(|e| {
                e.tenant_id_str() == tenant
                    && e.entity_id_str() == entity
                    && e.event_type_str() == MODEL_EVENT
            })
            .collect();
        commits.sort_by_key(|event| event.version());
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
        if serde_json::to_vec(&commit)?.len() > 240 * 1024 || commit.changes.len() > 500 {
            return Err(AllSourceError::InvalidInput(
                "read model commit exceeds 500 changes or 240 KiB".into(),
            ));
        }
        let entity = model_entity(tenant, name)?;
        let head = self.tenant_feed(tenant, None, 0)?.head;
        if commit.checkpoint.epoch != head.epoch || commit.checkpoint.position > head.position {
            return Err(AllSourceError::InvalidInput(
                "read model checkpoint is outside current feed generation".into(),
            ));
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
        if !commit.reset
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
        Ok(revision)
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
        if !(1..=500).contains(&limit) || prefix.len() > 512 || after.is_some_and(|s| s.len() > 512)
        {
            return Err(AllSourceError::InvalidInput(
                "bounded read requires limit 1..500 and keys <=512 bytes".into(),
            ));
        }
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
        let content_revision = model
            .versions
            .range(prefix.to_string()..)
            .take_while(|(key, _)| key.starts_with(prefix))
            .map(|(_, revision)| *revision)
            .max()
            .unwrap_or(0)
            .max(model.reset_revision);
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
