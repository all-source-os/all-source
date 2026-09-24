//! Regression: embedded boot must hydrate Parquet so a store whose events are
//! durable in Parquet but whose WAL is empty/missing is NOT read as empty.
//!
//! Reproduces the chronis 0.7.1 data-loss-LOOKING regression: events were
//! checkpointed to Parquet, the WAL was empty on the next boot, and the store
//! read "No tasks found" because `EmbeddedCore::open` left Parquet cold. Nothing
//! was lost — the read path skipped the durable archive. `EmbeddedCore::open`
//! now calls `hydrate_all_from_storage()`; this test locks that in.
//!
//! Run: cargo test --features embedded --test embedded_cold_parquet_boot

#[cfg(feature = "embedded")]
mod tests {
    use allsource_core::embedded::{Config, EmbeddedCore, IngestEvent, Query};
    use serde_json::json;
    use tempfile::TempDir;

    #[tokio::test]
    async fn boot_hydrates_parquet_when_wal_is_empty() {
        let tmp = TempDir::new().unwrap();
        let data_dir = tmp.path();

        // 1. Write events and force them down to the Parquet archive.
        {
            let core = EmbeddedCore::open(Config::builder().data_dir(data_dir).build().unwrap())
                .await
                .expect("open");
            for i in 0..5 {
                core.ingest(IngestEvent {
                    entity_id: "e-1",
                    event_type: "thing.happened",
                    payload: json!({ "n": i }),
                    metadata: None,
                    tenant_id: None,
                })
                .await
                .expect("ingest");
            }
            core.inner().checkpoint().expect("checkpoint to Parquet");
            assert_eq!(core.stats().total_events, 5);
        }

        // 2. Simulate the regression: zero every WAL segment so the next boot has
        //    no WAL to recover from — Parquet is the only surviving source.
        let wal_dir = data_dir.join("wal");
        if wal_dir.is_dir() {
            for entry in std::fs::read_dir(&wal_dir).unwrap().flatten() {
                if entry.path().extension().is_some_and(|e| e == "log") {
                    std::fs::write(entry.path(), b"").unwrap(); // truncate to 0 bytes
                }
            }
        }

        // 3. Reopen. Before the fix this read 0 events ("No tasks found"); the
        //    boot must now hydrate the 5 durable events from Parquet.
        let core = EmbeddedCore::open(Config::builder().data_dir(data_dir).build().unwrap())
            .await
            .expect("reopen");
        assert_eq!(
            core.stats().total_events,
            5,
            "embedded boot must hydrate Parquet when the WAL is empty (chronis 0.7.1 regression)"
        );

        // Queryable, not merely counted.
        let events = core
            .query(Query::new().entity_id("e-1"))
            .await
            .expect("query");
        assert_eq!(
            events.len(),
            5,
            "all 5 events must be readable after reopen"
        );
    }

    /// Boot hydration reads the whole archive, so the tenants in it are warm.
    /// An embedding app that then calls `ensure_tenant_loaded` (Longhand does,
    /// for every store it opens) must not read the same Parquet a second time.
    #[tokio::test]
    async fn boot_hydration_leaves_the_tenant_marked_loaded() {
        let tmp = TempDir::new().unwrap();
        let data_dir = tmp.path();

        {
            let core = EmbeddedCore::open(Config::builder().data_dir(data_dir).build().unwrap())
                .await
                .expect("open");
            for i in 0..5 {
                core.ingest(IngestEvent {
                    entity_id: "e-1",
                    event_type: "thing.happened",
                    payload: json!({ "n": i }),
                    metadata: None,
                    tenant_id: None,
                })
                .await
                .expect("ingest");
            }
            core.inner().checkpoint().expect("checkpoint to Parquet");
        }

        let core = EmbeddedCore::open(Config::builder().data_dir(data_dir).build().unwrap())
            .await
            .expect("reopen");

        assert!(
            core.inner().is_tenant_loaded("default"),
            "a tenant boot already hydrated must not be cold"
        );
        core.inner()
            .ensure_tenant_loaded("default")
            .expect("ensure_tenant_loaded");
        assert_eq!(core.stats().total_events, 5);
    }
}
