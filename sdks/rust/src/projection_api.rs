//! HTTP helpers on [`CoreClient`] for Core's durable-consumer endpoints. These
//! power the [`ProjectionWorker`](crate::ws) infrastructure but are also useful
//! standalone for users who want to interact with Core's consumer registry
//! directly.
//!
//! Server-side state for a worker lives in a read model; see
//! [`CoreClient::read_model`].

use serde::Serialize;
use serde_json::Value;

use crate::{
    client::CoreClient,
    types::{ConsumerEvent, ConsumerState},
    Error,
};

impl CoreClient {
    /// Register a durable consumer with Core.
    ///
    /// Durable consumers track their own WAL position server-side; reconnecting
    /// to `/api/v1/events/stream?consumer_id=<name>` auto-replays all events
    /// since the last ack before switching to live delivery.
    pub async fn register_consumer(
        &self,
        consumer_id: &str,
        event_type_filters: &[String],
    ) -> Result<ConsumerState, Error> {
        let body = RegisterConsumerRequest {
            consumer_id: consumer_id.to_string(),
            event_type_filters: event_type_filters.to_vec(),
        };
        self.transport().post("/api/v1/consumers", &body).await
    }

    /// Fetch a consumer's current state (cursor position + filters).
    pub async fn get_consumer(&self, consumer_id: &str) -> Result<ConsumerState, Error> {
        let path = format!("/api/v1/consumers/{}", urlencode(consumer_id));
        self.transport().get(&path).await
    }

    /// Poll a durable consumer for events since its last ack.
    ///
    /// Uses `GET /api/v1/consumers/:id/events`. This is the pull-based
    /// alternative to [`EventStreamClient`](crate::EventStreamClient) for
    /// environments where a long-lived WebSocket is awkward. `limit` caps the
    /// batch size (Core defaults to 100 when omitted); the cursor only moves
    /// once you [`Self::ack_consumer`] the highest [`ConsumerEvent::position`]
    /// you processed, so an unacked poll returns the same events again.
    pub async fn poll_consumer_events(
        &self,
        consumer_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ConsumerEvent>, Error> {
        let path = format!("/api/v1/consumers/{}/events", urlencode(consumer_id));
        let query: Vec<(&str, String)> = limit
            .map(|limit| vec![("limit", limit.to_string())])
            .unwrap_or_default();
        let resp: ConsumerEventsResponse = self.transport().get_with_query(&path, &query).await?;
        Ok(resp.events)
    }

    /// Acknowledge events up to `position`, advancing the consumer's cursor.
    ///
    /// After ack, subsequent reconnections replay from `position + 1`.
    pub async fn ack_consumer(&self, consumer_id: &str, position: u64) -> Result<(), Error> {
        let path = format!("/api/v1/consumers/{}/ack", urlencode(consumer_id));
        let body = AckRequest { position };
        let _: Value = self.transport().post(&path, &body).await?;
        Ok(())
    }

    /// Persist a projection worker's checkpoint.
    ///
    /// Thin wrapper over [`Self::ack_consumer`] — the two concepts are the
    /// same primitive (a WAL offset we've committed to), and sharing the
    /// implementation keeps the SDK from reinventing position tracking.
    pub async fn save_checkpoint(&self, worker_name: &str, position: u64) -> Result<(), Error> {
        self.ack_consumer(worker_name, position).await
    }

    /// Load a projection worker's last checkpoint (returns `None` for fresh workers).
    pub async fn load_checkpoint(&self, worker_name: &str) -> Result<Option<u64>, Error> {
        let state = self.get_consumer(worker_name).await?;
        Ok(state.cursor_position)
    }
}

// --- Internal wire types ---

#[derive(Debug, serde::Deserialize)]
struct ConsumerEventsResponse {
    #[serde(default)]
    events: Vec<ConsumerEvent>,
}

#[derive(Debug, Serialize)]
struct RegisterConsumerRequest {
    consumer_id: String,
    event_type_filters: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AckRequest {
    position: u64,
}

/// Percent-encode one path segment: every byte outside `[A-Za-z0-9-._~]`.
pub(crate) fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    async fn make_client(server: &MockServer) -> CoreClient {
        CoreClient::new(&server.uri(), "test-key").unwrap()
    }

    #[test]
    fn urlencode_escapes_everything_outside_the_unreserved_set() {
        assert_eq!(urlencode("skill:abc"), "skill%3Aabc");
        assert_eq!(urlencode("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(urlencode("a/b c"), "a%2Fb%20c");
    }

    #[tokio::test]
    async fn poll_consumer_events_decodes_flattened_events() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/consumers/w1/events"))
            .and(query_param("limit", "2"))
            // Core flattens the event next to `position` (ConsumerEventDto).
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "events": [
                    {
                        "position": 1,
                        "id": "11111111-1111-4111-8111-111111111111",
                        "event_type": "user.created",
                        "entity_id": "user-1",
                        "tenant_id": "default",
                        "payload": {"name": "Ada"},
                        "metadata": null,
                        "timestamp": "2026-01-01T00:00:00Z",
                        "version": 1
                    }
                ],
                "count": 1
            })))
            .mount(&server)
            .await;

        let client = make_client(&server).await;
        let events = client.poll_consumer_events("w1", Some(2)).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].position, 1);
        assert_eq!(events[0].event.event_type, "user.created");
        assert_eq!(events[0].event.entity_id, "user-1");
    }

    #[tokio::test]
    async fn poll_consumer_events_omits_limit_when_unset() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/consumers/w1/events"))
            .and(wiremock::matchers::query_param_is_missing("limit"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "events": [],
                "count": 0
            })))
            .mount(&server)
            .await;

        let client = make_client(&server).await;
        let events = client.poll_consumer_events("w1", None).await.unwrap();
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn register_consumer_returns_state() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/consumers"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "consumer_id": "w1",
                "event_type_filters": ["asset.*"],
                "cursor_position": null
            })))
            .mount(&server)
            .await;

        let client = make_client(&server).await;
        let state = client
            .register_consumer("w1", &["asset.*".to_string()])
            .await
            .unwrap();
        assert_eq!(state.consumer_id, "w1");
        assert_eq!(state.cursor_position, None);
    }

    #[tokio::test]
    async fn ack_consumer_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/consumers/w1/ack"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "ok",
                "consumer_id": "w1",
                "position": 42
            })))
            .mount(&server)
            .await;

        let client = make_client(&server).await;
        client.ack_consumer("w1", 42).await.unwrap();
    }

    #[tokio::test]
    async fn save_checkpoint_is_ack_alias() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/consumers/w1/ack"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "ok"
            })))
            .mount(&server)
            .await;

        let client = make_client(&server).await;
        client.save_checkpoint("w1", 100).await.unwrap();
    }

    #[tokio::test]
    async fn load_checkpoint_returns_cursor_position() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/consumers/w1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "consumer_id": "w1",
                "event_type_filters": [],
                "cursor_position": 57
            })))
            .mount(&server)
            .await;

        let client = make_client(&server).await;
        let pos = client.load_checkpoint("w1").await.unwrap();
        assert_eq!(pos, Some(57));
    }

    #[tokio::test]
    async fn server_error_propagates() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/consumers"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": "internal"
            })))
            .mount(&server)
            .await;

        let client = make_client(&server).await;
        // Use a short-retry config so the test doesn't spin for the default retry budget.
        let err = client.register_consumer("w1", &[]).await.unwrap_err();
        assert!(matches!(
            err,
            Error::Api { status: 500, .. } | Error::CircuitOpen { .. }
        ));
    }
}
