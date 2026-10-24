use super::EventSourcedTenantRepository;
use crate::{
    domain::{
        entities::egress::*,
        value_objects::{TenantId, system_stream::tenant_events},
    },
    error::Result,
};
use chrono::{Datelike, Utc};
use serde_json::{Value, json};

pub(super) const EVENT_KEY: &str = "egress_ledger_v1";

impl EventSourcedTenantRepository {
    pub(super) fn current_egress(&self, id: &TenantId) -> Option<EgressSnapshot> {
        let tenant = self.cache.get(id.as_str())?;
        if !tenant.is_active() {
            return None;
        }
        let mut account = self
            .egress_accounts
            .get(id.as_str())
            .map(|a| a.clone())
            .unwrap_or_default();
        let now = Utc::now();
        let command = if !account.configured() || account.plan_managed() {
            let subscription = &tenant.metadata()["subscription"];
            let tier = if subscription["status"].as_str() == Some("trialing") {
                "trial"
            } else if subscription["status"].as_str() == Some("active") {
                subscription["tier"].as_str().unwrap_or("")
            } else {
                ""
            };
            EgressCommand::ConfigurePlan {
                included_bytes: included_egress_bytes(tier)?,
            }
        } else {
            EgressCommand::Configure {
                included_bytes: account.snapshot().included_bytes,
            }
        };
        account
            .apply(&EgressEvent {
                command,
                at: now.timestamp(),
                period: u32::try_from(now.year()).ok()? * 12 + now.month(),
            })
            .ok()?;
        Some(account.snapshot())
    }

    pub(super) fn apply_egress_event(&self, tenant: &str, value: &Value) {
        let mut account = self.egress_accounts.entry(tenant.to_string()).or_default();
        let outcome = serde_json::from_value::<EgressEvent>(value.clone())
            .map_err(|e| e.to_string())
            .and_then(|event| {
                account
                    .apply(&event)
                    .map(|_| ())
                    .map_err(|d| format!("{d:?}"))
            });
        if let Err(reason) = outcome {
            account.invalidate();
            tracing::error!(tenant, %reason, "Invalid durable egress event; admission disabled");
        }
    }

    pub(super) async fn change_egress(
        &self,
        id: &TenantId,
        command: EgressCommand,
    ) -> Result<Option<std::result::Result<EgressAdmission, EgressDenial>>> {
        let lock = self.usage_lock_for(id.as_str());
        let _guard = lock.lock().await;
        let tenant = self.cache.get(id.as_str()).map(|t| t.clone());
        let financial = matches!(
            &command,
            EgressCommand::Credit { .. }
                | EgressCommand::Refund { .. }
                | EgressCommand::Payment { .. }
        );
        if tenant.is_none() && !financial {
            return Ok(None);
        }
        if tenant.as_ref().is_some_and(|tenant| !tenant.is_active()) && !financial {
            return Ok(Some(Err(EgressDenial::Invalid)));
        }
        let now = Utc::now();
        let at = now.timestamp();
        let period = u32::try_from(now.year()).unwrap_or_default() * 12 + now.month();
        let mut account = self
            .egress_accounts
            .get(id.as_str())
            .map(|a| a.clone())
            .unwrap_or_default();
        if (!account.configured() || account.plan_managed())
            && matches!(command, EgressCommand::Reserve { .. })
        {
            let Some(tenant) = tenant.as_ref() else {
                return Ok(None);
            };
            let subscription = &tenant.metadata()["subscription"];
            let tier = if subscription["status"].as_str() == Some("trialing") {
                "trial"
            } else if subscription["status"].as_str() == Some("active") {
                subscription["tier"].as_str().unwrap_or("")
            } else {
                ""
            };
            let Some(included_bytes) = included_egress_bytes(tier) else {
                return Ok(Some(Err(EgressDenial::Unconfigured)));
            };
            let configured = EgressEvent {
                command: EgressCommand::ConfigurePlan { included_bytes },
                at,
                period,
            };
            if !account.configured() || account.snapshot().included_bytes != included_bytes {
                if let Err(denial) = account.apply(&configured) {
                    return Ok(Some(Err(denial)));
                }
                self.emit_event(
                    tenant_events::UPDATED,
                    id.as_str(),
                    json!({ EVENT_KEY: configured }),
                )?;
            }
        }
        let event = EgressEvent {
            command,
            at,
            period,
        };
        match account.apply(&event) {
            Ok(admission) => {
                if !admission.replayed {
                    self.emit_event(
                        tenant_events::UPDATED,
                        id.as_str(),
                        json!({ EVENT_KEY: event }),
                    )?;
                }
                Ok(Some(Ok(admission)))
            }
            Err(denial) => Ok(Some(Err(denial))),
        }
    }
}
