use std::{collections::HashSet, path::Path, sync::Arc};

use allsource_core::embedded::{EmbeddedCore, IngestEvent, Query};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    domain::error::ChronError,
    infrastructure::{
        backend::CoreBackend, config::ChronisConfig, http_core_client::HttpCoreClient,
    },
};

#[derive(Debug, Serialize, Deserialize, Default)]
struct SyncState {
    last_push_timestamp: Option<DateTime<Utc>>,
    last_pull_timestamp: Option<DateTime<Utc>>,
    /// Remote event UUIDs already imported into local Core.
    #[serde(default)]
    pulled_ids: HashSet<String>,
    /// Local event UUIDs already pushed to remote Core.
    #[serde(default)]
    pushed_ids: HashSet<String>,
}

fn sync_state_path(workspace_root: &Path) -> std::path::PathBuf {
    workspace_root.join(".chronis/sync/sync_state.json")
}

fn load_sync_state(workspace_root: &Path) -> Result<SyncState, ChronError> {
    let path = sync_state_path(workspace_root);
    if !path.exists() {
        return Ok(SyncState::default());
    }
    let content = std::fs::read_to_string(&path)?;
    serde_json::from_str(&content).map_err(|e| ChronError::Sync(format!("parse sync state: {e}")))
}

fn save_sync_state(workspace_root: &Path, state: &SyncState) -> Result<(), ChronError> {
    let path = sync_state_path(workspace_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(state)
        .map_err(|e| ChronError::Sync(format!("serialize sync state: {e}")))?;
    std::fs::write(&path, content)?;
    Ok(())
}

/// Sync local embedded Core events with a remote Core over HTTP.
///
/// Flow: pull remote events → ingest locally, push local events → ingest remotely.
/// Deduplication uses UUID tracking (pulled_ids/pushed_ids) plus `source_id` metadata.
pub async fn sync_http(
    backend: &CoreBackend,
    config: &ChronisConfig,
    workspace_root: &Path,
) -> Result<(), ChronError> {
    let embedded: &Arc<EmbeddedCore> = backend
        .as_embedded()
        .ok_or_else(|| ChronError::Sync("sync only works in embedded mode".into()))?;

    let remote_url = config
        .remote_url()
        .ok_or_else(|| ChronError::Sync("sync requires [sync] remote_url in config.toml".into()))?;

    let remote = HttpCoreClient::new(remote_url).with_auth(config.remote_auth()?);
    remote
        .health()
        .await
        .map_err(|_| ChronError::Sync(format!("cannot reach remote Core at {remote_url}")))?;

    let instance_id = &config.instance_id;
    let mut state = load_sync_state(workspace_root)?;

    // ── Phase 1: Pull (remote → local) ──────────────────────────────────
    let pull_since = state.last_pull_timestamp.map(|t| t - Duration::seconds(1));

    let remote_events = remote.query_events(pull_since).await?;
    let mut pull_count = 0u64;

    for ev in &remote_events {
        let remote_id = ev.id.to_string();

        // Skip events already imported (UUID dedup)
        if state.pulled_ids.contains(&remote_id) {
            continue;
        }

        // Skip events that originated from this instance
        if let Some(ref meta) = ev.metadata
            && meta.get("source_id").and_then(|v| v.as_str()) == Some(instance_id)
        {
            // Still track the ID so we don't re-check next time
            state.pulled_ids.insert(remote_id);
            continue;
        }

        let mut metadata = ev.metadata.clone().unwrap_or_default();
        metadata["synced_from"] = serde_json::json!("remote");

        embedded
            .ingest(IngestEvent {
                entity_id: &ev.entity_id,
                event_type: &ev.event_type,
                payload: ev.payload.clone(),
                metadata: Some(metadata),
                tenant_id: Some(&ev.tenant_id),
            })
            .await
            .map_err(|e| ChronError::Sync(format!("local ingest: {e}")))?;

        state.pulled_ids.insert(remote_id);
        pull_count += 1;
    }

    if let Some(latest) = remote_events.iter().map(|e| e.timestamp).max() {
        state.last_pull_timestamp = Some(latest);
    }

    if pull_count > 0 {
        println!("Pulled {pull_count} events from remote.");
    }

    // ── Phase 2: Push (local → remote) ──────────────────────────────────
    let push_since = state.last_push_timestamp.map(|t| t - Duration::seconds(1));

    let mut q = Query::new();
    if let Some(since) = push_since {
        q = q.since(since);
    }

    let local_events = embedded
        .query(q)
        .await
        .map_err(|e| ChronError::Sync(format!("local query: {e}")))?;

    let mut push_count = 0u64;

    for ev in &local_events {
        let local_id = ev.id.to_string();

        // Skip events already pushed (UUID dedup)
        if state.pushed_ids.contains(&local_id) {
            continue;
        }

        // Skip events that were pulled from remote (avoid echo)
        if let Some(ref meta) = ev.metadata
            && meta.get("synced_from").is_some()
        {
            state.pushed_ids.insert(local_id);
            continue;
        }

        let mut metadata = ev.metadata.clone().unwrap_or_default();
        metadata["source_id"] = serde_json::json!(instance_id);

        remote
            .ingest_event(
                &ev.entity_id,
                &ev.event_type,
                &ev.payload,
                Some(&metadata),
                Some(&ev.tenant_id),
            )
            .await?;

        state.pushed_ids.insert(local_id);
        push_count += 1;
    }

    if let Some(latest) = local_events.iter().map(|e| e.timestamp).max() {
        state.last_push_timestamp = Some(latest);
    }

    if push_count > 0 {
        println!("Pushed {push_count} events to remote.");
    }

    save_sync_state(workspace_root, &state)?;

    if pull_count == 0 && push_count == 0 {
        println!("Already up to date.");
    }

    Ok(())
}
