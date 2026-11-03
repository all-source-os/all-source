use super::{EventSourcedTenantRepository, egress, query_usage, query_usage::QueryUsageState};
use crate::{
    domain::{
        entities::{
            Event, SchemaEnforcement, Tenant, TenantQuotas, TenantUsage, egress::EgressAccount,
        },
        value_objects::system_stream::{SystemDomain, tenant_events},
    },
    error::{AllSourceError, Result},
    infrastructure::persistence::{SystemMetadataStore, system_checkpoint::FoldedStream},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Bumped whenever the shape below changes; a checkpoint folded in another
/// format is not used and the tenant stream is replayed from the WAL.
const FORMAT: u32 = 1;

/// Payload keys `apply_event` reads. Adding one here makes every fold that
/// recorded it as uninterpreted unusable, so the stream is replayed.
const CREATED_KEYS: &[&str] = &["name", "description", "is_demo", "quotas", "metadata"];
const UPDATED_KEYS: &[&str] = &[
    "name",
    "description",
    "is_demo",
    "metadata",
    egress::EVENT_KEY,
    query_usage::ADMITTED,
    query_usage::RESET,
];
const TYPES: &[&str] = &[
    tenant_events::CREATED,
    tenant_events::UPDATED,
    tenant_events::SUSPENDED,
    tenant_events::REACTIVATED,
    tenant_events::DELETED,
    tenant_events::QUOTA_UPDATED,
    tenant_events::SCHEMA_ENFORCEMENT_UPDATED,
    tenant_events::USAGE_UPDATED,
];

#[derive(Serialize, Deserialize)]
struct TenantFold {
    tenants: Vec<Tenant>,
    query_usage: BTreeMap<String, QueryUsageState>,
    egress: BTreeMap<String, EgressAccount>,
    /// What the folding binary applied but ignored, as `type` or `type#key`.
    uninterpreted: BTreeSet<String>,
}

fn interprets(entry: &str) -> bool {
    match entry.split_once('#') {
        None => TYPES.contains(&entry),
        Some((kind, key)) if kind == tenant_events::CREATED => CREATED_KEYS.contains(&key),
        Some((kind, key)) if kind == tenant_events::UPDATED => UPDATED_KEYS.contains(&key),
        Some(_) => false,
    }
}

/// Keys of `payload` that do not survive a round trip through `T`.
fn dropped_by<T: Serialize + for<'de> Deserialize<'de>>(payload: &Value) -> Vec<String> {
    let Some(object) = payload.as_object() else {
        return Vec::new();
    };
    let kept = serde_json::from_value::<T>(payload.clone())
        .ok()
        .and_then(|t| serde_json::to_value(t).ok());
    object
        .keys()
        .filter(|key| kept.as_ref().and_then(|k| k.get(key.as_str())).is_none())
        .cloned()
        .collect()
}

/// Record what of `event` this binary's `apply_event` ignores.
pub(super) fn note_uninterpreted(event: &Event, into: &parking_lot::Mutex<BTreeSet<String>>) {
    let kind = event.event_type_str();
    let payload = event.payload();
    let keys = |known: &[&str]| -> Vec<String> {
        payload
            .as_object()
            .map(|o| {
                o.keys()
                    .filter(|k| !known.contains(&k.as_str()))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    };
    let ignored = if kind == tenant_events::CREATED {
        keys(CREATED_KEYS)
    } else if kind == tenant_events::UPDATED {
        keys(UPDATED_KEYS)
    } else if kind == tenant_events::QUOTA_UPDATED {
        dropped_by::<TenantQuotas>(payload)
    } else if kind == tenant_events::SCHEMA_ENFORCEMENT_UPDATED {
        dropped_by::<SchemaEnforcement>(payload)
    } else if TYPES.contains(&kind) {
        Vec::new()
    } else {
        into.lock().insert(kind.to_string());
        return;
    };
    if !ignored.is_empty() {
        let mut set = into.lock();
        for key in ignored {
            set.insert(format!("{kind}#{key}"));
        }
    }
}

impl EventSourcedTenantRepository {
    /// Boot from the store: the loaded checkpoint's fold plus the resident
    /// tail, or the whole stream when there is no usable fold.
    pub(super) fn boot(&self) {
        if self.system_store.loaded_checkpoint().is_none() {
            let events = self.system_store.read_stream(SystemDomain::Tenant);
            self.rebuild_cache(&events, "wal");
            return;
        }
        let Some(folded) = self.system_store.folded(SystemDomain::Tenant) else {
            // The checkpoint did not fold tenants, so it kept them verbatim.
            let events = self.system_store.read_stream(SystemDomain::Tenant);
            self.rebuild_cache(&events, "checkpoint verbatim");
            return;
        };
        match Self::decode(folded) {
            Ok(fold) => {
                self.restore(fold);
                let tail = self.system_store.read_stream(SystemDomain::Tenant);
                self.rebuild_cache(&tail, "checkpoint tail");
            }
            Err(e) => {
                tracing::error!("Tenant checkpoint unusable, replaying the tenant WAL: {e}");
                match self.system_store.read_domain_history(SystemDomain::Tenant) {
                    Ok(history) => {
                        let events: Vec<Event> = history.into_iter().map(|(_, e)| e).collect();
                        self.rebuild_cache(&events, "wal after unusable checkpoint");
                    }
                    Err(e) => tracing::error!("Tenant WAL unreadable: {e}"),
                }
            }
        }
    }

    /// A fold this binary would have produced itself: same format, nothing
    /// lost or invented by decoding it, and nothing it ignored that this
    /// binary interprets.
    fn decode(folded: &FoldedStream) -> Result<TenantFold> {
        let unusable = |why: String| AllSourceError::StorageError(format!("tenant fold {why}"));
        if folded.format != FORMAT {
            return Err(unusable(format!(
                "format {} is not {FORMAT}",
                folded.format
            )));
        }
        let fold: TenantFold = serde_json::from_value(folded.state.clone())
            .map_err(|e| unusable(format!("unreadable: {e}")))?;
        let reencoded =
            serde_json::to_value(&fold).map_err(|e| unusable(format!("encode: {e}")))?;
        if reencoded != folded.state {
            return Err(unusable(
                "does not round-trip through this binary's types".into(),
            ));
        }
        if let Some(entry) = fold.uninterpreted.iter().find(|e| interprets(e)) {
            return Err(unusable(format!(
                "was folded by a binary that ignored {entry}, which this one interprets"
            )));
        }
        Ok(fold)
    }

    fn restore(&self, fold: TenantFold) {
        for mut tenant in fold.tenants {
            // Usage counters are volatile and start over on every boot.
            *tenant.usage_mut() = TenantUsage::new();
            self.cache.insert(tenant.id().as_str().to_string(), tenant);
        }
        for (id, state) in fold.query_usage {
            self.query_usage.insert(id, state);
        }
        for (id, account) in fold.egress {
            self.egress_accounts.insert(id, account);
        }
        self.uninterpreted.lock().extend(fold.uninterpreted);
    }

    /// The folded tenant state. Equal to a fold of the stream only before any
    /// runtime write that bypasses events, so call it at boot or on a
    /// repository built by [`Self::fold_checkpoint`].
    pub fn export_fold(&self) -> Result<FoldedStream> {
        let mut tenants: Vec<Tenant> = self.cache.iter().map(|e| e.value().clone()).collect();
        tenants.sort_by(|a, b| a.id().as_str().cmp(b.id().as_str()));
        let now = chrono::Utc::now().timestamp();
        let query_usage = self
            .query_usage
            .iter()
            .map(|e| (e.key().clone(), e.value().unexpired(now)))
            .collect();
        let egress = self
            .egress_accounts
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect();
        let state = serde_json::to_value(TenantFold {
            tenants,
            query_usage,
            egress,
            uninterpreted: self.uninterpreted.lock().clone(),
        })
        .map_err(|e| AllSourceError::StorageError(format!("tenant fold encode: {e}")))?;
        Ok(FoldedStream {
            format: FORMAT,
            state,
        })
    }

    /// Fold `tail`, tenant events in sequence order, onto `base`, without
    /// touching any live repository. `store` is never written.
    pub fn fold_checkpoint(
        store: Arc<SystemMetadataStore>,
        base: Option<&FoldedStream>,
        tail: &[Event],
    ) -> Result<FoldedStream> {
        let folder = Self::unbooted(store);
        if let Some(base) = base {
            folder.restore(Self::decode(base)?);
        }
        for event in tail {
            folder.apply_event(event);
        }
        folder.export_fold()
    }
}
