use super::*;
use crate::store::EventStoreConfig;
use std::{sync::atomic::Ordering, time::Duration};

#[tokio::test(flavor = "current_thread")]
async fn snapshot_lease_blocks_eviction_and_observes_cancellation_before_return() {
    for cancel in [false, true] {
        let directory = tempfile::TempDir::new().unwrap();
        let store = Arc::new(EventStore::with_config(EventStoreConfig::with_persistence(
            directory.path(),
        )));
        let event = Event::from_strings(
            "synthetic.updated".into(),
            "run".into(),
            "synthetic".into(),
            serde_json::json!({}),
            None,
        )
        .unwrap();
        store.ingest_with_expected_version(&event, Some(0)).unwrap();
        store.flush_storage().unwrap();
        // This test is about the residency lease, so the cache must be
        // warm before the reader starts.
        store
            .ensure_tenant_loaded_with_integrity("synthetic", true)
            .unwrap();
        let cancellation = Arc::new(AtomicBool::new(false));
        // Hold only the materialization lock: the reader can first acquire
        // its residency lease, then wait here with its original deadline.
        let lock_store = Arc::clone(&store);
        let (held_tx, held_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let _events = lock_store.events.write();
            held_tx.send(()).unwrap();
            // test-hang-allow: bounded synthetic contention, including test failures.
            release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        });
        held_rx.await.unwrap();
        let read_store = Arc::clone(&store);
        let flag = Arc::clone(&cancellation);
        let reader = tokio::task::spawn_blocking(move || {
            read_store.query_retained_entity_cancellable(
                "synthetic",
                "run",
                1001,
                &ReadScope::unrestricted(),
                Some(&flag),
            )
        });
        // test-hang-allow: bounded observation of the real reader pinning cache residency.
        tokio::time::timeout(Duration::from_secs(1), async {
            while store.cache_residency_gate.try_write().is_some() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let evict_store = Arc::clone(&store);
        let (eviction_tx, eviction_rx) = tokio::sync::oneshot::channel();
        let eviction = tokio::task::spawn_blocking(move || {
            eviction_tx.send(()).unwrap();
            evict_store.evict_tenant("synthetic");
        });
        eviction_rx.await.unwrap();
        if cancel {
            cancellation.store(true, Ordering::Release);
        }
        assert!(!eviction.is_finished());
        release_tx.send(()).unwrap();
        let result = reader.await.unwrap();
        eviction.await.unwrap();
        holder.join().unwrap();
        if cancel {
            let error = result.unwrap_err().to_string();
            assert!(error.contains("cancelled"), "{error}");
        } else {
            let (snapshot, total) = result.unwrap();
            assert_eq!(total, 1);
            assert_eq!(snapshot[0].id, event.id);
        }
        assert_eq!(store.total_events(), 0);
    }
}
