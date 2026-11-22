//! Auto-pagination over the offset-based list endpoints.
//!
//! Both [`QueryClient::query_events`](crate::QueryClient::query_events) and
//! [`QueryClient::list_entities`](crate::QueryClient::list_entities) page with
//! `limit`/`offset` and return a `has_more` flag. The paginators here drive that
//! loop for you, so you never hand-roll offset arithmetic or risk silently
//! truncating a result set at the first page.
//!
//! Paging is only correct over a *stable total order*. Both endpoints provide
//! one: events are ordered by `(timestamp, version)`, and entities by
//! last-event time with an `entity_id` tie-break. For an append-only ascending
//! event scan no row is skipped or repeated between pages. A descending scan of
//! a stream that is still receiving writes can shift; prefer ascending order
//! when paging a live stream.

use crate::{
    client::QueryClient,
    error::Error,
    read_id::ReadId,
    types::{EntitySummary, Event, ListEntitiesParams, QueryEventsParams},
};

/// Per-page item count used when the supplied params set no `limit`.
pub const DEFAULT_PAGE_SIZE: u32 = 100;

/// Auto-paginating cursor over [`QueryClient::query_events`].
///
/// Construct one with
/// [`QueryClient::query_events_paged`](crate::QueryClient::query_events_paged).
///
/// # Example
/// ```no_run
/// # async fn run(qs: allsource::QueryClient) -> Result<(), allsource::Error> {
/// use allsource::QueryEventsParams;
///
/// let mut pages = qs.query_events_paged(
///     QueryEventsParams::new().event_type_prefix("auth.org.").limit(200),
/// );
/// while let Some(events) = pages.next_page().await? {
///     println!("got {} events", events.len());
/// }
/// # Ok(()) }
/// ```
pub struct EventPaginator {
    client: QueryClient,
    params: QueryEventsParams,
    page_size: u32,
    offset: u32,
    exhausted: bool,
    /// `id` of the first event on the page most recently returned. A server
    /// that drops `offset` answers every request with page one, so comparing
    /// the head of consecutive pages is how the walk notices — see
    /// [`no_progress_error`].
    last_page_head: Option<String>,
    read_id: ReadId,
}

/// The error raised when a page repeats the previous page's first item.
///
/// Core's `GET /api/v1/events/query` did not implement `offset` until the fix
/// for issue #250: the DTO never declared the field, `serde_urlencoded` ignores
/// unknown query parameters, and `has_more` was computed without the offset —
/// so every page came back as page one with `has_more: true` and a paginator
/// looped forever, accumulating duplicates until it ran out of memory. A client
/// that keeps asking cannot fix that; it can only refuse to spin, and say which
/// parameter the server dropped.
///
/// The check needs a stable total order to be sound, which ascending paging
/// gives. A descending walk of a stream still taking writes can shift under the
/// window (see the module docs) and, in the pathological case of exactly one
/// page of new events between two requests, would report this error instead of
/// silently returning duplicates — which is still the more useful answer.
fn no_progress_error(what: &str, offset: u32, head: &str) -> Error {
    Error::Protocol(format!(
        "server ignored `offset={offset}`: this page of {what} starts at `{head}` again, \
         the same item the previous page started at. Core applies `offset` on the list \
         endpoints since the fix for issue #250; an older Core drops it and serves page \
         one forever. Upgrade Core, or narrow the query so one page covers it."
    ))
}

impl EventPaginator {
    pub(crate) fn new(client: QueryClient, params: QueryEventsParams, page_size: u32) -> Self {
        let offset = params.offset.unwrap_or(0);
        let read_id = params.read_id.clone().unwrap_or_default();
        Self {
            client,
            params,
            page_size: page_size.max(1),
            offset,
            exhausted: false,
            last_page_head: None,
            read_id,
        }
    }

    /// Items requested per page (the `limit` sent on each request).
    pub fn page_size(&self) -> u32 {
        self.page_size
    }

    /// Offset the next page will start at — i.e. how many items have been
    /// consumed so far.
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// Whether every page has been consumed.
    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    /// Fetch the next page. Returns `Ok(None)` once results are exhausted.
    pub async fn next_page(&mut self) -> Result<Option<Vec<Event>>, Error> {
        if self.exhausted {
            return Ok(None);
        }
        let requested_offset = self.offset;
        let params = self
            .params
            .clone()
            .limit(self.page_size)
            .offset(requested_offset)
            .read_id(self.read_id.clone());
        let resp = self.client.query_events(params).await?;
        let head = resp.events.first().map(|e| e.id.clone());
        if requested_offset > 0 && head.is_some() && head == self.last_page_head {
            self.exhausted = true;
            return Err(no_progress_error(
                "events",
                requested_offset,
                head.as_deref().unwrap_or_default(),
            ));
        }
        self.last_page_head = head;
        let fetched = resp.events.len() as u32;
        self.offset += fetched;
        // Stop when the server says there is no more, or when a short page came
        // back. `has_more` is `None` on older Core: fall back to short-page
        // detection (which may cost one extra empty request on an exact boundary).
        if resp.has_more == Some(false) || fetched < self.page_size {
            self.exhausted = true;
        }
        if fetched == 0 {
            return Ok(None);
        }
        Ok(Some(resp.events))
    }

    /// Drain every remaining page into a single `Vec`.
    ///
    /// Convenience for "give me all matching events". Be deliberate with this on
    /// large streams — it holds the whole result set in memory.
    pub async fn collect_all(mut self) -> Result<Vec<Event>, Error> {
        let mut all = Vec::new();
        while let Some(page) = self.next_page().await? {
            all.extend(page);
        }
        Ok(all)
    }
}

/// Auto-paginating cursor over [`QueryClient::list_entities`].
///
/// Construct one with
/// [`QueryClient::list_entities_paged`](crate::QueryClient::list_entities_paged).
///
/// # Example
/// ```no_run
/// # async fn run(qs: allsource::QueryClient) -> Result<(), allsource::Error> {
/// use allsource::ListEntitiesParams;
///
/// let all = qs
///     .list_entities_paged(ListEntitiesParams::new().event_type_prefix("auth.org."))
///     .collect_all()
///     .await?;
/// println!("{} entities", all.len());
/// # Ok(()) }
/// ```
pub struct EntityPaginator {
    client: QueryClient,
    params: ListEntitiesParams,
    page_size: u32,
    offset: u32,
    exhausted: bool,
    /// `entity_id` of the first entity on the page most recently returned.
    /// Same non-progress guard as [`EventPaginator::last_page_head`].
    last_page_head: Option<String>,
}

impl EntityPaginator {
    pub(crate) fn new(client: QueryClient, params: ListEntitiesParams, page_size: u32) -> Self {
        let offset = params.offset.unwrap_or(0);
        Self {
            client,
            params,
            page_size: page_size.max(1),
            offset,
            exhausted: false,
            last_page_head: None,
        }
    }

    /// Items requested per page (the `limit` sent on each request).
    pub fn page_size(&self) -> u32 {
        self.page_size
    }

    /// Offset the next page will start at — i.e. how many entities have been
    /// consumed so far.
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// Whether every page has been consumed.
    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    /// Fetch the next page. Returns `Ok(None)` once results are exhausted.
    pub async fn next_page(&mut self) -> Result<Option<Vec<EntitySummary>>, Error> {
        if self.exhausted {
            return Ok(None);
        }
        let requested_offset = self.offset;
        let params = self
            .params
            .clone()
            .limit(self.page_size)
            .offset(requested_offset);
        let resp = self.client.list_entities(params).await?;
        let head = resp.entities.first().map(|e| e.entity_id.clone());
        if requested_offset > 0 && head.is_some() && head == self.last_page_head {
            self.exhausted = true;
            return Err(no_progress_error(
                "entities",
                requested_offset,
                head.as_deref().unwrap_or_default(),
            ));
        }
        self.last_page_head = head;
        let fetched = resp.entities.len() as u32;
        self.offset += fetched;
        if !resp.has_more || fetched < self.page_size {
            self.exhausted = true;
        }
        if fetched == 0 {
            return Ok(None);
        }
        Ok(Some(resp.entities))
    }

    /// Drain every remaining page into a single `Vec`.
    pub async fn collect_all(mut self) -> Result<Vec<EntitySummary>, Error> {
        let mut all = Vec::new();
        while let Some(page) = self.next_page().await? {
            all.extend(page);
        }
        Ok(all)
    }
}

#[cfg(test)]
mod tests {
    use crate::QueryClient;
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    fn event_json(id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "event_type": "auth.org.created",
            "entity_id": id,
            "payload": {},
            "timestamp": "2026-01-01T00:00:00Z",
            "version": 1,
            "tenant_id": "default"
        })
    }

    fn entity_json(id: &str) -> serde_json::Value {
        serde_json::json!({
            "entity_id": id,
            "event_count": 1,
            "last_event_type": "auth.org.created",
            "last_event_at": "2026-01-01T00:00:00Z"
        })
    }

    #[tokio::test]
    async fn event_paginator_walks_every_page() {
        let server = MockServer::start().await;
        // Page 1: full page, more to come.
        Mock::given(method("GET"))
            .and(path("/api/v1/events/query"))
            .and(query_param("offset", "0"))
            .and(query_param("limit", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "events": [event_json("e1"), event_json("e2")],
                "count": 2, "total_count": 3, "has_more": true
            })))
            .mount(&server)
            .await;
        // Page 2: short page, done.
        Mock::given(method("GET"))
            .and(path("/api/v1/events/query"))
            .and(query_param("offset", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "events": [event_json("e3")],
                "count": 1, "total_count": 3, "has_more": false
            })))
            .mount(&server)
            .await;

        let client = QueryClient::new(&server.uri(), "test-key").unwrap();
        let all = client
            .query_events_paged(crate::QueryEventsParams::new().limit(2))
            .collect_all()
            .await
            .unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].id, "e1");
        assert_eq!(all[2].id, "e3");
    }

    #[tokio::test]
    async fn event_paginator_stops_on_short_page_when_has_more_absent() {
        // Older Core omits `has_more`; a short page must end the walk.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/events/query"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "events": [event_json("e1")],
                "count": 1
            })))
            .mount(&server)
            .await;

        let client = QueryClient::new(&server.uri(), "test-key").unwrap();
        let mut pages = client.query_events_paged(crate::QueryEventsParams::new().limit(10));
        let first = pages.next_page().await.unwrap();
        assert_eq!(first.map(|p| p.len()), Some(1));
        assert!(pages.next_page().await.unwrap().is_none());
        assert!(pages.is_exhausted());
    }

    #[tokio::test]
    async fn entity_paginator_walks_every_page() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities"))
            .and(query_param("offset", "0"))
            .and(query_param("limit", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "entities": [entity_json("org-a"), entity_json("org-b")],
                "total": 3, "has_more": true
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities"))
            .and(query_param("offset", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "entities": [entity_json("org-c")],
                "total": 3, "has_more": false
            })))
            .mount(&server)
            .await;

        let client = QueryClient::new(&server.uri(), "test-key").unwrap();
        let all = client
            .list_entities_paged(crate::ListEntitiesParams::new().limit(2))
            .collect_all()
            .await
            .unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].entity_id, "org-a");
        assert_eq!(all[2].entity_id, "org-c");
    }

    #[tokio::test]
    async fn paginator_honors_starting_offset() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities"))
            .and(query_param("offset", "5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "entities": [entity_json("org-f")],
                "total": 6, "has_more": false
            })))
            .mount(&server)
            .await;

        let client = QueryClient::new(&server.uri(), "test-key").unwrap();
        let mut pages =
            client.list_entities_paged(crate::ListEntitiesParams::new().limit(10).offset(5));
        assert_eq!(pages.offset(), 5);
        let page = pages.next_page().await.unwrap().unwrap();
        assert_eq!(page[0].entity_id, "org-f");
    }
}
