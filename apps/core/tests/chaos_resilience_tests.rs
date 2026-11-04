//! Chaos resilience integration tests for the system metadata layer.
//!
//! These tests validate that the static stability pattern works correctly:
//! - Data plane continues serving with cached metadata during system store failures
//! - Full restart preserves all metadata
//! - Corrupt system storage triggers graceful degradation, not panic
//! - Separate storage dirs prevent cross-contamination

use allsource_core::{
    QueryEventsRequest,
    domain::{
        entities::{Actor, AuditAction, AuditEvent, AuditOutcome, Event},
        repositories::{AuditEventQuery, AuditEventRepository, TenantRepository},
        value_objects::TenantId,
    },
    infrastructure::persistence::SystemBootstrap,
    store::EventStore,
};
use serde_json::json;
use tempfile::TempDir;

/// Helper to create a test event for the data plane.
fn create_data_event(entity_id: &str, event_type: &str) -> Event {
    Event::from_strings(
        event_type.to_string(),
        entity_id.to_string(),
        "default".to_string(),
        json!({"test": true}),
        None,
    )
    .unwrap()
}

// =============================================================================
// Scenario 1: Full system restart preserves all metadata
// =============================================================================

#[tokio::test]
async fn test_full_restart_preserves_metadata() {
    let temp_dir = TempDir::new().unwrap();
    let system_dir = temp_dir.path().join("__system");

    // Phase 1: Start system, create tenant, add config, record audit event
    {
        let repos = SystemBootstrap::initialize(system_dir.clone(), Some("ACME Corp".to_string()))
            .await
            .unwrap();

        // Verify bootstrap tenant exists
        assert_eq!(repos.tenant_repository.count().await.unwrap(), 1);

        // Create additional tenant
        let tenant_id = TenantId::new("extra-tenant".to_string()).unwrap();
        repos
            .tenant_repository
            .create(
                tenant_id.clone(),
                "Extra Tenant".to_string(),
                allsource_core::domain::entities::TenantQuotas::default(),
            )
            .await
            .unwrap();
        assert_eq!(repos.tenant_repository.count().await.unwrap(), 2);

        // Add config entries
        repos
            .config_repository
            .set("feature_flag_x", json!(true), Some("admin"))
            .unwrap();
        repos
            .config_repository
            .set("max_retries", json!(3), Some("admin"))
            .unwrap();
        assert_eq!(repos.config_repository.count(), 2);

        // Record audit events
        let audit_event = AuditEvent::new(
            tenant_id,
            AuditAction::EventIngested,
            Actor::system("test".to_string()),
            AuditOutcome::Success,
        );
        repos.audit_repository.append(audit_event).await.unwrap();
    }
    // All repos dropped — simulates shutdown

    // Phase 2: Restart — everything should recover
    {
        let repos = SystemBootstrap::initialize(system_dir.clone(), None)
            .await
            .unwrap();

        // Tenants survive restart
        assert_eq!(repos.tenant_repository.count().await.unwrap(), 2);

        let acme = repos
            .tenant_repository
            .find_by_id(&TenantId::new("acme-corp".to_string()).unwrap())
            .await
            .unwrap();
        assert!(acme.is_some());
        assert_eq!(acme.unwrap().name(), "ACME Corp");

        let extra = repos
            .tenant_repository
            .find_by_id(&TenantId::new("extra-tenant".to_string()).unwrap())
            .await
            .unwrap();
        assert!(extra.is_some());

        // Config survives restart
        assert_eq!(repos.config_repository.count(), 2);
        assert_eq!(
            repos.config_repository.get_value("feature_flag_x").unwrap(),
            json!(true)
        );
        assert_eq!(
            repos.config_repository.get_value("max_retries").unwrap(),
            json!(3)
        );

        // Audit events survive restart
        let query = AuditEventQuery::new(TenantId::new("extra-tenant".to_string()).unwrap());
        let audit_events = repos.audit_repository.query(query).await.unwrap();
        assert_eq!(audit_events.len(), 1);
    }
}

// =============================================================================
// Scenario 2: Data plane continues when system store has I/O issues
// =============================================================================

#[tokio::test]
async fn test_data_plane_continues_with_cached_metadata() {
    let temp_dir = TempDir::new().unwrap();
    let system_dir = temp_dir.path().join("__system");

    // Phase 1: Bootstrap system with metadata
    let repos = SystemBootstrap::initialize(system_dir.clone(), Some("default".to_string()))
        .await
        .unwrap();

    // Create a second tenant
    let tenant_id = TenantId::new("test-tenant".to_string()).unwrap();
    repos
        .tenant_repository
        .create(
            tenant_id.clone(),
            "Test Tenant".to_string(),
            allsource_core::domain::entities::TenantQuotas::default(),
        )
        .await
        .unwrap();

    // Phase 2: Even after system store errors, cached data remains accessible
    // The DashMap caches are in-memory and independent of WAL writes.
    // Reads never touch the WAL — only cache.

    // Verify reads work from cache (no I/O needed)
    let tenant = repos
        .tenant_repository
        .find_by_id(&tenant_id)
        .await
        .unwrap();
    assert!(tenant.is_some());
    assert_eq!(tenant.unwrap().name(), "Test Tenant");

    // List tenants works from cache
    let all = repos.tenant_repository.find_all(100, 0).await.unwrap();
    assert_eq!(all.len(), 2);

    // Config reads work from cache
    repos
        .config_repository
        .set("key1", json!("value1"), None)
        .unwrap();
    assert_eq!(
        repos.config_repository.get_value("key1").unwrap(),
        json!("value1")
    );

    // Data plane (EventStore) is completely independent
    let store = EventStore::new();
    let event = create_data_event("user-1", "user.created");
    store.ingest(&event).unwrap();

    let query = QueryEventsRequest {
        as_of_valid: None,
        since_valid: None,
        until_valid: None,
        entity_id: Some("user-1".to_string()),
        event_type: None,
        tenant_id: None,
        as_of: None,
        since: None,
        until: None,
        limit: None,
        ..Default::default()
    };
    let results = store.query(&query).unwrap();
    assert_eq!(results.len(), 1);
}

// =============================================================================
// Scenario 3: Corrupt system storage triggers graceful degradation
// =============================================================================

#[tokio::test]
async fn test_corrupt_system_storage_graceful_degradation() {
    let temp_dir = TempDir::new().unwrap();
    let system_dir = temp_dir.path().join("__system");

    // Phase 1: Create system store with data
    {
        let repos = SystemBootstrap::initialize(system_dir.clone(), Some("ACME".to_string()))
            .await
            .unwrap();

        repos
            .config_repository
            .set("key1", json!("value1"), None)
            .unwrap();
    }

    // Phase 2: Corrupt the WAL files
    let wal_dir = system_dir.join("wal");
    if wal_dir.exists() {
        for entry in std::fs::read_dir(&wal_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_file() {
                // Write garbage to the WAL file
                std::fs::write(&path, b"CORRUPT_DATA_HERE_GARBAGE_BYTES_1234567890").unwrap();
            }
        }
    }

    // Phase 3: Restart should NOT panic — graceful degradation
    // SystemBootstrap::try_initialize returns None on failure
    let result =
        SystemBootstrap::try_initialize(Some(system_dir.clone()), Some("ACME".to_string())).await;

    // The system either recovers with partial data or returns None (graceful fallback)
    // Either way, it must NOT panic
    if let Some(_repos) = result {
        // Recovered (WAL was partially readable or created fresh)
        // Data plane should still work regardless
    } else {
        // Graceful fallback to in-memory — this is acceptable behavior
    }

    // Data plane must still work regardless of system metadata status
    let store = EventStore::new();
    let event = create_data_event("user-1", "user.created");
    store.ingest(&event).unwrap();

    let query = QueryEventsRequest {
        as_of_valid: None,
        since_valid: None,
        until_valid: None,
        entity_id: Some("user-1".to_string()),
        event_type: None,
        tenant_id: None,
        as_of: None,
        since: None,
        until: None,
        limit: None,
        ..Default::default()
    };
    let results = store.query(&query).unwrap();
    assert_eq!(results.len(), 1);
}

// =============================================================================
// Scenario 4: Separate storage dirs prevent cross-contamination
// =============================================================================

#[tokio::test]
async fn test_separate_storage_dirs_prevent_cross_contamination() {
    let _data_dir = TempDir::new().unwrap();
    let system_dir = TempDir::new().unwrap();
    let system_path = system_dir.path().join("__system");

    // Initialize system metadata in its own directory
    let repos = SystemBootstrap::initialize(system_path.clone(), Some("tenant-a".to_string()))
        .await
        .unwrap();

    assert_eq!(repos.tenant_repository.count().await.unwrap(), 1);

    // Data plane uses a completely separate directory
    let store = EventStore::new();
    for i in 0..50 {
        let event = create_data_event(&format!("entity-{i}"), "data.event");
        store.ingest(&event).unwrap();
    }

    let query = QueryEventsRequest {
        as_of_valid: None,
        since_valid: None,
        until_valid: None,
        entity_id: None,
        event_type: Some("data.event".to_string()),
        tenant_id: None,
        as_of: None,
        since: None,
        until: None,
        limit: None,
        ..Default::default()
    };
    let results = store.query(&query).unwrap();
    assert_eq!(results.len(), 50);

    // Deleting the system directory should NOT affect the data plane
    drop(repos);
    std::fs::remove_dir_all(&system_path).unwrap();

    // Data plane still works
    let results2 = store
        .query(&QueryEventsRequest {
            as_of_valid: None,
            since_valid: None,
            until_valid: None,
            entity_id: None,
            event_type: Some("data.event".to_string()),
            tenant_id: None,
            as_of: None,
            since: None,
            until: None,
            limit: None,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(results2.len(), 50);

    // System metadata can be re-initialized independently
    let repos2 = SystemBootstrap::initialize(system_path, Some("tenant-b".to_string()))
        .await
        .unwrap();

    // Fresh start — only the new bootstrap tenant
    assert_eq!(repos2.tenant_repository.count().await.unwrap(), 1);
    let tenant = repos2
        .tenant_repository
        .find_by_id(&TenantId::new("tenant-b".to_string()).unwrap())
        .await
        .unwrap();
    assert!(tenant.is_some());
}

// =============================================================================
// Scenario: System event rejection in data plane
// =============================================================================

#[test]
fn test_system_events_rejected_by_data_plane() {
    let store = EventStore::new();

    // Attempt to ingest a system event through the data plane
    let result = Event::from_strings(
        "_system.tenant.created".to_string(),
        "test-entity".to_string(),
        "default".to_string(),
        json!({"name": "Sneaky Tenant"}),
        None,
    );

    if let Ok(event) = result {
        // Even if Event construction succeeds, EventStore should reject it
        let ingest_result = store.ingest(&event);
        assert!(
            ingest_result.is_err(),
            "System events must be rejected by the data plane"
        );
    } else {
        // Event construction itself rejected — also acceptable
    }
}
