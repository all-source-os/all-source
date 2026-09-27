//! Durable canonical metering, including legacy writers and period boundaries.
use allsource_core::{
    domain::{
        entities::{Tenant, TenantQuotas, UsageMeter, query_usage::*},
        repositories::TenantRepository,
        value_objects::{TenantId, system_stream::SystemDomain},
    },
    infrastructure::{
        persistence::SystemMetadataStore, repositories::EventSourcedTenantRepository,
    },
};
use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

fn request(count: u64) -> QueryUsageRequest {
    QueryUsageRequest {
        operation_id: format!("{}:{}", Utc::now().timestamp(), uuid::Uuid::new_v4()),
        fingerprint: "a".repeat(64),
        count,
        expected_period: 0,
    }
}

async fn setup(
    limit: i64,
) -> (
    TempDir,
    Arc<SystemMetadataStore>,
    Arc<EventSourcedTenantRepository>,
    Tenant,
) {
    let directory = TempDir::new().unwrap();
    let store = Arc::new(SystemMetadataStore::new(directory.path().join("system")).unwrap());
    let repo = Arc::new(EventSourcedTenantRepository::new(Arc::clone(&store)));
    let mut tenant = Tenant::new(
        TenantId::new("synthetic-meter".into()).unwrap(),
        "Synthetic meter".into(),
        TenantQuotas::standard(),
    )
    .unwrap();
    tenant.update_metadata(json!({"quotas": {"queries_quota": limit, "queries_used": 0, "events_used": 7}, "subscription": {"tier": "indie"}}));
    let tenant = repo.create_initialized(tenant).await.unwrap();
    (directory, store, repo, tenant)
}

async fn used(repo: &EventSourcedTenantRepository, tenant: &Tenant) -> u64 {
    repo.find_by_id(tenant.id())
        .await
        .unwrap()
        .unwrap()
        .metadata()["quotas"]["queries_used"]
        .as_u64()
        .unwrap()
}

#[tokio::test]
async fn lost_ack_retry_recovers_one_charge_and_receipt_after_wal_reopen() {
    let (directory, store, repo, tenant) = setup(10).await;
    let request = request(2);
    let first = repo
        .admit_query_usage(tenant.id(), request.clone())
        .await
        .unwrap()
        .unwrap();
    let QueryUsageDecision::Admitted {
        receipt,
        replayed: false,
    } = first
    else {
        panic!("first admission refused")
    };
    assert_eq!(receipt.used, 2);
    assert_eq!(store.read_stream(SystemDomain::Tenant).len(), 2);
    let persisted = store.read_stream(SystemDomain::Tenant);
    assert_eq!(persisted[1].event_type_str(), "_system.tenant.updated");
    assert_eq!(
        persisted[1].payload()["metadata"]["quotas"]["queries_used"],
        2
    );
    assert_eq!(
        persisted[1].payload()["query_usage_admission_v1"]["used"],
        2
    );
    drop(repo);
    drop(store);
    let store = Arc::new(SystemMetadataStore::new(directory.path().join("system")).unwrap());
    let repo = EventSourcedTenantRepository::new(Arc::clone(&store));
    assert_eq!(
        repo.admit_query_usage(tenant.id(), request.clone())
            .await
            .unwrap(),
        Some(QueryUsageDecision::Admitted {
            receipt,
            replayed: true
        })
    );
    assert_eq!(used(&repo, &tenant).await, 2);
    assert_eq!(store.read_stream(SystemDomain::Tenant).len(), 2);
    let mut changed = request;
    changed.count = 1;
    assert_eq!(
        repo.admit_query_usage(tenant.id(), changed).await.unwrap(),
        Some(QueryUsageDecision::Denied(
            QueryUsageDenial::OperationConflict
        ))
    );
    assert_eq!(used(&repo, &tenant).await, 2);
}

#[tokio::test]
async fn concurrent_last_unit_has_one_winner_and_exact_retry_is_free() {
    let (_directory, store, repo, tenant) = setup(1).await;
    let mut callers = tokio::task::JoinSet::new();
    for _ in 0..16 {
        let (repo, id) = (Arc::clone(&repo), tenant.id().clone());
        callers.spawn(async move {
            repo.admit_query_usage(&id, request(1))
                .await
                .unwrap()
                .unwrap()
        });
    }
    let mut winners = Vec::new();
    while let Some(result) = callers.join_next().await {
        match result.unwrap() {
            QueryUsageDecision::Admitted {
                receipt,
                replayed: false,
            } => winners.push(receipt),
            QueryUsageDecision::Denied(QueryUsageDenial::QuotaExceeded) => (),
            other => panic!("unexpected admission result: {other:?}"),
        }
    }
    assert_eq!(winners.len(), 1);
    assert_eq!(used(&repo, &tenant).await, 1);
    assert_eq!(store.read_stream(SystemDomain::Tenant).len(), 2);
    let receipt = winners.remove(0);
    let retry = QueryUsageRequest {
        operation_id: receipt.operation_id.clone(),
        fingerprint: receipt.fingerprint.clone(),
        count: 1,
        expected_period: 0,
    };
    assert_eq!(
        repo.admit_query_usage(tenant.id(), retry).await.unwrap(),
        Some(QueryUsageDecision::Admitted {
            receipt,
            replayed: true
        })
    );
}

#[tokio::test]
async fn stale_tenant_saves_and_patches_preserve_managed_counter_and_period() {
    let (directory, store, repo, mut stale) = setup(100).await;
    repo.admit_query_usage(stale.id(), request(1))
        .await
        .unwrap();
    repo.increment_usage(stale.id(), UsageMeter::Queries, 8)
        .await
        .unwrap();
    stale.update_name("Updated display name".into()).unwrap();
    // Also exercises the save() quota branch without retaining a DashMap guard
    // while the durable event updates that same cache shard.
    stale.update_quotas(TenantQuotas::trial_tier());
    tokio::time::timeout(std::time::Duration::from_secs(3), repo.save(&stale))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(used(&repo, &stale).await, 9);
    let merged = repo.merge_metadata(stale.id(), json!({"quotas": {"queries_used": 0, "reset_date": "2099-01-01"}, "projections": {"enabled": ["synthetic"]}})).await.unwrap().unwrap();
    assert_eq!(merged["quotas"]["queries_used"], 9);
    assert_eq!(merged["quotas"]["reset_date"], "2099-01-01");
    assert_eq!(
        repo.get_query_usage(stale.id())
            .await
            .unwrap()
            .unwrap()
            .period,
        0
    );
    assert_eq!(merged["projections"]["enabled"], json!(["synthetic"]));
    drop(repo);
    drop(store);
    let repo = EventSourcedTenantRepository::new(Arc::new(
        SystemMetadataStore::new(directory.path().join("system")).unwrap(),
    ));
    assert_eq!(used(&repo, &stale).await, 9);
    let restored = repo.find_by_id(stale.id()).await.unwrap().unwrap();
    assert_eq!(restored.name(), "Updated display name");
    assert_eq!(restored.quotas(), &TenantQuotas::trial_tier());
}

#[tokio::test]
async fn reset_retry_cannot_erase_later_usage_or_reuse_an_old_operation() {
    let (directory, store, repo, tenant) = setup(10).await;
    let old = request(2);
    repo.admit_query_usage(tenant.id(), old.clone())
        .await
        .unwrap();
    let reset = || QueryUsageReset { expected_period: 0 };
    assert!(matches!(
        repo.reset_query_usage(tenant.id(), reset()).await.unwrap(),
        Some(QueryUsageResetDecision::Reset {
            replayed: false,
            ..
        })
    ));
    let mut fresh = request(1);
    fresh.expected_period = 1;
    repo.admit_query_usage(tenant.id(), fresh).await.unwrap();
    assert_eq!(used(&repo, &tenant).await, 1);
    drop(repo);
    drop(store);
    let repo = EventSourcedTenantRepository::new(Arc::new(
        SystemMetadataStore::new(directory.path().join("system")).unwrap(),
    ));
    assert!(matches!(
        repo.reset_query_usage(tenant.id(), reset()).await.unwrap(),
        Some(QueryUsageResetDecision::Reset { replayed: true, .. })
    ));
    assert_eq!(used(&repo, &tenant).await, 1);
    assert_eq!(
        repo.admit_query_usage(tenant.id(), old.clone())
            .await
            .unwrap(),
        Some(QueryUsageDecision::Denied(QueryUsageDenial::PeriodChanged))
    );
    let mut forged_retry = old;
    forged_retry.expected_period = 1;
    assert_eq!(
        repo.admit_query_usage(tenant.id(), forged_retry)
            .await
            .unwrap(),
        Some(QueryUsageDecision::Denied(
            QueryUsageDenial::OperationConflict
        ))
    );
    assert_eq!(used(&repo, &tenant).await, 1);
}

#[tokio::test]
async fn expired_future_malformed_and_inactive_requests_never_charge() {
    let (_directory, store, repo, tenant) = setup(10).await;
    for offset in [-3_601, 60] {
        let mut bad = request(1);
        bad.operation_id = format!(
            "{}:{}",
            Utc::now().timestamp() + offset,
            uuid::Uuid::new_v4()
        );
        assert_eq!(
            repo.admit_query_usage(tenant.id(), bad).await.unwrap(),
            Some(QueryUsageDecision::Denied(
                QueryUsageDenial::ExpiredOperation
            ))
        );
    }
    for count in [0, 5, u64::MAX] {
        assert!(
            repo.admit_query_usage(tenant.id(), request(count))
                .await
                .is_err()
        );
    }
    let mut malformed = request(1);
    malformed.fingerprint = "private text must not become an audit field".into();
    assert!(
        repo.admit_query_usage(tenant.id(), malformed)
            .await
            .is_err()
    );
    assert_eq!(store.read_stream(SystemDomain::Tenant).len(), 1);
    repo.deactivate(tenant.id()).await.unwrap();
    assert_eq!(
        repo.admit_query_usage(tenant.id(), request(1))
            .await
            .unwrap(),
        Some(QueryUsageDecision::Denied(QueryUsageDenial::InactiveTenant))
    );
    assert_eq!(used(&repo, &tenant).await, 0);
}

#[tokio::test]
async fn concurrent_legacy_increments_and_new_admissions_share_one_counter() {
    let (_directory, _store, repo, tenant) = setup(100).await;
    let mut callers = tokio::task::JoinSet::new();
    for index in 0..32 {
        let (repo, id) = (Arc::clone(&repo), tenant.id().clone());
        callers.spawn(async move {
            if index % 2 == 0 {
                repo.increment_usage(&id, UsageMeter::Queries, 1)
                    .await
                    .unwrap();
            } else {
                assert!(matches!(
                    repo.admit_query_usage(&id, request(1)).await.unwrap(),
                    Some(QueryUsageDecision::Admitted { .. })
                ));
            }
        });
    }
    while let Some(result) = callers.join_next().await {
        result.unwrap();
    }
    assert_eq!(used(&repo, &tenant).await, 32);
    assert_eq!(
        repo.find_by_id(tenant.id())
            .await
            .unwrap()
            .unwrap()
            .metadata()["quotas"]["events_used"],
        7
    );
}

#[tokio::test]
async fn receipt_capacity_is_bounded_without_losing_retry_or_canonical_usage() {
    let (directory, store, repo, tenant) = setup(50_000).await;
    let first = request(1);
    repo.admit_query_usage(tenant.id(), first.clone())
        .await
        .unwrap();
    for _ in 1..MAX_QUERY_RECEIPTS {
        assert!(matches!(
            repo.admit_query_usage(tenant.id(), request(1))
                .await
                .unwrap(),
            Some(QueryUsageDecision::Admitted { .. })
        ));
    }
    assert_eq!(
        repo.admit_query_usage(tenant.id(), request(1))
            .await
            .unwrap(),
        Some(QueryUsageDecision::Denied(
            QueryUsageDenial::ReceiptCapacity
        ))
    );
    assert!(matches!(
        repo.admit_query_usage(tenant.id(), first.clone())
            .await
            .unwrap(),
        Some(QueryUsageDecision::Admitted { replayed: true, .. })
    ));
    assert_eq!(used(&repo, &tenant).await, MAX_QUERY_RECEIPTS as u64);
    drop(repo);
    drop(store);
    let repo = EventSourcedTenantRepository::new(Arc::new(
        SystemMetadataStore::new(directory.path().join("system")).unwrap(),
    ));
    assert!(matches!(
        repo.admit_query_usage(tenant.id(), first).await.unwrap(),
        Some(QueryUsageDecision::Admitted { replayed: true, .. })
    ));
    assert_eq!(
        repo.admit_query_usage(tenant.id(), request(1))
            .await
            .unwrap(),
        Some(QueryUsageDecision::Denied(
            QueryUsageDenial::ReceiptCapacity
        ))
    );
    assert_eq!(used(&repo, &tenant).await, MAX_QUERY_RECEIPTS as u64);
}

#[tokio::test]
async fn malformed_canonical_quota_metadata_is_never_treated_as_free_or_unlimited() {
    for quotas in [
        json!({}),
        json!({"queries_quota": -2, "queries_used": 0}),
        json!({"queries_quota": 10, "queries_used": -1}),
        json!({"queries_quota": 10, "queries_used": "0"}),
    ] {
        let (_directory, store, repo, mut tenant) = setup(10).await;
        tenant.update_metadata(json!({"quotas": quotas}));
        repo.save(&tenant).await.unwrap();
        let before = store.read_stream(SystemDomain::Tenant).len();
        assert_eq!(
            repo.admit_query_usage(tenant.id(), request(1))
                .await
                .unwrap(),
            Some(QueryUsageDecision::Denied(
                QueryUsageDenial::InvalidQuotaMetadata
            ))
        );
        assert_eq!(store.read_stream(SystemDomain::Tenant).len(), before);
    }
}
