//! Writes system checkpoints and checks them against a full replay.

use crate::{
    domain::{entities::Event, value_objects::system_stream::SystemDomain},
    error::{AllSourceError, Result},
    infrastructure::{
        persistence::{
            SystemMetadataStore,
            system_checkpoint::{self, FoldedStream, SystemCheckpoint},
            system_store::CheckpointCapture,
        },
        repositories::EventSourcedTenantRepository,
    },
};
use parking_lot::Mutex;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

/// Environment switches, read once at boot.
#[derive(Debug, Clone, Copy)]
pub struct SystemCheckpointSettings {
    /// `ALLSOURCE_SYSTEM_CHECKPOINT`, default on. Off means boot replays the
    /// whole WAL and no checkpoint is written.
    pub enabled: bool,
    /// `ALLSOURCE_SYSTEM_CHECKPOINT_VERIFY`, default off. On compares the
    /// checkpoint a boot loaded with a full replay of the WAL, in the
    /// background, and rejects every checkpoint if they differ.
    pub verify: bool,
}

impl Default for SystemCheckpointSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            verify: false,
        }
    }
}

impl SystemCheckpointSettings {
    pub fn from_env() -> Self {
        let flag = |name: &str, default: bool| {
            std::env::var(name).map_or(default, |v| {
                matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "on")
            })
        };
        Self {
            enabled: flag("ALLSOURCE_SYSTEM_CHECKPOINT", true),
            verify: flag("ALLSOURCE_SYSTEM_CHECKPOINT_VERIFY", false),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CheckpointOutcome {
    pub covered_seq: u64,
    pub tail_events: usize,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyOutcome {
    NoCheckpointLoaded,
    Matches {
        covered_seq: u64,
        tenant_events: usize,
    },
    /// Every checkpoint was moved aside; the next boot replays the WAL.
    Rejected {
        covered_seq: u64,
        reason: String,
    },
}

struct Base {
    covered_seq: u64,
    tenant: FoldedStream,
}

pub struct SystemCheckpointer {
    store: Arc<SystemMetadataStore>,
    settings: SystemCheckpointSettings,
    /// The state of the last checkpoint built, whether or not its file was
    /// written. Held for a whole build, so builds never interleave.
    base: Mutex<Option<Base>>,
    pending: Mutex<Option<JoinHandle<Result<CheckpointOutcome>>>>,
    /// Held across each file write and across a rejection, so a write that
    /// started before a rejection can never land after it.
    write_lock: Mutex<()>,
    rejected: AtomicBool,
}

impl SystemCheckpointer {
    pub fn new(store: Arc<SystemMetadataStore>, settings: SystemCheckpointSettings) -> Self {
        let base = store.loaded_checkpoint().and_then(|checkpoint| {
            Some(Base {
                covered_seq: checkpoint.covered_seq,
                tenant: checkpoint.folds.get(SystemDomain::Tenant.as_str())?.clone(),
            })
        });
        Self {
            store,
            settings,
            base: Mutex::new(base),
            pending: Mutex::new(None),
            write_lock: Mutex::new(()),
            rejected: AtomicBool::new(false),
        }
    }

    pub fn settings(&self) -> SystemCheckpointSettings {
        self.settings
    }

    /// Checkpoint the live state while nothing else can write: the tenant
    /// repository has applied every resident tenant event. Builds now and
    /// writes the file on a background thread; see [`Self::wait_pending`].
    pub fn checkpoint_at_boot(self: &Arc<Self>, tenant: &EventSourcedTenantRepository) {
        if !self.settings.enabled {
            return;
        }
        let built = {
            let _base = self.base.lock();
            self.store.capture(u64::MAX).and_then(|capture| {
                let fold = tenant.export_fold()?;
                Ok(self.build(capture, fold))
            })
        };
        let checkpoint = match built {
            Ok(checkpoint) => checkpoint,
            Err(e) => {
                tracing::error!("Boot system checkpoint not built: {e}");
                return;
            }
        };
        let this = Arc::clone(self);
        let handle = std::thread::spawn(move || {
            let mut base = this.base.lock();
            let outcome = this.persist(&checkpoint, 0);
            match &outcome {
                Ok(o) => tracing::info!("Boot system checkpoint written at {}", o.covered_seq),
                Err(e) => tracing::error!("Boot system checkpoint not written: {e}"),
            }
            if outcome.is_ok() {
                this.commit(&mut base, &checkpoint);
            }
            outcome
        });
        *self.pending.lock() = Some(handle);
    }

    /// Wait for a background checkpoint write started at boot.
    pub fn wait_pending(&self) -> Option<Result<CheckpointOutcome>> {
        let handle = self.pending.lock().take()?;
        Some(handle.join().unwrap_or_else(|_| {
            Err(AllSourceError::InternalError(
                "checkpoint writer panicked".into(),
            ))
        }))
    }

    /// Fold every tenant event since the last checkpoint onto it and write
    /// the result. Returns `None` when nothing was written since.
    pub fn checkpoint(&self) -> Result<Option<CheckpointOutcome>> {
        if !self.settings.enabled {
            return Ok(None);
        }
        self.refuse_if_rejected()?;
        let mut base = self.base.lock();
        let after = base.as_ref().map_or(0, |b| b.covered_seq);
        let capture = self.store.capture(after)?;
        if base.is_some() && capture.covered_seq == after {
            return Ok(None);
        }
        let fold = EventSourcedTenantRepository::fold_checkpoint(
            Arc::clone(&self.store),
            base.as_ref().map(|b| &b.tenant),
            &capture.tail,
        )?;
        let tail = capture.tail.len();
        let checkpoint = self.build(capture, fold);
        let outcome = self.persist(&checkpoint, tail)?;
        self.commit(&mut base, &checkpoint);
        Ok(Some(outcome))
    }

    /// Makes a durable checkpoint the base the next build folds onto, then
    /// drops the folded events it covers. A checkpoint no newer than the base
    /// changes nothing, since the base already covers it.
    fn commit(&self, base: &mut Option<Base>, checkpoint: &SystemCheckpoint) {
        if base
            .as_ref()
            .is_some_and(|b| b.covered_seq >= checkpoint.covered_seq)
        {
            return;
        }
        let Some(tenant) = checkpoint.folds.get(SystemDomain::Tenant.as_str()) else {
            return;
        };
        *base = Some(Base {
            covered_seq: checkpoint.covered_seq,
            tenant: tenant.clone(),
        });
        self.store.release_folded(checkpoint.covered_seq);
    }

    fn build(&self, capture: CheckpointCapture, tenant: FoldedStream) -> SystemCheckpoint {
        SystemCheckpoint {
            format: system_checkpoint::format(),
            covered_seq: capture.covered_seq,
            created_at: chrono::Utc::now(),
            segments: capture.segments,
            domain_counts: capture.domain_counts,
            verbatim: capture.verbatim,
            folds: BTreeMap::from([(SystemDomain::Tenant.as_str().to_string(), tenant)]),
        }
    }

    fn persist(
        &self,
        checkpoint: &SystemCheckpoint,
        tail_events: usize,
    ) -> Result<CheckpointOutcome> {
        let _writes = self.write_lock.lock();
        self.refuse_if_rejected()?;
        let path = system_checkpoint::write(&self.store.checkpoint_dir(), checkpoint)?;
        Ok(CheckpointOutcome {
            covered_seq: checkpoint.covered_seq,
            tail_events,
            path: Some(path),
        })
    }

    fn refuse_if_rejected(&self) -> Result<()> {
        if self.rejected.load(Ordering::SeqCst) {
            return Err(AllSourceError::StorageError(
                "System checkpoints were rejected in this process; none are written until restart"
                    .into(),
            ));
        }
        Ok(())
    }

    /// Stop every checkpoint this process would write, including one the boot
    /// thread is still writing, and move every file aside.
    fn reject(&self) -> Result<usize> {
        *self.base.lock() = None;
        let moved = {
            let _writes = self.write_lock.lock();
            self.rejected.store(true, Ordering::SeqCst);
            system_checkpoint::reject_all(&self.store.checkpoint_dir())?
        };
        let _ = self.wait_pending();
        Ok(moved + system_checkpoint::reject_all(&self.store.checkpoint_dir())?)
    }

    /// Compare the checkpoint this store booted from with a fold of every
    /// WAL event it covers. On a difference every checkpoint is moved aside.
    pub fn verify_loaded(&self) -> Result<VerifyOutcome> {
        let Some(loaded) = self.store.loaded_checkpoint() else {
            return Ok(VerifyOutcome::NoCheckpointLoaded);
        };
        let covered_seq = loaded.covered_seq;
        let wal = self.store.wal().read_all_entries()?;
        let folded = self.store.options().folded;
        let mut tenant_events: Vec<Event> = Vec::new();
        let mut verbatim: Vec<(u64, String)> = Vec::new();
        for (sequence, event) in wal
            .into_iter()
            .map(|entry| (entry.sequence, entry.event))
            .filter(|(s, _)| *s <= covered_seq)
        {
            if !crate::domain::value_objects::system_stream::is_system_event_type(
                event.event_type_str(),
            ) {
                continue;
            }
            if folded.iter().any(|d| {
                event
                    .entity_id_str()
                    .starts_with(&format!("_system:{}", d.as_str()))
            }) {
                tenant_events.push(event);
            } else {
                verbatim.push((sequence, event.id().to_string()));
            }
        }
        let loaded_verbatim: Vec<(u64, String)> = loaded
            .verbatim
            .iter()
            .map(|(s, e)| (*s, e.id().to_string()))
            .collect();
        let mismatch = if loaded_verbatim == verbatim {
            let replayed = EventSourcedTenantRepository::fold_checkpoint(
                Arc::clone(&self.store),
                None,
                &tenant_events,
            )?;
            let stored = EventSourcedTenantRepository::fold_checkpoint(
                Arc::clone(&self.store),
                loaded.folds.get(SystemDomain::Tenant.as_str()),
                &[],
            )?;
            let (replayed, stored) = (comparable(replayed), comparable(stored));
            (replayed != stored).then(|| describe_difference(&replayed, &stored))
        } else {
            Some(format!(
                "verbatim events differ: checkpoint has {}, WAL has {}",
                loaded_verbatim.len(),
                verbatim.len()
            ))
        };
        match mismatch {
            None => Ok(VerifyOutcome::Matches {
                covered_seq,
                tenant_events: tenant_events.len(),
            }),
            Some(reason) => {
                let moved = self.reject()?;
                tracing::error!(
                    "System checkpoint {covered_seq} differs from a full replay ({reason}); \
                     {moved} checkpoints moved to rejected/ and no more are written by this \
                     process"
                );
                Ok(VerifyOutcome::Rejected {
                    covered_seq,
                    reason,
                })
            }
        }
    }
}

/// A fold without the fields replay stamps with the clock: a tenant's
/// `updated_at` and its volatile usage window.
fn comparable(mut fold: FoldedStream) -> FoldedStream {
    if let Some(tenants) = fold.state["tenants"].as_array_mut() {
        for tenant in tenants
            .iter_mut()
            .filter_map(serde_json::Value::as_object_mut)
        {
            tenant.remove("updated_at");
            tenant.remove("usage");
        }
    }
    fold
}

/// Which tenants differ, by id, without printing their state.
fn describe_difference(replayed: &FoldedStream, stored: &FoldedStream) -> String {
    let index = |fold: &FoldedStream, section: &str| -> BTreeMap<String, serde_json::Value> {
        let value = &fold.state[section];
        if let Some(list) = value.as_array() {
            return list
                .iter()
                .map(|t| (t["id"].to_string(), t.clone()))
                .collect();
        }
        value
            .as_object()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default()
    };
    let mut differing = Vec::new();
    for section in ["tenants", "query_usage", "egress"] {
        let a = index(replayed, section);
        let b = index(stored, section);
        for key in a
            .keys()
            .chain(b.keys())
            .collect::<std::collections::BTreeSet<_>>()
        {
            if a.get(key) != b.get(key) {
                differing.push(format!("{section}:{key}"));
            }
        }
    }
    format!(
        "{} entries differ: {}",
        differing.len(),
        differing.join(", ")
    )
}
