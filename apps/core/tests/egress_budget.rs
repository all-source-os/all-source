use allsource_core::{
    domain::{
        entities::{Tenant, TenantQuotas, egress::*},
        repositories::TenantRepository,
        value_objects::TenantId,
    },
    infrastructure::{
        persistence::SystemMetadataStore, repositories::EventSourcedTenantRepository,
    },
};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn event(command: EgressCommand, at: i64, period: u32) -> EgressEvent {
    EgressEvent {
        command,
        at,
        period,
    }
}

#[test]
fn included_then_prepaid_then_hard_stop() {
    let mut account = EgressAccount::default();
    account
        .apply(&event(
            EgressCommand::Configure { included_bytes: 10 },
            100,
            1,
        ))
        .unwrap();
    account
        .apply(&event(
            EgressCommand::Credit {
                payment_id: "order:1".into(),
                bytes: 5,
            },
            100,
            1,
        ))
        .unwrap();
    let operation = id();
    assert_eq!(
        account
            .apply(&event(
                EgressCommand::Reserve {
                    id: operation.clone()
                },
                100,
                1
            ))
            .unwrap()
            .reserved_bytes,
        15
    );
    assert_eq!(
        account.apply(&event(EgressCommand::Reserve { id: id() }, 100, 1)),
        Err(EgressDenial::Busy)
    );
    account
        .apply(&event(
            EgressCommand::Complete {
                id: operation.clone(),
                bytes: 15,
            },
            101,
            1,
        ))
        .unwrap();
    assert!(
        account
            .apply(&event(
                EgressCommand::Complete {
                    id: operation,
                    bytes: 15
                },
                102,
                1
            ))
            .unwrap()
            .replayed
    );
    assert_eq!(account.snapshot().used_bytes, 10);
    assert_eq!(account.snapshot().prepaid_available, 0);
    assert_eq!(
        account.apply(&event(EgressCommand::Reserve { id: id() }, 102, 1)),
        Err(EgressDenial::Exhausted)
    );
}

#[test]
fn expiration_cannot_authorize_a_late_response() {
    let mut account = EgressAccount::default();
    account
        .apply(&event(
            EgressCommand::Configure { included_bytes: 10 },
            100,
            1,
        ))
        .unwrap();
    let operation = id();
    account
        .apply(&event(
            EgressCommand::Reserve {
                id: operation.clone(),
            },
            100,
            1,
        ))
        .unwrap();
    assert_eq!(
        account.apply(&event(
            EgressCommand::Complete {
                id: operation,
                bytes: 10
            },
            130,
            1
        )),
        Err(EgressDenial::Expired)
    );
    assert_eq!(
        account
            .apply(&event(EgressCommand::Reserve { id: id() }, 130, 1))
            .unwrap()
            .reserved_bytes,
        10
    );
}

#[test]
fn duplicate_payment_and_refund_do_not_create_allowance() {
    let mut account = EgressAccount::default();
    account
        .apply(&event(
            EgressCommand::Configure { included_bytes: 1 },
            100,
            1,
        ))
        .unwrap();
    let credit = EgressCommand::Credit {
        payment_id: "order:paid".into(),
        bytes: 20,
    };
    account.apply(&event(credit.clone(), 100, 1)).unwrap();
    assert!(
        account
            .apply(&event(credit.clone(), 101, 1))
            .unwrap()
            .replayed
    );
    let operation = id();
    account
        .apply(&event(
            EgressCommand::Reserve {
                id: operation.clone(),
            },
            101,
            1,
        ))
        .unwrap();
    account
        .apply(&event(
            EgressCommand::Refund {
                payment_id: "order:paid".into(),
            },
            102,
            1,
        ))
        .unwrap();
    assert_eq!(
        account.apply(&event(
            EgressCommand::Complete {
                id: operation,
                bytes: 21
            },
            103,
            1
        )),
        Err(EgressDenial::Exhausted)
    );
    assert!(account.apply(&event(credit, 104, 1)).unwrap().replayed);
    assert_eq!(account.snapshot().prepaid_available, 0);
}

#[test]
fn period_rollover_preserves_prepaid_and_old_reservations() {
    let mut account = EgressAccount::default();
    account
        .apply(&event(
            EgressCommand::Configure { included_bytes: 10 },
            100,
            1,
        ))
        .unwrap();
    account
        .apply(&event(
            EgressCommand::Credit {
                payment_id: "order:2".into(),
                bytes: 5,
            },
            100,
            1,
        ))
        .unwrap();
    let old = id();
    account
        .apply(&event(EgressCommand::Reserve { id: old.clone() }, 100, 1))
        .unwrap();
    let fresh = account
        .apply(&event(EgressCommand::Reserve { id: id() }, 101, 2))
        .unwrap();
    assert_eq!(fresh.reserved_bytes, 10);
    account
        .apply(&event(
            EgressCommand::Complete { id: old, bytes: 15 },
            102,
            2,
        ))
        .unwrap();
    assert_eq!(account.snapshot().used_bytes, 0);
    assert_eq!(account.snapshot().prepaid_available, 0);
}

async fn setup(bytes: u64) -> (TempDir, Arc<EventSourcedTenantRepository>, TenantId) {
    let directory = TempDir::new().unwrap();
    let store = Arc::new(SystemMetadataStore::new(directory.path().join("system")).unwrap());
    let repo = Arc::new(EventSourcedTenantRepository::new(store));
    let tenant_id = TenantId::new("egress-fixture".into()).unwrap();
    let mut tenant = Tenant::new(
        tenant_id.clone(),
        "Egress fixture".into(),
        TenantQuotas::standard(),
    )
    .unwrap();
    tenant.update_metadata(json!({"subscription":{"tier":"indie","status":"active"}}));
    repo.create_initialized(tenant).await.unwrap();
    repo.mutate_egress(
        &tenant_id,
        EgressCommand::Configure {
            included_bytes: bytes,
        },
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    (directory, repo, tenant_id)
}

#[tokio::test]
async fn concurrent_callers_share_one_durable_budget_after_restart() {
    let (directory, repo, tenant) = setup(4096).await;
    let mut callers = Vec::new();
    for _ in 0..16 {
        let repo = Arc::clone(&repo);
        let tenant = tenant.clone();
        callers.push(tokio::spawn(async move {
            let operation = id();
            let result = repo
                .mutate_egress(
                    &tenant,
                    EgressCommand::Reserve {
                        id: operation.clone(),
                    },
                )
                .await
                .unwrap()
                .unwrap();
            if let Ok(admitted) = result {
                repo.mutate_egress(
                    &tenant,
                    EgressCommand::Complete {
                        id: operation,
                        bytes: admitted.reserved_bytes,
                    },
                )
                .await
                .unwrap()
                .unwrap()
                .unwrap();
                admitted.reserved_bytes
            } else {
                0
            }
        }));
    }
    let mut total = 0;
    for caller in callers {
        total += caller.await.unwrap();
    }
    assert_eq!(total, 4096);
    assert_eq!(
        repo.egress_snapshot(&tenant)
            .await
            .unwrap()
            .unwrap()
            .used_bytes,
        4096
    );
    drop(repo);
    let store = Arc::new(SystemMetadataStore::new(directory.path().join("system")).unwrap());
    let reopened = EventSourcedTenantRepository::new(store);
    assert_eq!(
        reopened
            .mutate_egress(&tenant, EgressCommand::Reserve { id: id() })
            .await
            .unwrap()
            .unwrap(),
        Err(EgressDenial::Exhausted)
    );
    let mut stale = reopened.find_by_id(&tenant).await.unwrap().unwrap();
    stale.update_metadata(json!({"subscription":{"tier":"scale"},"egress":{"used_bytes":0}}));
    reopened.save(&stale).await.unwrap();
    assert_eq!(
        reopened
            .egress_snapshot(&tenant)
            .await
            .unwrap()
            .unwrap()
            .used_bytes,
        4096
    );
}

#[tokio::test]
async fn hosted_plan_initializes_and_cancellation_fails_closed() {
    let (_directory, repo, _) = setup(4096).await;
    let tenant_id = TenantId::new("managed-plan".into()).unwrap();
    let mut tenant = Tenant::new(
        tenant_id.clone(),
        "Managed".into(),
        TenantQuotas::standard(),
    )
    .unwrap();
    tenant.update_metadata(json!({"subscription":{"tier":"indie","status":"active"}}));
    repo.create_initialized(tenant.clone()).await.unwrap();
    let operation = id();
    let first = repo
        .mutate_egress(
            &tenant_id,
            EgressCommand::Reserve {
                id: operation.clone(),
            },
        )
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(first.snapshot.included_bytes, 25_000_000_000);
    repo.mutate_egress(
        &tenant_id,
        EgressCommand::Complete {
            id: operation,
            bytes: 10,
        },
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    tenant.update_metadata(json!({"subscription":{"tier":"scale","status":"active"}}));
    repo.save(&tenant).await.unwrap();
    let upgraded = repo
        .mutate_egress(&tenant_id, EgressCommand::Reserve { id: id() })
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(upgraded.snapshot.included_bytes, 750_000_000_000);
    assert_eq!(upgraded.snapshot.used_bytes, 10);
    tenant.update_metadata(json!({"subscription":{"tier":"scale","status":"cancelled"}}));
    repo.save(&tenant).await.unwrap();
    assert_eq!(
        repo.mutate_egress(&tenant_id, EgressCommand::Reserve { id: id() })
            .await
            .unwrap()
            .unwrap(),
        Err(EgressDenial::Unconfigured)
    );
    assert!(repo.egress_snapshot(&tenant_id).await.unwrap().is_none());
}

#[tokio::test]
async fn inactive_refunds_and_tenant_recreation_preserve_financial_history() {
    let (directory, repo, tenant_id) = setup(1).await;
    let payment = |refunded| EgressCommand::Payment {
        payment_id: "order:recreated".into(),
        bytes: 3,
        refunded,
    };
    repo.mutate_egress(&tenant_id, payment(false))
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let operation = id();
    repo.mutate_egress(
        &tenant_id,
        EgressCommand::Reserve {
            id: operation.clone(),
        },
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    repo.mutate_egress(
        &tenant_id,
        EgressCommand::Complete {
            id: operation,
            bytes: 4,
        },
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    assert!(repo.deactivate(&tenant_id).await.unwrap());
    repo.mutate_egress(&tenant_id, payment(true))
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(repo.delete(&tenant_id).await.unwrap());
    repo.mutate_egress(&tenant_id, payment(true))
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let mut recreated = Tenant::new(
        tenant_id.clone(),
        "Recreated".into(),
        TenantQuotas::standard(),
    )
    .unwrap();
    recreated.update_metadata(json!({"subscription":{"tier":"indie","status":"active"}}));
    repo.create_initialized(recreated).await.unwrap();
    repo.mutate_egress(&tenant_id, payment(false))
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        repo.mutate_egress(&tenant_id, EgressCommand::Reserve { id: id() })
            .await
            .unwrap()
            .unwrap(),
        Err(EgressDenial::Exhausted)
    );
    drop(repo);
    let store = Arc::new(SystemMetadataStore::new(directory.path().join("system")).unwrap());
    let reopened = EventSourcedTenantRepository::new(store);
    assert_eq!(
        reopened
            .mutate_egress(&tenant_id, EgressCommand::Reserve { id: id() })
            .await
            .unwrap()
            .unwrap(),
        Err(EgressDenial::Exhausted)
    );
}

#[test]
fn refunded_payment_tombstone_survives_delayed_paid_replay() {
    let mut account = EgressAccount::default();
    account
        .apply(&event(
            EgressCommand::Configure { included_bytes: 0 },
            100,
            1,
        ))
        .unwrap();
    let payment = |refunded| EgressCommand::Payment {
        payment_id: "order:refund-first".into(),
        bytes: 100,
        refunded,
    };
    account.apply(&event(payment(true), 100, 1)).unwrap();
    account.apply(&event(payment(false), 101, 1)).unwrap();
    assert_eq!(account.snapshot().prepaid_available, 0);
    assert_eq!(
        account.apply(&event(EgressCommand::Reserve { id: id() }, 102, 1)),
        Err(EgressDenial::Exhausted)
    );
}

#[test]
fn changing_plan_does_not_reset_consumption() {
    let mut account = EgressAccount::default();
    account
        .apply(&event(
            EgressCommand::ConfigurePlan { included_bytes: 10 },
            100,
            1,
        ))
        .unwrap();
    let operation = id();
    account
        .apply(&event(
            EgressCommand::Reserve {
                id: operation.clone(),
            },
            100,
            1,
        ))
        .unwrap();
    account
        .apply(&event(
            EgressCommand::Complete {
                id: operation,
                bytes: 10,
            },
            101,
            1,
        ))
        .unwrap();
    account
        .apply(&event(
            EgressCommand::ConfigurePlan { included_bytes: 20 },
            102,
            1,
        ))
        .unwrap();
    assert_eq!(account.snapshot().included_available, 10);
    account
        .apply(&event(
            EgressCommand::ConfigurePlan { included_bytes: 5 },
            103,
            1,
        ))
        .unwrap();
    assert_eq!(account.snapshot().included_available, 0);
    assert_eq!(
        account.apply(&event(EgressCommand::Reserve { id: id() }, 104, 1)),
        Err(EgressDenial::Exhausted)
    );
}
