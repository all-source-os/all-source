use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use allsource_core::embedded::{Config, EmbeddedCore};

use super::{
    backend::CoreBackend,
    config::{ChronisConfig, CoreMode},
    core_task_repo::CoreTaskRepository,
    http_core_client::HttpCoreClient,
    projection::TaskProjection,
};
use crate::domain::error::{ChronError, CoreError};

const CHRONIS_DIR: &str = ".chronis";

pub struct Workspace {
    pub root: PathBuf,
    pub config: ChronisConfig,
    backend: Arc<CoreBackend>,
}

impl Workspace {
    /// Walk up from `start` looking for a `.chronis/` directory.
    pub fn find_root(start: &Path) -> Option<PathBuf> {
        let mut dir = start.to_path_buf();
        loop {
            if dir.join(CHRONIS_DIR).is_dir() {
                return Some(dir);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// Discover workspace from cwd and open the appropriate backend.
    pub async fn open() -> Result<Self, ChronError> {
        let cwd = std::env::current_dir()?;
        let root = Self::find_root(&cwd).ok_or(ChronError::NoWorkspace(cwd))?;
        let config = ChronisConfig::load(&root)?;

        let backend = match config.mode {
            CoreMode::Embedded => {
                let data_dir = root.join(CHRONIS_DIR);
                let core_config = Config::builder()
                    .data_dir(&data_dir)
                    .single_tenant(true)
                    .wal_fsync_interval_ms(100)
                    .build()
                    .map_err(|e| CoreError(e.to_string()))?;

                let core = EmbeddedCore::open(core_config)
                    .await
                    .map_err(|e| CoreError(e.to_string()))?;

                let core = Arc::new(core);

                // CRITICAL: hydrate the in-memory pile from the durable Parquet
                // archive BEFORE registering the projection. EmbeddedCore::open
                // recovers the WAL but leaves Parquet cold — the multi-tenant
                // server hydrates lazily on first query, but embedded chronis
                // has no such query path: the projection *is* the read surface.
                // Without this, whenever the WAL is short or has been truncated/
                // compacted the projection boots EMPTY and `cn list` reports
                // "No tasks found" even though every event is durable on disk.
                // The backfill below then replays the full hydrated history.
                core.inner()
                    .hydrate_all_from_storage()
                    .map_err(|e| CoreError(e.to_string()))?;

                core.inner()
                    .register_projection_with_backfill(
                        &(Arc::new(TaskProjection::new())
                            as Arc<dyn allsource_core::application::Projection>),
                    )
                    .map_err(|e| CoreError(e.to_string()))?;

                let backend = CoreBackend::new_embedded(core);

                // Spawn the WAL-tail task so changes from another chronis
                // process sharing this `.chronis/` directory show up live.
                if let Some((change_tx, core)) = backend.embedded_change_sender() {
                    let wal_dir = data_dir.join("wal");
                    tokio::spawn(async move {
                        super::wal_tail::run_wal_tail(wal_dir, core, change_tx).await;
                    });
                }

                backend
            }
            CoreMode::Remote => {
                let url = config.remote_url().ok_or_else(|| {
                    ChronError::Sync(
                        "mode = \"remote\" requires [sync] remote_url in config.toml".into(),
                    )
                })?;
                let auth = config.remote_auth()?;
                let client = HttpCoreClient::new(url).with_auth(auth.clone());
                client.health().await?;

                // Create an in-memory Core for local projection cache
                let local_config = Config::builder()
                    .single_tenant(true)
                    .build()
                    .map_err(|e| CoreError(e.to_string()))?;
                let local = EmbeddedCore::open(local_config)
                    .await
                    .map_err(|e| CoreError(e.to_string()))?;
                let local = Arc::new(local);

                local
                    .inner()
                    .register_projection_with_backfill(
                        &(Arc::new(TaskProjection::new())
                            as Arc<dyn allsource_core::application::Projection>),
                    )
                    .map_err(|e| CoreError(e.to_string()))?;

                // Replay all remote events into local Core for projections
                let events = client.query_events(None).await?;
                for ev in &events {
                    local
                        .ingest(allsource_core::embedded::IngestEvent {
                            entity_id: &ev.entity_id,
                            event_type: &ev.event_type,
                            payload: ev.payload.clone(),
                            metadata: ev.metadata.clone(),
                            tenant_id: Some(&ev.tenant_id),
                        })
                        .await
                        .map_err(|e| CoreError(e.to_string()))?;
                }

                let backend = CoreBackend::new_remote(client, Arc::clone(&local));

                // Spawn the WebSocket live-stream client so subscribers see
                // events arrive in real time without HTTP polling.
                let ws_url = derive_ws_url(url);
                let change_tx = backend
                    .remote_change_sender()
                    .expect("remote backend has a change_tx");
                let local_for_task = Arc::clone(&local);
                tokio::spawn(async move {
                    super::remote_stream::run_ws_client(ws_url, auth, local_for_task, change_tx)
                        .await;
                });

                backend
            }
        };

        Ok(Self {
            root,
            config,
            backend: Arc::new(backend),
        })
    }

    /// Build a `CoreTaskRepository` from this workspace's backend.
    pub fn repo(&self) -> CoreTaskRepository {
        CoreTaskRepository::new(Arc::clone(&self.backend))
    }

    pub fn backend(&self) -> &CoreBackend {
        &self.backend
    }

    pub async fn shutdown(self) -> Result<(), ChronError> {
        let Some(backend) = Arc::into_inner(self.backend) else {
            // Other references still alive (e.g., leaked repo) — can't take ownership.
            // Not an error: the EmbeddedCore will flush on drop anyway.
            return Ok(());
        };
        match backend {
            CoreBackend::Embedded { core, .. } => match Arc::into_inner(core) {
                Some(core) => core
                    .shutdown()
                    .await
                    .map_err(|e| CoreError(e.to_string()).into()),
                None => Ok(()),
            },
            CoreBackend::Remote { .. } => Ok(()),
        }
    }
}

/// Convert an HTTP(S) Core URL into the WebSocket live-stream URL.
///
/// `http://localhost:3900` becomes `ws://localhost:3900/api/v1/events/stream`;
/// `https://core.example.com` becomes `wss://core.example.com/api/v1/events/stream`.
fn derive_ws_url(http_url: &str) -> String {
    let trimmed = http_url.trim_end_matches('/');
    let base = if let Some(rest) = trimmed.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        trimmed.to_string()
    };
    format!("{base}/api/v1/events/stream")
}

/// Create a new .chronis/ workspace in the given directory.
pub fn init_workspace(path: &Path) -> Result<(), ChronError> {
    init_workspace_inner(path, None)
}

/// Create a new .chronis/ workspace pre-configured for remote sync.
pub fn init_workspace_with_remote(
    path: &Path,
    remote_url: &str,
    api_key: Option<&str>,
) -> Result<(), ChronError> {
    init_workspace_with_sync(
        path,
        super::config::SyncConfig {
            remote_url: Some(remote_url.to_string()),
            api_key: api_key.map(String::from),
            ..Default::default()
        },
    )
}

/// Create a new .chronis/ workspace with the given `[sync]` section.
pub fn init_workspace_with_sync(
    path: &Path,
    sync: super::config::SyncConfig,
) -> Result<(), ChronError> {
    init_workspace_inner(path, Some(sync))
}

fn init_workspace_inner(
    path: &Path,
    sync: Option<super::config::SyncConfig>,
) -> Result<(), ChronError> {
    let chronis_dir = path.join(CHRONIS_DIR);
    if chronis_dir.exists() {
        return Err(ChronError::WorkspaceExists(path.display().to_string()));
    }
    std::fs::create_dir_all(&chronis_dir)?;

    let config = ChronisConfig {
        sync,
        ..ChronisConfig::default()
    };

    let config_content = toml::to_string_pretty(&config)
        .map_err(|e| ChronError::Sync(format!("serialize config: {e}")))?;

    let config_path = chronis_dir.join("config.toml");
    std::fs::write(&config_path, config_content)?;

    // .gitignore: track config, ignore data and sync state
    let gitignore_path = chronis_dir.join(".gitignore");
    std::fs::write(&gitignore_path, "wal/\nstorage/\nsync/\n")?;

    println!("Initialized chronis workspace in {}", chronis_dir.display());
    if config.sync.as_ref().is_some_and(|s| s.remote_url.is_some()) {
        println!("Remote sync configured. Run `cn sync` to push/pull events.");
    }
    Ok(())
}
