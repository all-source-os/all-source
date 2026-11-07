//! Pull-side ingester: folds remote Core `email.*` events into the local Prime
//! graph as `interaction` nodes (plus `thread` and `person` nodes and the edges
//! between them). It is the inverse of `sync.rs` (which pushes local `prime.*`
//! events out): here we poll the remote Core for `email.*` events the Control
//! Plane wrote and project each into the graph so the inbox is visible via
//! recall/neighbors next to the people it involves.
//!
//! See `docs/proposals/AI_INBOX_ON_ALLSOURCE.md` §4.4 and docs/contracts/email-events.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use allsource_core::prime::{EntityId, Prime};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::remote_auth::RemoteAuth;

const CURSOR_FILE: &str = ".prime_email_ingest_cursor.json";
const EMAIL_EVENT_PREFIX: &str = "email.";
const BATCH_SIZE: usize = 200;

/// Options for a single fold.
#[derive(Clone, Copy, Debug)]
pub struct IngestOpts {
    /// Embed subject+snippet into the interaction node's vector. Disabled in
    /// tests so the embedding model is never loaded.
    pub embed: bool,
}

impl Default for IngestOpts {
    fn default() -> Self {
        Self { embed: true }
    }
}

/// What a fold produced — for observability and test assertions.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct IngestOutcome {
    pub skipped: bool,
    pub interaction_entity_id: Option<String>,
    pub thread_entity_id: Option<String>,
    pub person_entity_ids: Vec<String>,
    pub edges: usize,
    pub embedded: bool,
}

fn addr_email(v: &Value) -> Option<String> {
    v.get("email").and_then(Value::as_str).map(str::to_string)
}

fn addr_name(v: &Value) -> Option<String> {
    v.get("name").and_then(Value::as_str).map(str::to_string)
}

/// `node:{type}:{id}` entity id for a graph node.
fn node_eid(node_type: &str, id: &str) -> String {
    EntityId::node(node_type, id).to_string()
}

/// Find an existing `person` node by email (case-insensitive), or create one.
async fn upsert_person(
    prime: &Prime,
    email: &str,
    name: Option<&str>,
    tenant_id: &str,
) -> Result<String> {
    for node in prime.nodes_by_type("person") {
        let matches = node
            .properties
            .get("emails")
            .and_then(Value::as_array)
            .is_some_and(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .any(|e| e.eq_ignore_ascii_case(email))
            })
            || node
                .properties
                .get("email")
                .and_then(Value::as_str)
                .is_some_and(|e| e.eq_ignore_ascii_case(email));
        if matches {
            return Ok(node_eid(&node.node_type, node.id.as_str()));
        }
    }
    let id = prime
        .add_node(
            "person",
            json!({ "name": name, "emails": [email], "domain": "workspace", "tenant_id": tenant_id }),
        )
        .await?;
    Ok(node_eid("person", id.as_str()))
}

/// Find an existing `thread` node by `conversation_id`, or create one.
async fn upsert_thread(
    prime: &Prime,
    conversation_id: &str,
    subject: &str,
    tenant_id: &str,
) -> Result<String> {
    for node in prime.nodes_by_type("thread") {
        if node
            .properties
            .get("conversation_id")
            .and_then(Value::as_str)
            == Some(conversation_id)
        {
            return Ok(node_eid(&node.node_type, node.id.as_str()));
        }
    }
    let id = prime
        .add_node(
            "thread",
            json!({ "conversation_id": conversation_id, "subject": subject, "domain": "inbox", "tenant_id": tenant_id }),
        )
        .await?;
    Ok(node_eid("thread", id.as_str()))
}

/// Entity id of an existing interaction node for this message id, if any.
fn existing_interaction(prime: &Prime, message_id: &str) -> Option<String> {
    prime
        .nodes_by_type("interaction")
        .into_iter()
        .find_map(|node| {
            if node.properties.get("message_id").and_then(Value::as_str) == Some(message_id) {
                Some(node_eid(&node.node_type, node.id.as_str()))
            } else {
                None
            }
        })
}

/// Fold one Core `email.*` event into the local Prime graph. Unknown event types
/// are skipped. Re-folding the same message id is a no-op (idempotent), so a
/// cursor replay does not duplicate the graph.
pub async fn ingest_email_event(
    prime: &Prime,
    event_type: &str,
    entity_id: &str,
    tenant_id: &str,
    payload: &Value,
    opts: IngestOpts,
) -> Result<IngestOutcome> {
    let direction = match event_type {
        "email.received" => "inbound",
        "email.sent" => "outbound",
        _ => {
            return Ok(IngestOutcome {
                skipped: true,
                ..Default::default()
            });
        }
    };

    if let Some(existing) = existing_interaction(prime, entity_id) {
        return Ok(IngestOutcome {
            interaction_entity_id: Some(existing),
            ..Default::default()
        });
    }

    let conversation_id = payload
        .get("thread_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let subject = payload
        .get("subject")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let snippet = payload
        .get("snippet")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let at = payload
        .get("received_at")
        .or_else(|| payload.get("sent_at"))
        .and_then(Value::as_str)
        .unwrap_or_default();

    let thread_entity = upsert_thread(prime, conversation_id, subject, tenant_id).await?;

    let interaction_id = prime
        .add_node(
            "interaction",
            json!({
                "channel": "email",
                "direction": direction,
                "subject": subject,
                "snippet": snippet,
                "message_id": entity_id,
                "conversation_id": conversation_id,
                "from": payload.get("from"),
                "to": payload.get("to"),
                "at": at,
                "tenant_id": tenant_id,
                "domain": "inbox",
            }),
        )
        .await?;
    let interaction_entity = node_eid("interaction", interaction_id.as_str());

    let mut outcome = IngestOutcome {
        interaction_entity_id: Some(interaction_entity.clone()),
        thread_entity_id: Some(thread_entity.clone()),
        ..Default::default()
    };

    // interaction --part_of--> thread
    prime
        .add_edge(&interaction_entity, &thread_entity, "part_of", None)
        .await?;
    outcome.edges += 1;

    // from-person (inbound only — a sent payload does not carry the mailbox owner)
    if direction == "inbound"
        && let Some(from) = payload.get("from")
        && let Some(email) = addr_email(from)
    {
        let person = upsert_person(prime, &email, addr_name(from).as_deref(), tenant_id).await?;
        prime
            .add_edge(&interaction_entity, &person, "from", None)
            .await?;
        outcome.edges += 1;
        outcome.person_entity_ids.push(person);
    }

    // to-persons
    if let Some(arr) = payload.get("to").and_then(Value::as_array) {
        for to in arr {
            if let Some(email) = addr_email(to) {
                let person =
                    upsert_person(prime, &email, addr_name(to).as_deref(), tenant_id).await?;
                prime
                    .add_edge(&interaction_entity, &person, "to", None)
                    .await?;
                outcome.edges += 1;
                outcome.person_entity_ids.push(person);
            }
        }
    }

    // Embed subject+snippet (best-effort: a missing model must not fail ingest;
    // the full body is never embedded — privacy, §7).
    if opts.embed {
        let text = format!("{subject} {snippet}");
        match prime.embed_text(&text) {
            Ok(vector) => match prime.embed(&interaction_entity, Some(&text), vector).await {
                Ok(()) => outcome.embedded = true,
                Err(e) => tracing::warn!(error = %e, "email ingest: embed store failed"),
            },
            Err(e) => {
                tracing::warn!(error = %e, "email ingest: embed_text failed (model unavailable?)");
            }
        }
    }

    Ok(outcome)
}

// ---------------------------------------------------------------------------
// Pull loop
// ---------------------------------------------------------------------------

/// Runtime config for the email ingest loop. Mirrors the remote target the
/// push-sync loop uses, so the same `--sync-to`/`--api-key` enable both.
#[derive(Clone, Debug)]
pub struct EmailIngestConfig {
    pub remote_url: String,
    pub auth: RemoteAuth,
    pub interval: Duration,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Cursor {
    last_ingested_at: Option<DateTime<Utc>>,
}

impl Cursor {
    fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(
            path,
            serde_json::to_string(self).context("serialize cursor")?,
        )
        .context("write cursor")?;
        Ok(())
    }
}

/// Path where the email ingest cursor is persisted.
pub fn cursor_path(data_dir: &Path) -> PathBuf {
    data_dir.join(CURSOR_FILE)
}

#[derive(Debug, Deserialize)]
struct RemoteEvent {
    event_type: String,
    entity_id: String,
    #[serde(default)]
    tenant_id: String,
    payload: Value,
    timestamp: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Default)]
struct RemoteEventsResponse {
    #[serde(default)]
    events: Vec<RemoteEvent>,
}

/// Poll the remote Core for `email.*` events and fold them into the local graph
/// until the process exits. Unreachable remote is logged and retried, not fatal.
pub async fn run_email_ingest_loop(
    prime: Arc<Prime>,
    config: EmailIngestConfig,
    data_dir: PathBuf,
) {
    let cursor_path = cursor_path(data_dir.as_path());
    let mut cursor = Cursor::load(&cursor_path);
    let http = reqwest::Client::new();
    let base_url = config.remote_url.trim_end_matches('/').to_string();

    tracing::info!(
        remote_url = %base_url,
        resume_from = ?cursor.last_ingested_at,
        "Prime email ingest started (pull email.* -> graph)"
    );

    let mut ticker = tokio::time::interval(config.interval);
    ticker.tick().await;

    loop {
        ticker.tick().await;
        match pull_once(&prime, &http, &base_url, &config.auth, &mut cursor).await {
            Ok(folded) if folded > 0 => {
                if let Err(e) = cursor.save(&cursor_path) {
                    tracing::warn!(error = %e, "failed to persist email ingest cursor");
                }
                tracing::debug!(folded, latest = ?cursor.last_ingested_at, "email ingest flushed");
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "email ingest tick failed — will retry"),
        }
    }
}

async fn pull_once(
    prime: &Prime,
    http: &reqwest::Client,
    base_url: &str,
    auth: &RemoteAuth,
    cursor: &mut Cursor,
) -> Result<usize> {
    // Build the query string manually (this reqwest build has no `.query()`).
    // The values are controlled; only the RFC3339 `since` needs encoding.
    let mut url = format!(
        "{base_url}/api/v1/events/query?event_type={EMAIL_EVENT_PREFIX}&order=asc&limit={BATCH_SIZE}"
    );
    if let Some(since) = cursor.last_ingested_at {
        let enc = since.to_rfc3339().replace(':', "%3A").replace('+', "%2B");
        url.push_str("&since=");
        url.push_str(&enc);
    }

    let resp = http
        .get(&url)
        .bearer_auth(auth.bearer()?)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("remote rejected events query: HTTP {status}: {text}");
    }
    let body: RemoteEventsResponse = resp.json().await.context("decode events response")?;

    let cutoff = cursor.last_ingested_at;
    let mut folded = 0;
    for ev in body.events {
        // `since` is inclusive of equal timestamps; skip anything already folded.
        if cutoff.is_some_and(|t| ev.timestamp <= t) {
            continue;
        }
        let outcome = ingest_email_event(
            prime,
            &ev.event_type,
            &ev.entity_id,
            &ev.tenant_id,
            &ev.payload,
            IngestOpts::default(),
        )
        .await
        .with_context(|| format!("fold {} {}", ev.event_type, ev.entity_id))?;
        tracing::trace!(
            skipped = outcome.skipped,
            edges = outcome.edges,
            embedded = outcome.embedded,
            persons = outcome.person_entity_ids.len(),
            thread = ?outcome.thread_entity_id,
            interaction = ?outcome.interaction_entity_id,
            "folded email event"
        );
        folded += 1;
        cursor.last_ingested_at = Some(ev.timestamp);
    }
    Ok(folded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn mem() -> Prime {
        Prime::open_in_memory().await.expect("open_in_memory")
    }

    fn no_embed() -> IngestOpts {
        IngestOpts { embed: false }
    }

    fn received(thread: &str, from: &str, to: &str) -> Value {
        json!({
            "thread_id": thread, "subject": "Re: Q3 renewal", "snippet": "following up",
            "from": { "name": "Dana", "email": from },
            "to": [{ "email": to }],
            "received_at": "2026-06-14T09:12:00Z", "folder": "inbox", "labels": []
        })
    }

    #[tokio::test]
    async fn folds_received_into_graph() {
        let p = mem().await;
        let out = ingest_email_event(
            &p,
            "email.received",
            "m1",
            "tnt1",
            &received("th1", "dana@acme.com", "me@all-source.xyz"),
            no_embed(),
        )
        .await
        .unwrap();

        assert!(!out.skipped);
        assert_eq!(p.nodes_by_type("interaction").len(), 1);
        assert_eq!(p.nodes_by_type("thread").len(), 1);
        assert_eq!(p.nodes_by_type("person").len(), 2); // from + to
        assert_eq!(out.edges, 3); // part_of + from + to

        let inter = &p.nodes_by_type("interaction")[0];
        assert_eq!(
            inter.properties.get("tenant_id").and_then(Value::as_str),
            Some("tnt1")
        );
        assert_eq!(
            inter.properties.get("direction").and_then(Value::as_str),
            Some("inbound")
        );
        assert_eq!(
            inter.properties.get("message_id").and_then(Value::as_str),
            Some("m1")
        );
    }

    #[tokio::test]
    async fn dedups_person_and_thread_across_messages() {
        let p = mem().await;
        ingest_email_event(
            &p,
            "email.received",
            "m1",
            "t",
            &received("th1", "dana@acme.com", "me@all-source.xyz"),
            no_embed(),
        )
        .await
        .unwrap();
        ingest_email_event(
            &p,
            "email.received",
            "m2",
            "t",
            &received("th1", "dana@acme.com", "me@all-source.xyz"),
            no_embed(),
        )
        .await
        .unwrap();

        assert_eq!(p.nodes_by_type("interaction").len(), 2);
        assert_eq!(p.nodes_by_type("thread").len(), 1); // same conversation
        assert_eq!(p.nodes_by_type("person").len(), 2); // dana + me, deduped
    }

    #[tokio::test]
    async fn sent_is_outbound_without_from_edge() {
        let p = mem().await;
        let payload = json!({
            "thread_id": "th1", "subject": "Re", "snippet": "thanks",
            "to": [{ "email": "dana@acme.com" }],
            "sent_at": "2026-06-14T09:40:00Z", "direction": "outbound"
        });
        let out = ingest_email_event(&p, "email.sent", "m9", "tnt1", &payload, no_embed())
            .await
            .unwrap();

        assert_eq!(out.edges, 2); // part_of + to (no from)
        assert_eq!(p.nodes_by_type("person").len(), 1);
        let inter = &p.nodes_by_type("interaction")[0];
        assert_eq!(
            inter.properties.get("direction").and_then(Value::as_str),
            Some("outbound")
        );
    }

    #[tokio::test]
    async fn skips_non_email_event() {
        let p = mem().await;
        let out = ingest_email_event(&p, "prime.node.created", "x", "t", &json!({}), no_embed())
            .await
            .unwrap();
        assert!(out.skipped);
        assert_eq!(p.nodes_by_type("interaction").len(), 0);
    }

    #[tokio::test]
    async fn idempotent_on_reingest() {
        let p = mem().await;
        let pl = received("th1", "dana@acme.com", "me@all-source.xyz");
        ingest_email_event(&p, "email.received", "m1", "t", &pl, no_embed())
            .await
            .unwrap();
        let out2 = ingest_email_event(&p, "email.received", "m1", "t", &pl, no_embed())
            .await
            .unwrap();

        assert_eq!(p.nodes_by_type("interaction").len(), 1); // no duplicate
        assert!(out2.interaction_entity_id.is_some());
    }

    #[test]
    fn cursor_roundtrip() {
        let dir = tempdir().unwrap();
        let path = cursor_path(dir.path());
        assert!(Cursor::load(&path).last_ingested_at.is_none());
        let c = Cursor {
            last_ingested_at: Some(Utc::now()),
        };
        c.save(&path).unwrap();
        assert_eq!(Cursor::load(&path).last_ingested_at, c.last_ingested_at);
    }
}
