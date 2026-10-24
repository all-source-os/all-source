use reqwest::header::HeaderMap;
use std::time::Duration;

use crate::types::QueryEventsResponse;

/// Response headers a caller needs to poll politely: the validator to send
/// back as `If-None-Match`, and the server's advertised rate-limit state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResponseMeta {
    /// `ETag`, verbatim, quotes and weak prefix included.
    pub etag: Option<String>,
    /// `RateLimit` (IETF draft structured field), verbatim.
    pub rate_limit: Option<String>,
    /// `RateLimit-Policy`, verbatim.
    pub rate_limit_policy: Option<String>,
}

impl ResponseMeta {
    pub(crate) fn from_headers(headers: &HeaderMap) -> Self {
        let header = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        Self {
            etag: header("etag"),
            rate_limit: header("ratelimit"),
            rate_limit_policy: header("ratelimit-policy"),
        }
    }
}

/// Outcome of [`crate::QueryClient::query_events_conditional`].
#[derive(Debug, Clone)]
pub enum EventsQuery {
    /// A 2xx: the events, with the response headers in
    /// [`QueryEventsResponse::meta`].
    Modified(QueryEventsResponse),
    /// A 304: nothing under the query changed since the `If-None-Match`
    /// validator. Carries the 304's own headers.
    NotModified(ResponseMeta),
}

/// `Retry-After` as delta-seconds. The HTTP-date form is not parsed and
/// yields `None`; AllSource sends delta-seconds only.
pub(crate) fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// The refusal `scope` from a rate-limit body, top level or under `error`.
pub(crate) fn refusal_scope(body: Option<&serde_json::Value>) -> Option<String> {
    body.and_then(|b| {
        b.get("scope")
            .or_else(|| b.get("error").and_then(|e| e.get("scope")))
    })
    .and_then(serde_json::Value::as_str)
    .map(str::to_string)
}

/// A 429 is always a rate-limit refusal. A 503 is one only when the body
/// names a refusal `scope`, which the control plane's admission layer sets
/// for its global capacity refusal; any other 503 stays a server error.
pub(crate) fn is_rate_limit_refusal(status: u16, scope: Option<&str>) -> bool {
    status == 429 || (status == 503 && scope.is_some())
}
