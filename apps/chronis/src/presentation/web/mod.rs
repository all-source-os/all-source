mod handlers;
mod state;

use crate::infrastructure::core_task_repo::CoreTaskRepository;
use axum::{
    Router,
    routing::{get, post},
};
use state::AppState;
use std::sync::Arc;

pub async fn run(repo: CoreTaskRepository, port: u16, open_browser: bool) -> anyhow::Result<()> {
    let state = AppState {
        repo: Arc::new(repo),
    };

    let app = Router::new()
        // Pages
        .route("/", get(handlers::index))
        .route("/kanban", get(handlers::kanban_page))
        .route("/graph", get(handlers::graph_page))
        .route("/tree", get(handlers::tree_page))
        // Static assets
        .route("/style.css", get(handlers::style_css))
        .route("/htmx.min.js", get(handlers::htmx_js))
        .route("/detail-pane.js", get(handlers::detail_pane_js))
        // JSON API
        .route("/api/tasks", get(handlers::api_tasks))
        .route("/api/tasks/{id}", get(handlers::api_task_detail))
        .route("/api/tasks/{id}/claim", post(handlers::api_claim))
        .route("/api/tasks/{id}/done", post(handlers::api_done))
        .route("/api/tasks/{id}/approve", post(handlers::api_approve))
        .route("/api/graph", get(handlers::api_graph))
        .route("/api/export", get(handlers::api_export))
        // SSE live-reload stream
        .route("/events/stream", get(handlers::events_stream))
        // HTMX partials
        .route("/partials/stats", get(handlers::partial_stats))
        .route("/partials/task-list", get(handlers::partial_task_list))
        .route(
            "/partials/task-detail/{id}",
            get(handlers::partial_task_detail),
        )
        .route("/partials/kanban", get(handlers::partial_kanban))
        .route("/partials/graph", get(handlers::partial_graph))
        .route("/partials/tree", get(handlers::partial_tree))
        .with_state(state);

    // Loopback, not 0.0.0.0. A wildcard bind COEXISTS with an existing bind to
    // the specific 127.0.0.1:<port> — BSD dispatches each connection to the most
    // specific listener — so the viewer would start, report success, and never
    // receive the loopback traffic it just advertised, while the other process
    // answered instead. Binding the address we print makes a taken port an
    // immediate EADDRINUSE. It also stops a local task dashboard being served to
    // the LAN. This collides in practice: the prime-identity service holds
    // 127.0.0.1:3905, which is also this default port.
    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            anyhow::anyhow!(
                "port {port} on 127.0.0.1 is already in use — another process owns it.\n\
                 Find it with `lsof -nP -iTCP:{port} -sTCP:LISTEN`, or pick another port \
                 with `cn serve -p <port>`."
            )
        } else {
            anyhow::Error::new(e).context(format!("cannot bind 127.0.0.1:{port}"))
        }
    })?;

    println!("chronis web viewer: http://localhost:{port}");
    println!("Press Ctrl+C to stop");

    if open_browser {
        let _ = std::process::Command::new("open")
            .arg(format!("http://localhost:{port}"))
            .spawn();
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
    println!("\nShutting down...");
}
