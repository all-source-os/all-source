//! VectorIndex projection — maintains an HNSW index over stored embeddings.
//!
//! Uses `instant-distance` for pure-Rust HNSW (ADR-015). Since the crate is
//! batch-only (no incremental insert), vectors are accumulated in a `DashMap`
//! and the HNSW index is rebuilt after mutations by one background builder at
//! a time. No search waits for a build: a search uses the last build and
//! brute-forces whatever was written since, or brute-forces everything when no
//! build exists yet.
//!
//! ## Key design decisions (ADR-015)
//!
//! - **Generation counter** instead of boolean dirty flag — avoids TOCTOU race
//!   where concurrent mutations during rebuild cause missed vectors.
//! - **No separate `deleted` DashMap** — deletions remove directly from `vectors`,
//!   eliminating snapshot/restore asymmetry and `len()` underflow bugs.

use dashmap::DashMap;
use instant_distance::{Builder, HnswMap, Point, Search};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    mem::size_of,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};

use crate::{
    application::services::projection::Projection, domain::entities::Event, error::Result,
    infrastructure::memory::ledger::Tally, prime::projections::charge,
};

use super::types::event_types as vec_events;

/// A point in the HNSW index — wraps a `Vec<f32>`.
#[derive(Clone, Debug)]
struct VecPoint(Vec<f32>);

impl Point for VecPoint {
    fn distance(&self, other: &Self) -> f32 {
        cosine_distance(&self.0, &other.0)
    }
}

/// Cosine distance = 1.0 - cosine_similarity
fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    let denom = norm_a * norm_b;
    if denom < f32::EPSILON {
        1.0
    } else {
        1.0 - (dot / denom)
    }
}

/// Serializable entry for snapshot/restore.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VectorRecord {
    entity_id: String,
    vector: Vec<f32>,
    text: Option<String>,
    metadata: Option<Value>,
    /// Generation that made this record current; above `built_generation` means
    /// the last build does not reflect it.
    #[serde(skip)]
    written_at: u64,
}

/// Configuration for the HNSW index.
#[derive(Debug, Clone)]
pub struct VectorIndexConfig {
    pub ef_construction: usize,
    pub ef_search: usize,
}

impl Default for VectorIndexConfig {
    fn default() -> Self {
        Self {
            ef_construction: 100,
            ef_search: 100,
        }
    }
}

/// Result from a vector search.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub entity_id: String,
    pub distance: f32,
    pub text: Option<String>,
    pub metadata: Option<Value>,
}

/// HNSW-indexed vector projection with generation-based staleness tracking.
///
/// Vectors are stored in a `DashMap` as events arrive. A search that finds the
/// generation advanced starts a background rebuild of the HNSW index.
pub struct VectorIndexProjection {
    name: String,
    /// entity_id -> VectorRecord (live vectors only — deletions remove entries)
    vectors: Arc<DashMap<String, VectorRecord>>,
    /// The built HNSW index (None when empty or invalidated)
    index: Arc<RwLock<Option<HnswMap<VecPoint, String>>>>,
    /// Mutation generation counter — incremented on each insert/delete.
    generation: Arc<AtomicU64>,
    /// Generation at which the index was last built.
    built_generation: Arc<AtomicU64>,
    /// Serializes writers, so two cannot stamp the same `written_at` and a
    /// build that misses a record always sees its generation as newer.
    write_lock: Arc<Mutex<()>>,
    /// A background rebuild is queued or running; at most one runs.
    rebuilding: Arc<AtomicBool>,
    /// Completed builds.
    builds: Arc<AtomicU64>,
    config: VectorIndexConfig,
    /// Shared with the background handle, so one projection holds one charge.
    charge: Arc<Tally>,
}

fn record_bytes(record: &VectorRecord) -> u64 {
    charge::text(&record.entity_id)
        + (record.vector.len() * size_of::<f32>()) as u64
        + charge::opt_text(record.text.as_deref())
        + record.metadata.as_ref().map_or(0, charge::json)
}

/// Clears `rebuilding` even if the build panics, so later rebuilds still start.
struct RebuildingFlag(Arc<AtomicBool>);

impl Drop for RebuildingFlag {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl VectorIndexProjection {
    pub fn new(name: impl Into<String>) -> Self {
        Self::with_config(name, VectorIndexConfig::default())
    }

    pub fn with_config(name: impl Into<String>, config: VectorIndexConfig) -> Self {
        Self {
            name: name.into(),
            vectors: Arc::new(DashMap::new()),
            index: Arc::new(RwLock::new(None)),
            generation: Arc::new(AtomicU64::new(0)),
            built_generation: Arc::new(AtomicU64::new(0)),
            write_lock: Arc::new(Mutex::new(())),
            rebuilding: Arc::new(AtomicBool::new(false)),
            builds: Arc::new(AtomicU64::new(0)),
            config,
            charge: Arc::new(charge::tally()),
        }
    }

    fn store(&self, record: VectorRecord) {
        let owned = record_bytes(&record);
        match self.vectors.insert(record.entity_id.clone(), record) {
            Some(old) => self.charge.adjust(record_bytes(&old), owned),
            None => self
                .charge
                .add(charge::entry::<String, VectorRecord>(owned)),
        }
    }

    /// Number of live vectors.
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Whether there are no live vectors.
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    /// Dimension of the vectors held by this index, if any.
    ///
    /// All live vectors in a given data-dir share one dimension (the embedding
    /// model's output size). Returns `None` when the index is empty. Used to
    /// reject a mismatched embedder before it corrupts similarity search — an
    /// HNSW built over mixed-dimension points produces meaningless distances.
    pub fn dimension(&self) -> Option<usize> {
        self.vectors.iter().next().map(|e| e.value().vector.len())
    }

    /// Search for the `top_k` nearest neighbors to `query`.
    pub fn search(&self, query: &[f32], top_k: usize) -> Vec<SearchHit> {
        self.ensure_index();

        // Read before the index, which a build publishes first: a record newer
        // than this counter is then always scanned, even if the index has it.
        let built_gen = self.built_generation.load(Ordering::Acquire);

        let index = self.index.read();
        let Some(hnsw) = index.as_ref() else {
            drop(index);
            return self.scan(query, top_k, |_| true);
        };
        let query_point = VecPoint(query.to_vec());
        let mut search = Search::default();
        let mut hits: Vec<SearchHit> = hnsw
            .search(&query_point, &mut search)
            .filter_map(|item| {
                let record = self.vectors.get(item.value)?;
                (record.written_at <= built_gen).then(|| SearchHit {
                    entity_id: item.value.clone(),
                    distance: item.distance,
                    text: record.text.clone(),
                    metadata: record.metadata.clone(),
                })
            })
            .take(top_k)
            .collect();
        drop(index);

        if self.generation.load(Ordering::Acquire) != built_gen {
            let newer = self.scan(query, top_k, |record| record.written_at > built_gen);
            // A record rewritten after the index pass appears in both; the
            // scan holds the current version.
            hits.retain(|hit| !newer.iter().any(|n| n.entity_id == hit.entity_id));
            hits.extend(newer);
            Self::keep_nearest(&mut hits, top_k);
        }
        hits
    }

    /// Exact search over vectors whose entity id starts with `entity_prefix`.
    ///
    /// Never touches the HNSW, so a small typed subset neither waits for a
    /// rebuild nor loses its top-k to vectors of other types.
    pub fn search_prefix(
        &self,
        query: &[f32],
        top_k: usize,
        entity_prefix: &str,
    ) -> Vec<SearchHit> {
        self.scan(query, top_k, |record| {
            record.entity_id.starts_with(entity_prefix)
        })
    }

    /// Exact `top_k` over the records `keep` accepts, nearest first. Text and
    /// metadata are cloned for the winners only.
    fn scan(
        &self,
        query: &[f32],
        top_k: usize,
        keep: impl Fn(&VectorRecord) -> bool,
    ) -> Vec<SearchHit> {
        let mut ranked: Vec<(f32, String)> = self
            .vectors
            .iter()
            .filter(|entry| keep(entry.value()))
            .map(|entry| {
                (
                    cosine_distance(query, &entry.value().vector),
                    entry.key().clone(),
                )
            })
            .collect();
        if ranked.len() > top_k {
            ranked.select_nth_unstable_by(top_k, |a, b| a.0.total_cmp(&b.0));
            ranked.truncate(top_k);
        }

        // A winner deleted or rewritten since it was ranked is dropped or
        // re-scored, never served with stale contents.
        let mut hits: Vec<SearchHit> = ranked
            .into_iter()
            .filter_map(|(_, entity_id)| {
                let record = self.vectors.get(&entity_id)?;
                Some(SearchHit {
                    distance: cosine_distance(query, &record.vector),
                    text: record.text.clone(),
                    metadata: record.metadata.clone(),
                    entity_id,
                })
            })
            .collect();
        Self::keep_nearest(&mut hits, top_k);
        hits
    }

    fn keep_nearest(hits: &mut Vec<SearchHit>, top_k: usize) {
        hits.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        hits.truncate(top_k);
    }

    /// Start a background rebuild when the index is behind.
    fn ensure_index(&self) {
        if !self.is_current() {
            self.spawn_rebuild();
        }
    }

    fn is_current(&self) -> bool {
        self.generation.load(Ordering::Acquire) == self.built_generation.load(Ordering::Acquire)
    }

    fn spawn_rebuild(&self) {
        if self.rebuilding.swap(true, Ordering::AcqRel) {
            return;
        }
        let flag = RebuildingFlag(Arc::clone(&self.rebuilding));
        let worker = self.background_handle();
        let spawned = std::thread::Builder::new()
            .name("prime-vector-rebuild".to_string())
            .spawn(move || {
                let _flag = flag;
                worker.rebuild();
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, index = %self.name, "could not start vector index rebuild");
        }
    }

    fn background_handle(&self) -> Self {
        Self {
            name: self.name.clone(),
            vectors: Arc::clone(&self.vectors),
            index: Arc::clone(&self.index),
            generation: Arc::clone(&self.generation),
            built_generation: Arc::clone(&self.built_generation),
            write_lock: Arc::clone(&self.write_lock),
            rebuilding: Arc::clone(&self.rebuilding),
            builds: Arc::clone(&self.builds),
            config: self.config.clone(),
            charge: Arc::clone(&self.charge),
        }
    }

    /// Caller holds the `rebuilding` flag.
    fn rebuild(&self) {
        // Read before collecting: a write landing mid-collect leaves
        // `built_generation` behind `generation`, so it is picked up again.
        let target = self.generation.load(Ordering::Acquire);

        let mut points = Vec::new();
        let mut values = Vec::new();
        for entry in self.vectors.iter() {
            points.push(VecPoint(entry.value().vector.clone()));
            values.push(entry.key().clone());
        }
        let vector_count = points.len();

        let started = Instant::now();
        if points.is_empty() {
            *self.index.write() = None;
        } else {
            let hnsw = Builder::default()
                .ef_construction(self.config.ef_construction)
                .build(points, values);
            *self.index.write() = Some(hnsw);
        }
        self.built_generation.store(target, Ordering::Release);
        self.builds.fetch_add(1, Ordering::Relaxed);

        tracing::info!(
            index = %self.name,
            vector_count,
            build_ms = started.elapsed().as_millis() as u64,
            "vector index built"
        );
    }
}

impl Projection for VectorIndexProjection {
    fn name(&self) -> &str {
        &self.name
    }

    fn process(&self, event: &Event) -> Result<()> {
        let event_type = event.event_type_str();
        let entity_id = event.entity_id_str().to_string();
        let payload = &event.payload;

        match event_type {
            vec_events::VECTOR_STORED => {
                let vector: Vec<f32> = event
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("embedding"))
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();

                let text = payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let metadata = payload.get("metadata").cloned();
                let _write = self.write_lock.lock();
                let written_at = self.generation.load(Ordering::Acquire) + 1;

                self.store(VectorRecord {
                    entity_id,
                    vector,
                    text,
                    metadata,
                    written_at,
                });
                self.generation.fetch_add(1, Ordering::Release);
            }
            vec_events::VECTOR_DELETED => {
                let _write = self.write_lock.lock();
                if let Some((_, record)) = self.vectors.remove(&entity_id) {
                    self.charge
                        .sub(charge::entry::<String, VectorRecord>(record_bytes(&record)));
                }
                self.generation.fetch_add(1, Ordering::Release);
            }
            _ => {}
        }

        Ok(())
    }

    fn get_state(&self, entity_id: &str) -> Option<Value> {
        self.vectors
            .get(entity_id)
            .map(|r| serde_json::to_value(r.value()).unwrap_or(Value::Null))
    }

    fn clear(&self) {
        let _write = self.write_lock.lock();
        self.vectors.clear();
        self.charge.take();
        *self.index.write() = None;
        self.generation.fetch_add(1, Ordering::Release);
    }

    fn snapshot(&self) -> Option<Value> {
        let records: Vec<VectorRecord> = self.vectors.iter().map(|e| e.value().clone()).collect();
        serde_json::to_value(records).ok()
    }

    fn restore(&self, snapshot: &Value) -> Result<()> {
        let records: Vec<VectorRecord> = serde_json::from_value(snapshot.clone())
            .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;
        let _write = self.write_lock.lock();
        self.vectors.clear();
        self.charge.take();
        // Newer than any build, so one already in flight cannot hide them.
        let written_at = self.generation.load(Ordering::Acquire) + 1;
        for mut record in records {
            record.written_at = written_at;
            self.store(record);
        }
        *self.index.write() = None;
        self.generation.fetch_add(1, Ordering::Release);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_vector_event(entity_id: &str, vector: &[f32], text: Option<&str>) -> Event {
        Event::reconstruct_from_strings(
            Uuid::new_v4(),
            vec_events::VECTOR_STORED.to_string(),
            entity_id.to_string(),
            "default".to_string(),
            serde_json::json!({
                "text": text,
                "dimensions": vector.len(),
            }),
            Utc::now(),
            Some(serde_json::json!({ "embedding": vector })),
            1,
        )
    }

    fn make_delete_event(entity_id: &str) -> Event {
        Event::reconstruct_from_strings(
            Uuid::new_v4(),
            vec_events::VECTOR_DELETED.to_string(),
            entity_id.to_string(),
            "default".to_string(),
            serde_json::json!({}),
            Utc::now(),
            None,
            1,
        )
    }

    #[test]
    fn test_insert_100_vectors() {
        let proj = VectorIndexProjection::new("vec_idx");

        for i in 0..100 {
            let vector: Vec<f32> = (0..8).map(|j| (i * 8 + j) as f32).collect();
            let event =
                make_vector_event(&format!("vec:doc-{i}"), &vector, Some(&format!("doc {i}")));
            proj.process(&event).unwrap();
        }

        assert_eq!(proj.len(), 100);
    }

    #[test]
    fn test_dimension_reports_stored_vector_size() {
        let proj = VectorIndexProjection::new("vec_idx");
        assert_eq!(proj.dimension(), None, "empty index has no dimension");

        proj.process(&make_vector_event(
            "vec:a",
            &[1.0, 2.0, 3.0, 4.0],
            Some("x"),
        ))
        .unwrap();
        assert_eq!(proj.dimension(), Some(4));
    }

    #[test]
    fn test_search_returns_results() {
        let proj = VectorIndexProjection::new("vec_idx");

        proj.process(&make_vector_event(
            "vec:a",
            &[1.0, 0.0, 0.0, 0.0],
            Some("close"),
        ))
        .unwrap();
        proj.process(&make_vector_event(
            "vec:b",
            &[0.7, 0.7, 0.0, 0.0],
            Some("medium"),
        ))
        .unwrap();
        proj.process(&make_vector_event(
            "vec:c",
            &[0.0, 0.0, 0.0, 1.0],
            Some("far"),
        ))
        .unwrap();

        let hits = proj.search(&[1.0, 0.0, 0.0, 0.0], 3);

        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].entity_id, "vec:a");
        assert!(hits[0].distance < 0.01);
        assert_eq!(hits[2].entity_id, "vec:c");
    }

    #[test]
    fn test_delete_excludes_from_search() {
        let proj = VectorIndexProjection::new("vec_idx");

        proj.process(&make_vector_event("vec:a", &[1.0, 0.0], Some("a")))
            .unwrap();
        proj.process(&make_vector_event("vec:b", &[0.9, 0.1], Some("b")))
            .unwrap();

        proj.process(&make_delete_event("vec:a")).unwrap();

        let hits = proj.search(&[1.0, 0.0], 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].entity_id, "vec:b");
    }

    #[test]
    fn test_snapshot_restore_roundtrip() {
        let proj = VectorIndexProjection::new("vec_idx");

        proj.process(&make_vector_event("vec:x", &[1.0, 0.0, 0.0], Some("x")))
            .unwrap();
        proj.process(&make_vector_event("vec:y", &[0.0, 1.0, 0.0], Some("y")))
            .unwrap();

        let snap = proj.snapshot().unwrap();
        proj.clear();
        assert_eq!(proj.len(), 0);

        proj.restore(&snap).unwrap();
        assert_eq!(proj.len(), 2);

        let hits = proj.search(&[1.0, 0.0, 0.0], 2);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].entity_id, "vec:x");
    }

    #[test]
    fn test_snapshot_restore_after_delete_is_clean() {
        // Regression: old code had deleted vectors reappearing after restore
        let proj = VectorIndexProjection::new("vec_idx");

        proj.process(&make_vector_event("vec:a", &[1.0, 0.0], Some("a")))
            .unwrap();
        proj.process(&make_vector_event("vec:b", &[0.0, 1.0], Some("b")))
            .unwrap();
        proj.process(&make_delete_event("vec:a")).unwrap();

        let snap = proj.snapshot().unwrap();
        proj.clear();
        proj.restore(&snap).unwrap();

        assert_eq!(proj.len(), 1);
        assert!(proj.get_state("vec:a").is_none());
        assert!(proj.get_state("vec:b").is_some());
    }

    #[test]
    fn test_search_empty_index() {
        let proj = VectorIndexProjection::new("vec_idx");
        let hits = proj.search(&[1.0, 0.0], 10);
        assert!(hits.is_empty());
    }

    #[test]
    fn test_delete_nonexistent_is_noop() {
        let proj = VectorIndexProjection::new("vec_idx");
        proj.process(&make_delete_event("vec:ghost")).unwrap();
        assert_eq!(proj.len(), 0);
    }

    fn wait_until_current(proj: &VectorIndexProjection) {
        for _ in 0..500 {
            if proj.is_current() && !proj.rebuilding.load(Ordering::Acquire) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("vector index rebuild did not finish");
    }

    #[test]
    fn a_write_after_a_build_is_found_by_the_very_next_search() {
        let proj = VectorIndexProjection::new("vec_idx");
        proj.process(&make_vector_event("vec:old", &[0.0, 1.0], Some("old")))
            .unwrap();
        proj.search(&[1.0, 0.0], 1);

        proj.process(&make_vector_event("vec:new", &[1.0, 0.0], Some("new")))
            .unwrap();

        let hits = proj.search(&[1.0, 0.0], 1);
        assert_eq!(hits[0].entity_id, "vec:new");
    }

    #[test]
    fn a_rewritten_vector_is_returned_once_at_its_new_position() {
        let proj = VectorIndexProjection::new("vec_idx");
        proj.process(&make_vector_event("vec:a", &[0.0, 1.0], Some("before")))
            .unwrap();
        proj.search(&[1.0, 0.0], 1);

        proj.process(&make_vector_event("vec:a", &[1.0, 0.0], Some("after")))
            .unwrap();

        let hits = proj.search(&[1.0, 0.0], 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].text.as_deref(), Some("after"));
        assert!(hits[0].distance < 0.01);
    }

    #[test]
    fn a_delete_after_a_build_is_not_served_from_the_old_build() {
        let proj = VectorIndexProjection::new("vec_idx");
        proj.process(&make_vector_event("vec:a", &[1.0, 0.0], Some("a")))
            .unwrap();
        proj.process(&make_vector_event("vec:b", &[0.0, 1.0], Some("b")))
            .unwrap();
        proj.search(&[1.0, 0.0], 2);

        proj.process(&make_delete_event("vec:a")).unwrap();

        let hits = proj.search(&[1.0, 0.0], 2);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].entity_id, "vec:b");
    }

    #[test]
    fn concurrent_searches_share_one_build() {
        let proj = Arc::new(VectorIndexProjection::new("vec_idx"));
        for i in 0..200 {
            let vector = vec![i as f32, 1.0, (200 - i) as f32];
            proj.process(&make_vector_event(&format!("vec:{i}"), &vector, None))
                .unwrap();
        }

        let search_from_many_threads = |proj: &Arc<VectorIndexProjection>| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let proj = Arc::clone(proj);
                    std::thread::spawn(move || proj.search(&[1.0, 1.0, 1.0], 5))
                })
                .collect();
            for handle in handles {
                assert_eq!(handle.join().unwrap().len(), 5);
            }
        };

        search_from_many_threads(&proj);
        wait_until_current(&proj);
        assert_eq!(proj.builds.load(Ordering::Relaxed), 1);

        proj.process(&make_vector_event("vec:late", &[1.0, 1.0, 1.0], None))
            .unwrap();
        search_from_many_threads(&proj);
        wait_until_current(&proj);
        assert_eq!(proj.builds.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn a_search_before_any_build_is_exact_and_leaves_the_build_to_the_background() {
        let proj = VectorIndexProjection::new("vec_idx");
        for i in 0..300 {
            let angle = i as f32 / 300.0;
            proj.process(&make_vector_event(
                &format!("vec:{i}"),
                &[angle.cos(), angle.sin()],
                None,
            ))
            .unwrap();
        }
        assert!(proj.index.read().is_none());

        let hits = proj.search(&[1.0, 0.0], 3);

        let ids: Vec<_> = hits.iter().map(|hit| hit.entity_id.as_str()).collect();
        assert_eq!(ids, ["vec:0", "vec:1", "vec:2"]);
        wait_until_current(&proj);
        assert_eq!(proj.builds.load(Ordering::Relaxed), 1);
        assert!(proj.index.read().is_some());
    }

    #[test]
    fn restored_vectors_are_found_while_an_older_build_is_still_published() {
        let proj = VectorIndexProjection::new("vec_idx");
        proj.process(&make_vector_event("vec:old", &[0.0, 1.0], None))
            .unwrap();
        let before_restore = proj.generation.load(Ordering::Acquire);

        let snap = serde_json::json!([
            { "entity_id": "vec:restored", "vector": [1.0, 0.0], "text": null, "metadata": null }
        ]);
        proj.restore(&snap).unwrap();
        // What a background build that collected before the restore publishes.
        let stale =
            Builder::default().build(vec![VecPoint(vec![0.0, 1.0])], vec!["vec:old".to_string()]);
        *proj.index.write() = Some(stale);
        proj.built_generation
            .store(before_restore, Ordering::Release);
        // Holding the flag keeps a real rebuild from replacing the stale build.
        proj.rebuilding.store(true, Ordering::Release);

        let hits = proj.search(&[1.0, 0.0], 1);
        assert_eq!(hits[0].entity_id, "vec:restored");
        assert_eq!(proj.builds.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn prefix_search_finds_a_small_subset_behind_closer_vectors_without_a_build() {
        let proj = VectorIndexProjection::new("vec_idx");
        for i in 0..50 {
            proj.process(&make_vector_event(
                &format!("vec:output:{i}"),
                &[1.0, 0.0],
                None,
            ))
            .unwrap();
        }
        proj.process(&make_vector_event("vec:tool:far", &[0.0, 1.0], None))
            .unwrap();
        proj.process(&make_vector_event("vec:tool:near", &[0.5, 0.5], None))
            .unwrap();

        let hits = proj.search_prefix(&[1.0, 0.0], 5, "vec:tool:");

        let ids: Vec<_> = hits.iter().map(|hit| hit.entity_id.as_str()).collect();
        assert_eq!(ids, ["vec:tool:near", "vec:tool:far"]);
        assert_eq!(proj.builds.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_zero_vector_search() {
        let proj = VectorIndexProjection::new("vec_idx");
        proj.process(&make_vector_event("vec:a", &[1.0, 0.0], Some("a")))
            .unwrap();
        proj.process(&make_vector_event("vec:zero", &[0.0, 0.0], Some("zero")))
            .unwrap();

        // Searching with zero vector should return results (distance = 1.0 for all)
        let hits = proj.search(&[0.0, 0.0], 10);
        assert_eq!(hits.len(), 2);
    }
}
