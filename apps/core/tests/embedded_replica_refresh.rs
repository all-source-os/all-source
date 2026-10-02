//! A read-only replica catches up with its writer through `refresh`, without
//! reopening the data dir and without counting an event twice when the writer
//! moves it from the WAL into Parquet.
//!
//! Run: cargo test --features embedded --test embedded_replica_refresh

#[cfg(feature = "embedded")]
mod tests {
    use allsource_core::embedded::{Config, EmbeddedCore, IngestEvent, Query};
    use serde_json::json;
    use tempfile::TempDir;

    async fn ingest(core: &EmbeddedCore, n: usize) {
        for i in 0..n {
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
    }

    #[tokio::test]
    async fn a_replica_sees_new_writes_after_refresh_and_never_double_counts() {
        let tmp = TempDir::new().unwrap();
        let data_dir = tmp.path();

        let writer = EmbeddedCore::open(Config::builder().data_dir(data_dir).build().unwrap())
            .await
            .expect("open writer");
        ingest(&writer, 3).await;

        let reader = EmbeddedCore::open(
            Config::builder()
                .data_dir(data_dir)
                .read_only(true)
                .build()
                .unwrap(),
        )
        .await
        .expect("open replica");
        assert_eq!(reader.stats().total_events, 3);

        let idle = reader.refresh().await.expect("refresh");
        assert_eq!(idle.new_events, 0);
        assert!(
            !idle.wal_changed,
            "nothing changed since the replica opened"
        );

        ingest(&writer, 2).await;
        assert_eq!(
            reader.stats().total_events,
            3,
            "a replica does not see writes before it refreshes"
        );

        let caught_up = reader.refresh().await.expect("refresh");
        assert_eq!(caught_up.new_events, 2);
        assert!(caught_up.wal_changed);
        assert!(caught_up.newest_event_at.is_some());
        assert_eq!(reader.stats().total_events, 5);

        writer.inner().checkpoint().expect("checkpoint to Parquet");
        let after_checkpoint = reader.refresh().await.expect("refresh");
        assert!(
            after_checkpoint.new_parquet_files >= 1,
            "the checkpoint's Parquet file is read: {after_checkpoint:?}"
        );
        assert_eq!(
            after_checkpoint.new_events, 0,
            "events moved from the WAL into Parquet are already in memory"
        );
        assert_eq!(reader.stats().total_events, 5);

        ingest(&writer, 1).await;
        let after_rotation = reader.refresh().await.expect("refresh");
        assert_eq!(after_rotation.new_events, 1);

        let events = reader
            .query(Query::new().entity_id("e-1"))
            .await
            .expect("query");
        assert_eq!(events.len(), 6);
        let distinct: std::collections::HashSet<_> = events.iter().map(|e| e.id).collect();
        assert_eq!(distinct.len(), 6, "no event is held twice");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_refresh_that_fails_on_the_wal_reads_its_new_parquet_again_next_time() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().unwrap();
        let data_dir = tmp.path();
        let writer = EmbeddedCore::open(Config::builder().data_dir(data_dir).build().unwrap())
            .await
            .expect("open writer");
        ingest(&writer, 3).await;
        let reader = EmbeddedCore::open(
            Config::builder()
                .data_dir(data_dir)
                .read_only(true)
                .build()
                .unwrap(),
        )
        .await
        .expect("open replica");

        ingest(&writer, 2).await;
        writer.inner().checkpoint().expect("checkpoint to Parquet");

        let wal_dir = data_dir.join("wal");
        let mode = std::fs::metadata(&wal_dir).unwrap().permissions();
        std::fs::set_permissions(&wal_dir, std::fs::Permissions::from_mode(0o000)).unwrap();
        let failed = reader.refresh().await;
        std::fs::set_permissions(&wal_dir, mode).unwrap();
        assert!(failed.is_err(), "an unreadable WAL fails the refresh");

        reader.refresh().await.expect("refresh");
        assert_eq!(
            reader.stats().total_events,
            5,
            "the checkpoint's events are not lost to the failed refresh"
        );
    }

    #[tokio::test]
    async fn refresh_on_a_writable_store_is_a_no_op() {
        let tmp = TempDir::new().unwrap();
        let writer = EmbeddedCore::open(Config::builder().data_dir(tmp.path()).build().unwrap())
            .await
            .expect("open writer");
        ingest(&writer, 2).await;

        let report = writer.refresh().await.expect("refresh");
        assert_eq!(report, allsource_core::embedded::RefreshReport::default());
        assert_eq!(writer.stats().total_events, 2);
    }
}
