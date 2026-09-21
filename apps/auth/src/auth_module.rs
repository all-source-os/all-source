//! better-auth-rs integration for AllSource Auth Service
//!
//! Adapted from wallet-auth for standalone deployment with axum 0.8.
//! Uses better-auth's framework-agnostic `handle_request()` API.
//!
//! Backend selection via `AUTH_BACKEND` env var:
//! - `allsource` — event-sourced persistence via AllSource Core
//!   (requires `AUTH_ALLSOURCE_URL` + `AUTH_ALLSOURCE_QUERY_URL`)
//! - `memory` — in-memory, sessions lost on restart (default)

use std::{collections::HashMap, sync::Arc, time::Instant};

use anyhow::Result;
use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use better_auth::{
    AuthConfig as BetterAuthConfig, BetterAuth, SessionOps, TypedAuthBuilder, UserOps,
    adapters::MemoryDatabaseAdapter,
    plugins::{OAuthPlugin, SessionManagementPlugin, oauth::OAuthProvider},
};
use better_auth_allsource::AllsourceAuthAdapter;
use better_auth_core::{
    AuthRequest, DatabaseAdapter, HttpMethod,
    entity::{AuthSession as AuthSessionTrait, AuthUser as AuthUserTrait},
};
use dashmap::DashMap;
use tracing::{error, info, warn};

/// Type alias for the concrete auth instances.
pub type MemoryAuth = BetterAuth<MemoryDatabaseAdapter>;
pub type AllsourceAuth = BetterAuth<AllsourceAuthAdapter>;

/// Configuration for the auth module, loaded from environment.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Explicit backend selection: "allsource" or "memory"
    pub backend: String,
    /// Secret key for signing sessions/tokens (min 32 chars)
    pub secret: String,
    /// Base URL of the auth service (e.g. "http://localhost:3903")
    pub base_url: String,
    /// AllSource Core URL for event-sourced auth persistence
    pub allsource_url: Option<String>,
    /// AllSource Query Service URL
    pub allsource_query_url: Option<String>,
    /// AllSource API key (required for production)
    pub allsource_api_key: Option<String>,
    /// Google OAuth client ID (optional)
    pub google_client_id: Option<String>,
    /// Google OAuth client secret
    pub google_client_secret: Option<String>,
    /// GitHub OAuth client ID (optional)
    pub github_client_id: Option<String>,
    /// GitHub OAuth client secret
    pub github_client_secret: Option<String>,
    /// Trusted origins for CSRF (e.g. the frontend origin when cross-origin)
    pub trusted_origins: Vec<String>,
}

impl AuthConfig {
    /// Load from environment variables.
    ///
    /// Required: `AUTH_SECRET`
    /// Backend selection: `AUTH_BACKEND` (allsource | memory)
    pub fn from_env() -> Option<Self> {
        let secret = std::env::var("AUTH_SECRET").ok()?;
        if secret.len() < 32 {
            warn!("AUTH_SECRET must be at least 32 characters");
            return None;
        }

        Some(Self {
            backend: std::env::var("AUTH_BACKEND").unwrap_or_else(|_| "allsource".to_string()),
            secret,
            base_url: {
                let base = std::env::var("AUTH_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:3903".to_string());
                format!("{}/api/auth", base.trim_end_matches('/'))
            },
            allsource_url: std::env::var("AUTH_ALLSOURCE_URL").ok(),
            allsource_query_url: std::env::var("AUTH_ALLSOURCE_QUERY_URL").ok(),
            allsource_api_key: std::env::var("ALLSOURCE_API_KEY").ok(),
            google_client_id: std::env::var("GOOGLE_CLIENT_ID").ok(),
            google_client_secret: std::env::var("GOOGLE_CLIENT_SECRET").ok(),
            github_client_id: std::env::var("GITHUB_CLIENT_ID").ok(),
            github_client_secret: std::env::var("GITHUB_CLIENT_SECRET").ok(),
            trusted_origins: std::env::var("AUTH_TRUSTED_ORIGINS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        })
    }
}

/// Enum wrapping all possible auth backends.
enum AuthBackend {
    Memory(Arc<MemoryAuth>),
    Allsource(Arc<AllsourceAuth>),
}

/// Thread-safe cache mapping authorization codes to (session_token, created_at).
/// Codes are single-use and expire after `AUTH_CODE_TTL`.
pub type AuthCodeCache = Arc<DashMap<String, (String, Instant)>>;

/// Authorization codes expire after 60 seconds.
const AUTH_CODE_TTL: std::time::Duration = std::time::Duration::from_secs(60);

/// Auth module wrapping better-auth-rs with an axum 0.8 router.
pub struct AuthModule {
    backend: AuthBackend,
    config: Arc<AuthConfig>,
    /// In-memory cache for single-use OAuth authorization codes.
    auth_codes: AuthCodeCache,
}

/// Add email/password and (optionally) Google + GitHub OAuth plugins to a builder.
fn configure_plugins<DB: DatabaseAdapter>(
    mut builder: TypedAuthBuilder<DB>,
    config: &AuthConfig,
) -> TypedAuthBuilder<DB> {
    builder = builder
        .plugin(SessionManagementPlugin::new())
        .plugin(crate::email_credentials::EmailCredentialsPlugin);

    // Build OAuth plugin with available providers
    let mut oauth_providers: Vec<(&str, OAuthProvider)> = Vec::new();

    if let (Some(client_id), Some(client_secret)) =
        (&config.google_client_id, &config.google_client_secret)
    {
        oauth_providers.push(("google", OAuthProvider::google(client_id, client_secret)));
        info!("Auth: Google OAuth enabled");
    } else {
        info!("Auth: Google OAuth disabled (GOOGLE_CLIENT_ID not set)");
    }

    if let (Some(client_id), Some(client_secret)) =
        (&config.github_client_id, &config.github_client_secret)
    {
        oauth_providers.push(("github", OAuthProvider::github(client_id, client_secret)));
        info!("Auth: GitHub OAuth enabled");
    } else {
        info!("Auth: GitHub OAuth disabled (GITHUB_CLIENT_ID not set)");
    }

    if !oauth_providers.is_empty() {
        let mut oauth_plugin = OAuthPlugin::new();
        for (name, provider) in oauth_providers {
            oauth_plugin = oauth_plugin.add_provider(name, provider);
        }
        builder = builder.plugin(oauth_plugin);
    }

    builder
}

impl AuthModule {
    /// Build a `BetterAuthConfig` with shared settings.
    ///
    /// `base_path` is set to "" because axum `nest` already strips the
    /// mount prefix (`/api/auth`) before requests reach better-auth's handler.
    /// `base_url` includes `/api/auth` so OAuth callback URLs are correct.
    fn auth_config(config: &AuthConfig) -> BetterAuthConfig {
        let is_localhost =
            config.base_url.contains("localhost") || config.base_url.contains("127.0.0.1");

        let mut cfg = BetterAuthConfig::new(&config.secret)
            .base_url(&config.base_url)
            .base_path("");

        // Match the JS better-auth cookie name
        cfg.session.cookie_name = "better-auth.session_token".to_string();

        if is_localhost {
            // Disable Secure flag on localhost (HTTP, not HTTPS)
            cfg.session.cookie_secure = false;
        } else {
            // Production: SameSite=None + Secure so cross-origin fetch
            // from app domain to API domain includes cookies
            cfg.session.cookie_same_site = better_auth_core::SameSite::None;
            cfg.session.cookie_secure = true;
        }

        if !config.trusted_origins.is_empty() {
            info!("Auth: trusted_origins = {:?}", config.trusted_origins);
            cfg = cfg.trusted_origins(config.trusted_origins.clone());
        }
        cfg
    }

    /// Build the auth module from config.
    ///
    /// Backend is selected explicitly via `AUTH_BACKEND` env var.
    pub async fn build(config: AuthConfig) -> Result<Self> {
        match config.backend.as_str() {
            "allsource" => Self::build_allsource(&config).await,
            "memory" => Self::build_memory(&config).await,
            other => Err(anyhow::anyhow!(
                "Unknown AUTH_BACKEND={other:?}. Valid options: allsource, memory"
            )),
        }
    }

    async fn build_memory(config: &AuthConfig) -> Result<Self> {
        let builder = BetterAuth::<MemoryDatabaseAdapter>::new(Self::auth_config(config))
            .database(MemoryDatabaseAdapter::new());

        let auth = configure_plugins(builder, config)
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to build auth: {e}"))?;

        warn!("Auth module built (in-memory adapter — sessions will be lost on restart)");
        Ok(Self {
            backend: AuthBackend::Memory(Arc::new(auth)),
            config: Arc::new(config.clone()),
            auth_codes: Arc::new(DashMap::new()),
        })
    }

    async fn build_allsource(config: &AuthConfig) -> Result<Self> {
        let core_url = config.allsource_url.as_deref().ok_or_else(|| {
            anyhow::anyhow!("AUTH_BACKEND=allsource but AUTH_ALLSOURCE_URL is not set")
        })?;
        let query_url = config.allsource_query_url.as_deref().ok_or_else(|| {
            anyhow::anyhow!("AUTH_BACKEND=allsource but AUTH_ALLSOURCE_QUERY_URL is not set")
        })?;
        let api_key = config.allsource_api_key.as_deref();

        let is_production = std::env::var("ENVIRONMENT")
            .map(|e| e == "production")
            .unwrap_or(false);
        if is_production && api_key.unwrap_or("").is_empty() {
            return Err(anyhow::anyhow!(
                "AUTH_BACKEND=allsource in production but ALLSOURCE_API_KEY is not set"
            ));
        }

        // Verify AllSource Core is reachable before committing to this backend
        let health_url = format!("{}/health", core_url.trim_end_matches('/'));
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()?;
        client
            .get(&health_url)
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "AUTH_BACKEND=allsource but AllSource Core is unreachable at {core_url}: {e}"
                )
            })?
            .error_for_status()
            .map_err(|e| {
                anyhow::anyhow!("AllSource Core health check failed at {health_url}: {e}")
            })?;

        let storage_tenant = std::env::var("AUTH_ALLSOURCE_TENANT_ID").map_err(|_| {
            anyhow::anyhow!("AUTH_ALLSOURCE_TENANT_ID is required for durable auth storage")
        })?;
        if storage_tenant.is_empty() {
            return Err(anyhow::anyhow!(
                "AUTH_ALLSOURCE_TENANT_ID must not be empty"
            ));
        }
        // An unauthenticated /health 200 does not prove the storage credential
        // works. Fail startup if it is revoked or belongs to another tenant.
        let identity: serde_json::Value = client
            .get(format!("{}/api/v1/auth/me", core_url.trim_end_matches('/')))
            .bearer_auth(api_key.unwrap_or(""))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if identity.get("tenant_id").and_then(|v| v.as_str()) != Some(storage_tenant.as_str()) {
            return Err(anyhow::anyhow!(
                "Auth storage credential tenant does not match AUTH_ALLSOURCE_TENANT_ID"
            ));
        }
        let db = AllsourceAuthAdapter::new(core_url, query_url, api_key.unwrap_or(""))
            .with_tenant_id(storage_tenant);

        let builder =
            BetterAuth::<AllsourceAuthAdapter>::new(Self::auth_config(config)).database(db);

        let auth = configure_plugins(builder, config)
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to build auth: {e}"))?;

        info!("Auth module built (AllSource adapter at {core_url})");
        Ok(Self {
            backend: AuthBackend::Allsource(Arc::new(auth)),
            config: Arc::new(config.clone()),
            auth_codes: Arc::new(DashMap::new()),
        })
    }

    /// Create an axum 0.8 Router that handles all auth routes.
    pub fn router(&self) -> Router {
        let config = self.config.clone();
        let auth_codes = self.auth_codes.clone();

        // Exchange endpoint: POST /exchange — exchanges a single-use auth code for a session token
        let exchange_router = Router::new()
            .route("/exchange", axum::routing::post(exchange_code_handler))
            .with_state(auth_codes.clone());

        let auth_router =
            match &self.backend {
                AuthBackend::Memory(auth) => Router::new()
                    .fallback(auth_handler_memory)
                    .with_state((auth.clone(), config, auth_codes)),
                AuthBackend::Allsource(auth) => Router::new()
                    .fallback(auth_handler_allsource)
                    .with_state((auth.clone(), config, auth_codes)),
            };

        // Merge exchange route first so it takes priority over the fallback
        exchange_router.merge(auth_router)
    }

    /// Validate a session token and return `user_id` if valid.
    pub async fn validate_session(&self, token: &str) -> Option<String> {
        match &self.backend {
            AuthBackend::Memory(auth) => {
                validate_session_impl(auth.database().as_ref(), token).await
            }
            AuthBackend::Allsource(auth) => {
                validate_session_impl(auth.database().as_ref(), token).await
            }
        }
    }

    /// Create a demo account by signing up with the given credentials.
    ///
    /// Uses the framework-agnostic `handle_request` API to simulate
    /// a POST /sign-up/email request.
    pub async fn create_demo_account(&self, email: &str, password: &str, name: &str) -> Result<()> {
        let body = serde_json::json!({
            "email": email,
            "password": password,
            "name": name,
        });
        let body_bytes = serde_json::to_vec(&body)?;

        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());

        let request = AuthRequest::from_parts(
            HttpMethod::Post,
            "/sign-up/email".to_string(),
            headers,
            Some(body_bytes),
            HashMap::new(),
        );

        let response = match &self.backend {
            AuthBackend::Memory(auth) => auth
                .handle_request(request)
                .await
                .map_err(|e| anyhow::anyhow!("Demo signup failed: {e}"))?,
            AuthBackend::Allsource(auth) => auth
                .handle_request(request)
                .await
                .map_err(|e| anyhow::anyhow!("Demo signup failed: {e}"))?,
        };

        if response.status >= 400 {
            let body_str = String::from_utf8_lossy(&response.body);
            return Err(anyhow::anyhow!(
                "Demo signup returned {}: {body_str}",
                response.status
            ));
        }

        Ok(())
    }
}

/// Shared session validation logic.
async fn validate_session_impl<DB>(db: &DB, token: &str) -> Option<String>
where
    DB: SessionOps + UserOps,
{
    let session = SessionOps::get_session(db, token).await.ok()??;
    let user_id = session.user_id().to_string();
    let user = UserOps::get_user_by_id(db, &user_id).await.ok()??;
    Some(user.id().to_string())
}

/// Catch-all handler for memory backend.
async fn auth_handler_memory(
    State((auth, config, auth_codes)): State<(Arc<MemoryAuth>, Arc<AuthConfig>, AuthCodeCache)>,
    request: Request,
) -> Response {
    auth_handler_impl(&auth, &config, &auth_codes, request).await
}

/// Catch-all handler for allsource backend.
async fn auth_handler_allsource(
    State((auth, config, auth_codes)): State<(Arc<AllsourceAuth>, Arc<AuthConfig>, AuthCodeCache)>,
    request: Request,
) -> Response {
    auth_handler_impl(&auth, &config, &auth_codes, request).await
}

/// Shared handler implementation.
///
/// Handles the OAuth callback redirect gap in better-auth-rs: when the
/// `/callback/{provider}` endpoint returns a 200 JSON response (with session
/// cookie set), we convert it to a 302 redirect back to the frontend.
/// The JS better-auth does this natively; better-auth-rs does not (yet).
async fn auth_handler_impl<DB: DatabaseAdapter>(
    auth: &BetterAuth<DB>,
    config: &AuthConfig,
    auth_codes: &AuthCodeCache,
    request: Request,
) -> Response {
    let auth_request = match convert_request(request).await {
        Ok(req) => req,
        Err(e) => {
            warn!("Auth request conversion failed: {e}");
            return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
        }
    };

    let path = auth_request.path.clone();
    let method = format!("{:?}", auth_request.method);
    let is_callback = path.starts_with("/callback/");

    match auth.handle_request(auth_request).await {
        Ok(auth_response) => {
            if auth_response.status >= 400 {
                let body_str = String::from_utf8_lossy(&auth_response.body);
                if auth_response.status >= 500 {
                    error!(
                        "Auth {method} {path} -> {}: {body_str}",
                        auth_response.status
                    );
                } else {
                    warn!(
                        "Auth {method} {path} -> {}: {body_str}",
                        auth_response.status
                    );
                }
            }

            // After successful OAuth callback, redirect to frontend instead of
            // returning raw JSON. The JS better-auth does this natively.
            if is_callback && auth_response.status == 200 {
                return convert_callback_to_redirect(auth_response, config, auth_codes);
            }

            convert_response(auth_response)
        }
        Err(err) => {
            let err_detail = format!("{err:?}");
            error!("Auth {method} {path} error: {err_detail}");
            let status = StatusCode::from_u16(err.status_code())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

            // Surface actual error details in non-production environments
            let is_production = std::env::var("ENVIRONMENT")
                .map(|e| e == "production")
                .unwrap_or(false);

            let body = if is_production {
                serde_json::json!({ "message": err.to_string() })
            } else {
                serde_json::json!({
                    "message": err.to_string(),
                    "detail": err_detail,
                    "path": path,
                    "method": method,
                })
            };
            (status, axum::Json(body)).into_response()
        }
    }
}

/// Convert a successful OAuth callback JSON response into a 302 redirect.
///
/// In cross-origin deployments (API on one domain, app on another),
/// cookies set on the API domain are not visible to the app's server-side code.
/// Instead of passing the raw session token in the URL (which leaks via Referer
/// headers, browser history, and server logs), we generate a short-lived,
/// single-use authorization code. The app's callback route exchanges this code
/// for the session token via a back-channel POST to `/api/auth/exchange`.
fn convert_callback_to_redirect(
    auth_response: better_auth_core::AuthResponse,
    config: &AuthConfig,
    auth_codes: &AuthCodeCache,
) -> Response {
    // Determine redirect target: first trusted origin, or fall back to base_url origin
    let redirect_to = config.trusted_origins.first().cloned().unwrap_or_else(|| {
        config
            .base_url
            .find("://")
            .and_then(|scheme_end| {
                let after_scheme = &config.base_url[scheme_end + 3..];
                after_scheme
                    .find('/')
                    .map(|path_start| config.base_url[..scheme_end + 3 + path_start].to_string())
            })
            .unwrap_or_else(|| config.base_url.clone())
    });

    // Extract session token from Set-Cookie header for cross-domain handoff
    let session_token = auth_response.headers.iter().find_map(|(name, value)| {
        if name.eq_ignore_ascii_case("set-cookie") && value.contains("better-auth.session_token=") {
            value
                .split("better-auth.session_token=")
                .nth(1)
                .and_then(|rest| rest.split(';').next())
                .map(|t| t.to_string())
        } else {
            None
        }
    });

    // Generate a single-use authorization code and redirect with ?code=...
    // instead of ?token=... to prevent token leakage via Referer/history/logs.
    let location = if let Some(ref token) = session_token {
        // Lazily purge expired codes to prevent unbounded memory growth
        auth_codes.retain(|_, (_, created_at)| created_at.elapsed() < AUTH_CODE_TTL);

        let code = uuid::Uuid::new_v4().to_string();
        auth_codes.insert(code.clone(), (token.clone(), Instant::now()));
        format!("{redirect_to}/api/auth/callback?code={code}")
    } else {
        redirect_to.clone()
    };

    info!("OAuth callback succeeded, redirecting to {location}");

    let mut builder = axum::http::Response::builder().status(StatusCode::FOUND);
    builder = builder.header("location", &location);

    // Also preserve Set-Cookie headers (useful for same-origin/localhost setups)
    for (name, value) in &auth_response.headers {
        if let (Ok(hn), Ok(hv)) = (
            axum::http::HeaderName::from_bytes(name.as_bytes()),
            axum::http::HeaderValue::from_str(value),
        ) {
            builder = builder.header(hn, hv);
        }
    }

    builder
        .body(axum::body::Body::empty())
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response())
}

/// Request body for the code exchange endpoint.
#[derive(serde::Deserialize)]
struct ExchangeCodeRequest {
    code: String,
}

/// Response body for the code exchange endpoint.
#[derive(serde::Serialize)]
struct ExchangeCodeResponse {
    token: String,
}

/// POST /exchange — exchanges a single-use authorization code for a session token.
///
/// This is the back-channel counterpart to the OAuth callback redirect.
/// The code is deleted on first use and expires after 60 seconds.
async fn exchange_code_handler(
    State(auth_codes): State<AuthCodeCache>,
    axum::Json(body): axum::Json<ExchangeCodeRequest>,
) -> Response {
    // Remove the code (single-use) and check TTL
    if let Some((_, (token, created_at))) = auth_codes.remove(&body.code) {
        if created_at.elapsed() < AUTH_CODE_TTL {
            return axum::Json(ExchangeCodeResponse { token }).into_response();
        }
        warn!("Auth code exchange failed: code expired");
    } else {
        warn!("Auth code exchange failed: invalid code");
    }

    (
        StatusCode::BAD_REQUEST,
        axum::Json(serde_json::json!({ "error": "Invalid or expired authorization code" })),
    )
        .into_response()
}

/// Convert axum 0.8 Request to better-auth `AuthRequest`.
async fn convert_request(req: Request) -> Result<AuthRequest> {
    let (parts, body) = req.into_parts();

    let method = match parts.method {
        axum::http::Method::GET => HttpMethod::Get,
        axum::http::Method::POST => HttpMethod::Post,
        axum::http::Method::PUT => HttpMethod::Put,
        axum::http::Method::DELETE => HttpMethod::Delete,
        axum::http::Method::PATCH => HttpMethod::Patch,
        axum::http::Method::OPTIONS => HttpMethod::Options,
        axum::http::Method::HEAD => HttpMethod::Head,
        _ => return Err(anyhow::anyhow!("Unsupported HTTP method")),
    };

    let mut headers = HashMap::new();
    for (name, value) in parts.headers.iter() {
        if let Ok(v) = value.to_str() {
            headers.insert(name.to_string(), v.to_string());
        }
    }

    let path = parts.uri.path().to_string();

    let mut query = HashMap::new();
    if let Some(q) = parts.uri.query() {
        for (k, v) in url::form_urlencoded::parse(q.as_bytes()) {
            query.insert(k.to_string(), v.to_string());
        }
    }

    let body_bytes = axum::body::to_bytes(body, 1024 * 1024)
        .await
        .ok()
        .and_then(|b| if b.is_empty() { None } else { Some(b.to_vec()) });

    Ok(AuthRequest::from_parts(
        method, path, headers, body_bytes, query,
    ))
}

/// Convert better-auth `AuthResponse` to axum 0.8 Response.
fn convert_response(auth_response: better_auth_core::AuthResponse) -> Response {
    let mut builder = axum::http::Response::builder().status(
        StatusCode::from_u16(auth_response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
    );

    for (name, value) in auth_response.headers {
        if let (Ok(hn), Ok(hv)) = (
            axum::http::HeaderName::from_bytes(name.as_bytes()),
            axum::http::HeaderValue::from_str(&value),
        ) {
            builder = builder.header(hn, hv);
        }
    }

    builder
        .body(axum::body::Body::from(auth_response.body))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt; // for oneshot()

    const TEST_SECRET: &str = "super-secret-jwt-token-with-at-least-32-characters-long";

    fn memory_config() -> AuthConfig {
        AuthConfig {
            backend: "memory".to_string(),
            secret: TEST_SECRET.to_string(),
            base_url: "http://localhost:3903/api/auth".to_string(),
            allsource_url: None,
            allsource_query_url: None,
            allsource_api_key: None,
            google_client_id: None,
            google_client_secret: None,
            github_client_id: None,
            github_client_secret: None,
            trusted_origins: vec![],
        }
    }

    #[test]
    fn config_defaults_to_allsource_backend() {
        // When AUTH_BACKEND is not set, default is "allsource"
        let config = memory_config();
        // (our test helper uses "memory" explicitly)
        assert_eq!(config.backend, "memory");
    }

    #[tokio::test]
    async fn build_memory_backend_succeeds() {
        let config = memory_config();
        let module = AuthModule::build(config).await;
        assert!(module.is_ok(), "Memory backend should build without issues");
    }

    #[tokio::test]
    async fn build_unknown_backend_fails() {
        let mut config = memory_config();
        config.backend = "sqlite".to_string();
        let result = AuthModule::build(config).await;
        let err = result
            .err()
            .expect("should fail for unknown backend")
            .to_string();
        assert!(
            err.contains("Unknown AUTH_BACKEND"),
            "Error should mention unknown backend: {err}"
        );
    }

    #[tokio::test]
    async fn build_allsource_without_urls_fails() {
        let mut config = memory_config();
        config.backend = "allsource".to_string();
        let result = AuthModule::build(config).await;
        let err = result.err().expect("should fail without URL").to_string();
        assert!(
            err.contains("AUTH_ALLSOURCE_URL"),
            "Error should mention missing URL: {err}"
        );
    }

    #[tokio::test]
    async fn memory_module_returns_router() {
        let config = memory_config();
        let module = AuthModule::build(config).await.unwrap();
        let _router: Router = module.router();
    }

    #[tokio::test]
    async fn validate_session_returns_none_for_invalid_token() {
        let config = memory_config();
        let module = AuthModule::build(config).await.unwrap();
        let result = module.validate_session("nonexistent-token").await;
        assert_eq!(result, None, "Invalid token should return None");
    }

    #[tokio::test]
    async fn signup_signin_session_roundtrip() {
        let config = memory_config();
        let module = AuthModule::build(config).await.unwrap();
        let router = module.router();

        // Sign up
        let signup_body = serde_json::json!({
            "email": "test@example.com",
            "password": "TestPassword123!",
            "name": "Test User"
        });

        let signup_req = Request::builder()
            .method("POST")
            .uri("/sign-up/email")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(signup_body.to_string()))
            .unwrap();

        let signup_resp = router.clone().oneshot(signup_req).await.unwrap();
        assert!(
            signup_resp.status().is_success(),
            "Sign-up should succeed, got {}",
            signup_resp.status()
        );

        // Regression: upstream 0.8 writes password_hash into user metadata;
        // the durable adapter rejects it. The replacement must use Account.
        if let AuthBackend::Memory(auth) = &module.backend {
            use better_auth_core::{AccountOps, entity::AuthAccount};
            let db = auth.database();
            let user = db
                .get_user_by_email("test@example.com")
                .await
                .unwrap()
                .unwrap();
            assert!(user.metadata().get("password_hash").is_none());
            let credential = db
                .get_account("credential", user.id())
                .await
                .unwrap()
                .unwrap();
            assert!(credential.password().unwrap().starts_with("$argon2"));
        }

        for (path, body, expected) in [
            ("/sign-up/email", signup_body.clone(), StatusCode::CONFLICT),
            (
                "/sign-in/email",
                serde_json::json!({"email":"test@example.com","password":"WrongPassword123!"}),
                StatusCode::UNAUTHORIZED,
            ),
        ] {
            let request = Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(body.to_string()))
                .unwrap();
            assert_eq!(
                router.clone().oneshot(request).await.unwrap().status(),
                expected
            );
        }

        // Sign in
        let signin_body = serde_json::json!({
            "email": "test@example.com",
            "password": "TestPassword123!"
        });

        let signin_req = Request::builder()
            .method("POST")
            .uri("/sign-in/email")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(signin_body.to_string()))
            .unwrap();

        let signin_resp = router.clone().oneshot(signin_req).await.unwrap();
        assert!(
            signin_resp.status().is_success(),
            "Sign-in should succeed, got {}",
            signin_resp.status()
        );

        // Extract session token from Set-Cookie header
        let set_cookie = signin_resp
            .headers()
            .get("set-cookie")
            .expect("Sign-in response should set a cookie")
            .to_str()
            .unwrap()
            .to_string();

        let token = set_cookie
            .split(';')
            .next()
            .unwrap()
            .split('=')
            .nth(1)
            .expect("Cookie should have a value");

        assert!(!token.is_empty(), "Session token should not be empty");

        // Validate the session token
        let user_id = module.validate_session(token).await;
        assert!(
            user_id.is_some(),
            "Session token from sign-in should be valid"
        );
    }

    #[tokio::test]
    async fn auth_handler_returns_404_for_unknown_route() {
        let config = memory_config();
        let module = AuthModule::build(config).await.unwrap();
        let router = module.router();

        let req = Request::builder()
            .method("GET")
            .uri("/nonexistent-route")
            .body(axum::body::Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn demo_account_creation_succeeds() {
        let config = memory_config();
        let module = AuthModule::build(config).await.unwrap();

        let result = module
            .create_demo_account("demo@test.com", "DemoPass123!", "Demo User")
            .await;
        assert!(result.is_ok(), "Demo account creation should succeed");
    }
}
