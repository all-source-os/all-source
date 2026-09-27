//! Explicit production-shape capacity probe. Never uses customer data.
// Match the server allocator; a system-allocator test is not a runtime memory probe.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use allsource_core::{
    domain::entities::Event,
    infrastructure::{
        persistence::{ParquetStorage, ParquetStorageConfig},
        web::api,
    },
    store::{EventStore, EventStoreConfig, ReadScope},
};
use axum::{Router, routing::post};
use parquet::{
    arrow::{ArrowWriter, arrow_reader::ParquetRecordBatchReaderBuilder},
    file::properties::WriterProperties,
};
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit filesystem capacity probe: creates 16,100 synthetic Parquet files"]
async fn bounded_http_warmup_accepts_existing_archive_shape() {
    compatibility_probe(16_100, 90_064).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit dense archive regression t-40896f: creates 566,486 synthetic events"]
async fn bounded_http_warmup_accepts_existing_dense_archive_shape() {
    compatibility_probe(112, 566_486).await;
}

const TENANT: &str = "synthetic-cold-compatibility";
const FIXTURE_MARKER: &str = "synthetic-capacity-fixture.json";

#[test]
#[ignore = "operator-only fixture generation: retains a synthetic temp directory for isolated measurement"]
fn generate_dense_archive_capacity_fixture() {
    let events_count = fixture_rows();
    let directory = seed_archive(112, events_count);
    std::fs::write(
        directory.path().join(FIXTURE_MARKER),
        serde_json::to_vec(&fixture_marker(events_count)).unwrap(),
    )
    .unwrap();
    eprintln!("capacity fixture retained: {}", directory.keep().display());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "operator-only fresh-process measurement; needs an owned synthetic fixture copy"]
async fn proposed_row_policy_http_only_in_existing_synthetic_fixture() {
    let events_count = fixture_rows();
    let directory = PathBuf::from(
        std::env::var_os("ALLSOURCE_SYNTHETIC_CAPACITY_DIR")
            .expect("set ALLSOURCE_SYNTHETIC_CAPACITY_DIR to an owned fixture copy"),
    );
    let marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join(FIXTURE_MARKER)).unwrap()).unwrap();
    assert_eq!(
        marker,
        fixture_marker(events_count),
        "not the generated synthetic fixture"
    );
    let background_bytes = std::env::var("ALLSOURCE_CAPACITY_BACKGROUND_BYTES")
        .ok()
        .map_or(0, |value| value.parse::<usize>().unwrap());
    assert!(background_bytes <= 3 * 1024 * 1024 * 1024);
    // Touch each page and retain it through hydration. This reserves measured
    // headroom without attributing generated fixture allocations to the server.
    let background = vec![1_u8; background_bytes];
    std::hint::black_box(&background);
    eprintln!("background resident reservation: {background_bytes} bytes");
    // Keep the experimental limit local to this explicit measurement. It can
    // reproduce the rejected 750,000-row proposal without widening runtime policy.
    http_probe(&directory, events_count, false, Some(750_000)).await;
    std::hint::black_box(&background);
}

fn fixture_rows() -> usize {
    let rows = std::env::var_os("ALLSOURCE_CAPACITY_ROWS").map_or(566_486, |value| {
        value
            .to_str()
            .expect("capacity row count must be Unicode")
            .parse::<usize>()
            .expect("capacity row count must be an integer")
    });
    assert!((566_486..=750_000).contains(&rows));
    rows
}

fn fixture_marker(events_count: usize) -> serde_json::Value {
    serde_json::json!({
        "protocol": "synthetic-dense-capacity-v1", "tenant": TENANT,
        "files": 112, "events": events_count,
    })
}

async fn compatibility_probe(files: usize, events_count: usize) {
    let directory = seed_archive(files, events_count);
    http_probe(directory.path(), events_count, true, None).await;
}

fn seed_archive(files: usize, events_count: usize) -> tempfile::TempDir {
    let directory = tempfile::TempDir::new().unwrap();
    let started = Instant::now();
    let storage = ParquetStorage::new(directory.path()).unwrap();
    let make_event = |index: usize| {
        let mut event = Event::from_strings(
            "synthetic.updated".into(),
            format!("synthetic-entity-{index}"),
            TENANT.into(),
            serde_json::json!({"synthetic": true, "index": index}),
            None,
        )
        .unwrap();
        event.version = 1;
        event
    };
    // Generate the real schema once, then split its unique records directly.
    // This read-capacity fixture avoids 16,100 production fsync pairs; it is
    // not a durability/crash test and never changes the production writer.
    let events: Vec<_> = (0..events_count).map(make_event).collect();
    let seed = storage
        .write_atomic_parquet(TENANT, "events-seed", &events)
        .unwrap();
    let mut reader = ParquetRecordBatchReaderBuilder::try_new(File::open(&seed).unwrap())
        .unwrap()
        .with_batch_size(events_count)
        .build()
        .unwrap();
    let batch = reader.next().unwrap().unwrap();
    assert_eq!(batch.num_rows(), events_count);
    assert!(reader.next().is_none());
    drop(reader);
    drop(events);
    let properties = WriterProperties::builder()
        .set_compression(ParquetStorageConfig::default().compression)
        .build();
    let mut index = 0;
    for file in 0..files {
        let count = events_count / files + usize::from(file < events_count % files);
        let path = seed
            .parent()
            .unwrap()
            .join(format!("events-synthetic-{file:05}.parquet"));
        let mut writer = ArrowWriter::try_new(
            File::create(path).unwrap(),
            batch.schema(),
            Some(properties.clone()),
        )
        .unwrap();
        writer.write(&batch.slice(index, count)).unwrap();
        writer.close().unwrap();
        index += count;
    }
    drop(batch);
    std::fs::remove_file(seed).unwrap();
    assert_eq!(index, events_count);
    assert_eq!(
        storage.list_parquet_files_for_tenant(TENANT).unwrap().len(),
        files
    );
    eprintln!(
        "synthetic seed: files={files} events={events_count} elapsed={:?}",
        started.elapsed()
    );
    drop(storage);
    directory
}

async fn http_probe(
    directory: &Path,
    events_count: usize,
    direct_control: bool,
    proposed_rows: Option<u64>,
) {
    let (mut config, _) = EventStoreConfig::from_env_vars(
        None,
        Some(directory.to_str().unwrap().into()),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    if let Some(rows) = proposed_rows {
        config.strict_archive_limits.max_rows = rows;
    }
    let store = Arc::new(EventStore::with_config(config));
    if direct_control {
        let started = Instant::now();
        let control_store = Arc::clone(&store);
        let result = tokio::task::spawn_blocking(move || {
            control_store.query_retained_entity(
                TENANT,
                "synthetic-entity-0",
                10,
                &ReadScope::unrestricted(),
            )
        })
        .await
        .unwrap();
        eprintln!(
            "direct cold read: elapsed={:?} error={:?} resident={}",
            started.elapsed(),
            result.as_ref().err(),
            store.total_events()
        );
        if let Err(error) = result {
            assert!(
                error.to_string().contains("elapsed time")
                    || error.to_string().contains("budget exceeded: rows")
            );
            // Time can expire during cache application after full decode. A
            // resident prefix remains unverified; HTTP must finish hydration and
            // deduplicate it before the later command can use this history.
            assert!(!store.is_tenant_loaded(TENANT));
        } else {
            // A faster machine may finish inside four seconds. Start HTTP cold
            // regardless, without appending anything in the diagnostic control.
            assert_eq!(store.total_events(), events_count);
            store.evict_tenant(TENANT);
            assert_eq!(store.total_events(), 0);
        }
    }
    let app = Router::new()
        .route("/events", post(api::ingest_event))
        .with_state(Arc::clone(&store));
    // test-hang-allow: owned loopback server, aborted and joined at test end.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/events", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(7))
        .build()
        .unwrap();
    let command = serde_json::json!({
        "tenant_id": TENANT, "entity_id": "synthetic-entity-0",
        "event_type": "synthetic.updated", "payload": {}, "expected_version": 1
    });
    let started = Instant::now();
    let response = client.post(&url).json(&command).send().await.unwrap();
    eprintln!(
        "cold HTTP: elapsed={:?} status={}",
        started.elapsed(),
        response.status()
    );
    if response.status() == 503 {
        assert!(
            response
                .text()
                .await
                .unwrap()
                .contains("response deadline exceeded")
        );
        // test-hang-allow: bounded observation of the service-owned 30-second warmup.
        tokio::time::timeout(Duration::from_secs(32), async {
            while !store.is_tenant_loaded(TENANT) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        eprintln!("verified HTTP warmup: elapsed={:?}", started.elapsed());
        assert_eq!(store.total_events(), events_count);
        assert_eq!(store.get_entity_version("synthetic-entity-0"), 1);
        assert_eq!(
            client
                .post(&url)
                .json(&command)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    } else {
        assert_eq!(response.status(), 200, "{}", response.text().await.unwrap());
    }
    assert_eq!(store.get_entity_version("synthetic-entity-0"), 2);
    assert_eq!(store.total_events(), events_count + 1);
    server.abort();
    assert!(server.await.unwrap_err().is_cancelled());
}
