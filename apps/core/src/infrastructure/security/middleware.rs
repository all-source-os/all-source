use crate::{
    error::AllSourceError,
    infrastructure::security::{
        auth::{AuthManager, Claims, Permission, Role},
        rate_limit::{RateLimitConfig, RateLimiter},
    },
};
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::{Arc, LazyLock};

/// Paths that bypass authentication (exact match)
pub const AUTH_SKIP_PATHS: &[&str] = &[
    "/health",
    "/metrics",
    "/api/v1/auth/register",
    "/api/v1/auth/login",
    "/api/v1/demo/seed",
];

/// Path prefixes that bypass authentication and rate limiting.
///
/// - `/internal/` — sentinel failover endpoints; must not require API keys or
///   be rate-limited, otherwise failover can timeout or fail when credentials
///   are unavailable.
/// - `/api/v1/events`, `/api/v1/projections`, `/api/v1/snapshots`,
///   `/api/v1/schemas`, `/api/v1/consumers`, `/api/v1/stats`,
///   `/api/v1/entities`, `/api/v1/duplicates`, `/api/v1/streams`,
///   `/api/v1/replay`, `/api/v1/webhooks`, `/api/v1/compaction`,
///   `/api/v1/pipelines`, `/api/v1/graphql` — data-plane routes. Core is
///   internal-only (bead t-0ff8); the gateway (Control Plane's delegation
///   layer) authenticates and scopes the caller before forwarding. Handlers
///   read `tenant_id` from request body or query param — that's the
///   authoritative source. Auth middleware running on these paths was legacy
///   defense-in-depth that added no real protection once Core went behind
///   Fly's private network.
pub const AUTH_SKIP_PREFIXES: &[&str] = &[
    "/internal/",
    "/api/v1/events",
    "/api/v1/projections",
    "/api/v1/snapshots",
    "/api/v1/schemas",
    "/api/v1/consumers",
    "/api/v1/stats",
    "/api/v1/entities",
    "/api/v1/duplicates",
    "/api/v1/streams",
    "/api/v1/replay",
    "/api/v1/webhooks",
    "/api/v1/compaction",
    "/api/v1/pipelines",
    "/api/v1/graphql",
];

/// Check if a path should skip authentication and rate limiting.
///
/// Paths that STILL require auth after the internal-only cutover:
/// - `/api/v1/auth/me` — used by CP's ValidateAPIKey fallback for legacy keys
/// - `/api/v1/auth/api-keys` — admin-only; used by CP's CreateCoreAPIKey
/// - `/api/v1/tenants/*` — admin-only tenant management
///
/// Everything else is skipped: internal network trust + handler-level
/// tenant-from-body is the security model now.
#[inline]
pub fn should_skip_auth(path: &str) -> bool {
    AUTH_SKIP_PATHS.contains(&path) || AUTH_SKIP_PREFIXES.iter().any(|pfx| path.starts_with(pfx))
}

/// Check if development mode is enabled via environment variable.
/// When enabled, authentication and rate limiting are bypassed for local
/// development (e.g., MCP server integration, quick curl queries against
/// `/api/v1/events/query` without generating a JWT).
///
/// Set any of the following to `true`/`1` to enable:
/// - `ALLSOURCE_DEV_MODE` — historical name, still supported
/// - `ALLSOURCE_AUTH_DISABLED` — explicit "turn auth off" alias (issue #131)
///
/// **WARNING**: Never enable this in production environments! The feature
/// grants admin context to any request that arrives without a token.
fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "yes"))
}

static DEV_MODE_ENABLED: LazyLock<bool> = LazyLock::new(|| {
    let via_dev = env_flag_enabled("ALLSOURCE_DEV_MODE");
    let via_auth_off = env_flag_enabled("ALLSOURCE_AUTH_DISABLED");
    let enabled = via_dev || via_auth_off;
    if enabled {
        let source = if via_auth_off && via_dev {
            "ALLSOURCE_DEV_MODE + ALLSOURCE_AUTH_DISABLED"
        } else if via_auth_off {
            "ALLSOURCE_AUTH_DISABLED"
        } else {
            "ALLSOURCE_DEV_MODE"
        };
        tracing::warn!(
            "⚠️  Auth disabled via {source} — all requests run as admin with no rate limits. DO NOT use in production."
        );
    }
    enabled
});

/// Check if dev mode is enabled
#[inline]
pub fn is_dev_mode() -> bool {
    *DEV_MODE_ENABLED
}

/// Create a development-mode AuthContext with admin privileges
fn dev_mode_auth_context() -> AuthContext {
    AuthContext {
        claims: Claims::new(
            "dev-user".to_string(),
            "dev-tenant".to_string(),
            Role::Admin,
            chrono::Duration::hours(24),
        ),
    }
}

/// Authentication state shared across requests
#[derive(Clone)]
pub struct AuthState {
    pub auth_manager: Arc<AuthManager>,
}

/// Rate limiting state
#[derive(Clone)]
pub struct RateLimitState {
    pub rate_limiter: Arc<RateLimiter>,
    pub egress_rate_limiter: Arc<RateLimiter>,
    pub read_model_reads: Arc<RateLimiter>,
    pub read_model_writes: Arc<RateLimiter>,
}

impl RateLimitState {
    pub fn new(rate_limiter: Arc<RateLimiter>) -> Self {
        Self {
            rate_limiter,
            read_model_reads: Arc::new(RateLimiter::new(RateLimitConfig {
                requests_per_minute: 6000,
                burst_size: 1000,
            })),
            read_model_writes: Arc::new(RateLimiter::new(RateLimitConfig {
                requests_per_minute: 600,
                burst_size: 120,
            })),
            // Each admitted data response reserves and completes one ledger entry.
            egress_rate_limiter: Arc::new(RateLimiter::new(RateLimitConfig {
                requests_per_minute: 1200,
                burst_size: 2000,
            })),
        }
    }
}

/// Authenticated request context
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub claims: Claims,
}

impl AuthContext {
    /// Check if user has required permission
    pub fn require_permission(&self, permission: Permission) -> Result<(), AllSourceError> {
        if self.claims.has_permission(permission) {
            Ok(())
        } else {
            Err(AllSourceError::ValidationError(
                "Insufficient permissions".to_string(),
            ))
        }
    }

    /// Get tenant ID from context
    pub fn tenant_id(&self) -> &str {
        &self.claims.tenant_id
    }

    /// Get user ID from context
    pub fn user_id(&self) -> &str {
        &self.claims.sub
    }
}

/// Extract token from Authorization header, with X-API-Key fallback for backwards compatibility.
fn extract_token(headers: &HeaderMap) -> Result<String, AllSourceError> {
    // Primary: Authorization header (Bearer <token> or plain <token>)
    // Fallback: X-API-Key header (legacy, deprecated)
    let auth_header = if let Some(val) = headers.get("authorization") {
        val.to_str()
            .map_err(|_| {
                AllSourceError::ValidationError("Invalid authorization header".to_string())
            })?
            .to_string()
    } else if let Some(val) = headers.get("x-api-key") {
        val.to_str()
            .map_err(|_| AllSourceError::ValidationError("Invalid X-API-Key header".to_string()))?
            .to_string()
    } else {
        return Err(AllSourceError::ValidationError(
            "Missing authorization header".to_string(),
        ));
    };

    // Support both "Bearer <token>" and "<token>" formats
    let token = if auth_header.starts_with("Bearer ") {
        auth_header.trim_start_matches("Bearer ").trim()
    } else if auth_header.starts_with("bearer ") {
        auth_header.trim_start_matches("bearer ").trim()
    } else {
        auth_header.trim()
    };

    if token.is_empty() {
        return Err(AllSourceError::ValidationError(
            "Empty authorization token".to_string(),
        ));
    }

    Ok(token.to_string())
}

/// Returns true for paths that require Admin permission regardless of other checks.
///
/// - POST /api/v1/auth/api-keys — key creation (agents must not self-replicate)
/// - /api/v1/tenants/* — tenant management (agents must not alter tenants)
///
/// Note: /api/v1/auth/register is in AUTH_SKIP_PATHS (public self-registration) and
/// additionally enforces Admin at the handler level when any auth token is present.
#[inline]
pub fn is_admin_only_path(path: &str, method: &str) -> bool {
    (path == "/api/v1/auth/api-keys" && method == "POST") || path.starts_with("/api/v1/tenants")
}

/// Authentication middleware
pub async fn auth_middleware(
    State(auth_state): State<AuthState>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    // Skip authentication for public and internal paths
    let path = request.uri().path();
    if should_skip_auth(path) {
        return Ok(next.run(request).await);
    }

    // Dev mode: if a valid token is present, authenticate normally so that
    // /me returns the real tenant_id (not a hardcoded "dev-tenant").
    // Fall back to the synthetic dev context only when no token is provided.
    if is_dev_mode() {
        let headers = request.headers();
        let auth_ctx = match extract_token(headers) {
            Ok(token) => {
                let claims = if token.starts_with("ask_") {
                    auth_state.auth_manager.validate_api_key(&token).ok()
                } else {
                    auth_state.auth_manager.validate_token(&token).ok()
                };
                claims.map_or_else(dev_mode_auth_context, |c| AuthContext { claims: c })
            }
            Err(_) => dev_mode_auth_context(),
        };
        request.extensions_mut().insert(auth_ctx);
        return Ok(next.run(request).await);
    }

    let headers = request.headers();

    // Extract and validate token (JWT or API key)
    let token = extract_token(headers)?;

    let claims = if token.starts_with("ask_") {
        // API Key authentication
        auth_state.auth_manager.validate_api_key(&token)?
    } else {
        // JWT authentication
        auth_state.auth_manager.validate_token(&token)?
    };

    let auth_ctx = AuthContext { claims };

    // Enforce that service accounts (agent API keys) cannot access admin-only paths.
    // Admin-only paths: user registration, API key creation, and all tenant management.
    // These are also enforced at handler level; this middleware block provides defence-in-depth.
    let path = request.uri().path();
    let method = request.method().as_str();
    let is_admin_only_path = is_admin_only_path(path, method);

    if is_admin_only_path {
        auth_ctx
            .require_permission(Permission::Admin)
            .map_err(|_| {
                AuthError(AllSourceError::ValidationError(
                    "Admin permission required".to_string(),
                ))
            })?;
    }

    // Insert auth context into request extensions
    request.extensions_mut().insert(auth_ctx);

    Ok(next.run(request).await)
}

/// Optional authentication middleware (allows unauthenticated requests)
pub async fn optional_auth_middleware(
    State(auth_state): State<AuthState>,
    mut request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();

    if let Ok(token) = extract_token(headers) {
        // Try to authenticate, but don't fail if invalid
        let claims = if token.starts_with("ask_") {
            auth_state.auth_manager.validate_api_key(&token).ok()
        } else {
            auth_state.auth_manager.validate_token(&token).ok()
        };

        if let Some(claims) = claims {
            request.extensions_mut().insert(AuthContext { claims });
        }
    }

    next.run(request).await
}

/// Error type for authentication failures
#[derive(Debug)]
pub struct AuthError(AllSourceError);

impl From<AllSourceError> for AuthError {
    fn from(err: AllSourceError) -> Self {
        AuthError(err)
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self.0 {
            AllSourceError::ValidationError(msg) => (StatusCode::UNAUTHORIZED, msg),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
            ),
        };

        (status, message).into_response()
    }
}

/// Axum extractor for authenticated requests
pub struct Authenticated(pub AuthContext);

impl<S> axum::extract::FromRequestParts<S> for Authenticated
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    // `async fn` is the signature axum declares for `FromRequestParts`; it is
    // not ours to change. Extensions are already populated by the auth layer,
    // so this particular extractor happens to need no `.await` — sibling
    // extractors that hit the token store do. Keep the trait's shape.
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthContext>()
            .cloned()
            .map(Authenticated)
            .ok_or((StatusCode::UNAUTHORIZED, "Unauthorized"))
    }
}

/// Axum extractor for optional authentication (never rejects, returns Option)
/// Use this for routes that work with or without authentication
pub struct OptionalAuth(pub Option<AuthContext>);

impl<S> axum::extract::FromRequestParts<S> for OptionalAuth
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    // `async fn` is dictated by axum's `FromRequestParts` trait, not by this body.
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(OptionalAuth(parts.extensions.get::<AuthContext>().cloned()))
    }
}

/// Axum extractor for admin-only requests
pub struct Admin(pub AuthContext);

impl<S> axum::extract::FromRequestParts<S> for Admin
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    // `async fn` is dictated by axum's `FromRequestParts` trait, not by this body.
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let auth_ctx = parts
            .extensions
            .get::<AuthContext>()
            .cloned()
            .ok_or((StatusCode::UNAUTHORIZED, "Unauthorized"))?;

        auth_ctx
            .require_permission(Permission::Admin)
            .map_err(|_| (StatusCode::FORBIDDEN, "Admin permission required"))?;

        Ok(Admin(auth_ctx))
    }
}

/// Rate limiting middleware
/// Checks rate limits based on tenant_id from auth context
pub async fn rate_limit_middleware(
    State(rate_limit_state): State<RateLimitState>,
    request: Request,
    next: Next,
) -> Result<Response, RateLimitError> {
    // Skip rate limiting for public and internal paths
    let path = request.uri().path();
    if should_skip_auth(path) {
        return Ok(next.run(request).await);
    }

    // Dev mode: bypass rate limiting entirely
    if is_dev_mode() {
        return Ok(next.run(request).await);
    }

    // Extract auth context from request
    let auth_ctx = request
        .extensions()
        .get::<AuthContext>()
        .ok_or(RateLimitError::Unauthorized)?;

    let ledger_tenant = path
        .strip_prefix("/api/v1/tenants/")
        .and_then(|suffix| suffix.strip_suffix("/egress"))
        .filter(|tenant| !tenant.is_empty() && !tenant.contains('/'))
        .filter(|_| auth_ctx.claims.has_permission(Permission::Admin));
    let read_model_path = path == "/api/v1/event-feed"
        || path
            .strip_prefix("/api/v1/read-models/")
            .is_some_and(|name| !name.is_empty() && !name.contains('/'));
    let result = if read_model_path {
        if request.method() == axum::http::Method::GET {
            rate_limit_state
                .read_model_reads
                .check_rate_limit(auth_ctx.tenant_id())
        } else {
            rate_limit_state
                .read_model_writes
                .check_rate_limit(auth_ctx.tenant_id())
        }
    } else if let Some(tenant) = ledger_tenant {
        rate_limit_state
            .egress_rate_limiter
            .check_rate_limit(tenant)
    } else {
        rate_limit_state
            .rate_limiter
            .check_rate_limit(auth_ctx.tenant_id())
    };

    if !result.allowed {
        return Err(RateLimitError::RateLimitExceeded {
            retry_after: result.retry_after.unwrap_or_default().as_secs(),
            limit: result.limit,
        });
    }

    // Add rate limit headers to response
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        "X-RateLimit-Limit",
        result.limit.to_string().parse().unwrap(),
    );
    headers.insert(
        "X-RateLimit-Remaining",
        result.remaining.to_string().parse().unwrap(),
    );

    Ok(response)
}

/// Error type for rate limiting failures
#[derive(Debug)]
pub enum RateLimitError {
    RateLimitExceeded { retry_after: u64, limit: u32 },
    Unauthorized,
}

impl IntoResponse for RateLimitError {
    fn into_response(self) -> Response {
        match self {
            RateLimitError::RateLimitExceeded { retry_after, limit } => {
                let mut response = (
                    StatusCode::TOO_MANY_REQUESTS,
                    format!("Rate limit exceeded. Limit: {limit} requests/min"),
                )
                    .into_response();

                if retry_after > 0 {
                    response
                        .headers_mut()
                        .insert("Retry-After", retry_after.to_string().parse().unwrap());
                }

                response
            }
            RateLimitError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "Authentication required for rate limiting",
            )
                .into_response(),
        }
    }
}

/// Helper macro to require specific permission
#[macro_export]
macro_rules! require_permission {
    ($auth:expr, $perm:expr) => {
        $auth.0.require_permission($perm).map_err(|_| {
            (
                axum::http::StatusCode::FORBIDDEN,
                "Insufficient permissions",
            )
        })?
    };
}

// ============================================================================
// Tenant Isolation Middleware (Phase 5B)
// ============================================================================

use crate::domain::{entities::Tenant, repositories::TenantRepository, value_objects::TenantId};

/// Tenant isolation state for middleware
#[derive(Clone)]
pub struct TenantState<R: TenantRepository> {
    pub tenant_repository: Arc<R>,
}

/// Validated tenant context injected into requests
///
/// This context is created by the tenant_isolation_middleware after
/// validating that the tenant exists and is active.
#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant: Tenant,
}

impl TenantContext {
    /// Get the tenant ID
    pub fn tenant_id(&self) -> &TenantId {
        self.tenant.id()
    }

    /// Check if tenant is active
    pub fn is_active(&self) -> bool {
        self.tenant.is_active()
    }
}

/// Tenant isolation middleware
///
/// Validates that the authenticated tenant exists and is active.
/// Injects TenantContext into the request for use by handlers.
///
/// # Phase 5B: Tenant Isolation
/// This middleware enforces tenant boundaries by:
/// 1. Extracting tenant_id from AuthContext
/// 2. Loading tenant from repository
/// 3. Validating tenant is active
/// 4. Injecting TenantContext into request extensions
///
/// Must be applied after auth_middleware.
pub async fn tenant_isolation_middleware<R: TenantRepository + 'static>(
    State(tenant_state): State<TenantState<R>>,
    mut request: Request,
    next: Next,
) -> Result<Response, TenantError> {
    // Extract auth context (must be authenticated)
    let auth_ctx = request
        .extensions()
        .get::<AuthContext>()
        .ok_or(TenantError::Unauthorized)?
        .clone();

    // Parse tenant ID
    let tenant_id =
        TenantId::new(auth_ctx.tenant_id().to_string()).map_err(|_| TenantError::InvalidTenant)?;

    // Load tenant from repository
    let tenant = tenant_state
        .tenant_repository
        .find_by_id(&tenant_id)
        .await
        .map_err(|e| TenantError::RepositoryError(e.to_string()))?
        .ok_or(TenantError::TenantNotFound)?;

    // Validate tenant is active
    if !tenant.is_active() {
        return Err(TenantError::TenantInactive);
    }

    // Inject tenant context into request
    request.extensions_mut().insert(TenantContext { tenant });

    // Continue to next middleware/handler
    Ok(next.run(request).await)
}

/// Error type for tenant isolation failures
#[derive(Debug)]
pub enum TenantError {
    Unauthorized,
    InvalidTenant,
    TenantNotFound,
    TenantInactive,
    RepositoryError(String),
}

impl IntoResponse for TenantError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            TenantError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "Authentication required for tenant access",
            ),
            TenantError::InvalidTenant => (StatusCode::BAD_REQUEST, "Invalid tenant identifier"),
            TenantError::TenantNotFound => (StatusCode::NOT_FOUND, "Tenant not found"),
            TenantError::TenantInactive => (StatusCode::FORBIDDEN, "Tenant is inactive"),
            TenantError::RepositoryError(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to validate tenant",
            ),
        };

        (status, message).into_response()
    }
}

// ============================================================================
// Request ID Middleware (Phase 5C)
// ============================================================================

use uuid::Uuid;

/// Request context with unique ID for tracing
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestId {
    /// Generate a new request ID
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Get the request ID as a string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Request ID middleware
///
/// Generates a unique request ID for each request and injects it into:
/// - Request extensions (for use in handlers/logging)
/// - Response headers (X-Request-ID)
///
/// If the request already has an X-Request-ID header, it will be used instead.
///
/// # Phase 5C: Request Tracing
/// This middleware enables distributed tracing by:
/// 1. Generating unique IDs for each request
/// 2. Propagating IDs through the request lifecycle
/// 3. Returning IDs in response headers
/// 4. Supporting client-provided request IDs
pub async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    // Check if request already has a request ID
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map_or_else(RequestId::new, |s| RequestId(s.to_string()));

    // Store request ID in extensions
    request.extensions_mut().insert(request_id.clone());

    // Process request
    let mut response = next.run(request).await;

    // Add request ID to response headers
    response
        .headers_mut()
        .insert("x-request-id", request_id.0.parse().unwrap());

    response
}

// ============================================================================
// Security Headers Middleware (Phase 5C)
// ============================================================================

/// Security headers configuration
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Enable HSTS (HTTP Strict Transport Security)
    pub enable_hsts: bool,
    /// HSTS max age in seconds
    pub hsts_max_age: u32,
    /// Enable X-Frame-Options
    pub enable_frame_options: bool,
    /// X-Frame-Options value
    pub frame_options: FrameOptions,
    /// Enable X-Content-Type-Options
    pub enable_content_type_options: bool,
    /// Enable X-XSS-Protection
    pub enable_xss_protection: bool,
    /// Content Security Policy
    pub csp: Option<String>,
    /// CORS allowed origins
    pub cors_origins: Vec<String>,
    /// CORS allowed methods
    pub cors_methods: Vec<String>,
    /// CORS allowed headers
    pub cors_headers: Vec<String>,
    /// CORS max age
    pub cors_max_age: u32,
}

#[derive(Debug, Clone)]
pub enum FrameOptions {
    Deny,
    SameOrigin,
    AllowFrom(String),
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_hsts: true,
            hsts_max_age: 31_536_000, // 1 year
            enable_frame_options: true,
            frame_options: FrameOptions::Deny,
            enable_content_type_options: true,
            enable_xss_protection: true,
            csp: Some("default-src 'self'".to_string()),
            cors_origins: vec!["*".to_string()],
            cors_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "DELETE".to_string(),
            ],
            cors_headers: vec!["Content-Type".to_string(), "Authorization".to_string()],
            cors_max_age: 3600,
        }
    }
}

#[derive(Clone)]
pub struct SecurityState {
    pub config: SecurityConfig,
}

/// Security headers middleware
///
/// Adds security-related HTTP headers to all responses:
/// - HSTS (Strict-Transport-Security)
/// - X-Frame-Options
/// - X-Content-Type-Options
/// - X-XSS-Protection
/// - Content-Security-Policy
/// - CORS headers
///
/// # Phase 5C: Security Hardening
/// This middleware provides defense-in-depth by:
/// 1. Preventing clickjacking (X-Frame-Options)
/// 2. Preventing MIME sniffing (X-Content-Type-Options)
/// 3. Enforcing HTTPS (HSTS)
/// 4. Preventing XSS (CSP, X-XSS-Protection)
/// 5. Enabling CORS for controlled access
pub async fn security_headers_middleware(
    State(security_state): State<SecurityState>,
    request: Request,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    let config = &security_state.config;

    // HSTS
    if config.enable_hsts {
        headers.insert(
            "strict-transport-security",
            format!("max-age={}", config.hsts_max_age).parse().unwrap(),
        );
    }

    // X-Frame-Options
    if config.enable_frame_options {
        let value = match &config.frame_options {
            FrameOptions::Deny => "DENY",
            FrameOptions::SameOrigin => "SAMEORIGIN",
            FrameOptions::AllowFrom(origin) => origin,
        };
        headers.insert("x-frame-options", value.parse().unwrap());
    }

    // X-Content-Type-Options
    if config.enable_content_type_options {
        headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    }

    // X-XSS-Protection
    if config.enable_xss_protection {
        headers.insert("x-xss-protection", "1; mode=block".parse().unwrap());
    }

    // Content-Security-Policy
    if let Some(csp) = &config.csp {
        headers.insert("content-security-policy", csp.parse().unwrap());
    }

    // CORS headers
    headers.insert(
        "access-control-allow-origin",
        config.cors_origins.join(", ").parse().unwrap(),
    );
    headers.insert(
        "access-control-allow-methods",
        config.cors_methods.join(", ").parse().unwrap(),
    );
    headers.insert(
        "access-control-allow-headers",
        config.cors_headers.join(", ").parse().unwrap(),
    );
    headers.insert(
        "access-control-max-age",
        config.cors_max_age.to_string().parse().unwrap(),
    );

    response
}

// ============================================================================
// IP Filtering Middleware (Phase 5C)
// ============================================================================

use crate::infrastructure::security::IpFilter;
use std::net::SocketAddr;

#[derive(Clone)]
pub struct IpFilterState {
    pub ip_filter: Arc<IpFilter>,
}

/// IP filtering middleware
///
/// Blocks or allows requests based on IP address rules.
/// Supports both global and per-tenant IP filtering.
///
/// # Phase 5C: Access Control
/// This middleware provides IP-based access control by:
/// 1. Extracting client IP from request
/// 2. Checking against global and tenant-specific rules
/// 3. Blocking requests from unauthorized IPs
/// 4. Supporting both allowlists and blocklists
pub async fn ip_filter_middleware(
    State(ip_filter_state): State<IpFilterState>,
    request: Request,
    next: Next,
) -> Result<Response, IpFilterError> {
    // Extract client IP address
    let client_ip = request
        .extensions()
        .get::<axum::extract::ConnectInfo<SocketAddr>>()
        .map(|connect_info| connect_info.0.ip())
        .ok_or(IpFilterError::NoIpAddress)?;

    // Check if this is a tenant-scoped request
    let result = if let Some(tenant_ctx) = request.extensions().get::<TenantContext>() {
        // Tenant-specific filtering
        ip_filter_state
            .ip_filter
            .is_allowed_for_tenant(tenant_ctx.tenant_id(), &client_ip)
    } else {
        // Global filtering only
        ip_filter_state.ip_filter.is_allowed(&client_ip)
    };

    // Block if not allowed
    if !result.allowed {
        return Err(IpFilterError::Blocked {
            reason: result.reason,
        });
    }

    // Allow request to proceed
    Ok(next.run(request).await)
}

/// Error type for IP filtering failures
#[derive(Debug)]
pub enum IpFilterError {
    NoIpAddress,
    Blocked { reason: String },
}

impl IntoResponse for IpFilterError {
    fn into_response(self) -> Response {
        match self {
            IpFilterError::NoIpAddress => (
                StatusCode::BAD_REQUEST,
                "Unable to determine client IP address",
            )
                .into_response(),
            IpFilterError::Blocked { reason } => {
                (StatusCode::FORBIDDEN, format!("Access denied: {reason}")).into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::security::auth::Role;

    #[test]
    fn test_extract_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer test_token_123".parse().unwrap());

        let token = extract_token(&headers).unwrap();
        assert_eq!(token, "test_token_123");
    }

    #[test]
    fn test_extract_lowercase_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "bearer test_token_123".parse().unwrap());

        let token = extract_token(&headers).unwrap();
        assert_eq!(token, "test_token_123");
    }

    #[test]
    fn test_extract_plain_token() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "test_token_123".parse().unwrap());

        let token = extract_token(&headers).unwrap();
        assert_eq!(token, "test_token_123");
    }

    #[test]
    fn test_missing_auth_header() {
        let headers = HeaderMap::new();
        assert!(extract_token(&headers).is_err());
    }

    #[test]
    fn test_empty_auth_header() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "".parse().unwrap());
        assert!(extract_token(&headers).is_err());
    }

    #[test]
    fn test_bearer_with_empty_token() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer ".parse().unwrap());
        assert!(extract_token(&headers).is_err());
    }

    // -------------------------------------------------------------------------
    // US-008: Agent Permission Enforcement
    // -------------------------------------------------------------------------

    #[test]
    fn test_service_account_blocked_on_admin_paths() {
        // ServiceAccount (agent API key) must NOT have Admin permission.
        // The middleware uses require_permission(Permission::Admin) to block them.
        let claims = Claims::new(
            "agent-key".to_string(),
            "tenant1".to_string(),
            Role::ServiceAccount,
            chrono::Duration::hours(1),
        );
        let ctx = AuthContext { claims };
        assert!(
            ctx.require_permission(Permission::Admin).is_err(),
            "ServiceAccount must not have Admin permission"
        );
        // But it should still be able to read/write events
        assert!(ctx.require_permission(Permission::Read).is_ok());
        assert!(ctx.require_permission(Permission::Write).is_ok());
    }

    #[test]
    fn test_admin_role_passes_admin_paths() {
        // Admin role must have Admin permission — passes admin-only paths.
        let claims = Claims::new(
            "admin-user".to_string(),
            "tenant1".to_string(),
            Role::Admin,
            chrono::Duration::hours(1),
        );
        let ctx = AuthContext { claims };
        assert!(
            ctx.require_permission(Permission::Admin).is_ok(),
            "Admin must have Admin permission"
        );
    }

    #[test]
    fn test_developer_blocked_on_admin_paths() {
        // Developer role cannot access admin-only paths.
        let claims = Claims::new(
            "dev-user".to_string(),
            "tenant1".to_string(),
            Role::Developer,
            chrono::Duration::hours(1),
        );
        let ctx = AuthContext { claims };
        assert!(
            ctx.require_permission(Permission::Admin).is_err(),
            "Developer must not have Admin permission"
        );
    }

    #[test]
    fn test_readonly_blocked_on_admin_paths() {
        let claims = Claims::new(
            "ro-user".to_string(),
            "tenant1".to_string(),
            Role::ReadOnly,
            chrono::Duration::hours(1),
        );
        let ctx = AuthContext { claims };
        assert!(ctx.require_permission(Permission::Admin).is_err());
        assert!(ctx.require_permission(Permission::Read).is_ok());
        assert!(ctx.require_permission(Permission::Write).is_err());
    }

    #[test]
    fn test_is_admin_only_path_api_keys_create() {
        // POST to api-keys is admin-only (agent keys must not self-replicate)
        assert!(is_admin_only_path("/api/v1/auth/api-keys", "POST"));
        // Other methods on api-keys are NOT admin-only (GET to list keys)
        assert!(!is_admin_only_path("/api/v1/auth/api-keys", "GET"));
        assert!(!is_admin_only_path("/api/v1/auth/api-keys", "DELETE"));
    }

    #[test]
    fn test_is_admin_only_path_tenants() {
        // All tenant management paths are admin-only
        assert!(is_admin_only_path("/api/v1/tenants", "GET"));
        assert!(is_admin_only_path("/api/v1/tenants", "POST"));
        assert!(is_admin_only_path("/api/v1/tenants/some-id", "DELETE"));
        assert!(is_admin_only_path("/api/v1/tenants/some-id/config", "PUT"));
    }

    #[test]
    fn test_is_admin_only_path_normal_paths() {
        // Normal event/query paths are NOT admin-only
        assert!(!is_admin_only_path("/api/v1/events", "POST"));
        assert!(!is_admin_only_path("/api/v1/events/query", "GET"));
        assert!(!is_admin_only_path("/api/v1/auth/me", "GET"));
        assert!(!is_admin_only_path("/api/v1/auth/login", "POST"));
        assert!(!is_admin_only_path("/api/v1/schemas", "GET"));
    }

    #[test]
    fn test_auth_context_permissions() {
        let claims = Claims::new(
            "user1".to_string(),
            "tenant1".to_string(),
            Role::Developer,
            chrono::Duration::hours(1),
        );

        let ctx = AuthContext { claims };

        assert!(ctx.require_permission(Permission::Read).is_ok());
        assert!(ctx.require_permission(Permission::Write).is_ok());
        assert!(ctx.require_permission(Permission::Admin).is_err());
    }

    #[test]
    fn test_auth_context_admin_permissions() {
        let claims = Claims::new(
            "admin1".to_string(),
            "tenant1".to_string(),
            Role::Admin,
            chrono::Duration::hours(1),
        );

        let ctx = AuthContext { claims };

        assert!(ctx.require_permission(Permission::Read).is_ok());
        assert!(ctx.require_permission(Permission::Write).is_ok());
        assert!(ctx.require_permission(Permission::Admin).is_ok());
    }

    #[test]
    fn test_auth_context_readonly_permissions() {
        let claims = Claims::new(
            "readonly1".to_string(),
            "tenant1".to_string(),
            Role::ReadOnly,
            chrono::Duration::hours(1),
        );

        let ctx = AuthContext { claims };

        assert!(ctx.require_permission(Permission::Read).is_ok());
        assert!(ctx.require_permission(Permission::Write).is_err());
        assert!(ctx.require_permission(Permission::Admin).is_err());
    }

    #[test]
    fn test_auth_context_tenant_id() {
        let claims = Claims::new(
            "user1".to_string(),
            "my-tenant".to_string(),
            Role::Developer,
            chrono::Duration::hours(1),
        );

        let ctx = AuthContext { claims };
        assert_eq!(ctx.tenant_id(), "my-tenant");
    }

    #[test]
    fn test_auth_context_user_id() {
        let claims = Claims::new(
            "my-user".to_string(),
            "tenant1".to_string(),
            Role::Developer,
            chrono::Duration::hours(1),
        );

        let ctx = AuthContext { claims };
        assert_eq!(ctx.user_id(), "my-user");
    }

    #[test]
    fn test_request_id_new() {
        let id1 = RequestId::new();
        let id2 = RequestId::new();

        // IDs should be unique
        assert_ne!(id1.as_str(), id2.as_str());
        // IDs should be valid UUIDs (36 chars with hyphens)
        assert_eq!(id1.as_str().len(), 36);
    }

    #[test]
    fn test_request_id_default() {
        let id = RequestId::default();
        assert_eq!(id.as_str().len(), 36);
    }

    #[test]
    fn test_security_config_default() {
        let config = SecurityConfig::default();

        assert!(config.enable_hsts);
        assert_eq!(config.hsts_max_age, 31536000);
        assert!(config.enable_frame_options);
        assert!(config.enable_content_type_options);
        assert!(config.enable_xss_protection);
        assert!(config.csp.is_some());
    }

    #[test]
    fn test_frame_options_variants() {
        let deny = FrameOptions::Deny;
        let same_origin = FrameOptions::SameOrigin;
        let allow_from = FrameOptions::AllowFrom("https://example.com".to_string());

        // Check that variants are distinct via debug formatting
        assert!(format!("{deny:?}").contains("Deny"));
        assert!(format!("{same_origin:?}").contains("SameOrigin"));
        assert!(format!("{allow_from:?}").contains("AllowFrom"));
    }

    #[test]
    fn test_auth_error_from_validation_error() {
        let error = AllSourceError::ValidationError("test error".to_string());
        let auth_error = AuthError::from(error);
        assert!(format!("{auth_error:?}").contains("ValidationError"));
    }

    #[test]
    fn test_rate_limit_error_display() {
        let error = RateLimitError::RateLimitExceeded {
            retry_after: 60,
            limit: 100,
        };
        assert!(format!("{error:?}").contains("RateLimitExceeded"));

        let unauth_error = RateLimitError::Unauthorized;
        assert!(format!("{unauth_error:?}").contains("Unauthorized"));
    }

    #[test]
    fn test_tenant_error_variants() {
        let errors = vec![
            TenantError::Unauthorized,
            TenantError::InvalidTenant,
            TenantError::TenantNotFound,
            TenantError::TenantInactive,
            TenantError::RepositoryError("test".to_string()),
        ];

        for error in errors {
            // Ensure each variant can be debug-formatted
            let _ = format!("{error:?}");
        }
    }

    #[test]
    fn test_ip_filter_error_variants() {
        let errors = vec![
            IpFilterError::NoIpAddress,
            IpFilterError::Blocked {
                reason: "blocked".to_string(),
            },
        ];

        for error in errors {
            let _ = format!("{error:?}");
        }
    }

    #[test]
    fn test_security_state_clone() {
        let config = SecurityConfig::default();
        let state = SecurityState {
            config: config.clone(),
        };
        let cloned = state.clone();
        assert_eq!(cloned.config.hsts_max_age, config.hsts_max_age);
    }

    #[test]
    fn test_auth_state_clone() {
        let auth_manager = Arc::new(AuthManager::new("test-secret"));
        let state = AuthState { auth_manager };
        let cloned = state.clone();
        assert!(Arc::ptr_eq(&state.auth_manager, &cloned.auth_manager));
    }

    #[test]
    fn test_rate_limit_state_clone() {
        use crate::infrastructure::security::rate_limit::RateLimitConfig;
        let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig::free_tier()));
        let state = RateLimitState::new(rate_limiter);
        let cloned = state.clone();
        assert!(Arc::ptr_eq(&state.rate_limiter, &cloned.rate_limiter));
    }

    #[test]
    fn test_auth_skip_paths_contains_expected() {
        // Verify public paths are configured for auth/rate-limit skipping
        assert!(should_skip_auth("/health"));
        assert!(should_skip_auth("/metrics"));
        assert!(should_skip_auth("/api/v1/auth/register"));
        assert!(should_skip_auth("/api/v1/auth/login"));
        assert!(should_skip_auth("/api/v1/demo/seed"));

        // Verify internal endpoints bypass auth (sentinel failover)
        assert!(should_skip_auth("/internal/promote"));
        assert!(should_skip_auth("/internal/repoint"));
        assert!(should_skip_auth("/internal/anything"));

        // Data-plane paths skip auth (Core is internal-only; gateway handles
        // authentication; handlers read tenant_id from body/query).
        assert!(should_skip_auth("/api/v1/events"));
        assert!(should_skip_auth("/api/v1/events/query"));
        assert!(should_skip_auth("/api/v1/events/batch"));
        assert!(should_skip_auth("/api/v1/projections/foo/bar"));
        assert!(should_skip_auth("/api/v1/snapshots"));

        // These still require auth — CP depends on them.
        assert!(!should_skip_auth("/api/v1/auth/me"));
        assert!(!should_skip_auth("/api/v1/auth/api-keys"));
        assert!(!should_skip_auth("/api/v1/tenants"));
        assert!(!should_skip_auth("/api/v1/tenants/abc"));
    }

    #[test]
    fn test_dev_mode_auth_context() {
        let ctx = dev_mode_auth_context();

        // Dev user should have admin privileges
        assert_eq!(ctx.tenant_id(), "dev-tenant");
        assert_eq!(ctx.user_id(), "dev-user");
        assert!(ctx.require_permission(Permission::Admin).is_ok());
        assert!(ctx.require_permission(Permission::Read).is_ok());
        assert!(ctx.require_permission(Permission::Write).is_ok());
    }

    #[test]
    fn test_dev_mode_disabled_by_default() {
        // Dev mode should be disabled by default (env var not set in tests)
        // Note: This test may fail if ALLSOURCE_DEV_MODE is set in the test environment
        // In a clean environment, dev mode is disabled
        let env_value = std::env::var("ALLSOURCE_DEV_MODE").unwrap_or_default();
        if env_value.is_empty() {
            assert!(!is_dev_mode());
        }
    }
}
