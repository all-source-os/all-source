use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::remote_auth::RemoteAuth;
use crate::domain::error::ChronError;

/// HTTP client for a remote AllSource Core instance.
pub struct HttpCoreClient {
    client: reqwest::Client,
    base_url: String,
    auth: RemoteAuth,
}

/// Matches `EventView` from allsource-core but is independently deserializable
/// from the Core HTTP API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteEvent {
    pub id: Uuid,
    pub event_type: String,
    pub entity_id: String,
    pub tenant_id: String,
    pub payload: serde_json::Value,
    pub metadata: Option<serde_json::Value>,
    pub timestamp: DateTime<Utc>,
    pub version: i64,
}

impl RemoteEvent {
    pub fn from_event_view(ev: allsource_core::embedded::EventView) -> Self {
        Self {
            id: ev.id,
            event_type: ev.event_type,
            entity_id: ev.entity_id,
            tenant_id: ev.tenant_id,
            payload: ev.payload,
            metadata: ev.metadata,
            timestamp: ev.timestamp,
            version: ev.version,
        }
    }
}

/// Query parameters for the Core events HTTP API.
#[derive(Default)]
pub struct QueryParams<'a> {
    pub entity_id: Option<&'a str>,
    pub event_type: Option<&'a str>,
    pub since: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
struct QueryResponse {
    events: Vec<RemoteEvent>,
}

#[derive(Serialize)]
struct IngestRequest<'a> {
    event_type: &'a str,
    entity_id: &'a str,
    payload: &'a serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<&'a serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_id: Option<&'a str>,
}

impl HttpCoreClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            auth: RemoteAuth::None,
        }
    }

    pub fn with_api_key(mut self, api_key: Option<&str>) -> Self {
        self.auth = api_key.map_or(RemoteAuth::None, |k| RemoteAuth::ApiKey(k.to_string()));
        self
    }

    pub fn with_auth(mut self, auth: RemoteAuth) -> Self {
        self.auth = auth;
        self
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> Result<reqwest::RequestBuilder, ChronError> {
        Ok(match self.auth.bearer()? {
            Some(token) => req.bearer_auth(token),
            None => req,
        })
    }

    pub async fn health(&self) -> Result<(), ChronError> {
        let req = self.client.get(format!("{}/health", self.base_url));
        let resp = self
            .auth(req)?
            .send()
            .await
            .map_err(|e| ChronError::Sync(format!("remote health check failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(ChronError::Sync(format!(
                "remote Core unhealthy: HTTP {}",
                resp.status()
            )));
        }
        Ok(())
    }

    /// Convenience wrapper: query with only a `since` filter.
    /// Paginates in chunks to avoid HTTP timeouts on large result sets.
    pub async fn query_events(
        &self,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<RemoteEvent>, ChronError> {
        const PAGE_SIZE: usize = 500;
        let mut all_events = Vec::new();
        let mut current_since = since;

        loop {
            let page = self
                .query_events_filtered(QueryParams {
                    since: current_since,
                    limit: Some(PAGE_SIZE),
                    ..Default::default()
                })
                .await?;

            let page_len = page.len();
            if page_len == 0 {
                break;
            }

            // Advance the cursor to after the last event's timestamp
            if let Some(last) = page.last() {
                current_since = Some(last.timestamp);
            }

            all_events.extend(page);

            // If we got fewer than PAGE_SIZE, we've reached the end
            if page_len < PAGE_SIZE {
                break;
            }
        }

        Ok(all_events)
    }

    /// Query events with full filter support.
    pub async fn query_events_filtered(
        &self,
        params: QueryParams<'_>,
    ) -> Result<Vec<RemoteEvent>, ChronError> {
        let url = format!("{}/api/v1/events/query", self.base_url);
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(entity_id) = params.entity_id {
            query.push(("entity_id", entity_id.to_string()));
        }
        if let Some(event_type) = params.event_type {
            query.push(("event_type", event_type.to_string()));
        }
        if let Some(since) = params.since {
            // Use `Z` suffix instead of `+00:00`: per RFC 3986 the `+` doesn't
            // need percent-encoding, but Axum's form-style query decoder treats
            // `+` as a space (the application/x-www-form-urlencoded rule), so
            // the gateway sees `2026-02-26T02:32:24 00:00` and rejects it with
            // "since: input contains invalid characters". The `Z` form is
            // canonical for UTC and avoids the `+`-vs-space ambiguity entirely.
            query.push(("since", since.to_rfc3339_opts(SecondsFormat::Micros, true)));
        }
        if let Some(limit) = params.limit {
            query.push(("limit", limit.to_string()));
        }
        let req = self.client.get(&url).query(&query);
        let resp = self
            .auth(req)?
            .send()
            .await
            .map_err(|e| ChronError::Sync(format!("query remote Core: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ChronError::Sync(format!(
                "query remote Core: HTTP {status}: {body}"
            )));
        }
        let body: QueryResponse = resp
            .json()
            .await
            .map_err(|e| ChronError::Sync(format!("parse query response: {e}")))?;
        Ok(body.events)
    }

    pub async fn ingest_event(
        &self,
        entity_id: &str,
        event_type: &str,
        payload: &serde_json::Value,
        metadata: Option<&serde_json::Value>,
        tenant_id: Option<&str>,
    ) -> Result<(), ChronError> {
        let req = IngestRequest {
            event_type,
            entity_id,
            payload,
            metadata,
            tenant_id,
        };
        let http_req = self
            .client
            .post(format!("{}/api/v1/events", self.base_url))
            .json(&req);
        let resp = self
            .auth(http_req)?
            .send()
            .await
            .map_err(|e| ChronError::Sync(format!("ingest to remote Core: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ChronError::Sync(format!(
                "ingest to remote Core: HTTP {status}: {body}"
            )));
        }
        Ok(())
    }
}
