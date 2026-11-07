//! WebSocket client that tails a remote Core's `/api/v1/events/stream`
//! endpoint, replays incoming events into the local query cache, and
//! forwards change notifications into the `CoreBackend::Remote` change
//! channel so TUI and web can live-reload without polling.
//!
//! Reconnect policy: exponential backoff capped at 30s. The task runs
//! for the lifetime of the process — `workspace.rs` spawns it and
//! forgets it.

use std::{sync::Arc, time::Duration};

use allsource_core::embedded::{EmbeddedCore, IngestEvent};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    http::header::{AUTHORIZATION, HeaderValue},
};

use super::{backend::ChangeEvent, remote_auth::RemoteAuth};

/// Main entry point — runs until the process exits.
pub async fn run_ws_client(
    ws_url: String,
    auth: RemoteAuth,
    local: Arc<EmbeddedCore>,
    change_tx: broadcast::Sender<ChangeEvent>,
) {
    let mut backoff_ms: u64 = 500;
    let backoff_cap_ms: u64 = 30_000;

    loop {
        match connect_and_pump(&ws_url, &auth, &local, &change_tx).await {
            Ok(()) => {
                tracing::info!("remote_stream: connection closed cleanly, reconnecting");
                backoff_ms = 500;
            }
            Err(e) => {
                tracing::warn!(
                    "remote_stream: connection failed ({e}); retrying in {}ms",
                    backoff_ms
                );
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(backoff_cap_ms);
            }
        }
    }
}

/// Open one WebSocket connection and pump events until it drops.
async fn connect_and_pump(
    ws_url: &str,
    auth: &RemoteAuth,
    local: &Arc<EmbeddedCore>,
    change_tx: &broadcast::Sender<ChangeEvent>,
) -> Result<(), String> {
    let mut request = ws_url
        .into_client_request()
        .map_err(|e| format!("invalid ws url {ws_url}: {e}"))?;

    // Asked per attempt so a reconnect never sends an expired assertion.
    if let Some(token) = auth.bearer().map_err(|e| e.to_string())? {
        let bearer = format!("Bearer {token}");
        let value =
            HeaderValue::from_str(&bearer).map_err(|e| format!("invalid bearer header: {e}"))?;
        request.headers_mut().insert(AUTHORIZATION, value);
    }

    let (ws_stream, _resp) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| format!("ws connect: {e}"))?;

    tracing::info!("remote_stream: connected to {ws_url}");

    let (mut write, mut read) = ws_stream.split();

    // Subscribe to task-relevant event families. Filtering upstream keeps
    // the local cache lean and avoids rebroadcasting noise as refresh
    // signals.
    let subscribe = serde_json::json!({
        "type": "subscribe",
        "filters": ["task.*", "workflow.*"],
    });
    write
        .send(Message::Text(subscribe.to_string()))
        .await
        .map_err(|e| format!("send subscribe: {e}"))?;

    while let Some(msg) = read.next().await {
        let msg = msg.map_err(|e| format!("recv: {e}"))?;
        match msg {
            Message::Text(text) => {
                if let Err(e) = handle_text_frame(&text, local, change_tx).await {
                    tracing::warn!("remote_stream: frame handler: {e}");
                }
            }
            Message::Binary(_) => {}
            Message::Ping(p) => {
                write
                    .send(Message::Pong(p))
                    .await
                    .map_err(|e| format!("pong: {e}"))?;
            }
            Message::Pong(_) => {}
            Message::Close(_) => {
                tracing::info!("remote_stream: server closed connection");
                break;
            }
            Message::Frame(_) => {}
        }
    }

    Ok(())
}

/// Parse one JSON text frame and route it.
///
/// Core sends four frame shapes through this WebSocket:
/// - `{"type":"replay","position":N,"event":{...}}` — replay of a past event
/// - `{"type":"replay_complete","replayed":N}` — end-of-replay sentinel
/// - `{"type":"lagged","missed":N}` — broadcast lag notification
/// - a bare `Event` object (live broadcast, no wrapper)
///
/// Batch mode wraps any of these in a JSON array.
async fn handle_text_frame(
    text: &str,
    local: &Arc<EmbeddedCore>,
    change_tx: &broadcast::Sender<ChangeEvent>,
) -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("parse json: {e}"))?;

    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                route_one(item, local, change_tx).await?;
            }
        }
        other => route_one(other, local, change_tx).await?,
    }

    Ok(())
}

async fn route_one(
    value: serde_json::Value,
    local: &Arc<EmbeddedCore>,
    change_tx: &broadcast::Sender<ChangeEvent>,
) -> Result<(), String> {
    let Some(obj) = value.as_object() else {
        return Ok(());
    };

    if let Some(kind) = obj.get("type").and_then(|v| v.as_str()) {
        match kind {
            "replay" => {
                if let Some(inner) = obj.get("event") {
                    apply_event(inner, local, change_tx).await?;
                }
                return Ok(());
            }
            "replay_complete" => {
                let count = obj.get("replayed").and_then(|v| v.as_u64()).unwrap_or(0);
                tracing::info!("remote_stream: replay complete ({count} events)");
                return Ok(());
            }
            "lagged" => {
                let missed = obj.get("missed").and_then(|v| v.as_u64()).unwrap_or(0);
                tracing::warn!("remote_stream: lagged {missed} events — consumers should re-query");
                let _ = change_tx.send(ChangeEvent {
                    entity_id: String::new(),
                    event_type: "chronis.lagged".to_string(),
                });
                return Ok(());
            }
            _ => {}
        }
    }

    // Bare-event shape.
    apply_event(&value, local, change_tx).await
}

/// Replay a single remote event into the local cache and emit a ChangeEvent.
async fn apply_event(
    raw: &serde_json::Value,
    local: &Arc<EmbeddedCore>,
    change_tx: &broadcast::Sender<ChangeEvent>,
) -> Result<(), String> {
    let entity_id =
        extract_str(raw, "entity_id").ok_or_else(|| "event missing entity_id".to_string())?;
    let event_type =
        extract_str(raw, "event_type").ok_or_else(|| "event missing event_type".to_string())?;
    let payload = raw
        .get("payload")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let metadata = raw
        .get("metadata")
        .and_then(|v| if v.is_null() { None } else { Some(v.clone()) });
    let tenant_id = extract_str(raw, "tenant_id");

    local
        .ingest(IngestEvent {
            entity_id: &entity_id,
            event_type: &event_type,
            payload,
            metadata,
            tenant_id: tenant_id.as_deref(),
        })
        .await
        .map_err(|e| format!("replay into local cache: {e}"))?;

    let _ = change_tx.send(ChangeEvent {
        entity_id,
        event_type,
    });
    Ok(())
}

fn extract_str(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(String::from)
}
