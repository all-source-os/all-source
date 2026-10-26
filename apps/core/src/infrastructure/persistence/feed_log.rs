//! Durable per-tenant feed log. One file per tenant: a JSON header line with
//! the tenant and its feed epoch, then one JSON line per event id in the order
//! the feed numbers them. A position is the ordinal among first occurrences,
//! so an append never moves an earlier one. Appends are buffered in memory and
//! become visible ([`FeedLog::durable_len`]) only once written and fsynced; a
//! crash drops exactly what was never synced, and so never served.
use crate::error::{AllSourceError, Result};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
use uuid::Uuid;

const FORMAT: &str = "allsource.feed_log";
const VERSION: u32 = 1;
const EXTENSION: &str = "feed";
const TEMP_SUFFIX: &str = ".feed.tmp";
const CORRUPT_EXTENSION: &str = "feed.corrupt";

fn storage_error(context: &str, path: &Path, error: impl std::fmt::Display) -> AllSourceError {
    AllSourceError::StorageError(format!("{context} {}: {error}", path.display()))
}

/// Which tenants keep a durable feed log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedLogScope {
    All,
    Tenants(HashSet<String>),
}

impl FeedLogScope {
    /// Off (`None`) for unset, empty, `0`, `false` or `off`; every tenant for
    /// `1`, `true`, `on`, `*` or `all`; otherwise a comma-separated allowlist.
    pub fn parse(value: Option<&str>) -> Option<Self> {
        let value = value?.trim();
        match value.to_ascii_lowercase().as_str() {
            "" | "0" | "false" | "off" => None,
            "1" | "true" | "on" | "*" | "all" => Some(Self::All),
            _ => {
                let tenants: HashSet<String> = value
                    .split(',')
                    .map(str::trim)
                    .filter(|tenant| !tenant.is_empty())
                    .map(str::to_owned)
                    .collect();
                (!tenants.is_empty()).then_some(Self::Tenants(tenants))
            }
        }
    }

    pub fn includes(&self, tenant: &str) -> bool {
        match self {
            Self::All => true,
            Self::Tenants(tenants) => tenants.contains(tenant),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedLogRecord {
    pub id: Uuid,
    pub event_type: Arc<str>,
    pub entity_id: Arc<str>,
}

impl FeedLogRecord {
    pub fn new(id: Uuid, event_type: &str, entity_id: &str) -> Self {
        Self {
            id,
            event_type: Arc::from(event_type),
            entity_id: Arc::from(entity_id),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appended {
    /// The tenant has no log yet; its first feed read backfills one.
    NotOpen,
    Duplicate,
    New,
}

#[derive(Serialize, Deserialize)]
struct Header {
    format: String,
    version: u32,
    tenant: String,
    epoch: u64,
}

#[derive(Serialize)]
struct LineOut<'a> {
    id: Uuid,
    #[serde(rename = "type")]
    event_type: &'a str,
    entity: &'a str,
}

#[derive(Deserialize)]
struct LineIn {
    id: Uuid,
    #[serde(rename = "type")]
    event_type: String,
    entity: String,
}

fn encode_line(buffer: &mut Vec<u8>, record: &FeedLogRecord) -> Result<()> {
    serde_json::to_writer(
        &mut *buffer,
        &LineOut {
            id: record.id,
            event_type: &record.event_type,
            entity: &record.entity_id,
        },
    )?;
    buffer.push(b'\n');
    Ok(())
}

fn file_stem(tenant: &str) -> String {
    let digest = hex::encode(Sha256::digest(tenant.as_bytes()));
    digest[..32].to_owned()
}

#[cfg(unix)]
fn sync_dir(dir: &Path) -> Result<()> {
    File::open(dir)
        .and_then(|handle| handle.sync_all())
        .map_err(|error| storage_error("Failed to sync feed log directory", dir, error))
}

#[cfg(not(unix))]
fn sync_dir(_dir: &Path) -> Result<()> {
    Ok(())
}

struct TenantLog {
    path: PathBuf,
    file: File,
    epoch: u64,
    records: Vec<FeedLogRecord>,
    ids: HashSet<Uuid>,
    durable: usize,
    durable_bytes: u64,
    pending: Vec<u8>,
}

impl TenantLog {
    fn push(&mut self, record: FeedLogRecord) -> Result<Appended> {
        if self.ids.contains(&record.id) {
            return Ok(Appended::Duplicate);
        }
        let mark = self.pending.len();
        if let Err(error) = encode_line(&mut self.pending, &record) {
            self.pending.truncate(mark);
            return Err(error);
        }
        self.ids.insert(record.id);
        self.records.push(record);
        Ok(Appended::New)
    }

    fn sync(&mut self) -> Result<()> {
        if self.durable == self.records.len() {
            return Ok(());
        }
        let written = self
            .file
            .write_all(&self.pending)
            .and_then(|()| self.file.sync_all());
        if let Err(error) = written {
            // A partial write must not sit in front of the retry's copy of the same lines.
            let _ = self.file.set_len(self.durable_bytes);
            return Err(storage_error("Failed to sync feed log", &self.path, error));
        }
        self.durable_bytes += self.pending.len() as u64;
        self.pending.clear();
        self.durable = self.records.len();
        Ok(())
    }
}

fn load(path: &Path, scope: &FeedLogScope) -> Result<Option<(String, TenantLog)>> {
    let mut file = OpenOptions::new()
        .read(true)
        .append(true)
        .open(path)
        .map_err(|error| storage_error("Failed to open feed log", path, error))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| storage_error("Failed to read feed log", path, error))?;
    let corrupt = |detail: &str| storage_error("Corrupt feed log", path, detail);
    let header_end = bytes
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or_else(|| corrupt("no header line"))?;
    let header: Header =
        serde_json::from_slice(&bytes[..header_end]).map_err(|_| corrupt("unreadable header"))?;
    if header.format != FORMAT || header.version != VERSION {
        return Err(corrupt("unknown format"));
    }
    if !scope.includes(&header.tenant) {
        return Ok(None);
    }
    // Lines a crashed process wrote after its WAL sync but before its own
    // fsync become durable here, before any of them is served.
    file.sync_all()
        .map_err(|error| storage_error("Failed to sync feed log", path, error))?;

    let mut names: HashMap<String, Arc<str>> = HashMap::new();
    let mut intern = |value: String| -> Arc<str> {
        names
            .entry(value)
            .or_insert_with_key(|value| Arc::from(value.as_str()))
            .clone()
    };
    let mut records = Vec::new();
    let mut ids = HashSet::new();
    let mut end = header_end + 1;
    while let Some(length) = bytes[end..].iter().position(|byte| *byte == b'\n') {
        let line: LineIn = serde_json::from_slice(&bytes[end..end + length])
            .map_err(|_| corrupt("unreadable record"))?;
        end += length + 1;
        if ids.insert(line.id) {
            records.push(FeedLogRecord {
                id: line.id,
                event_type: intern(line.event_type),
                entity_id: intern(line.entity),
            });
        }
    }
    if end < bytes.len() {
        // A write cut short by a crash: never synced, so never served.
        file.set_len(end as u64)
            .and_then(|()| file.sync_all())
            .map_err(|error| storage_error("Failed to drop torn feed log tail", path, error))?;
    }
    Ok(Some((
        header.tenant,
        TenantLog {
            path: path.to_owned(),
            file,
            epoch: header.epoch,
            durable: records.len(),
            records,
            ids,
            durable_bytes: end as u64,
            pending: Vec::new(),
        },
    )))
}

pub struct FeedLog {
    dir: PathBuf,
    scope: FeedLogScope,
    tenants: RwLock<HashMap<String, Arc<Mutex<TenantLog>>>>,
}

impl FeedLog {
    /// Load every in-scope tenant's log under `dir`. A log that cannot be read
    /// is renamed to `*.feed.corrupt`, so that tenant starts a new epoch rather
    /// than a renumbered one.
    pub fn open(dir: impl Into<PathBuf>, scope: FeedLogScope) -> Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)
            .map_err(|error| storage_error("Failed to create feed log directory", &dir, error))?;
        let entries = fs::read_dir(&dir)
            .map_err(|error| storage_error("Failed to read feed log directory", &dir, error))?;
        let mut tenants = HashMap::new();
        for entry in entries {
            let path = entry
                .map_err(|error| storage_error("Failed to read feed log directory", &dir, error))?
                .path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name.ends_with(TEMP_SUFFIX) {
                // A backfill that never reached its rename.
                let _ = fs::remove_file(&path);
                continue;
            }
            if path
                .extension()
                .is_none_or(|extension| extension != EXTENSION)
            {
                continue;
            }
            match load(&path, &scope) {
                Ok(Some((tenant, log))) => {
                    tenants.insert(tenant, Arc::new(Mutex::new(log)));
                }
                Ok(None) => {}
                Err(error) => {
                    let aside = path.with_extension(CORRUPT_EXTENSION);
                    tracing::error!("{error}; moving it to {}", aside.display());
                    fs::rename(&path, &aside).map_err(|error| {
                        storage_error("Failed to set aside feed log", &path, error)
                    })?;
                }
            }
        }
        Ok(Self {
            dir,
            scope,
            tenants: RwLock::new(tenants),
        })
    }

    pub fn includes(&self, tenant: &str) -> bool {
        self.scope.includes(tenant)
    }

    fn tenant(&self, tenant: &str) -> Option<Arc<Mutex<TenantLog>>> {
        self.tenants.read().get(tenant).cloned()
    }

    pub fn is_open(&self, tenant: &str) -> bool {
        self.tenants.read().contains_key(tenant)
    }

    pub fn epoch(&self, tenant: &str) -> Option<u64> {
        let log = self.tenant(tenant)?;
        let guard = log.lock();
        Some(guard.epoch)
    }

    /// Every record in position order, including ones not yet visible.
    pub fn records(&self, tenant: &str) -> Option<(u64, Vec<FeedLogRecord>)> {
        let log = self.tenant(tenant)?;
        let guard = log.lock();
        Some((guard.epoch, guard.records.clone()))
    }

    /// How many records are fsynced, and so may be served.
    pub fn durable_len(&self, tenant: &str) -> usize {
        let Some(log) = self.tenant(tenant) else {
            return 0;
        };
        let guard = log.lock();
        guard.durable
    }

    pub fn has_pending(&self, tenant: &str) -> bool {
        let Some(log) = self.tenant(tenant) else {
            return false;
        };
        let guard = log.lock();
        guard.durable < guard.records.len()
    }

    pub fn has_any_pending(&self) -> bool {
        self.tenants.read().values().any(|log| {
            let guard = log.lock();
            guard.durable < guard.records.len()
        })
    }

    /// Buffer `record` at the next position unless its id is already logged.
    /// Nothing reaches the file until a sync.
    pub fn append(&self, tenant: &str, record: FeedLogRecord) -> Result<Appended> {
        let Some(log) = self.tenant(tenant) else {
            return Ok(Appended::NotOpen);
        };
        let mut guard = log.lock();
        guard.push(record)
    }

    /// Start a tenant's log with a new epoch and `records` as its first
    /// positions, durable on return.
    pub fn create(&self, tenant: &str, records: &[FeedLogRecord]) -> Result<u64> {
        if !self.scope.includes(tenant) {
            return Err(AllSourceError::InvalidInput(format!(
                "tenant {tenant} is not in the durable feed log scope"
            )));
        }
        let mut tenants = self.tenants.write();
        if tenants.contains_key(tenant) {
            return Err(AllSourceError::InvalidInput(format!(
                "feed log for tenant {tenant} already exists"
            )));
        }
        let epoch = Uuid::new_v4().as_u64_pair().0 & ((1_u64 << 52) - 1);
        let mut bytes = serde_json::to_vec(&Header {
            format: FORMAT.to_owned(),
            version: VERSION,
            tenant: tenant.to_owned(),
            epoch,
        })?;
        bytes.push(b'\n');
        let mut ids = HashSet::with_capacity(records.len());
        let mut kept = Vec::with_capacity(records.len());
        for record in records {
            if ids.insert(record.id) {
                encode_line(&mut bytes, record)?;
                kept.push(record.clone());
            }
        }

        let stem = file_stem(tenant);
        let path = self.dir.join(format!("{stem}.{EXTENSION}"));
        let temp = self.dir.join(format!("{stem}{TEMP_SUFFIX}"));
        let mut file = File::create(&temp)
            .map_err(|error| storage_error("Failed to create feed log", &temp, error))?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| storage_error("Failed to write feed log", &temp, error))?;
        drop(file);
        fs::rename(&temp, &path)
            .map_err(|error| storage_error("Failed to publish feed log", &path, error))?;
        sync_dir(&self.dir)?;
        let file = OpenOptions::new()
            .read(true)
            .append(true)
            .open(&path)
            .map_err(|error| storage_error("Failed to open feed log", &path, error))?;

        tenants.insert(
            tenant.to_owned(),
            Arc::new(Mutex::new(TenantLog {
                path,
                file,
                epoch,
                durable: kept.len(),
                records: kept,
                ids,
                durable_bytes: bytes.len() as u64,
                pending: Vec::new(),
            })),
        );
        Ok(epoch)
    }

    pub fn sync_tenant(&self, tenant: &str) -> Result<()> {
        let Some(log) = self.tenant(tenant) else {
            return Ok(());
        };
        let mut guard = log.lock();
        guard.sync()
    }

    pub fn sync_all(&self) -> Result<()> {
        let logs: Vec<_> = self.tenants.read().values().cloned().collect();
        for log in logs {
            let mut guard = log.lock();
            guard.sync()?;
        }
        Ok(())
    }
}
