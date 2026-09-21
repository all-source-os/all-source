//! AllSource Auth Service
//!
//! Standalone authentication server built on better-auth-rs.
//! Provides email/password sign-up/sign-in, OAuth (Google + GitHub),
//! session management, and demo account provisioning.
//!
//! All auth routes are mounted at `/api/auth`.

use std::{
    net::{Ipv6Addr, SocketAddr},
    sync::Arc,
};

use anyhow::Result;
use axum::{Router, extract::State, http::StatusCode, response::IntoResponse};
use clap::Parser;
use tokio::net::TcpListener;
use tracing::{error, info};

use allsource_auth::auth_module::{AuthConfig, AuthModule};

/// AllSource Auth Service — standalone authentication server.
#[derive(Parser, Debug)]
#[command(name = "allsource-auth", version, about)]
struct Cli {
    /// Port to listen on
    #[arg(long, env = "AUTH_PORT", default_value = "3903")]
    port: u16,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, env = "AUTH_LOG_LEVEL", default_value = "info")]
    log_level: String,
}

/// Shared application state.
#[derive(Clone)]
struct AppState {
    auth_module: Arc<AuthModule>,
    backend_name: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cli.log_level)),
        )
        .init();

    info!("AllSource Auth Service starting on port {}", cli.port);

    // Load auth config from environment
    let auth_config = AuthConfig::from_env().ok_or_else(|| {
        anyhow::anyhow!(
            "AUTH_SECRET must be set (minimum 32 characters). \
             See AUTH_BACKEND, AUTH_ALLSOURCE_URL, AUTH_ALLSOURCE_QUERY_URL for backend config."
        )
    })?;

    let backend_name = auth_config.backend.clone();

    // Build auth module
    let auth_module = AuthModule::build(auth_config).await?;
    let auth_module = Arc::new(auth_module);

    let state = AppState {
        auth_module: auth_module.clone(),
        backend_name: backend_name.clone(),
    };

    // Build router
    let app = Router::new()
        .route("/health", axum::routing::get(health_handler))
        .with_state(state.clone())
        // Demo account provisioning
        .route(
            "/api/auth/demo/start",
            axum::routing::post(demo_start_handler),
        )
        .with_state(state.clone())
        // Mount all better-auth routes at /api/auth
        .nest("/api/auth", auth_module.router());

    // Bind and serve
    // Fly .internal DNS resolves to private IPv6 addresses. A v4-only bind
    // passes the proxy health check while refusing service-to-service traffic.
    let addr = listen_address(cli.port);
    let listener = TcpListener::bind(addr).await?;
    info!(
        "AllSource Auth Service listening on {} (backend: {})",
        addr, backend_name
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("AllSource Auth Service stopped");
    Ok(())
}

fn listen_address(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv6Addr::UNSPECIFIED, port))
}

#[cfg(test)]
mod listener_tests {
    #[test]
    fn listens_on_private_ipv6_interface() {
        assert_eq!(super::listen_address(3903).to_string(), "[::]:3903");
    }
}

/// GET /health — health check endpoint.
async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "status": "ok",
            "backend": state.backend_name,
        })),
    )
}

/// POST /api/auth/demo/start — create a temporary demo account.
///
/// Returns credentials for a demo user with a random email and password.
/// Useful for "Try without signing up" flows.
async fn demo_start_handler(State(state): State<AppState>) -> impl IntoResponse {
    let demo_id = uuid::Uuid::new_v4().to_string();
    let email = format!("demo-{}@allsource.dev", &demo_id[..8]);
    let password = format!("Demo-{}", &demo_id[..12]);
    let name = "Demo User".to_string();

    // Sign up the demo user via the auth module
    match state
        .auth_module
        .create_demo_account(&email, &password, &name)
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            axum::Json(serde_json::json!({
                "email": email,
                "password": password,
                "name": name,
                "demo": true,
            })),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to create demo account: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({
                    "error": "Failed to create demo account",
                })),
            )
                .into_response()
        }
    }
}

/// Wait for SIGTERM or SIGINT for graceful shutdown.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => info!("Received SIGINT, shutting down"),
        () = terminate => info!("Received SIGTERM, shutting down"),
    }
}
