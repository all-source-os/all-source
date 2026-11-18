//! `GET /api/v1/streams` and `GET /api/v1/stats`: stream sizes and the tenant census.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::{client::CoreClient, Error};

/// Order for [`StreamsParams::sort`], applied when sizing with [`StreamsParams::with_bytes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamSort {
    Events,
    Bytes,
}

impl StreamSort {
    fn as_str(self) -> &'static str {
        match self {
            StreamSort::Events => "events",
            StreamSort::Bytes => "bytes",
        }
    }
}

/// Query for [`CoreClient::list_streams`].
#[derive(Debug, Clone, Default)]
pub struct StreamsParams {
    /// Size each stream's events in bytes. Core walks every event of the tenant.
    pub with_bytes: bool,
    /// Sort order. `None` leaves Core's default (event count).
    pub sort: Option<StreamSort>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

impl StreamsParams {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_bytes(mut self) -> Self {
        self.with_bytes = true;
        self
    }

    pub fn sort(mut self, sort: StreamSort) -> Self {
        self.sort = Some(sort);
        self
    }

    pub fn limit(mut self, n: u32) -> Self {
        self.limit = Some(n);
        self
    }

    pub fn offset(mut self, n: u32) -> Self {
        self.offset = Some(n);
        self
    }

    pub(crate) fn to_query_pairs(&self) -> Vec<(&'static str, String)> {
        let mut pairs = Vec::new();
        if self.with_bytes {
            pairs.push(("with_bytes", "true".to_string()));
        }
        if let Some(sort) = self.sort {
            pairs.push(("sort", sort.as_str().to_string()));
        }
        if let Some(limit) = self.limit {
            pairs.push(("limit", limit.to_string()));
        }
        if let Some(offset) = self.offset {
            pairs.push(("offset", offset.to_string()));
        }
        pairs
    }
}

/// One stream. The byte and seven-day fields are present only with
/// [`StreamsParams::with_bytes`].
#[derive(Debug, Clone, Deserialize)]
pub struct StreamInfo {
    pub stream_id: String,
    pub event_count: u64,
    /// RFC 3339.
    pub last_event_at: Option<String>,
    #[serde(default)]
    pub estimated_bytes: Option<u64>,
    #[serde(default)]
    pub events_last_7d: Option<u64>,
}

/// Response from [`CoreClient::list_streams`]. `total` counts the tenant's streams
/// before `offset` and `limit`.
#[derive(Debug, Clone, Deserialize)]
pub struct ListStreamsResponse {
    pub streams: Vec<StreamInfo>,
    pub total: u64,
}

/// The calling tenant's event census from [`CoreClient::tenant_stats`].
#[derive(Debug, Clone, Deserialize)]
pub struct TenantStats {
    #[serde(default)]
    pub event_types: BTreeMap<String, u64>,
    /// RFC 3339.
    #[serde(rename = "oldest_event")]
    pub oldest: Option<String>,
    /// RFC 3339.
    #[serde(rename = "newest_event")]
    pub newest: Option<String>,
}

impl CoreClient {
    /// List the calling tenant's streams. The gateway scopes the listing to the tenant.
    pub async fn list_streams(&self, params: StreamsParams) -> Result<ListStreamsResponse, Error> {
        let pairs = params.to_query_pairs();
        self.transport()
            .get_with_query("/api/v1/streams", &pairs)
            .await
    }

    /// Per-event-type counts and the time range of the calling tenant's events.
    pub async fn tenant_stats(&self) -> Result<TenantStats, Error> {
        self.transport().get("/api/v1/stats").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    #[test]
    fn with_bytes_sends_the_flag_sort_limit_and_offset() {
        let pairs = StreamsParams::new()
            .with_bytes()
            .sort(StreamSort::Bytes)
            .limit(20)
            .offset(40)
            .to_query_pairs();
        assert_eq!(
            pairs,
            vec![
                ("with_bytes", "true".to_string()),
                ("sort", "bytes".to_string()),
                ("limit", "20".to_string()),
                ("offset", "40".to_string()),
            ]
        );
    }

    #[test]
    fn plain_listing_sends_no_pairs() {
        assert!(StreamsParams::new().to_query_pairs().is_empty());
    }

    #[tokio::test]
    async fn list_streams_decodes_sized_streams() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/streams"))
            .and(query_param("with_bytes", "true"))
            .and(query_param("sort", "bytes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "streams": [{
                    "stream_id": "order-1",
                    "event_count": 3,
                    "last_event_at": "2026-11-01T00:00:00Z",
                    "estimated_bytes": 4096,
                    "events_last_7d": 2
                }],
                "total": 9
            })))
            .mount(&server)
            .await;

        let client = CoreClient::new(&server.uri(), "test-key").unwrap();
        let listed = client
            .list_streams(StreamsParams::new().with_bytes().sort(StreamSort::Bytes))
            .await
            .unwrap();
        assert_eq!(listed.total, 9);
        assert_eq!(listed.streams[0].stream_id, "order-1");
        assert_eq!(listed.streams[0].estimated_bytes, Some(4096));
        assert_eq!(listed.streams[0].events_last_7d, Some(2));
    }

    #[tokio::test]
    async fn list_streams_without_bytes_leaves_size_fields_empty() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/streams"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "streams": [{"stream_id": "a", "event_count": 1, "last_event_at": null}],
                "total": 1
            })))
            .mount(&server)
            .await;

        let client = CoreClient::new(&server.uri(), "test-key").unwrap();
        let listed = client.list_streams(StreamsParams::new()).await.unwrap();
        assert_eq!(listed.streams[0].estimated_bytes, None);
        assert_eq!(listed.streams[0].events_last_7d, None);
    }

    #[tokio::test]
    async fn tenant_stats_decodes_census_and_range() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/stats"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_events": 5,
                "total_entities": 2,
                "total_event_types": 2,
                "total_ingested": 5,
                "event_types": {"order.placed": 3, "user.created": 2},
                "oldest_event": "2026-10-01T00:00:00Z",
                "newest_event": "2026-11-01T00:00:00Z"
            })))
            .mount(&server)
            .await;

        let client = CoreClient::new(&server.uri(), "test-key").unwrap();
        let stats = client.tenant_stats().await.unwrap();
        assert_eq!(stats.event_types.get("order.placed"), Some(&3));
        assert_eq!(stats.event_types.get("user.created"), Some(&2));
        assert_eq!(stats.oldest.as_deref(), Some("2026-10-01T00:00:00Z"));
        assert_eq!(stats.newest.as_deref(), Some("2026-11-01T00:00:00Z"));
    }
}
