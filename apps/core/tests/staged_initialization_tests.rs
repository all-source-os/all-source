/// Staged Initialization & Static Stability Tests (US-006)
///
/// These tests verify:
/// 1. Staged initialization sequence (4 stages)
/// 2. System boots from local storage without external dependencies
/// 3. First-boot bootstrap creates default tenant from env var config
/// 4. Static stability: data plane serves with cached metadata during errors
/// 5. Metadata survives restart (start → ingest → restart → verify)
/// 6. Health endpoint reports system stream health
use allsource_core::{
    QueryEventsRequest,
    domain::{entities::TenantQuotas, repositories::TenantRepository, value_objects::TenantId},
    infrastructure::{di::ContainerBuilder, persistence::SystemBootstrap},
    store::EventStore,
};
use serde_json::json;
use tempfile::TempDir;

// =============================================================================
// Test 1: Full staged initialization sequence
// =============================================================================

#[tokio::test]
async fn test_staged_initialization_creates_all_repos() {
    let temp_dir = TempDir::new().unwrap();
    let system_dir = temp_dir.path().join("__system");

    let repos = SystemBootstrap::initialize(system_dir.clone(), Some("Test Corp".to_string()))
        .await
        .unwrap();

    // Stage 1: SystemMetadataStore should be initialized
    assert!(
        repos.system_store.total_events() > 0,
        "System store should have bootstrap events"
    );

    // Stage 2: Caches should be populated
    let tenant_count = repos.tenant_repository.count().await.unwrap();
    assert_eq!(tenant_count, 1, "Should have exactly 1 bootstrapped tenant");

    // Stage 3: Default tenant should be created
    let tenant_id = TenantId::new("test-corp".to_string()).unwrap();
    let tenant = repos
        .tenant_repository
        .find_by_id(&tenant_id)
        .await
        .unwrap();
    assert!(tenant.is_some(), "Default tenant should exist");
    assert_eq!(tenant.unwrap().name(), "Test Corp");

    // Stage 4: All repos should be usable
    assert!(
        repos.config_repository.count() == 0,
        "No config entries at boot"
    );
    repos
        .config_repository
        .set("test_key", json!("test_value"), None)
        .unwrap();
    assert_eq!(repos.config_repository.count(), 1);
}

// =============================================================================
// Test 2: System boots from local WAL without external dependencies
// =============================================================================

#[tokio::test]
async fn test_boots_from_local_storage_only() {
    let temp_dir = TempDir::new().unwrap();
    let system_dir = temp_dir.path().join("__system");

    // No external dependencies needed — just a directory path
    let repos = SystemBootstrap::initialize(system_dir, None).await.unwrap();

    // System should be functional with zero tenants
    assert_eq!(repos.tenant_repository.count().await.unwrap(), 0);
    assert_eq!(repos.system_store.total_events(), 0);

    // Should be able to create tenants
    let tid = TenantId::new("local-tenant".to_string()).unwrap();
    repos
        .tenant_repository
        .create(tid, "Local Tenant".to_string(), TenantQuotas::unlimited())
        .await
        .unwrap();
    assert_eq!(repos.tenant_repository.count().await.unwrap(), 1);
}

// =============================================================================
// Test 3: First-boot bootstrap creates default tenant from env var
// =============================================================================

#[tokio::test]
async fn test_first_boot_bootstrap_tenant() {
    let temp_dir = TempDir::new().unwrap();
    let system_dir = temp_dir.path().join("__system");

    // With bootstrap tenant
    let repos = SystemBootstrap::initialize(system_dir.clone(), Some("ACME Corp".to_string()))
        .await
        .unwrap();

    let tid = TenantId::new("acme-corp".to_string()).unwrap();
    let tenant = repos
        .tenant_repository
        .find_by_id(&tid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(tenant.name(), "ACME Corp");
    assert!(tenant.is_active());

    // Second boot should NOT create duplicate
    let repos2 = SystemBootstrap::initialize(system_dir, Some("ACME Corp".to_string()))
        .await
        .unwrap();
    assert_eq!(repos2.tenant_repository.count().await.unwrap(), 1);
}

// =============================================================================
// Test 4: No bootstrap tenant when not configured
// =============================================================================

#[tokio::test]
async fn test_no_bootstrap_without_config() {
    let temp_dir = TempDir::new().unwrap();
    let system_dir = temp_dir.path().join("__system");

    let repos = SystemBootstrap::initialize(system_dir, None).await.unwrap();
    assert_eq!(repos.tenant_repository.count().await.unwrap(), 0);
}

// =============================================================================
// Test 5: try_initialize returns None when no system dir configured
// =============================================================================

#[tokio::test]
async fn test_try_initialize_returns_none_without_dir() {
    let result = SystemBootstrap::try_initialize(None, None).await;
    assert!(result.is_none());
}

// =============================================================================
// Test 6: Static stability — data plane continues with cached metadata
// =============================================================================

#[tokio::test]
async fn test_static_stability_data_plane_uses_cache() {
    let temp_dir = TempDir::new().unwrap();
    let system_dir = temp_dir.path().join("__system");

    // Bootstrap with a tenant
    let repos = SystemBootstrap::initialize(system_dir, Some("Stable Corp".to_string()))
        .await
        .unwrap();

    // Wire into DI container
    let container = ContainerBuilder::new()
        .with_in_memory_repositories()
        .with_system_repositories(repos)
        .build();

    assert!(container.has_system_repositories());

    // Data plane: EventStore operations work independently of system streams
    let store = EventStore::new();
    let event = allsource_core::domain::entities::Event::from_strings(
        "order.created".to_string(),
        "order-1".to_string(),
        "default".to_string(),
        json!({"total": 100}),
        None,
    )
    .unwrap();
    store.ingest(&event).unwrap();

    // Verify data plane works
    let events = store
        .query(&QueryEventsRequest {
            as_of_valid: None,
            since_valid: None,
            until_valid: None,
            entity_id: Some("order-1".to_string()),
            event_type: None,
            tenant_id: None,
            as_of: None,
            since: None,
            until: None,
            limit: None,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(events.len(), 1);

    // System metadata is available from cache
    let tenant_repo = container
        .tenant_repository()
        .expect("should have tenant repo");
    let tid = TenantId::new("stable-corp".to_string()).unwrap();
    let tenant = tenant_repo.find_by_id(&tid).await.unwrap();
    assert!(tenant.is_some(), "Tenant should be readable from cache");
}

// =============================================================================
// Test 7: Metadata survives restart (the critical integration test)
// =============================================================================

#[tokio::test]
async fn test_metadata_survives_restart() {
    let temp_dir = TempDir::new().unwrap();
    let system_dir = temp_dir.path().join("__system");

    // --- First boot ---
    {
        let repos =
            SystemBootstrap::initialize(system_dir.clone(), Some("Restart Test".to_string()))
                .await
                .unwrap();

        // Verify bootstrap tenant
        assert_eq!(repos.tenant_repository.count().await.unwrap(), 1);

        // Create additional tenant
        let tid2 = TenantId::new("extra-tenant".to_string()).unwrap();
        repos
            .tenant_repository
            .create(tid2, "Extra Tenant".to_string(), TenantQuotas::standard())
            .await
            .unwrap();
        assert_eq!(repos.tenant_repository.count().await.unwrap(), 2);

        // Set config values
        repos
            .config_repository
            .set("feature_x", json!(true), Some("admin"))
            .unwrap();
        repos
            .config_repository
            .set("max_events", json!(1000000), None)
            .unwrap();

        // Verify pre-restart state
        assert_eq!(repos.config_repository.count(), 2);
        assert_eq!(repos.system_store.total_events(), 4); // 2 tenant.created + 2 config.set
    }
    // --- repos dropped, simulates process shutdown ---

    // --- Second boot (restart) ---
    {
        let repos = SystemBootstrap::initialize(system_dir.clone(), None)
            .await
            .unwrap();

        // Tenants should survive
        assert_eq!(repos.tenant_repository.count().await.unwrap(), 2);

        let tid1 = TenantId::new("restart-test".to_string()).unwrap();
        let t1 = repos.tenant_repository.find_by_id(&tid1).await.unwrap();
        assert!(t1.is_some(), "Bootstrap tenant should survive restart");
        assert_eq!(t1.unwrap().name(), "Restart Test");

        let tid2 = TenantId::new("extra-tenant".to_string()).unwrap();
        let t2 = repos.tenant_repository.find_by_id(&tid2).await.unwrap();
        assert!(t2.is_some(), "Extra tenant should survive restart");

        // Config should survive
        assert_eq!(repos.config_repository.count(), 2);
        assert_eq!(
            repos.config_repository.get_value("feature_x").unwrap(),
            json!(true)
        );
        assert_eq!(
            repos.config_repository.get_value("max_events").unwrap(),
            json!(1000000)
        );
    }

    // --- Third boot (another restart, no bootstrap config) ---
    {
        let repos = SystemBootstrap::initialize(system_dir, Some("Restart Test".to_string()))
            .await
            .unwrap();

        // Should NOT create duplicate bootstrap tenant
        assert_eq!(repos.tenant_repository.count().await.unwrap(), 2);
    }
}

// =============================================================================
// Test 8: DI container wiring with system repositories
// =============================================================================

#[tokio::test]
async fn test_di_container_system_repo_wiring() {
    let temp_dir = TempDir::new().unwrap();
    let system_dir = temp_dir.path().join("__system");

    let repos = SystemBootstrap::initialize(system_dir, Some("DI Test".to_string()))
        .await
        .unwrap();

    let container = ContainerBuilder::new()
        .with_in_memory_repositories()
        .with_system_repositories(repos)
        .build();

    // System repos should be available
    assert!(container.has_system_repositories());
    assert!(container.tenant_repository().is_some());
    assert!(container.audit_repository().is_some());
    assert!(container.config_repository().is_some());
    assert!(container.system_store().is_some());

    // Tenant repo from DI should work
    let tenant_repo = container.tenant_repository().unwrap();
    let tid = TenantId::new("di-test".to_string()).unwrap();
    let tenant = tenant_repo.find_by_id(&tid).await.unwrap();
    assert!(tenant.is_some());
}

// =============================================================================
// Test 9: DI container without system repositories (backward compat)
// =============================================================================

#[test]
fn test_di_container_without_system_repos() {
    let container = ContainerBuilder::new()
        .with_in_memory_repositories()
        .build();

    assert!(!container.has_system_repositories());
    assert!(container.tenant_repository().is_none());
    assert!(container.audit_repository().is_none());
    assert!(container.config_repository().is_none());
    assert!(container.system_store().is_none());
}

// =============================================================================
// Test 10: System events are rejected from user-facing EventStore
// =============================================================================

#[test]
fn test_system_events_rejected_from_user_eventstore() {
    let store = EventStore::new();

    let event = allsource_core::domain::entities::Event::from_strings(
        "_system.tenant.created".to_string(),
        "_system:tenant:hack".to_string(),
        "_system".to_string(),
        json!({"name": "Hacked Tenant"}),
        None,
    )
    .unwrap();

    let result = store.ingest(&event);
    assert!(
        result.is_err(),
        "System events must be rejected from user-facing ingestion"
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("reserved for internal use")
    );
}
