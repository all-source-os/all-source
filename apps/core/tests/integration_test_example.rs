/// Integration tests for AllSource v1.0
///
/// These tests demonstrate end-to-end workflows for:
/// - Authentication flow
/// - Multi-tenant operations
/// - Rate limiting
/// - Event ingestion with auth
use allsource_core::{
    QueryEventsRequest,
    auth::{AuthManager, Permission, Role},
    domain::{
        entities::{Event, TenantQuotas},
        repositories::TenantRepository,
        value_objects::TenantId,
    },
    infrastructure::repositories::InMemoryTenantRepository,
    rate_limit::{RateLimitConfig, RateLimiter},
    store::EventStore,
};
use std::sync::Arc;

#[test]
fn test_complete_auth_flow() {
    // 1. Create auth manager
    let auth_manager = Arc::new(AuthManager::default());

    // 2. Register a user
    let user = auth_manager
        .register_user(
            "test_user".to_string(),
            "test@example.com".to_string(),
            "secure_password_123",
            Role::Developer,
            "default".to_string(),
        )
        .expect("Failed to register user");

    assert_eq!(user.username, "test_user");
    assert_eq!(user.role, Role::Developer);

    // 3. Authenticate with password
    let token = auth_manager
        .authenticate("test_user", "secure_password_123")
        .expect("Failed to authenticate");

    assert!(!token.is_empty());

    // 4. Validate token
    let claims = auth_manager
        .validate_token(&token)
        .expect("Failed to validate token");

    assert_eq!(claims.sub, user.id.to_string());
    assert_eq!(claims.tenant_id, "default");

    // 5. Create API key
    let (_api_key, key_string) = auth_manager.create_api_key(
        "test_api_key".to_string(),
        "default".to_string(),
        Role::ServiceAccount,
        None,
    );

    assert!(key_string.starts_with("ask_"));

    // 6. Validate API key
    let api_claims = auth_manager
        .validate_api_key(&key_string)
        .expect("Failed to validate API key");

    assert_eq!(api_claims.tenant_id, "default");
    assert_eq!(api_claims.role, Role::ServiceAccount);

    println!("Complete auth flow test passed!");
}

#[tokio::test]
async fn test_multi_tenant_isolation() {
    let repo = InMemoryTenantRepository::new();

    // 1. Create two tenants
    let tenant1 = repo
        .create(
            TenantId::new("tenant1".to_string()).unwrap(),
            "Tenant 1".to_string(),
            TenantQuotas::professional(),
        )
        .await
        .expect("Failed to create tenant1");

    let tenant2 = repo
        .create(
            TenantId::new("tenant2".to_string()).unwrap(),
            "Tenant 2".to_string(),
            TenantQuotas::free_tier(),
        )
        .await
        .expect("Failed to create tenant2");

    assert_eq!(tenant1.id().as_str(), "tenant1");
    assert_eq!(tenant2.id().as_str(), "tenant2");

    // 2. Verify different quota tiers
    assert_eq!(tenant1.quotas().max_events_per_day(), 1_000_000); // Professional
    assert_eq!(tenant2.quotas().max_events_per_day(), 10_000); // Free

    println!("Multi-tenant isolation test passed!");
}

#[test]
fn test_rate_limiting_enforcement() {
    let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig {
        requests_per_minute: 5,
        burst_size: 5,
    }));

    // 1. First 5 requests should succeed (burst)
    for i in 0..5 {
        let result = rate_limiter.check_rate_limit("test_tenant");
        assert!(result.allowed, "Request {} should be allowed", i + 1);
    }

    // 2. 6th request should be rate limited
    let result = rate_limiter.check_rate_limit("test_tenant");
    assert!(!result.allowed, "Request 6 should be rate limited");
    assert!(result.retry_after.is_some());

    println!("Rate limiting enforcement test passed!");
}

#[test]
fn test_event_store_with_tenants() {
    let store = Arc::new(EventStore::new());

    // Create events for different tenants using from_strings
    let event1 = Event::from_strings(
        "user.created".to_string(),
        "user-123".to_string(),
        "tenant1".to_string(),
        serde_json::json!({"name": "Alice"}),
        None,
    )
    .unwrap();

    let event2 = Event::from_strings(
        "user.created".to_string(),
        "user-456".to_string(),
        "tenant2".to_string(),
        serde_json::json!({"name": "Bob"}),
        None,
    )
    .unwrap();

    let event1_entity_id = event1.entity_id().to_string();

    // Ingest events
    store.ingest(&event1).expect("Failed to ingest event1");
    store.ingest(&event2).expect("Failed to ingest event2");

    // Query by entity using query method
    let query = QueryEventsRequest {
        as_of_valid: None,
        since_valid: None,
        until_valid: None,
        entity_id: Some(event1_entity_id),
        event_type: None,
        tenant_id: None,
        as_of: None,
        since: None,
        until: None,
        limit: None,
        ..Default::default()
    };
    let entity_events = store.query(&query).expect("Failed to query by entity");

    assert_eq!(entity_events.len(), 1);
    assert_eq!(entity_events[0].tenant_id_str(), "tenant1");

    println!("Event store with tenants test passed!");
}

#[test]
fn test_permission_based_access() {
    let auth_manager = Arc::new(AuthManager::default());

    // Create users with different roles
    let admin = auth_manager
        .register_user(
            "admin".to_string(),
            "admin@example.com".to_string(),
            "password",
            Role::Admin,
            "default".to_string(),
        )
        .expect("Failed to create admin");

    let developer = auth_manager
        .register_user(
            "dev".to_string(),
            "dev@example.com".to_string(),
            "password",
            Role::Developer,
            "default".to_string(),
        )
        .expect("Failed to create developer");

    let readonly = auth_manager
        .register_user(
            "reader".to_string(),
            "reader@example.com".to_string(),
            "password",
            Role::ReadOnly,
            "default".to_string(),
        )
        .expect("Failed to create readonly user");

    // Admin can do everything
    assert!(admin.role.has_permission(Permission::Admin));
    assert!(admin.role.has_permission(Permission::Write));
    assert!(admin.role.has_permission(Permission::Read));

    // Developer can read and write
    assert!(!developer.role.has_permission(Permission::Admin));
    assert!(developer.role.has_permission(Permission::Write));
    assert!(developer.role.has_permission(Permission::Read));

    // ReadOnly can only read
    assert!(!readonly.role.has_permission(Permission::Admin));
    assert!(!readonly.role.has_permission(Permission::Write));
    assert!(readonly.role.has_permission(Permission::Read));

    println!("Permission-based access test passed!");
}

#[tokio::test]
async fn test_quota_enforcement() {
    let repo = InMemoryTenantRepository::new();

    // Create tenant with very low quota (1 event per day)
    let quotas = TenantQuotas::new(1, 10_737_418_240, 100_000, 10, 50, 20);

    let tenant = repo
        .create(
            TenantId::new("low_quota".to_string()).unwrap(),
            "Low Quota Tenant".to_string(),
            quotas,
        )
        .await
        .expect("Failed to create tenant");

    // First event should succeed (domain Tenant enforces quotas)
    let mut tenant = tenant;
    assert!(tenant.can_ingest_event().is_ok());

    // Record an event via usage tracking
    tenant.usage_mut().record_event();
    let tid = TenantId::new("low_quota".to_string()).unwrap();
    repo.save(&tenant).await.unwrap();

    // Second event should fail (quota exceeded)
    let mut tenant = repo.find_by_id(&tid).await.unwrap().unwrap();
    assert!(
        tenant.can_ingest_event().is_err(),
        "Second event should exceed quota"
    );

    println!("Quota enforcement test passed!");
}
