//! `EventStore` trait — Prime's abstraction over the event I/O backend.
//!
//! Prime holds its backend behind this trait (`Arc<dyn EventStore>`) rather
//! than a concrete [`EmbeddedCore`], so an alternative implementation (e.g. a
//! remote HTTP-backed core) can slot in without changing the facade. The
//! trait exposes exactly the event operations the facade and its callers need:
//! `ingest`, `ingest_batch`, `query`, and `shutdown`.
//!
//! This is intentionally narrow. Construction-time concerns (projection
//! registration, Parquet hydration) still go through the concrete
//! [`EmbeddedCore`] before it is wrapped behind the trait — see
//! [`Prime::from_core`](super::facade::Prime).

use chrono::{DateTime, Utc};

use crate::{
    embedded::{EmbeddedCore, EventView, IngestEvent, Query},
    error::Result,
};

/// Event I/O backend for [`Prime`](super::facade::Prime).
///
/// Object-safe: every method is dispatched through `async_trait`, which
/// rewrites the `async fn`s into `Box`ed-future-returning methods, so
/// `dyn EventStore` is usable behind an `Arc`.
#[async_trait::async_trait]
pub trait EventStore: Send + Sync {
    /// Ingest a single event. See [`EmbeddedCore::ingest`].
    async fn ingest(&self, event: IngestEvent<'_>) -> Result<()>;

    /// Ingest a batch of events with a single write lock acquisition.
    /// See [`EmbeddedCore::ingest_batch`].
    async fn ingest_batch(&self, events: Vec<IngestEvent<'_>>) -> Result<()>;

    /// Ingest an event with an explicit valid interval.
    /// See [`EmbeddedCore::ingest_with_valid_time`].
    async fn ingest_with_valid_time(
        &self,
        event: IngestEvent<'_>,
        valid_from: Option<DateTime<Utc>>,
        valid_to: Option<DateTime<Utc>>,
    ) -> Result<()>;

    /// Query events. See [`EmbeddedCore::query`].
    async fn query(&self, query: Query) -> Result<Vec<EventView>>;

    /// Flush pending writes and shut down cleanly. See [`EmbeddedCore::shutdown`].
    async fn shutdown(&self) -> Result<()>;
}

#[async_trait::async_trait]
impl EventStore for EmbeddedCore {
    async fn ingest(&self, event: IngestEvent<'_>) -> Result<()> {
        EmbeddedCore::ingest(self, event).await
    }

    async fn ingest_batch(&self, events: Vec<IngestEvent<'_>>) -> Result<()> {
        EmbeddedCore::ingest_batch(self, events).await
    }

    async fn ingest_with_valid_time(
        &self,
        event: IngestEvent<'_>,
        valid_from: Option<DateTime<Utc>>,
        valid_to: Option<DateTime<Utc>>,
    ) -> Result<()> {
        EmbeddedCore::ingest_with_valid_time(self, event, valid_from, valid_to).await
    }

    async fn query(&self, query: Query) -> Result<Vec<EventView>> {
        EmbeddedCore::query(self, query).await
    }

    async fn shutdown(&self) -> Result<()> {
        EmbeddedCore::shutdown(self).await
    }
}
