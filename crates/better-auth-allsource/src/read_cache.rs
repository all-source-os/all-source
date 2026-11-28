//! Conditional reads and read attribution for [`crate::client::AllsourceClient`].
//!
//! Every read remembers the `ETag` Core returned and revalidates with
//! `If-None-Match`, so an unchanged answer costs a `304` with no body. With a
//! TTL set, a fresh entry is served without a request at all.

use crate::client::StoredEvent;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

/// Distinct reads remembered. Past it the stalest entry is dropped.
const ENTRIES: usize = 4096;

/// Larger answers are not kept: they are scans, not lookups, and would make
/// the cache's memory proportional to a tenant's history.
const MAX_EVENTS: usize = 64;

pub(crate) const DEFAULT_CALLER_PREFIX: &str = "better_auth";

pub(crate) struct CachedRead {
    pub etag: String,
    pub events: Arc<Vec<StoredEvent>>,
    fetched: Instant,
}

impl CachedRead {
    pub fn fresh(&self, ttl: Option<Duration>) -> bool {
        ttl.is_some_and(|ttl| self.fetched.elapsed() < ttl)
    }
}

#[derive(Default)]
pub(crate) struct ReadCache {
    entries: HashMap<String, Arc<CachedRead>>,
}

impl ReadCache {
    pub fn get(&self, key: &str) -> Option<Arc<CachedRead>> {
        self.entries.get(key).cloned()
    }

    /// Remember `events` under `key` as fetched now.
    pub fn put(&mut self, key: String, etag: String, events: Arc<Vec<StoredEvent>>) {
        if events.len() > MAX_EVENTS {
            self.entries.remove(&key);
            return;
        }
        if self.entries.len() >= ENTRIES && !self.entries.contains_key(&key) {
            let stalest = self
                .entries
                .iter()
                .min_by_key(|(_, read)| read.fetched)
                .map(|(key, _)| key.clone());
            if let Some(stalest) = stalest {
                self.entries.remove(&stalest);
            }
        }
        self.entries.insert(
            key,
            Arc::new(CachedRead {
                etag,
                events,
                fetched: Instant::now(),
            }),
        );
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// The `x-allsource-caller` value for a read of `stream`, an entity id
/// (`auth-member:…`) or an event-type prefix (`auth.member.`):
/// `<prefix>.member`. The gateway accepts `^[a-z][a-z0-9_.]{0,63}$` and counts
/// anything else as `invalid`.
pub(crate) fn caller_label(prefix: &str, stream: &str) -> String {
    let rest = stream
        .strip_prefix("auth-")
        .or_else(|| stream.strip_prefix("auth."))
        .unwrap_or(stream);
    let kind: String = rest
        .split([':', '.'])
        .next()
        .unwrap_or("")
        .chars()
        .map(|c| match c.to_ascii_lowercase() {
            c @ ('a'..='z' | '0'..='9' | '_') => c,
            _ => '_',
        })
        .collect();
    let mut label = if kind.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}.{kind}")
    };
    label.truncate(64);
    label
}
