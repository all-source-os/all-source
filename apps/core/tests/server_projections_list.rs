//! `entity_snapshots` is registered only when `EventStoreConfig::register_entity_snapshots`
//! is set: server config leaves it unset, embedded config sets it.

#[cfg(feature = "server")]
mod server {
    use std::sync::Arc;

    use allsource_core::{
        infrastructure::web::api::list_projections,
        store::{EventStore, EventStoreConfig},
    };
    use axum::{Json, extract::State};

    #[tokio::test]
    async fn server_projections_list() {
        let (config, _mode) =
            EventStoreConfig::from_env_vars(None, None, None, None, None, None, None, None);
        let store = Arc::new(EventStore::with_config(config));

        let Json(body) = list_projections(State(store)).await;
        let names: Vec<&str> = body["projections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|projection| projection["name"].as_str().unwrap())
            .collect();

        assert!(!names.contains(&"entity_snapshots"), "{names:?}");
        assert!(names.contains(&"event_counters"), "{names:?}");
    }
}

#[cfg(feature = "embedded")]
mod embedded {
    use allsource_core::embedded::{Config, EmbeddedCore, IngestEvent};
    use serde_json::json;
    use tempfile::TempDir;

    async fn snapshot_after_ingest(core: &EmbeddedCore) -> Option<serde_json::Value> {
        core.ingest(IngestEvent {
            entity_id: "order-1",
            event_type: "order.placed",
            payload: json!({"total": 1}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();
        core.projection("entity_snapshots", "order-1")
    }

    #[tokio::test]
    async fn embedded_keeps_entity_snapshots_in_memory() {
        let core = EmbeddedCore::open(Config::builder().build().unwrap())
            .await
            .unwrap();

        assert!(snapshot_after_ingest(&core).await.is_some());
    }

    #[tokio::test]
    async fn embedded_keeps_entity_snapshots_with_data_dir() {
        let tmp = TempDir::new().unwrap();
        let config = Config::builder().data_dir(tmp.path()).build().unwrap();
        let core = EmbeddedCore::open(config).await.unwrap();

        assert!(snapshot_after_ingest(&core).await.is_some());
    }
}
