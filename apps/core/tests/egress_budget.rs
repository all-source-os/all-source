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

fn configured(included_bytes: u64) -> EgressAccount {
    let mut account = EgressAccount::default();
    account
        .apply(&event(EgressCommand::Configure { included_bytes }, 100, 1))
        .unwrap();
    account
}

#[test]
fn lease_grants_min_of_request_available_and_max() {
    let mut account = configured(10);
    let first = id();
    let admission = account
        .apply(&event(
            EgressCommand::Lease {
                id: first.clone(),
                bytes: 4,
            },
            100,
            1,
        ))
        .unwrap();
    assert_eq!(admission.reserved_bytes, 4);
    assert_eq!(admission.lease_expires_at, Some(100 + LEASE_SECONDS));
    assert!(!admission.replayed);
    let replay = account
        .apply(&event(
            EgressCommand::Lease {
                id: first,
                bytes: 4,
            },
            101,
            1,
        ))
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.reserved_bytes, 4);
    assert_eq!(
        account
            .apply(&event(
                EgressCommand::Lease {
                    id: id(),
                    bytes: 100
                },
                101,
                1
            ))
            .unwrap()
            .reserved_bytes,
        6
    );
    assert_eq!(
        account.apply(&event(EgressCommand::Lease { id: id(), bytes: 1 }, 101, 1)),
        Err(EgressDenial::Busy),
        "the balance is held by open leases, not spent"
    );
    let mut large = configured(1_000_000_000_000);
    assert_eq!(
        large
            .apply(&event(
                EgressCommand::Lease {
                    id: id(),
                    bytes: u64::MAX / 4
                },
                100,
                1
            ))
            .unwrap()
            .reserved_bytes,
        MAX_LEASE_BYTES
    );
}

#[test]
fn leases_are_not_capped_at_four_reservations() {
    let mut account = configured(1_000_000_000_000);
    for _ in 0..MAX_LEASES {
        account
            .apply(&event(EgressCommand::Lease { id: id(), bytes: 1 }, 100, 1))
            .unwrap();
    }
    assert_eq!(account.snapshot().in_flight, MAX_LEASES);
    assert_eq!(
        account.apply(&event(EgressCommand::Lease { id: id(), bytes: 1 }, 100, 1)),
        Err(EgressDenial::Capacity)
    );
}

#[test]
fn settle_charges_included_then_prepaid() {
    let mut account = configured(10);
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
    account
        .apply(&event(
            EgressCommand::Lease {
                id: operation.clone(),
                bytes: 12,
            },
            100,
            1,
        ))
        .unwrap();
    account
        .apply(&event(
            EgressCommand::Settle {
                id: operation,
                bytes: 12,
                next: None,
            },
            101,
            1,
        ))
        .unwrap();
    let snapshot = account.snapshot();
    assert_eq!(snapshot.used_bytes, 10);
    assert_eq!(snapshot.included_available, 0);
    assert_eq!(snapshot.prepaid_available, 3);
    assert_eq!(snapshot.in_flight, 0);
}

#[test]
fn settle_replay_is_idempotent_and_conflicts_on_different_bytes() {
    let mut account = configured(100);
    let operation = id();
    account
        .apply(&event(
            EgressCommand::Lease {
                id: operation.clone(),
                bytes: 10,
            },
            100,
            1,
        ))
        .unwrap();
    let settle = |bytes| EgressCommand::Settle {
        id: operation.clone(),
        bytes,
        next: None,
    };
    assert!(!account.apply(&event(settle(4), 101, 1)).unwrap().replayed);
    assert!(account.apply(&event(settle(4), 102, 1)).unwrap().replayed);
    assert_eq!(account.snapshot().used_bytes, 4);
    assert_eq!(
        account.apply(&event(settle(5), 102, 1)),
        Err(EgressDenial::Conflict)
    );
    assert_eq!(
        account.apply(&event(
            EgressCommand::Lease {
                id: operation,
                bytes: 1
            },
            102,
            1
        )),
        Err(EgressDenial::Conflict)
    );
}

#[test]
fn settle_over_lease_total_is_invalid() {
    let mut account = configured(10);
    let operation = id();
    account
        .apply(&event(
            EgressCommand::Lease {
                id: operation.clone(),
                bytes: 5,
            },
            100,
            1,
        ))
        .unwrap();
    assert_eq!(
        account.apply(&event(
            EgressCommand::Settle {
                id: operation,
                bytes: 6,
                next: None
            },
            101,
            1
        )),
        Err(EgressDenial::Invalid)
    );
}

#[test]
fn settle_with_next_grants_next_lease_in_one_transition() {
    let mut account = configured(100);
    let operation = id();
    let next = id();
    account
        .apply(&event(
            EgressCommand::Lease {
                id: operation.clone(),
                bytes: 10,
            },
            100,
            1,
        ))
        .unwrap();
    let admission = account
        .apply(&event(
            EgressCommand::Settle {
                id: operation,
                bytes: 10,
                next: Some(LeaseRequest {
                    id: next,
                    bytes: 20,
                }),
            },
            101,
            1,
        ))
        .unwrap();
    assert_eq!(admission.reserved_bytes, 20);
    assert_eq!(admission.lease_expires_at, Some(101 + LEASE_SECONDS));
    assert!(!admission.replayed);
    assert_eq!(admission.snapshot.used_bytes, 10);
    assert_eq!(admission.snapshot.in_flight, 1);
    assert_eq!(admission.snapshot.included_available, 70);
}

#[test]
fn settle_with_next_reports_zero_when_exhausted() {
    let mut account = configured(10);
    let operation = id();
    account
        .apply(&event(
            EgressCommand::Lease {
                id: operation.clone(),
                bytes: 10,
            },
            100,
            1,
        ))
        .unwrap();
    let admission = account
        .apply(&event(
            EgressCommand::Settle {
                id: operation,
                bytes: 10,
                next: Some(LeaseRequest {
                    id: id(),
                    bytes: 20,
                }),
            },
            101,
            1,
        ))
        .unwrap();
    assert_eq!(admission.reserved_bytes, 0);
    assert_eq!(admission.lease_expires_at, None);
    assert_eq!(admission.snapshot.in_flight, 0);
}

#[test]
fn late_settle_of_expired_lease_is_charged_capped_and_recorded() {
    let mut account = configured(4_000_000_000);
    let late = id();
    account
        .apply(&event(
            EgressCommand::Lease {
                id: late.clone(),
                bytes: 100,
            },
            100,
            1,
        ))
        .unwrap();
    let settle = EgressCommand::Settle {
        id: late,
        bytes: 50,
        next: None,
    };
    let admission = account.apply(&event(settle.clone(), 300, 1)).unwrap();
    assert!(!admission.replayed);
    assert_eq!(account.snapshot().used_bytes, 50);
    assert!(account.apply(&event(settle, 301, 1)).unwrap().replayed);
    assert_eq!(account.snapshot().used_bytes, 50);

    let unknown = id();
    account
        .apply(&event(
            EgressCommand::Settle {
                id: unknown,
                bytes: 3_000_000_000,
                next: None,
            },
            300,
            1,
        ))
        .unwrap();
    assert_eq!(account.snapshot().used_bytes, 50 + MAX_LEASE_BYTES);
}

#[test]
fn expired_lease_is_dropped_without_charge() {
    let mut account = configured(10);
    account
        .apply(&event(
            EgressCommand::Lease {
                id: id(),
                bytes: 10,
            },
            100,
            1,
        ))
        .unwrap();
    assert_eq!(account.snapshot().included_available, 0);
    let admission = account
        .apply(&event(
            EgressCommand::Lease {
                id: id(),
                bytes: 10,
            },
            100 + LEASE_SECONDS,
            1,
        ))
        .unwrap();
    assert_eq!(admission.reserved_bytes, 10);
    assert_eq!(admission.snapshot.used_bytes, 0);
}

#[test]
fn exhausted_account_denies_lease() {
    let mut account = configured(0);
    assert_eq!(
        account.apply(&event(EgressCommand::Lease { id: id(), bytes: 1 }, 100, 1)),
        Err(EgressDenial::Exhausted)
    );
}

#[test]
fn a_free_tier_account_serves_a_second_lease_holder() {
    let mut account = configured(included_egress_bytes("free").unwrap());
    let lease = |at| {
        event(
            EgressCommand::Lease {
                id: id(),
                bytes: MAX_LEASE_BYTES,
            },
            at,
            1,
        )
    };
    let first = account.apply(&lease(100)).unwrap();
    assert_eq!(first.reserved_bytes, 500_000_000);
    let second = account.apply(&lease(101)).unwrap();
    assert_eq!(second.reserved_bytes, 250_000_000);
    assert_eq!(second.lease_included, Some(250_000_000));
}

#[test]
fn a_lease_refused_while_others_hold_the_balance_is_busy() {
    let mut account = configured(10);
    let held = id();
    account
        .apply(&event(
            EgressCommand::Lease {
                id: held.clone(),
                bytes: 10,
            },
            100,
            1,
        ))
        .unwrap();
    let other = id();
    account
        .apply(&event(
            EgressCommand::Credit {
                payment_id: "order:1".into(),
                bytes: 4,
            },
            100,
            1,
        ))
        .unwrap();
    account
        .apply(&event(
            EgressCommand::Lease {
                id: other.clone(),
                bytes: 4,
            },
            100,
            1,
        ))
        .unwrap();
    assert_eq!(
        account.apply(&event(EgressCommand::Lease { id: id(), bytes: 1 }, 101, 1)),
        Err(EgressDenial::Busy)
    );
    let settled = account
        .apply(&event(
            EgressCommand::Settle {
                id: other,
                bytes: 4,
                next: Some(LeaseRequest {
                    id: id(),
                    bytes: 10,
                }),
            },
            101,
            1,
        ))
        .unwrap();
    assert_eq!(settled.reserved_bytes, 0);
    assert_eq!(settled.next_refused, Some(EgressDenial::Busy));
    assert_eq!(settled.snapshot.prepaid_available, 0);
}

#[test]
fn a_lease_reports_its_included_share() {
    let mut account = configured(10);
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
    let granted = account
        .apply(&event(
            EgressCommand::Lease {
                id: id(),
                bytes: 12,
            },
            100,
            1,
        ))
        .unwrap();
    assert_eq!(granted.reserved_bytes, 12);
    assert_eq!(granted.lease_included, Some(10));
    assert_eq!(granted.next_refused, None);
}

#[test]
fn snapshot_holds_leased_bytes_until_settled() {
    let mut account = configured(10);
    let lease = id();
    account
        .apply(&event(
            EgressCommand::Lease {
                id: lease.clone(),
                bytes: 6,
            },
            100,
            1,
        ))
        .unwrap();
    let snapshot = account.snapshot();
    assert_eq!(snapshot.included_available, 4);
    assert_eq!(snapshot.in_flight, 1);
    account
        .apply(&event(
            EgressCommand::Settle {
                id: lease,
                bytes: 6,
                next: None,
            },
            101,
            1,
        ))
        .unwrap();
    assert_eq!(account.snapshot().included_available, 4);
    assert_eq!(account.snapshot().used_bytes, 6);
    assert_eq!(account.snapshot().in_flight, 0);
}

fn settle_renewing(lease: &str, bytes: u64) -> EgressCommand {
    EgressCommand::Settle {
        id: lease.to_owned(),
        bytes,
        next: Some(LeaseRequest {
            id: id(),
            bytes: 1 << 20,
        }),
    }
}

async fn open_lease(repo: &EventSourcedTenantRepository, tenant: &TenantId) -> String {
    let lease = id();
    let granted = repo
        .mutate_egress(
            tenant,
            EgressCommand::Lease {
                id: lease.clone(),
                bytes: 1 << 20,
            },
        )
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(granted.reserved_bytes > 0);
    lease
}

/// Bytes served under a lease are billed even when the plan lapsed before the
/// control plane settled; the settle simply opens no next lease.
#[tokio::test]
async fn a_lapsed_plan_still_settles_its_last_lease() {
    let (_directory, repo, _) = setup(4096).await;
    let tenant_id = TenantId::new("lapsing-plan".into()).unwrap();
    let mut tenant = Tenant::new(
        tenant_id.clone(),
        "Lapsing".into(),
        TenantQuotas::standard(),
    )
    .unwrap();
    tenant.update_metadata(json!({"subscription":{"tier":"indie","status":"active"}}));
    repo.create_initialized(tenant.clone()).await.unwrap();
    let lease = open_lease(&repo, &tenant_id).await;

    tenant.update_metadata(json!({"subscription":{"tier":"indie","status":"cancelled"}}));
    repo.save(&tenant).await.unwrap();
    let settled = repo
        .mutate_egress(&tenant_id, settle_renewing(&lease, 500))
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        settled.reserved_bytes, 0,
        "a lapsed plan gets no next lease"
    );
    assert_eq!(settled.snapshot.used_bytes, 500);
    assert_eq!(
        repo.mutate_egress(&tenant_id, EgressCommand::Lease { id: id(), bytes: 1 })
            .await
            .unwrap()
            .unwrap(),
        Err(EgressDenial::Unconfigured)
    );
}

/// Same for a deactivated tenant, and the settle replays after a restart.
#[tokio::test]
async fn an_inactive_tenant_still_settles_its_last_lease() {
    let (directory, repo, tenant) = setup(4096).await;
    let lease = open_lease(&repo, &tenant).await;
    assert!(repo.deactivate(&tenant).await.unwrap());

    let settled = repo
        .mutate_egress(&tenant, settle_renewing(&lease, 700))
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(settled.reserved_bytes, 0);
    assert_eq!(settled.snapshot.used_bytes, 700);
    assert_eq!(
        repo.mutate_egress(&tenant, EgressCommand::Lease { id: id(), bytes: 1 })
            .await
            .unwrap()
            .unwrap(),
        Err(EgressDenial::Invalid)
    );

    drop(repo);
    let store = Arc::new(SystemMetadataStore::new(directory.path().join("system")).unwrap());
    let reopened = EventSourcedTenantRepository::new(store);
    let replayed = reopened
        .mutate_egress(&tenant, settle_renewing(&lease, 700))
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(replayed.replayed, "the settle's receipt survives replay");
    assert_eq!(replayed.snapshot.used_bytes, 700);
}
