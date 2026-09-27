//! Explicit production-shape capacity probe. Never uses customer data.
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
    sync::Arc,
    time::{Duration, Instant},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit filesystem capacity probe: creates 16,100 synthetic Parquet files"]
async fn bounded_http_warmup_accepts_existing_archive_shape() {
    compatibility_probe(16_100, 90_064).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "known row-capacity regression t-40896f: creates 566,486 synthetic events"]
async fn bounded_http_warmup_accepts_existing_dense_archive_shape() {
    compatibility_probe(112, 566_486).await;
}

async fn compatibility_probe(files: usize, events_count: usize) {
    const TENANT: &str = "synthetic-cold-compatibility";
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

    let (config, _) = EventStoreConfig::from_env_vars(
        None,
        Some(directory.path().to_str().unwrap().into()),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    let store = Arc::new(EventStore::with_config(config));
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
