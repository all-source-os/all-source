//! Process-global writer to the remote Core, for tool handlers that must
//! persist events the local Prime store can't ship (e.g. `inbox_draft` →
//! `email.drafted`). Mirrors `tools::SyncStatus`: set once at startup from
//! `--sync-to` and its credential. Tool handlers otherwise have no access to
//! the credential. The remote gateway injects `tenant_id` from it.

use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::remote_auth::RemoteAuth;

struct CoreWriter {
    base_url: String,
    auth: RemoteAuth,
    client: reqwest::Client,
}

static CORE_WRITER: OnceLock<CoreWriter> = OnceLock::new();

/// Record the remote Core target. Called once at startup; later calls are
/// ignored (same as `tools::set_sync_status`).
pub fn set_core_writer(remote_url: &str, auth: RemoteAuth) {
    let _ = CORE_WRITER.set(CoreWriter {
        base_url: remote_url.trim_end_matches('/').to_string(),
        auth,
        client: reqwest::Client::new(),
    });
}

/// True when a remote Core target is configured (sync enabled).
pub fn is_configured() -> bool {
    CORE_WRITER.get().is_some()
}

/// POST an event to the remote Core. `tenant_id` is injected by the gateway
/// from the credential, so it is not sent. Returns the created event id.
pub async fn ingest_event(
    event_type: &str,
    entity_id: &str,
    payload: &Value,
    metadata: &Value,
) -> Result<String> {
    let w = CORE_WRITER
        .get()
        .context("remote Core not configured (pass --sync-to and a credential)")?;
    let url = format!("{}/api/v1/events", w.base_url);
    let body = json!({
        "event_type": event_type,
        "entity_id": entity_id,
        "payload": payload,
        "metadata": metadata,
    });
    let resp = w
        .client
        .post(&url)
        .bearer_auth(w.auth.bearer()?)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        bail!("remote rejected event: HTTP {status}: {text}");
    }
    let v: Value = resp.json().await.unwrap_or(Value::Null);
    Ok(v.get("event_id")
        .or_else(|| v.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string())
}
