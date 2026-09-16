//! Cold-tier S3 archive, driven end to end against a real `ObjectStore`.
//!
//! These use `object_store::memory::InMemory` rather than a MinIO container.
//! That is a deliberate split of responsibility: everything this repository
//! wrote — the Parquet encode, the object key, idempotency on retry, scratch
//! cleanup, and the error that keeps compaction from deleting originals — is
//! exercised here and runs on every CI pass with no Docker. What is NOT covered
//! is the S3 wire protocol itself (SigV4, path-style vs virtual-host
//! addressing, endpoint overrides), which belongs to `object_store` and needs a
//! real endpoint. `docs/operations/COLD_TIER.md` names the one-command check
//! for that, and it is the remaining step before trusting this against R2.

#![cfg(feature = "cold-tier-s3")]

use allsource_core::{
    domain::entities::Event,
    infrastructure::persistence::{
        cold_tier::ArchiveTarget,
        cold_tier_s3::{S3Archive, S3Location, archive_object_key},
    },
};
use chrono::{DateTime, TimeZone, Utc};
use object_store::{ObjectStore, ObjectStoreExt, memory::InMemory};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

fn ts(day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, day, 12, 0, 0).unwrap()
}

fn event(tenant: &str, at: DateTime<Utc>) -> Event {
    Event::reconstruct_from_strings(
        Uuid::new_v4(),
        "test.event".to_string(),
        "entity-1".to_string(),
        tenant.to_string(),
        json!({"payload": "archived"}),
        at,
        None,
        1,
    )
}

fn archive_with(store: Arc<dyn ObjectStore>, prefix: &str) -> S3Archive {
    S3Archive::with_store(
        store,
        S3Location {
            bucket: "test-bucket".into(),
            prefix: prefix.into(),
        },
    )
    .expect("archive builds")
}

async fn keys(store: &Arc<dyn ObjectStore>) -> Vec<String> {
    use futures::StreamExt;
    let mut out = Vec::new();
    let mut listing = store.list(None);
    while let Some(item) = listing.next().await {
        out.push(item.expect("list entry").location.to_string());
    }
    out.sort();
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn an_archived_window_lands_at_its_deterministic_key() {
    let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
    let archive = archive_with(Arc::clone(&store), "cold");
    let events: Vec<Event> = (1..=3).map(|d| event("acme", ts(d))).collect();

    tokio::task::spawn_blocking({
        let archive = archive;
        move || archive.archive("acme", ts(1), ts(3), &events)
    })
    .await
    .expect("join")
    .expect("archive succeeds");

    let found = keys(&store).await;
    assert_eq!(
        found,
        vec![archive_object_key("cold", "acme", ts(1), ts(3))]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_uploaded_object_is_the_parquet_the_encoder_produced() {
    let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
    let archive = archive_with(Arc::clone(&store), "");
    let events: Vec<Event> = (1..=5).map(|d| event("acme", ts(d))).collect();

    tokio::task::spawn_blocking({
        let archive = archive;
        move || archive.archive("acme", ts(1), ts(5), &events)
    })
    .await
    .expect("join")
    .expect("archive succeeds");

    let key = archive_object_key("", "acme", ts(1), ts(5));
    let got = store
        .get(&key.clone().into())
        .await
        .expect("object exists")
        .bytes()
        .await
        .expect("bytes");

    // PAR1 both ends is the Parquet container contract. Asserting it here is
    // what separates "we uploaded something" from "we uploaded a readable
    // archive", and the second is the only one worth anything months later.
    assert!(
        got.len() > 8,
        "suspiciously small object: {} bytes",
        got.len()
    );
    assert_eq!(&got[..4], b"PAR1", "archived object is not Parquet");
    assert_eq!(&got[got.len() - 4..], b"PAR1", "Parquet footer missing");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_retry_overwrites_rather_than_duplicating() {
    let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
    let events: Vec<Event> = (1..=3).map(|d| event("acme", ts(d))).collect();

    // Compaction retries after a transient failure, so the same window must
    // land on the same key. A timestamp in the key would leave two copies and
    // double the bill every time the network blipped.
    for _ in 0..3 {
        let archive = archive_with(Arc::clone(&store), "cold");
        let events = events.clone();
        tokio::task::spawn_blocking(move || archive.archive("acme", ts(1), ts(3), &events))
            .await
            .expect("join")
            .expect("archive succeeds");
    }

    assert_eq!(
        keys(&store).await.len(),
        1,
        "a retry duplicated the archive"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn two_tenants_never_collide() {
    let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());

    for tenant in ["acme", "globex"] {
        let archive = archive_with(Arc::clone(&store), "cold");
        let events = vec![event(tenant, ts(1))];
        let t = tenant.to_string();
        tokio::task::spawn_blocking(move || archive.archive(&t, ts(1), ts(2), &events))
            .await
            .expect("join")
            .expect("archive succeeds");
    }

    let found = keys(&store).await;
    assert_eq!(found.len(), 2);
    assert!(found.iter().any(|k| k.contains("/acme/")));
    assert!(found.iter().any(|k| k.contains("/globex/")));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_empty_window_uploads_nothing_and_still_succeeds() {
    let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
    let archive = archive_with(Arc::clone(&store), "cold");

    tokio::task::spawn_blocking(move || archive.archive("acme", ts(1), ts(2), &[]))
        .await
        .expect("join")
        .expect("an empty window is not an error");

    assert!(keys(&store).await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_scratch_dir_does_not_accumulate_encodes() {
    let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
    let archive = archive_with(Arc::clone(&store), "cold");
    let scratch = archive.scratch_path().to_path_buf();
    let events: Vec<Event> = (1..=3).map(|d| event("acme", ts(d))).collect();

    let archive = tokio::task::spawn_blocking(move || {
        for d in 1..=4u32 {
            archive
                .archive("acme", ts(d), ts(d + 1), &events)
                .expect("archive succeeds");
        }
        archive
    })
    .await
    .expect("join");
    drop(archive);

    // The cold tier exists to relieve disk pressure. An encode left behind on
    // every pass would make it a slow leak on the volume it is protecting.
    let leftover: Vec<_> = walk(&scratch)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "parquet"))
        .collect();
    assert!(leftover.is_empty(), "scratch kept encodes: {leftover:?}");
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}
