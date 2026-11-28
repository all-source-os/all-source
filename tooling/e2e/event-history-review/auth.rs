use super::{AuthBackend, AuthConfig, AuthModule};
use anyhow::{Result, ensure};
use axum::{Router, routing::get};
use better_auth_core::{UpdateUser, UserOps, entity::AuthUser};
use std::time::Duration;
use tokio::net::TcpListener;
use uuid::Uuid;

#[tokio::test]
#[ignore = "starts an isolated reviewer sign-in service"]
async fn serve() -> Result<()> {
    const EMAIL: &str = "reviewer@example.test";
    let password = std::env::var("HISTORY_FIXTURE_PASSWORD")?;
    ensure!(
        password.len() >= 32,
        "fixture requires a fresh generated password"
    );
    let config = AuthConfig {
        backend: "memory".into(),
        secret: format!("{}{}", Uuid::new_v4(), Uuid::new_v4()),
        base_url: "http://127.0.0.1:14905/api/auth".into(),
        allsource_url: None,
        allsource_query_url: None,
        allsource_api_key: None,
        google_client_id: None,
        google_client_secret: None,
        github_client_id: None,
        github_client_secret: None,
        trusted_origins: vec![],
    };
    let auth = AuthModule::build(config).await?;
    auth.create_demo_account(EMAIL, &password, "Review engineer")
        .await?;
    let AuthBackend::Memory(backend) = &auth.backend else {
        anyhow::bail!("fixture requires in-memory account storage")
    };
    let db = backend.database();
    let user = db
        .get_user_by_email(EMAIL)
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing reviewer account"))?;
    db.update_user(
        user.id(),
        UpdateUser {
            email_verified: Some(true),
            ..Default::default()
        },
    )
    .await?;

    let listener = TcpListener::bind(("127.0.0.1", 14905)).await?;
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .nest("/api/auth", auth.router());
    axum::serve(listener, app)
        .with_graceful_shutdown(tokio::time::sleep(Duration::from_secs(280)))
        .await?;
    Ok(())
}
