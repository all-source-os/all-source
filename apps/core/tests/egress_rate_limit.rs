use allsource_core::infrastructure::security::{
    auth::{Claims, Role},
    middleware::{Admin, AuthContext, RateLimitState, rate_limit_middleware},
    rate_limit::{RateLimitConfig, RateLimiter},
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    middleware,
    routing::{get, post},
};
use std::sync::Arc;
use tower::ServiceExt;

fn router() -> Router {
    let state = RateLimitState {
        rate_limiter: Arc::new(RateLimiter::new(RateLimitConfig {
            requests_per_minute: 1,
            burst_size: 1,
        })),
        egress_rate_limiter: Arc::new(RateLimiter::new(RateLimitConfig {
            requests_per_minute: 1,
            burst_size: 2,
        })),
    };
    Router::new()
        .route("/api/v1/auth/me", get(|| async { StatusCode::OK }))
        .route(
            "/api/v1/tenants/{tenant}/egress",
            post(|Admin(_): Admin| async { StatusCode::OK }),
        )
        .layer(middleware::from_fn_with_state(state, rate_limit_middleware))
}

async fn request(router: &Router, path: &str, role: Option<Role>) -> StatusCode {
    let mut request = Request::builder()
        .method(if path.ends_with("/egress") {
            "POST"
        } else {
            "GET"
        })
        .uri(path)
        .body(Body::empty())
        .unwrap();
    if let Some(role) = role {
        request.extensions_mut().insert(AuthContext {
            claims: Claims::new(
                "service".into(),
                "shared-service-tenant".into(),
                role,
                chrono::Duration::minutes(1),
            ),
        });
    }
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        router.clone().oneshot(request),
    )
    .await
    .expect("middleware request deadline")
    .unwrap()
    .status()
}

#[tokio::test]
async fn ledger_and_metadata_buckets_are_independent_and_finite() {
    let app = router();
    let ledger = "/api/v1/tenants/tenant-a/egress";
    assert_eq!(
        request(&app, "/api/v1/auth/me", Some(Role::Admin)).await,
        StatusCode::OK
    );
    assert_eq!(
        request(&app, ledger, Some(Role::Admin)).await,
        StatusCode::OK
    );
    assert_eq!(
        request(&app, ledger, Some(Role::Admin)).await,
        StatusCode::OK
    );
    assert_eq!(
        request(&app, "/api/v1/auth/me", Some(Role::Admin)).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        request(&app, ledger, Some(Role::Admin)).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        request(&app, "/api/v1/tenants/tenant-b/egress", Some(Role::Admin)).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn non_admin_cannot_use_the_ledger_lane() {
    let app = router();
    assert_eq!(
        request(
            &app,
            "/api/v1/tenants/tenant-a/egress",
            Some(Role::ReadOnly)
        )
        .await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(&app, "/api/v1/auth/me", Some(Role::ReadOnly)).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        request(&app, "/api/v1/tenants/tenant-a/egress", Some(Role::Admin)).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn ledger_still_requires_authentication() {
    assert_eq!(
        request(&router(), "/api/v1/tenants/tenant-a/egress", None).await,
        StatusCode::UNAUTHORIZED
    );
}

#[test]
fn ledger_allowance_is_finite_without_changing_metadata_limit() {
    let state = RateLimitState::new(Arc::new(RateLimiter::default()));
    assert_eq!(state.rate_limiter.check_rate_limit("tenant").limit, 600);
    assert_eq!(
        state.egress_rate_limiter.check_rate_limit("tenant").limit,
        1200
    );
}
