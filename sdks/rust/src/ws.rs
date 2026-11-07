//! WebSocket client for Core's `/api/v1/events/stream` endpoint.
//!
//! Only compiled when the `ws` feature is enabled.
//!
//! # Protocol
//!
//! Client connects to `ws(s)://<host>/api/v1/events/stream?consumer_id=<name>`
//! and sends a subscribe frame to apply prefix filters:
//!
//! ```json
//! {"type": "subscribe", "filters": ["asset.*", "trade.*"]}
//! ```
//!
//! Server emits frames of three shapes:
//! - Replay event: `{"type":"replay","position":N,"event":{...}}`
//! - Sentinel: `{"type":"replay_complete","replayed":N}` — transitions to live mode
//! - Live event: bare `Event` JSON (no `type` field)
//! - Lagged: `{"type":"lagged","missed":N}` — broadcast channel overflowed

use crate::{
    credentials::{Credential, CredentialProvider},
    Error, Event,
};
use futures_util::{SinkExt, Stream};
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    tungstenite::{client::IntoClientRequest, protocol::Message},
    MaybeTlsStream, WebSocketStream,
};

/// Phase of the event stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamMode {
    /// Event is from catch-up replay — `position` is authoritative.
    Replay,
    /// Event is live — `position` is None; client must track its own counter.
    Live,
}

/// A single event received on the stream.
#[derive(Debug, Clone)]
pub struct StreamedEvent {
    /// WAL offset. `Some` during replay, `None` during live.
    pub position: Option<u64>,
    /// The event itself.
    pub event: Event,
    /// Which phase this event belongs to.
    pub mode: StreamMode,
}

/// An item yielded by the event stream.
// `Event` is much larger than the two control variants, but it is also the
// variant that dominates: `ReplayComplete` fires once per connection and
// `Lagged` only on overflow. Boxing to even out the size would add a heap
// allocation to every streamed event — paying on the hot path to shrink the
// rare cases. Kept inline deliberately; boxing would also be a breaking change
// for consumers matching on `StreamItem::Event(e)`.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum StreamItem {
    /// An event (replay or live).
    Event(StreamedEvent),
    /// Catch-up phase finished; subsequent events are live.
    ReplayComplete {
        /// Number of events replayed.
        replayed: u64,
    },
    /// Server dropped events because its broadcast channel overflowed.
    /// Consumer should reconnect with a known position to catch up.
    Lagged {
        /// Number of events missed.
        missed: u64,
    },
}

/// Builder + factory for opening a subscription.
#[derive(Debug, Clone)]
pub struct EventStreamClient {
    base_url: String,
    credential: Credential,
}

impl EventStreamClient {
    /// Create a client pointed at a Core base URL (e.g. `http://localhost:3900`).
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self::with_credential(base_url.into(), Credential::Static(api_key.into()))
    }

    /// Create a client that asks `provider` for the bearer on every connect.
    pub fn with_credential_provider(
        base_url: impl Into<String>,
        provider: impl CredentialProvider + 'static,
    ) -> Self {
        Self::with_credential(base_url.into(), Credential::Provider(Arc::new(provider)))
    }

    pub(crate) fn with_credential(base_url: String, credential: Credential) -> Self {
        Self {
            base_url,
            credential,
        }
    }

    /// Open a subscription.
    ///
    /// - `consumer_id` — durable consumer name. Core will replay events since the
    ///   consumer's last acked position before switching to live delivery.
    /// - `filters` — prefix filters (e.g. `["asset.*"]`) sent in the subscribe frame.
    pub async fn connect(
        &self,
        consumer_id: &str,
        filters: &[String],
    ) -> Result<EventStream, Error> {
        let ws_url = build_ws_url(&self.base_url, consumer_id)?;
        let mut request = ws_url
            .into_client_request()
            .map_err(|e| Error::WebSocket(format!("failed to build WS request: {e}")))?;
        request.headers_mut().insert(
            "Authorization",
            format!("Bearer {}", self.credential.bearer()?)
                .parse()
                .map_err(|e| Error::WebSocket(format!("invalid api key: {e}")))?,
        );

        let (mut ws, _resp) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|e| Error::WebSocket(format!("connect failed: {e}")))?;

        // Send subscribe frame with prefix filters.
        let subscribe = serde_json::json!({
            "type": "subscribe",
            "filters": filters,
        });
        ws.send(Message::Text(subscribe.to_string()))
            .await
            .map_err(|e| Error::WebSocket(format!("subscribe send failed: {e}")))?;

        Ok(EventStream {
            inner: ws,
            mode: StreamMode::Replay,
        })
    }
}

/// Stream of parsed [`StreamItem`]s over a live WebSocket connection.
pub struct EventStream {
    inner: WebSocketStream<MaybeTlsStream<TcpStream>>,
    mode: StreamMode,
}

impl EventStream {
    /// Current phase. Starts as `Replay`; transitions to `Live` after a
    /// `replay_complete` frame.
    pub fn mode(&self) -> StreamMode {
        self.mode
    }

    /// Close the WebSocket gracefully.
    pub async fn close(mut self) -> Result<(), Error> {
        self.inner
            .close(None)
            .await
            .map_err(|e| Error::WebSocket(format!("close failed: {e}")))
    }
}

impl Stream for EventStream {
    type Item = Result<StreamItem, Error>;

    // Every other arm of the match below `return`s, so the `continue` arms are
    // technically redundant — but they are the arms that say "this frame is not
    // an event, go read another message". Written as bare `{}` blocks they read
    // as "do nothing" in a poll loop where doing nothing and looping are very
    // different outcomes. Keeping them explicit.
    #[allow(clippy::needless_continue)]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match futures_util::ready!(Pin::new(&mut self.inner).poll_next(cx)) {
                None => return Poll::Ready(None),
                Some(Err(e)) => {
                    return Poll::Ready(Some(Err(Error::WebSocket(format!("read failed: {e}")))))
                }
                Some(Ok(Message::Text(text))) => {
                    match parse_frame(text.as_str(), self.mode) {
                        Ok(Some(StreamItem::ReplayComplete { replayed })) => {
                            self.mode = StreamMode::Live;
                            return Poll::Ready(Some(Ok(StreamItem::ReplayComplete { replayed })));
                        }
                        Ok(Some(item)) => return Poll::Ready(Some(Ok(item))),
                        Ok(None) => continue, // frame ignored, loop for next message
                        Err(e) => return Poll::Ready(Some(Err(e))),
                    }
                }
                Some(Ok(Message::Binary(_))) => continue,
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
                Some(Ok(Message::Close(_))) => return Poll::Ready(None),
                Some(Ok(Message::Frame(_))) => continue,
            }
        }
    }
}

fn build_ws_url(base_url: &str, consumer_id: &str) -> Result<String, Error> {
    let trimmed = base_url.trim_end_matches('/');
    let scheme_split = trimmed.split_once("://").ok_or_else(|| {
        Error::WebSocket(format!("invalid base_url (missing scheme): {base_url}"))
    })?;
    let ws_scheme = match scheme_split.0 {
        "http" => "ws",
        "https" => "wss",
        "ws" | "wss" => scheme_split.0,
        other => {
            return Err(Error::WebSocket(format!(
                "unsupported scheme '{other}' (expected http/https/ws/wss)"
            )))
        }
    };
    let encoded_id = url_encode(consumer_id);
    Ok(format!(
        "{ws_scheme}://{host}/api/v1/events/stream?consumer_id={encoded_id}",
        host = scheme_split.1,
    ))
}

fn url_encode(s: &str) -> String {
    // Minimal percent-encoding for reserved characters that realistically appear
    // in a consumer_id (space, &, ?, #, =, +, /, %). Consumer ids are usually
    // ascii identifiers so this is a defensive pass.
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

fn parse_frame(text: &str, current_mode: StreamMode) -> Result<Option<StreamItem>, Error> {
    let val: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| Error::WebSocket(format!("invalid JSON frame: {e}")))?;

    // Frames with a "type" tag are control/replay frames.
    if let Some(type_tag) = val.get("type").and_then(|v| v.as_str()) {
        match type_tag {
            "replay" => {
                let position = val
                    .get("position")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| Error::WebSocket("replay frame missing position".into()))?;
                let event_val = val
                    .get("event")
                    .ok_or_else(|| Error::WebSocket("replay frame missing event".into()))?;
                let event: Event = serde_json::from_value(event_val.clone())?;
                Ok(Some(StreamItem::Event(StreamedEvent {
                    position: Some(position),
                    event,
                    mode: StreamMode::Replay,
                })))
            }
            "replay_complete" => {
                let replayed = val
                    .get("replayed")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                Ok(Some(StreamItem::ReplayComplete { replayed }))
            }
            "lagged" => {
                let missed = val
                    .get("missed")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                Ok(Some(StreamItem::Lagged { missed }))
            }
            "subscribe" => Ok(None), // echo of our own subscribe — ignore
            other => {
                tracing::debug!("unknown frame type: {other}");
                Ok(None)
            }
        }
    } else {
        // Bare event (live mode after replay_complete).
        let event: Event = serde_json::from_value(val)?;
        Ok(Some(StreamItem::Event(StreamedEvent {
            position: None,
            event,
            mode: current_mode,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event_json() -> serde_json::Value {
        serde_json::json!({
            "id": "evt-1",
            "event_type": "asset.registered",
            "entity_id": "BTC",
            "payload": {"symbol": "BTC"},
            "metadata": {},
            "timestamp": "2026-04-17T00:00:00Z",
            "version": 1,
        })
    }

    #[test]
    fn parses_replay_frame() {
        let frame = serde_json::json!({
            "type": "replay",
            "position": 42,
            "event": sample_event_json(),
        })
        .to_string();
        let item = parse_frame(&frame, StreamMode::Replay).unwrap().unwrap();
        match item {
            StreamItem::Event(e) => {
                assert_eq!(e.position, Some(42));
                assert_eq!(e.mode, StreamMode::Replay);
                assert_eq!(e.event.event_type, "asset.registered");
            }
            other => panic!("expected Event, got {other:?}"),
        }
    }

    #[test]
    fn parses_replay_complete() {
        let frame = r#"{"type":"replay_complete","replayed":17}"#;
        let item = parse_frame(frame, StreamMode::Replay).unwrap().unwrap();
        match item {
            StreamItem::ReplayComplete { replayed } => assert_eq!(replayed, 17),
            other => panic!("expected ReplayComplete, got {other:?}"),
        }
    }

    #[test]
    fn parses_lagged() {
        let frame = r#"{"type":"lagged","missed":99}"#;
        let item = parse_frame(frame, StreamMode::Live).unwrap().unwrap();
        match item {
            StreamItem::Lagged { missed } => assert_eq!(missed, 99),
            other => panic!("expected Lagged, got {other:?}"),
        }
    }

    #[test]
    fn parses_bare_event_as_live() {
        let frame = sample_event_json().to_string();
        let item = parse_frame(&frame, StreamMode::Live).unwrap().unwrap();
        match item {
            StreamItem::Event(e) => {
                assert_eq!(e.position, None);
                assert_eq!(e.mode, StreamMode::Live);
                assert_eq!(e.event.entity_id, "BTC");
            }
            other => panic!("expected Event, got {other:?}"),
        }
    }

    #[test]
    fn unknown_type_is_ignored() {
        let frame = r#"{"type":"heartbeat"}"#;
        let item = parse_frame(frame, StreamMode::Live).unwrap();
        assert!(item.is_none(), "unknown frame should be skipped");
    }

    #[test]
    fn invalid_json_is_error() {
        let result = parse_frame("not json", StreamMode::Live);
        assert!(result.is_err());
    }

    #[test]
    fn replay_missing_position_errors() {
        let frame = r#"{"type":"replay","event":{}}"#;
        let result = parse_frame(frame, StreamMode::Replay);
        assert!(result.is_err());
    }

    #[test]
    fn builds_ws_url_from_http() {
        let url = build_ws_url("http://localhost:3900", "my-worker").unwrap();
        assert_eq!(
            url,
            "ws://localhost:3900/api/v1/events/stream?consumer_id=my-worker"
        );
    }

    #[test]
    fn builds_wss_url_from_https() {
        let url = build_ws_url("https://core.example.com/", "w1").unwrap();
        assert_eq!(
            url,
            "wss://core.example.com/api/v1/events/stream?consumer_id=w1"
        );
    }

    #[test]
    fn preserves_ws_scheme() {
        let url = build_ws_url("ws://localhost:3900", "w").unwrap();
        assert!(url.starts_with("ws://"));
    }

    #[test]
    fn encodes_consumer_id() {
        let url = build_ws_url("http://localhost:3900", "worker with spaces").unwrap();
        assert!(
            url.ends_with("consumer_id=worker%20with%20spaces"),
            "url was: {url}"
        );
    }

    #[test]
    fn rejects_missing_scheme() {
        let err = build_ws_url("localhost:3900", "w").unwrap_err();
        assert!(matches!(err, Error::WebSocket(_)));
    }

    #[test]
    fn rejects_unsupported_scheme() {
        let err = build_ws_url("ftp://x", "w").unwrap_err();
        assert!(matches!(err, Error::WebSocket(_)));
    }

    // End-to-end test using a real TCP listener that accepts the WS handshake
    // and streams canned frames.
    #[tokio::test]
    async fn stream_yields_replay_then_live_sequence() {
        use futures_util::StreamExt;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Server task: accept, upgrade, send canned frames, then drop.
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws_stream = tokio_tungstenite::accept_async(stream).await.unwrap();

            // Drain the subscribe frame (format covered by builds_ws_url_* tests).
            let _subscribe = ws_stream.next().await.unwrap().unwrap();

            let frames = vec![
                serde_json::json!({
                    "type": "replay",
                    "position": 1,
                    "event": sample_event_json(),
                })
                .to_string(),
                serde_json::json!({
                    "type": "replay",
                    "position": 2,
                    "event": sample_event_json(),
                })
                .to_string(),
                r#"{"type":"replay_complete","replayed":2}"#.to_string(),
                sample_event_json().to_string(),
            ];

            for f in frames {
                ws_stream.send(Message::Text(f)).await.unwrap();
            }
            // Close cleanly; drop ws_stream to release the TCP socket.
            let _ = ws_stream.close(None).await;
            drop(ws_stream);
        });

        let client = EventStreamClient::new(format!("http://{addr}"), "test-key");
        let mut stream = client
            .connect("test-worker", &["asset.*".into()])
            .await
            .unwrap();

        let item1 = stream.next().await.unwrap().unwrap();
        match item1 {
            StreamItem::Event(e) => {
                assert_eq!(e.position, Some(1));
                assert_eq!(e.mode, StreamMode::Replay);
            }
            other => panic!("expected replay event, got {other:?}"),
        }

        let item2 = stream.next().await.unwrap().unwrap();
        assert!(matches!(
            item2,
            StreamItem::Event(StreamedEvent {
                position: Some(2),
                mode: StreamMode::Replay,
                ..
            })
        ));

        let item3 = stream.next().await.unwrap().unwrap();
        assert!(matches!(item3, StreamItem::ReplayComplete { replayed: 2 }));
        assert_eq!(stream.mode(), StreamMode::Live);

        let item4 = stream.next().await.unwrap().unwrap();
        match item4 {
            StreamItem::Event(e) => {
                assert_eq!(e.position, None);
                assert_eq!(e.mode, StreamMode::Live);
            }
            other => panic!("expected live event, got {other:?}"),
        }

        // After server drops its ws_stream, our stream should yield None (EOF).
        drop(stream);
        server.await.unwrap();
    }
}
