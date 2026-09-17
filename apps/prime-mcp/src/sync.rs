//! Push-only sync from local Prime → remote `AllSource` Core.
//!
//! Prime runs on the user's laptop; the remote Core can't reach it. So sync is
//! one-directional: a background task queries the local store for events with
//! the `prime.` prefix that the remote hasn't seen yet, then `POST`s each
//! event to `<remote_url>/api/v1/events` with the tenant API key as Bearer
//! auth. The cursor (last-pushed event timestamp) is persisted to the data
//! directory so restarts resume from where they left off instead of replaying
//! the whole history.
//!
//! The remote endpoint is the same one chronis uses (see
//! `apps/chronis/src/infrastructure/http_core_client.rs`); the tenant is
//! derived from the API key by the gateway.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use allsource_core::{
    embedded::{EventView, Query},
    prime::Prime,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const CURSOR_FILE: &str = ".prime_sync_cursor.json";
const PRIME_EVENT_PREFIX: &str = "prime.";
const BATCH_SIZE: usize = 200;

/// Runtime configuration for the sync loop. Parsed from CLI flags in `main`.
#[derive(Clone, Debug)]
pub struct SyncConfig {
    /// Base URL of the remote `AllSource` Core (e.g. `https://api.all-source.xyz`).
    pub remote_url: String,
    /// Tenant API key. Sent as `Authorization: Bearer <key>`.
    pub api_key: String,
    /// How often to drain the queue. Default 1s.
    pub interval: Duration,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Cursor {
    /// Latest event timestamp we've successfully pushed.
    last_pushed_at: Option<DateTime<Utc>>,
}

impl Cursor {
    fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    fn save(&self, path: &Path) -> Result<()> {
        let s = serde_json::to_string(self).context("serialize cursor")?;
        std::fs::write(path, s).context("write cursor")?;
        Ok(())
    }
}

#[derive(Serialize)]
struct IngestRequest<'a> {
    event_type: &'a str,
    entity_id: &'a str,
    payload: &'a Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<&'a Value>,
}

/// Build the path where the sync cursor is persisted.
pub fn cursor_path(data_dir: &Path) -> PathBuf {
    data_dir.join(CURSOR_FILE)
}

/// Run the push-only sync loop until the process exits.
///
/// On each tick, query Prime's `EmbeddedCore` for `prime.*` events newer than
/// the persisted cursor, push them to the remote in order, and advance the
/// cursor. Failures are logged to stderr and retried on the next tick — the
/// remote being unreachable is not fatal.
pub async fn run_sync_loop(prime: Arc<Prime>, config: SyncConfig, data_dir: PathBuf) {
    let cursor_path = cursor_path(data_dir.as_path());
    let mut cursor = Cursor::load(&cursor_path);
    let http = reqwest::Client::new();
    let base_url = config.remote_url.trim_end_matches('/').to_string();

    tracing::info!(
        remote_url = %base_url,
        interval_ms = config.interval.as_millis() as u64,
        resume_from = ?cursor.last_pushed_at,
        "Prime sync started (push-only)"
    );

    let mut ticker = tokio::time::interval(config.interval);
    // Skip the initial immediate tick — wait one interval before the first push
    // to avoid hammering the remote while Prime is still warming up.
    ticker.tick().await;

    loop {
        ticker.tick().await;

        match drain_once(&prime, &http, &base_url, &config.api_key, &mut cursor).await {
            Ok(pushed) if pushed > 0 => {
                if let Err(e) = cursor.save(&cursor_path) {
                    tracing::warn!(error = %e, "failed to persist sync cursor");
                }
                tracing::debug!(pushed, latest = ?cursor.last_pushed_at, "Prime sync flushed");
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, "Prime sync tick failed — will retry");
            }
        }
    }
}

/// Drain **all** pending `prime.*` events to the remote in batches, then return.
///
/// Unlike [`run_sync_loop`] (which ticks forever), this is the one-shot path for
/// `--mode hound`: the extractor writes the code graph into the local store, then
/// we push the whole thing once and exit. Loops [`drain_once`] until a tick pushes
/// nothing, persisting the cursor after each non-empty batch so a mid-flush crash
/// resumes instead of replaying. Returns the total number of events pushed.
pub async fn flush_all(prime: &Prime, config: &SyncConfig, data_dir: &Path) -> Result<usize> {
    let cursor_path = cursor_path(data_dir);
    let mut cursor = Cursor::load(&cursor_path);
    let http = reqwest::Client::new();
    let base_url = config.remote_url.trim_end_matches('/').to_string();

    let mut total = 0;
    loop {
        let pushed = drain_once(prime, &http, &base_url, &config.api_key, &mut cursor).await?;
        if pushed == 0 {
            break;
        }
        total += pushed;
        cursor
            .save(&cursor_path)
            .context("persist sync cursor during flush")?;
    }
    Ok(total)
}

/// Push one batch of pending events. Returns the number pushed.
async fn drain_once(
    prime: &Prime,
    http: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    cursor: &mut Cursor,
) -> Result<usize> {
    let mut query = Query::new()
        .event_type_prefix(PRIME_EVENT_PREFIX)
        .limit(BATCH_SIZE);
    if let Some(since) = cursor.last_pushed_at {
        query = query.since(since);
    }

    let events: Vec<EventView> = prime
        .core()
        .query(query)
        .await
        .context("query local Prime for pending events")?;

    // The `since` filter is inclusive of equal timestamps, so the last cursor
    // event can echo back. Drop anything we've already pushed.
    let cutoff = cursor.last_pushed_at;
    let pending: Vec<&EventView> = events
        .iter()
        .filter(|e| match cutoff {
            Some(t) => e.timestamp > t,
            None => true,
        })
        .collect();

    if pending.is_empty() {
        return Ok(0);
    }

    let mut pushed = 0;
    for ev in &pending {
        push_event(http, base_url, api_key, ev).await?;
        pushed += 1;
        cursor.last_pushed_at = Some(ev.timestamp);
    }

    Ok(pushed)
}

async fn push_event(
    http: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    ev: &EventView,
) -> Result<()> {
    let url = format!("{base_url}/api/v1/events");
    let body = IngestRequest {
        event_type: &ev.event_type,
        entity_id: &ev.entity_id,
        payload: &ev.payload,
        metadata: ev.metadata.as_ref(),
    };

    let resp = http
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("remote rejected event {}: HTTP {status}: {text}", ev.id);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn cursor_roundtrip() {
        let dir = tempdir().expect("tempdir");
        let path = cursor_path(dir.path());

        // Missing → default
        let loaded = Cursor::load(&path);
        assert!(loaded.last_pushed_at.is_none());

        // Save then load
        let cursor = Cursor {
            last_pushed_at: Some(Utc::now()),
        };
        cursor.save(&path).expect("save");
        let reloaded = Cursor::load(&path);
        assert_eq!(reloaded.last_pushed_at, cursor.last_pushed_at);
    }

    #[test]
    fn cursor_load_ignores_garbage() {
        let dir = tempdir().expect("tempdir");
        let path = cursor_path(dir.path());
        std::fs::write(&path, "not json").expect("write garbage");
        // Should not panic; should return default.
        let loaded = Cursor::load(&path);
        assert!(loaded.last_pushed_at.is_none());
    }

    #[tokio::test]
    async fn flush_all_pushes_every_event_then_is_idempotent() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/events"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let dir = tempdir().expect("tempdir");
        let prime = Prime::open(dir.path()).await.expect("open prime");
        // Two nodes + one edge between them → three prime.* events.
        let a = prime
            .add_node("file", serde_json::json!({ "path": "a.rs" }))
            .await
            .expect("add file");
        let b = prime
            .add_node("function", serde_json::json!({ "name": "foo" }))
            .await
            .expect("add fn");
        prime
            .add_edge_weighted(
                &format!("node:file:{}", a.as_str()),
                &format!("node:function:{}", b.as_str()),
                "defines",
                1.0,
                Some(serde_json::json!({ "confidence": "EXTRACTED" })),
            )
            .await
            .expect("add edge");

        let config = SyncConfig {
            remote_url: server.uri(),
            api_key: "test-key".to_string(),
            interval: Duration::from_millis(10),
        };

        // First flush pushes all three events.
        let pushed = flush_all(&prime, &config, dir.path()).await.expect("flush");
        assert_eq!(pushed, 3);

        // Second flush is a no-op — the cursor has advanced past everything.
        let again = flush_all(&prime, &config, dir.path())
            .await
            .expect("flush again");
        assert_eq!(again, 0);

        // Sanity: at least one event carried our confidence tag to the remote.
        let confidence_hits = server
            .received_requests()
            .await
            .expect("requests")
            .iter()
            .filter(|r| {
                std::str::from_utf8(&r.body).is_ok_and(|s| s.contains("EXTRACTED"))
            })
            .count();
        assert_eq!(confidence_hits, 1);
    }
}
