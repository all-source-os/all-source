use super::EventSourcedTenantRepository;
use crate::{
    domain::{
        entities::query_usage::*,
        value_objects::{TenantId, system_stream::tenant_events},
    },
    error::{AllSourceError, Result},
};
use chrono::Utc;
use serde_json::{Value, json};
use std::collections::HashMap;

pub(super) const ADMITTED: &str = "query_usage_admission_v1";
pub(super) const RESET: &str = "query_usage_reset_v1";

#[derive(Default)]
pub(super) struct QueryUsageState {
    period: u64,
    receipts: HashMap<String, QueryUsageReceipt>,
    last_reset: Option<QueryUsageResetReceipt>,
}

impl EventSourcedTenantRepository {
    pub(super) async fn query_usage_snapshot(
        &self,
        id: &TenantId,
    ) -> Result<Option<QueryUsageSnapshot>> {
        let lock = self.usage_lock_for(id.as_str());
        let _guard = lock.lock().await;
        let Some(tenant) = self.cache.get(id.as_str()) else {
            return Ok(None);
        };
        let (used, quota) = quota(tenant.metadata())
            .ok_or_else(|| AllSourceError::InternalError("Invalid canonical query quota".into()))?;
        let state = self.query_usage.get(id.as_str());
        Ok(Some(QueryUsageSnapshot {
            period: state.as_ref().map_or(0, |s| s.period),
            used,
            quota,
            managed: state.is_some(),
        }))
    }

    pub(super) async fn admit_query(
        &self,
        id: &TenantId,
        request: QueryUsageRequest,
    ) -> Result<Option<QueryUsageDecision>> {
        let expires_at = request.expires_at()?;
        let now = Utc::now().timestamp();
        let denied = |reason| Ok(Some(QueryUsageDecision::Denied(reason)));
        if expires_at <= now || expires_at - QUERY_RETRY_SECONDS > now {
            return denied(QueryUsageDenial::ExpiredOperation);
        }
        let lock = self.usage_lock_for(id.as_str());
        let _guard = lock.lock().await;
        let now = Utc::now().timestamp();
        if expires_at <= now {
            return denied(QueryUsageDenial::ExpiredOperation);
        }
        let Some(tenant) = self.cache.get(id.as_str()).map(|entry| entry.clone()) else {
            return Ok(None);
        };
        if !tenant.is_active() {
            return denied(QueryUsageDenial::InactiveTenant);
        }
        let Some((used, quota)) = quota(tenant.metadata()) else {
            return denied(QueryUsageDenial::InvalidQuotaMetadata);
        };
        let period = self
            .query_usage
            .get(id.as_str())
            .map_or(0, |state| state.period);
        if period != request.expected_period {
            return denied(QueryUsageDenial::PeriodChanged);
        }
        if let Some(mut state) = self.query_usage.get_mut(id.as_str()) {
            state.receipts.retain(|_, receipt| receipt.expires_at > now);
            if let Some(receipt) = state.receipts.get(&request.operation_id) {
                return if receipt.matches(&request) {
                    Ok(Some(QueryUsageDecision::Admitted {
                        receipt: receipt.clone(),
                        replayed: true,
                    }))
                } else {
                    denied(QueryUsageDenial::OperationConflict)
                };
            }
            if state.receipts.len() >= MAX_QUERY_RECEIPTS {
                return denied(QueryUsageDenial::ReceiptCapacity);
            }
        }
        let Some(next) = used.checked_add(request.count) else {
            return denied(QueryUsageDenial::QuotaExceeded);
        };
        if quota >= 0 && next > quota as u64 {
            return denied(QueryUsageDenial::QuotaExceeded);
        }
        // Recheck after lock contention. An expired operation cannot charge.
        if expires_at <= Utc::now().timestamp() {
            return denied(QueryUsageDenial::ExpiredOperation);
        }
        let receipt = QueryUsageReceipt {
            operation_id: request.operation_id,
            fingerprint: request.fingerprint,
            count: request.count,
            period,
            used: next,
            expires_at,
        };
        let mut metadata = tenant.metadata().clone();
        metadata["quotas"]["queries_used"] = json!(next);
        // Existing readers still replay the canonical counter. Only the
        // bounded receipt extension requires support for this protocol.
        self.emit_event(
            tenant_events::UPDATED,
            id.as_str(),
            json!({
                "metadata": metadata, ADMITTED: receipt,
            }),
        )?;
        Ok(Some(QueryUsageDecision::Admitted {
            receipt,
            replayed: false,
        }))
    }

    pub(super) async fn reset_queries(
        &self,
        id: &TenantId,
        request: QueryUsageReset,
    ) -> Result<Option<QueryUsageResetDecision>> {
        let denied = |reason| Ok(Some(QueryUsageResetDecision::Denied(reason)));
        let next = request
            .expected_period
            .checked_add(1)
            .ok_or_else(|| AllSourceError::ValidationError("Query period exhausted".into()))?;
        let lock = self.usage_lock_for(id.as_str());
        let _guard = lock.lock().await;
        let Some(tenant) = self.cache.get(id.as_str()).map(|entry| entry.clone()) else {
            return Ok(None);
        };
        let Some((_, _)) = quota(tenant.metadata()) else {
            return denied(QueryUsageDenial::InvalidQuotaMetadata);
        };
        let transition = QueryUsageResetReceipt {
            previous_period: request.expected_period,
            period: next,
        };
        let current = self
            .query_usage
            .get(id.as_str())
            .map_or(0, |state| state.period);
        if let Some(state) = self.query_usage.get(id.as_str())
            && state.last_reset.as_ref() == Some(&transition)
            && current == next
        {
            return Ok(Some(QueryUsageResetDecision::Reset {
                receipt: transition,
                replayed: true,
            }));
        }
        if current != request.expected_period {
            return denied(QueryUsageDenial::PeriodChanged);
        }
        let mut metadata = tenant.metadata().clone();
        metadata["quotas"]["queries_used"] = json!(0);
        self.emit_event(
            tenant_events::UPDATED,
            id.as_str(),
            json!({
                "metadata": metadata, RESET: transition,
            }),
        )?;
        Ok(Some(QueryUsageResetDecision::Reset {
            receipt: transition,
            replayed: false,
        }))
    }

    pub(super) fn apply_query_usage(&self, event_type: &str, id: &str, payload: &Value) {
        if event_type == ADMITTED {
            let Ok(receipt) = serde_json::from_value::<QueryUsageReceipt>(payload.clone()) else {
                tracing::error!("Invalid persisted query admission receipt");
                return;
            };
            if self.set_query_meter(id, receipt.used) {
                let mut state = self.query_usage.entry(id.into()).or_default();
                state.period = receipt.period;
                let now = Utc::now().timestamp();
                state.receipts.retain(|_, receipt| receipt.expires_at > now);
                if receipt.expires_at > now {
                    state.receipts.insert(receipt.operation_id.clone(), receipt);
                }
            }
        } else if event_type == RESET {
            let Ok(receipt) = serde_json::from_value::<QueryUsageResetReceipt>(payload.clone())
            else {
                tracing::error!("Invalid persisted query reset receipt");
                return;
            };
            if self.set_query_meter(id, 0) {
                let mut state = self.query_usage.entry(id.into()).or_default();
                state.period = receipt.period;
                state
                    .receipts
                    .retain(|_, receipt| receipt.expires_at > Utc::now().timestamp());
                state.last_reset = Some(receipt);
            }
        }
    }

    fn set_query_meter(&self, id: &str, used: u64) -> bool {
        let Some(mut tenant) = self.cache.get_mut(id) else {
            return false;
        };
        let mut metadata = tenant.metadata().clone();
        let Some(quotas) = metadata.get_mut("quotas").and_then(Value::as_object_mut) else {
            tracing::error!("Missing quota metadata during query-meter replay");
            return false;
        };
        quotas.insert("queries_used".into(), json!(used));
        tenant.update_metadata(metadata);
        true
    }

    /// Generic metadata writers cannot roll back an owned canonical query meter.
    pub(super) fn preserve_query_meter(&self, id: &str, incoming: &mut Value) -> Result<()> {
        if !self.query_usage.contains_key(id) {
            return Ok(());
        }
        let invalid =
            || AllSourceError::ValidationError("Managed query metadata must be an object".into());
        let current = self
            .cache
            .get(id)
            .ok_or_else(|| AllSourceError::TenantNotFound(id.into()))?;
        let saved = current
            .metadata()
            .get("quotas")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        let quotas = incoming
            .as_object_mut()
            .ok_or_else(invalid)?
            .entry("quotas")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(invalid)?;
        quotas.insert(
            "queries_used".into(),
            saved.get("queries_used").cloned().ok_or_else(invalid)?,
        );
        Ok(())
    }
}

fn quota(metadata: &Value) -> Option<(u64, i64)> {
    let quotas = metadata.get("quotas")?.as_object()?;
    let used = quotas.get("queries_used")?.as_u64()?;
    let limit = quotas.get("queries_quota")?.as_i64()?;
    if limit < -1 {
        return None;
    }
    Some((used, limit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{
            entities::{Tenant, TenantQuotas},
            repositories::TenantRepository,
        },
        infrastructure::persistence::SystemMetadataStore,
    };
    use std::{sync::Arc, time::Duration};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn queued_retry_rechecks_expiry_before_returning_a_cached_receipt() {
        let directory = tempfile::TempDir::new().unwrap();
        let repo = Arc::new(EventSourcedTenantRepository::new(Arc::new(
            SystemMetadataStore::new(directory.path().join("system")).unwrap(),
        )));
        let id = TenantId::new("synthetic-expiring-meter".into()).unwrap();
        let mut tenant = Tenant::new(
            id.clone(),
            "Synthetic expiry".into(),
            TenantQuotas::standard(),
        )
        .unwrap();
        tenant.update_metadata(json!({"quotas": {"queries_used": 0, "queries_quota": 10}}));
        repo.create_initialized(tenant).await.unwrap();
        let request = QueryUsageRequest {
            operation_id: format!(
                "{}:{}",
                Utc::now().timestamp() - QUERY_RETRY_SECONDS + 3,
                uuid::Uuid::new_v4()
            ),
            fingerprint: "b".repeat(64),
            count: 1,
            expected_period: 0,
        };
        assert!(matches!(
            repo.admit_query(&id, request.clone()).await.unwrap(),
            Some(QueryUsageDecision::Admitted { .. })
        ));
        let lock = repo.usage_lock_for(id.as_str());
        let guard = lock.lock().await;
        let mut waiting = std::pin::pin!(repo.admit_query(&id, request));
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(std::future::Future::poll(waiting.as_mut(), &mut context).is_pending());
        // test-hang-allow: fixed four-second contention crosses this owned
        // receipt's three-second remaining window without changing the clock.
        tokio::time::sleep(Duration::from_secs(4)).await;
        drop(guard);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), waiting)
                .await
                .unwrap()
                .unwrap(),
            Some(QueryUsageDecision::Denied(
                QueryUsageDenial::ExpiredOperation
            ))
        );
    }
}
