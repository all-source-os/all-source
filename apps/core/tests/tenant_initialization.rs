//! Initial subscription metadata must exist in the durable tenant-create event.
use allsource_core::{
    domain::{
        entities::{Tenant, TenantQuotas},
        repositories::TenantRepository,
        value_objects::{TenantId, system_stream::SystemDomain},
    },
    error::AllSourceError,
    infrastructure::{
        persistence::SystemMetadataStore, repositories::EventSourcedTenantRepository,
    },
};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

fn initial_tenant() -> Tenant {
    let mut tenant = Tenant::new(
        TenantId::new("initialized-tenant".into()).unwrap(),
        "Synthetic owner".into(),
        TenantQuotas::trial_tier(),
    )
    .unwrap();
    tenant.update_metadata(json!({
        "subscription": {"tier": "trial", "trial_expires_at": "2026-10-10T00:00:00Z"},
        "quotas": {"queries_quota": 100, "queries_used": 0}
    }));
    tenant
}

#[tokio::test]
async fn initialized_creation_is_one_durable_event_and_survives_restart() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("system");
    let initial = initial_tenant();
    {
        let store = Arc::new(SystemMetadataStore::new(&path).unwrap());
        let repo = EventSourcedTenantRepository::new(Arc::clone(&store));
        let created = repo.create_initialized(initial.clone()).await.unwrap();
        assert_eq!(created.metadata(), initial.metadata());
        let events = store.read_stream(SystemDomain::Tenant);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload()["metadata"], *initial.metadata());
    }
    let store = Arc::new(SystemMetadataStore::new(&path).unwrap());
    let repo = EventSourcedTenantRepository::new(store);
    let recovered = repo.find_by_id(initial.id()).await.unwrap().unwrap();
    assert_eq!(recovered.metadata(), initial.metadata());
}

#[tokio::test]
async fn concurrent_signup_has_one_winner_and_never_replaces_paid_state() {
    let directory = TempDir::new().unwrap();
    let store = Arc::new(SystemMetadataStore::new(directory.path().join("system")).unwrap());
    let repo = Arc::new(EventSourcedTenantRepository::new(Arc::clone(&store)));
    let mut callers = tokio::task::JoinSet::new();
    for _ in 0..12 {
        let repo = Arc::clone(&repo);
        callers.spawn(async move { repo.create_initialized(initial_tenant()).await });
    }
    let mut created = Vec::new();
    while let Some(result) = callers.join_next().await {
        match result.unwrap() {
            Ok(tenant) => created.push(tenant),
            Err(AllSourceError::TenantAlreadyExists(_)) => {}
            Err(error) => panic!("unexpected create failure: {error}"),
        }
    }
    assert_eq!(created.len(), 1);
    assert_eq!(store.read_stream(SystemDomain::Tenant).len(), 1);
    let mut paid = created.remove(0);
    paid.update_metadata(json!({"subscription": {"tier": "indie", "status": "active"}}));
    repo.save(&paid).await.unwrap();
    assert!(matches!(
        repo.create_initialized(initial_tenant()).await,
        Err(AllSourceError::TenantAlreadyExists(_))
    ));
    assert_eq!(
        repo.find_by_id(paid.id())
            .await
            .unwrap()
            .unwrap()
            .metadata(),
        paid.metadata()
    );
}
