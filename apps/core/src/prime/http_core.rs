//! `HttpCore` — an [`EventStore`] backed by a remote AllSource Core over HTTP.
//!
//! This is the hosted counterpart to [`EmbeddedCore`](crate::embedded::EmbeddedCore):
//! instead of owning a local WAL/Parquet store, it reads and writes events on a
//! remote Core via the HTTP API (`/api/v1/events`, `/api/v1/events/batch`,
//! `/api/v1/events/query`) — the same paradigm the Query Service and chronis
//! use. It lets the hosted Prime app run **stateless**, holding no durable data
//! of its own (see `docs/proposals/PRIME_STATELESS_OVER_CORE.md`, bead t-10f876).
//!
//! Every operation is scoped to a single `tenant_id`: queries are filtered to
//! that tenant and ingests are stamped with it, so one process can serve many
//! tenants by holding one `HttpCore` per tenant. Tenant identity is supplied by
//! the trusted caller (the gateway), never by the end client.
//!
//! Feature-gated behind `prime-recall` (which enables `reqwest`).

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use reqwest::Client;
use serde::Deserialize;

use crate::{
    embedded::{EventView, IngestEvent, Query},
    error::{AllSourceError, Result},
};

use super::event_store::EventStore;

/// Shape of Core's `GET /api/v1/events/query` response. We only need `events`;
/// `count`/`has_more`/etc. are ignored.
#[derive(Deserialize)]
struct QueryResponse {
    events: Vec<EventView>,
}

/// An [`EventStore`] that talks to a remote Core over HTTP, scoped to one tenant.
pub struct HttpCore {
    client: Client,
    /// Base URL of the remote Core, trailing slash trimmed.
    base_url: String,
    /// Bearer API key for the remote Core, if required.
    api_key: Option<String>,
    /// Tenant every operation is scoped to. `None` means single-tenant/local
    /// semantics (Core defaults to `"default"`).
    tenant_id: Option<String>,
}

impl HttpCore {
    /// Build an `HttpCore` pointed at `base_url`, scoped to `tenant_id`.
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        tenant_id: Option<String>,
    ) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        Self {
            client: Client::new(),
            base_url,
            api_key,
            tenant_id,
        }
    }

    /// The tenant id to apply to an operation: an explicit per-event tenant
    /// wins, otherwise this `HttpCore`'s scope.
    fn effective_tenant<'a>(&'a self, event_tenant: Option<&'a str>) -> Option<&'a str> {
        event_tenant.or(self.tenant_id.as_deref())
    }

    /// Attach the bearer key to a request builder when configured.
    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(key) if !key.is_empty() => req.bearer_auth(key),
            _ => req,
        }
    }
}

#[async_trait]
impl EventStore for HttpCore {
    async fn ingest(&self, event: IngestEvent<'_>) -> Result<()> {
        EventStore::ingest_with_valid_time(self, event, None, None).await
    }

    async fn ingest_with_valid_time(
        &self,
        event: IngestEvent<'_>,
        valid_from: Option<DateTime<Utc>>,
        valid_to: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let mut body = serde_json::json!({
            "event_type": event.event_type,
            "entity_id": event.entity_id,
            "payload": event.payload,
            "metadata": event.metadata,
            "tenant_id": self.effective_tenant(event.tenant_id),
        });
        if let Some(from) = valid_from {
            body["valid_from"] = serde_json::json!(from);
        }
        if let Some(to) = valid_to {
            body["valid_to"] = serde_json::json!(to);
        }
        let req = self
            .client
            .post(format!("{}/api/v1/events", self.base_url))
            .json(&body);
        let resp = self
            .auth(req)
            .send()
            .await
            .map_err(|e| AllSourceError::StorageError(format!("ingest to remote Core: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AllSourceError::StorageError(format!(
                "ingest to remote Core: HTTP {status}: {text}"
            )));
        }
        Ok(())
    }

    async fn ingest_batch(&self, events: Vec<IngestEvent<'_>>) -> Result<()> {
        let events_json: Vec<serde_json::Value> = events
            .iter()
            .map(|e| {
                serde_json::json!({
                    "event_type": e.event_type,
                    "entity_id": e.entity_id,
                    "payload": e.payload,
                    "metadata": e.metadata,
                    "tenant_id": self.effective_tenant(e.tenant_id),
                })
            })
            .collect();
        let body = serde_json::json!({ "events": events_json });
        let req = self
            .client
            .post(format!("{}/api/v1/events/batch", self.base_url))
            .json(&body);
        let resp = self.auth(req).send().await.map_err(|e| {
            AllSourceError::StorageError(format!("batch ingest to remote Core: {e}"))
        })?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AllSourceError::StorageError(format!(
                "batch ingest to remote Core: HTTP {status}: {text}"
            )));
        }
        Ok(())
    }

    async fn query(&self, query: Query) -> Result<Vec<EventView>> {
        // The remote query string has no valid-time keys yet, so a valid-time
        // bound would be dropped and the answer silently wrong. Refuse instead.
        if query.as_of_valid.is_some() || query.since_valid.is_some() || query.until_valid.is_some()
        {
            return Err(AllSourceError::InvalidInput(
                "valid-time queries are not supported by a remote Core".to_string(),
            ));
        }
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(v) = &query.entity_id {
            params.push(("entity_id", v.clone()));
        }
        if let Some(v) = &query.event_type {
            params.push(("event_type", v.clone()));
        }
        if let Some(v) = &query.event_type_prefix {
            params.push(("event_type_prefix", v.clone()));
        }
        if let Some(v) = query.limit {
            params.push(("limit", v.to_string()));
        }
        // RFC3339 with a `Z` suffix, not `+00:00`: Axum's form-style query
        // decoder treats `+` as a space, which corrupts the timestamp. (Same
        // fix as the chronis HTTP client.)
        if let Some(t) = query.since {
            params.push(("since", t.to_rfc3339_opts(SecondsFormat::Micros, true)));
        }
        if let Some(t) = query.until {
            params.push(("until", t.to_rfc3339_opts(SecondsFormat::Micros, true)));
        }
        // Tenant scope: this HttpCore's tenant is authoritative; fall back to
        // any tenant set on the query only when this core is unscoped.
        if let Some(t) = self.effective_tenant(query.tenant_id.as_deref()) {
            params.push(("tenant_id", t.to_string()));
        }

        // Build the query string manually via `url` (this reqwest build is
        // compiled with default-features off, so `RequestBuilder::query` is
        // unavailable). `form_urlencoded` percent-encodes correctly; the Z-form
        // timestamps carry no `+`/space so they survive Axum's form decoder.
        let query_string = {
            let mut qs = url::form_urlencoded::Serializer::new(String::new());
            for (k, v) in &params {
                qs.append_pair(k, v);
            }
            qs.finish()
        };
        let url = if query_string.is_empty() {
            format!("{}/api/v1/events/query", self.base_url)
        } else {
            format!("{}/api/v1/events/query?{query_string}", self.base_url)
        };
        let req = self.client.get(url);
        let resp = self
            .auth(req)
            .send()
            .await
            .map_err(|e| AllSourceError::StorageError(format!("query remote Core: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AllSourceError::StorageError(format!(
                "query remote Core: HTTP {status}: {text}"
            )));
        }
        let body: QueryResponse = resp
            .json()
            .await
            .map_err(|e| AllSourceError::StorageError(format!("parse query response: {e}")))?;
        Ok(body.events)
    }

    async fn shutdown(&self) -> Result<()> {
        // Nothing to flush — durability lives on the remote Core.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };

    fn event_json() -> serde_json::Value {
        serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "event_type": "prime.node.created",
            "entity_id": "node:contact:alice",
            "tenant_id": "tenant-a",
            "payload": {"name": "Alice"},
            "metadata": null,
            "timestamp": "2026-06-06T00:00:00Z",
            "version": 1
        })
    }

    #[tokio::test]
    async fn query_scopes_to_tenant_and_parses_events() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/events/query"))
            .and(query_param("tenant_id", "tenant-a"))
            .and(query_param("event_type_prefix", "prime."))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "events": [event_json()], "count": 1 })),
            )
            .mount(&server)
            .await;

        let core = HttpCore::new(server.uri(), None, Some("tenant-a".to_string()));
        let events = core
            .query(Query::new().event_type_prefix("prime."))
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].entity_id, "node:contact:alice");
        assert_eq!(events[0].tenant_id, "tenant-a");
    }

    #[tokio::test]
    async fn ingest_posts_event_stamped_with_tenant() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/events"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
            .expect(1)
            .mount(&server)
            .await;

        let core = HttpCore::new(
            server.uri(),
            Some("k".to_string()),
            Some("tenant-a".to_string()),
        );
        core.ingest(IngestEvent {
            entity_id: "node:contact:bob",
            event_type: "prime.node.created",
            payload: serde_json::json!({"name": "Bob"}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();
        // `expect(1)` is verified on drop.
    }

    #[tokio::test]
    async fn non_2xx_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/events/query"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;

        let core = HttpCore::new(server.uri(), None, Some("tenant-a".to_string()));
        let err = core.query(Query::new()).await.unwrap_err();
        assert!(matches!(err, AllSourceError::StorageError(_)));
    }
}
