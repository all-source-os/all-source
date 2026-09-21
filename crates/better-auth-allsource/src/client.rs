use crate::error::AllsourceAuthError;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Convert a `snake_case` field name to `camelCase`. Idempotent for inputs
/// that have no underscores (e.g. `email`, `slug`, `credentialID`).
///
/// Used to bridge the gap between adapter callers, which pass snake_case
/// field names, and serialized payloads, whose keys are camelCase because
/// `better_auth_core`'s structs carry `#[serde(rename = "userId")]` etc.
fn to_camel_case(field: &str) -> String {
    if !field.contains('_') {
        return field.to_string();
    }
    let mut out = String::with_capacity(field.len());
    let mut upper_next = false;
    for ch in field.chars() {
        if ch == '_' {
            upper_next = true;
        } else if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Look up `field` on `payload`, trying the literal key first and then
/// its camelCase form. Returns `None` if neither resolves to a string.
fn payload_field_str<'a>(payload: &'a serde_json::Value, field: &str) -> Option<&'a str> {
    payload
        .get(field)
        .and_then(|v| v.as_str())
        .or_else(|| {
            let camel = to_camel_case(field);
            if camel == field {
                None
            } else {
                payload.get(&camel).and_then(|v| v.as_str())
            }
        })
}

/// Build the `payload_filter` query value Core expects: a flat
/// `{"<field>": "<value>"}` map. Core's filter parser
/// (`apps/core/src/store.rs::matches_filter`) decodes the string as a
/// `Map<String, Value>` and requires `payload[k] == v` for every entry.
///
/// An earlier version of this client sent a structured
/// `{"field": "payload.<field>", "op": "eq", "value": <value>}` body. Core
/// interpreted that as "payload must contain literal keys `field`, `op`,
/// `value`", which never matches a real event, so every server-side filter
/// query returned an empty result. The adapter's scan fallback only fires on
/// HTTP 400/422, not on 200-with-empty, so `find_by_field` always returned
/// `None` — duplicating users on every signin (issue #187).
fn build_payload_filter(field: &str, value: &str) -> String {
    serde_json::Value::Object(
        [(field.to_string(), serde_json::Value::String(value.to_string()))]
            .into_iter()
            .collect(),
    )
    .to_string()
}

/// Low-level HTTP client for Allsource Core and Query Service.
#[derive(Clone)]
pub struct AllsourceClient {
    http: Client,
    core_url: String,
    query_url: String,
    api_key: String,
    tenant_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct IngestEvent<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_id: Option<&'a str>,
    entity_id: &'a str,
    event_type: &'a str,
    payload: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct QueryResponse {
    events: Vec<StoredEvent>,
}

#[derive(Debug, Deserialize)]
pub struct StoredEvent {
    pub payload: serde_json::Value,
    /// Core stamps every event with an ingestion timestamp and a per-entity
    /// version. We carry both so the client can pick the genuine newest event
    /// itself rather than trusting the order Core returned them in — see
    /// `newest_event`. Optional/defaulted so a Core build that omits either
    /// field still deserializes (the event then sorts oldest).
    #[serde(default)]
    pub timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    pub version: Option<i64>,
}

/// How many of an entity's most-recent events to pull when resolving its
/// current state. Auth entities accrue only a handful of events, so this is
/// effectively "the whole stream" while bounding pathological cases.
const ENTITY_HISTORY_LIMIT: usize = 1000;

/// True when an event payload carries the soft-delete tombstone written by
/// `AllsourceClient::append_delete` (`{"_deleted": true, ...}`).
fn is_deleted(payload: &serde_json::Value) -> bool {
    payload
        .get("_deleted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Pick the genuinely-newest event by `(timestamp, version)`, independent of
/// the order Core returned them in.
///
/// The deletion-honoring read path used to trust `order=desc` and inspect only
/// `events.first()`. If Core (any version/deployment) failed to honor `order`,
/// `first()` was the entity's *oldest* event — so a deleted user, whose
/// `auth.user.deleted` tombstone is the newest event, read back as live because
/// the `created` event was the one inspected (issues #177, #188). Selecting the
/// max here removes that dependency entirely. Events missing a timestamp sort
/// oldest (`None < Some`), so any well-formed event wins.
fn newest_event(events: &[StoredEvent]) -> Option<&StoredEvent> {
    events.iter().max_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then_with(|| a.version.cmp(&b.version))
    })
}

impl AllsourceClient {
    pub fn new(core_url: &str, query_url: &str, api_key: &str) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build HTTP client");

        Self {
            http,
            core_url: core_url.trim_end_matches('/').to_string(),
            query_url: query_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            tenant_id: None,
        }
    }

    /// Scope writes and reads identically when connecting directly to Core.
    /// Core ingestion does not infer the tenant from the service credential.
    pub fn with_tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    fn tenant_query(&self) -> Vec<(&str, &str)> {
        self.tenant_id.as_deref().map(|id| vec![("tenant_id", id)]).unwrap_or_default()
    }

    /// Append an event to Allsource Core.
    pub async fn append_event(
        &self,
        entity_id: &str,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<(), AllsourceAuthError> {
        let url = format!("{}/api/v1/events", self.core_url);
        let event = IngestEvent {
            tenant_id: self.tenant_id.as_deref(),
            entity_id,
            event_type,
            payload,
        };

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&event)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let message = extract_error_message(resp).await;
            return Err(AllsourceAuthError::Api {
                status: status.as_u16(),
                message,
            });
        }

        Ok(())
    }

    /// Query the latest event for an entity and deserialize the payload.
    pub async fn get_latest<T: DeserializeOwned>(
        &self,
        entity_id: &str,
    ) -> Result<Option<T>, AllsourceAuthError> {
        let url = format!("{}/api/v1/events/query", self.query_url);

        // Fetch the entity's recent events and choose the newest CLIENT-SIDE by
        // (timestamp, version). `order=desc` is sent as a hint so Core can keep
        // the window cheap, but correctness does NOT depend on it: if a deleted
        // user's `auth.user.deleted` tombstone is anywhere in the window, it is
        // the newest event, so `get_latest` returns `None` (issues #177, #188).
        // The limit is generous because auth entities (user/session/account)
        // accrue only a handful of events.
        let resp = self
            .http
            .get(&url)
            .query(&self.tenant_query())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .query(&[
                ("entity_id", entity_id),
                ("limit", &ENTITY_HISTORY_LIMIT.to_string()),
                ("order", "desc"),
            ])
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let message = extract_error_message(resp).await;
            return Err(AllsourceAuthError::Api {
                status: status.as_u16(),
                message,
            });
        }

        let query_resp: QueryResponse = resp.json().await?;

        match newest_event(&query_resp.events) {
            Some(event) => {
                if is_deleted(&event.payload) {
                    return Ok(None);
                }
                let entity: T = serde_json::from_value(event.payload.clone())?;
                Ok(Some(entity))
            }
            None => Ok(None),
        }
    }

    /// Query all non-deleted entities of a given type by event_type prefix.
    /// Scans events, groups by entity_id, takes the latest per entity.
    pub async fn query_all<T: DeserializeOwned>(
        &self,
        event_type_prefix: &str,
        limit: usize,
    ) -> Result<Vec<T>, AllsourceAuthError> {
        let url = format!("{}/api/v1/events/query", self.query_url);

        let resp = self
            .http
            .get(&url)
            .query(&self.tenant_query())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .query(&[
                ("event_type_prefix", event_type_prefix),
                ("limit", &limit.to_string()),
                ("order", "desc"),
            ])
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let message = extract_error_message(resp).await;
            return Err(AllsourceAuthError::Api {
                status: status.as_u16(),
                message,
            });
        }

        let query_resp: QueryResponse = resp.json().await?;
        let mut results = Vec::new();

        // Group by entity_id, take latest per entity. The `order=desc` param
        // above makes Core return newest-first, so the first event seen for
        // each entity_id is its current state.
        let mut seen = std::collections::HashSet::new();
        for event in &query_resp.events {
            let entity_id = event
                .payload
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if seen.contains(entity_id) {
                continue;
            }
            seen.insert(entity_id.to_string());

            // Skip deleted entities
            if is_deleted(&event.payload) {
                continue;
            }

            if let Ok(entity) = serde_json::from_value::<T>(event.payload.clone()) {
                results.push(entity);
            }
        }

        Ok(results)
    }

    /// Search for entities matching a field value using payload filtering.
    pub async fn find_by_field<T: DeserializeOwned>(
        &self,
        event_type_prefix: &str,
        field: &str,
        value: &str,
    ) -> Result<Option<T>, AllsourceAuthError> {
        // Try the literal field name first, then the camelCase form if it
        // differs. Payloads are stored with `#[serde(rename = "userId")]`
        // style keys, so callers passing `user_id` need the camel retry.
        match self
            .find_by_field_filtered::<T>(event_type_prefix, field, value)
            .await?
        {
            Some(entity) => Ok(Some(entity)),
            None => {
                let camel = to_camel_case(field);
                if camel != field {
                    self.find_by_field_filtered::<T>(event_type_prefix, &camel, value)
                        .await
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Inner helper: issues exactly one server-side `payload_filter` query for
    /// `field`. On `400`/`422` (filter unsupported) it falls back to the
    /// case-tolerant in-memory scan.
    async fn find_by_field_filtered<T: DeserializeOwned>(
        &self,
        event_type_prefix: &str,
        field: &str,
        value: &str,
    ) -> Result<Option<T>, AllsourceAuthError> {
        let url = format!("{}/api/v1/events/query", self.query_url);

        let filter = build_payload_filter(field, value);

        let resp = self
            .http
            .get(&url)
            .query(&self.tenant_query())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .query(&[
                ("event_type_prefix", event_type_prefix),
                ("payload_filter", &filter),
                ("limit", "1"),
                ("order", "desc"),
            ])
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            if status.as_u16() == 400 || status.as_u16() == 422 {
                return self
                    .find_by_field_scan(event_type_prefix, field, value)
                    .await;
            }
            let message = extract_error_message(resp).await;
            return Err(AllsourceAuthError::Api {
                status: status.as_u16(),
                message,
            });
        }

        let query_resp: QueryResponse = resp.json().await?;
        match query_resp.events.first() {
            Some(event) => {
                if is_deleted(&event.payload) {
                    return Ok(None);
                }
                match serde_json::from_value::<T>(event.payload.clone()) {
                    Ok(entity) => Ok(Some(entity)),
                    Err(_) => {
                        // event_type_prefix filter may be broken, returning wrong event types.
                        // Fall back to in-memory scan which skips non-matching types gracefully.
                        self.find_by_field_scan(event_type_prefix, field, value)
                            .await
                    }
                }
            }
            None => Ok(None),
        }
    }

    /// Fallback: scan all entities and filter in-memory.
    async fn find_by_field_scan<T: DeserializeOwned>(
        &self,
        event_type_prefix: &str,
        field: &str,
        value: &str,
    ) -> Result<Option<T>, AllsourceAuthError> {
        let url = format!("{}/api/v1/events/query", self.query_url);

        let resp = self
            .http
            .get(&url)
            .query(&self.tenant_query())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .query(&[
                ("event_type_prefix", event_type_prefix),
                ("limit", "10000"),
                ("order", "desc"),
            ])
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let message = extract_error_message(resp).await;
            return Err(AllsourceAuthError::Api {
                status: status.as_u16(),
                message,
            });
        }

        let query_resp: QueryResponse = resp.json().await?;
        let mut seen = std::collections::HashSet::new();

        for event in &query_resp.events {
            let entity_id = event
                .payload
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if seen.contains(entity_id) {
                continue;
            }
            seen.insert(entity_id.to_string());

            if is_deleted(&event.payload) {
                continue;
            }

            let field_val = payload_field_str(&event.payload, field);
            if field_val == Some(value) {
                if let Ok(entity) = serde_json::from_value::<T>(event.payload.clone()) {
                    return Ok(Some(entity));
                }
            }
        }

        Ok(None)
    }

    /// Find all entities matching a field value.
    pub async fn find_all_by_field<T: DeserializeOwned>(
        &self,
        event_type_prefix: &str,
        field: &str,
        value: &str,
    ) -> Result<Vec<T>, AllsourceAuthError> {
        let url = format!("{}/api/v1/events/query", self.query_url);

        let resp = self
            .http
            .get(&url)
            .query(&self.tenant_query())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .query(&[
                ("event_type_prefix", event_type_prefix),
                ("limit", "10000"),
                ("order", "desc"),
            ])
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let message = extract_error_message(resp).await;
            return Err(AllsourceAuthError::Api {
                status: status.as_u16(),
                message,
            });
        }

        let query_resp: QueryResponse = resp.json().await?;
        let mut results = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for event in &query_resp.events {
            let entity_id = event
                .payload
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if seen.contains(entity_id) {
                continue;
            }
            seen.insert(entity_id.to_string());

            if is_deleted(&event.payload) {
                continue;
            }

            let field_val = payload_field_str(&event.payload, field);
            if field_val == Some(value) {
                if let Ok(entity) = serde_json::from_value::<T>(event.payload.clone()) {
                    results.push(entity);
                }
            }
        }

        Ok(results)
    }

    /// Append a deletion marker event.
    pub async fn append_delete(
        &self,
        entity_id: &str,
        event_type: &str,
    ) -> Result<(), AllsourceAuthError> {
        self.append_event(
            entity_id,
            event_type,
            serde_json::json!({ "_deleted": true, "id": entity_id }),
        )
        .await
    }
}

/// Extract a human-readable error message from an API error response.
///
/// Core returns `{"error": "..."}`, so we parse the JSON and extract the
/// `error` field. Falls back to the raw response body if parsing fails.
async fn extract_error_message(resp: reqwest::Response) -> String {
    let body = resp.text().await.unwrap_or_default();
    // Try to extract structured error from Core's JSON response
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
        if let Some(msg) = json.get("error").and_then(|e| e.as_str()) {
            return msg.to_string();
        }
    }
    body
}

#[cfg(test)]
mod tests {
    #[test]
    fn scoped_client_uses_same_tenant_for_ingest_and_query() {
        let client = super::AllsourceClient::new("http://core", "http://core", "test")
            .with_tenant_id("allsource-auth");
        assert_eq!(client.tenant_query(), vec![("tenant_id", "allsource-auth")]);
        let event = super::IngestEvent {
            tenant_id: client.tenant_id.as_deref(),
            entity_id: "auth-user:test",
            event_type: "auth.user.created",
            payload: serde_json::json!({"id":"test"}),
        };
        assert_eq!(serde_json::to_value(event).unwrap()["tenant_id"], "allsource-auth");
        let legacy = super::AllsourceClient::new("http://core", "http://gateway", "test");
        assert!(legacy.tenant_query().is_empty());
    }
    use super::*;
    use better_auth_core::types::User;
    use std::sync::Arc;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    #[test]
    fn to_camel_case_basic() {
        assert_eq!(to_camel_case("user_id"), "userId");
        assert_eq!(to_camel_case("organization_id"), "organizationId");
        assert_eq!(to_camel_case("created_at"), "createdAt");
        assert_eq!(to_camel_case("two_factor_id"), "twoFactorId");
    }

    #[test]
    fn to_camel_case_idempotent_for_camel_or_flat() {
        assert_eq!(to_camel_case("email"), "email");
        assert_eq!(to_camel_case("slug"), "slug");
        assert_eq!(to_camel_case("credentialID"), "credentialID");
        assert_eq!(to_camel_case("userId"), "userId");
    }

    #[test]
    fn payload_field_str_prefers_literal_then_camel() {
        // Camel-keyed payload (what better_auth_core actually serializes).
        let camel = serde_json::json!({
            "userId": "uid-1",
            "organizationId": "org-1",
            "email": "a@b",
        });
        // Snake caller keys resolve via the camel fallback.
        assert_eq!(payload_field_str(&camel, "user_id"), Some("uid-1"));
        assert_eq!(payload_field_str(&camel, "organization_id"), Some("org-1"));
        // Non-renamed keys still work.
        assert_eq!(payload_field_str(&camel, "email"), Some("a@b"));
        // Missing key returns None.
        assert_eq!(payload_field_str(&camel, "missing_field"), None);

        // Snake-keyed payload (hypothetical legacy data) still resolves.
        let snake = serde_json::json!({ "user_id": "uid-2" });
        assert_eq!(payload_field_str(&snake, "user_id"), Some("uid-2"));
    }

    /// Regression for issue #187. Core's `payload_filter` parser expects a
    /// flat `{field: value}` JSON map. The previous structured shape
    /// `{"field": "payload.email", "op": "eq", ...}` matched no events and
    /// made `find_by_field` return `None` for every signin lookup.
    #[test]
    fn build_payload_filter_emits_flat_map() {
        let raw = build_payload_filter("email", "alias+test1@example.com");
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            parsed,
            serde_json::json!({ "email": "alias+test1@example.com" })
        );
    }

    #[test]
    fn build_payload_filter_preserves_special_chars() {
        // Make sure `+` and `@` survive serialization; URL encoding is the
        // HTTP client's job, not ours.
        let raw = build_payload_filter("email", "a+b@c.io");
        assert!(raw.contains("a+b@c.io"), "got {raw}");
    }

    fn event_at(ts: &str, version: i64, payload: serde_json::Value) -> StoredEvent {
        // Build through serde to also exercise StoredEvent deserialization of
        // the Core-shaped event envelope (extra fields are ignored).
        serde_json::from_value(serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000000",
            "event_type": "auth.user.created",
            "entity_id": "auth-user:u1",
            "tenant_id": "t1",
            "payload": payload,
            "timestamp": ts,
            "version": version,
        }))
        .unwrap()
    }

    #[test]
    fn is_deleted_detects_tombstone() {
        assert!(is_deleted(&serde_json::json!({ "_deleted": true, "id": "u1" })));
        assert!(!is_deleted(&serde_json::json!({ "_deleted": false, "id": "u1" })));
        assert!(!is_deleted(&serde_json::json!({ "id": "u1", "email": "a@b" })));
    }

    /// Regression for issue #188. `get_user_by_id` reads via `get_latest`,
    /// which must return the entity's NEWEST event so an `auth.user.deleted`
    /// tombstone hides the user. Crucially this must hold even when Core hands
    /// the events back oldest-first (i.e. ignores `order=desc`): the previous
    /// `events.first()` logic then inspected the `created` event and reported a
    /// deleted user as live. `newest_event` selects by (timestamp, version)
    /// instead, so input order is irrelevant.
    #[test]
    fn newest_event_picks_latest_regardless_of_input_order() {
        let created = event_at(
            "2026-01-01T00:00:00Z",
            0,
            serde_json::json!({ "id": "u1", "email": "a@b" }),
        );
        let deleted = event_at(
            "2026-06-01T00:00:00Z",
            1,
            serde_json::json!({ "_deleted": true, "id": "u1" }),
        );

        // Oldest-first (as a Core that ignores order=desc returns).
        let ascending = vec![
            event_at("2026-01-01T00:00:00Z", 0, created.payload.clone()),
            event_at("2026-06-01T00:00:00Z", 1, deleted.payload.clone()),
        ];
        let newest = newest_event(&ascending).expect("non-empty");
        assert!(is_deleted(&newest.payload), "tombstone must win → user hidden");

        // Newest-first (order=desc honored) yields the same winner.
        let descending = vec![
            event_at("2026-06-01T00:00:00Z", 1, deleted.payload.clone()),
            event_at("2026-01-01T00:00:00Z", 0, created.payload.clone()),
        ];
        assert!(is_deleted(&newest_event(&descending).unwrap().payload));
    }

    #[test]
    fn newest_event_breaks_timestamp_ties_by_version() {
        // Same timestamp: the higher per-entity version is the later write.
        let events = vec![
            event_at("2026-01-01T00:00:00Z", 5, serde_json::json!({ "id": "u1" })),
            event_at(
                "2026-01-01T00:00:00Z",
                6,
                serde_json::json!({ "_deleted": true, "id": "u1" }),
            ),
        ];
        assert!(is_deleted(&newest_event(&events).unwrap().payload));
    }

    #[test]
    fn newest_event_empty_is_none() {
        assert!(newest_event(&[]).is_none());
    }

    /// End-to-end check that `find_by_field("email", ...)` sends Core a
    /// `payload_filter` whose JSON decodes to `{"email": "<value>"}`. This
    /// asserts the wire contract directly; if a future change re-wraps the
    /// filter we'll catch it here.
    #[tokio::test]
    async fn find_by_field_sends_flat_payload_filter() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Capture the inbound request line so we can assert on the query
        // string after the handler returns.
        let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let captured_clone = captured.clone();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read_half, mut write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);
            let mut request_line = String::new();
            reader.read_line(&mut request_line).await.unwrap();
            *captured_clone.lock().await = Some(request_line.clone());

            // Drain headers
            loop {
                let mut buf = String::new();
                reader.read_line(&mut buf).await.unwrap();
                if buf == "\r\n" || buf.is_empty() {
                    break;
                }
            }

            let body = r#"{"events":[]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            write_half.write_all(response.as_bytes()).await.unwrap();
            write_half.shutdown().await.ok();
        });

        let core_url = format!("http://{addr}");
        let client = AllsourceClient::new(&core_url, &core_url, "test-key");
        let _: Option<User> = client
            .find_by_field("auth.user.created", "email", "alias+test1@example.com")
            .await
            .unwrap();

        server.await.unwrap();
        let line = captured.lock().await.clone().expect("request not captured");

        // Extract the URL portion of the request line: `GET <url> HTTP/1.1`.
        let path_and_query = line
            .split_whitespace()
            .nth(1)
            .expect("request line malformed");
        let qs = path_and_query
            .split_once('?')
            .map(|(_, q)| q)
            .expect("query string missing");

        // Find `payload_filter=<value>` and decode it.
        let filter_raw = qs
            .split('&')
            .find_map(|pair| pair.strip_prefix("payload_filter="))
            .expect("payload_filter param missing");
        // `reqwest` percent-encodes the JSON; decode minimally for assertion.
        let decoded = percent_decode(filter_raw);
        let parsed: serde_json::Value = serde_json::from_str(&decoded)
            .unwrap_or_else(|e| panic!("payload_filter not valid JSON: {decoded:?} ({e})"));

        assert_eq!(
            parsed,
            serde_json::json!({ "email": "alias+test1@example.com" }),
            "regression for #187: filter must be a flat key/value map Core can match"
        );
    }

    /// Minimal percent-decoder for the assertion above; we only need to
    /// reverse what `reqwest`'s query encoder produced.
    fn percent_decode(input: &str) -> String {
        let bytes = input.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'+' => {
                    out.push(b' ');
                    i += 1;
                }
                b'%' if i + 2 < bytes.len() => {
                    let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap();
                    out.push(u8::from_str_radix(hex, 16).unwrap());
                    i += 3;
                }
                b => {
                    out.push(b);
                    i += 1;
                }
            }
        }
        String::from_utf8(out).unwrap()
    }
}
