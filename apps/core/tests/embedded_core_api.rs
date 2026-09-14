//! TDD tests for EmbeddedCore facade API.
//!
//! Run with: cargo test --features embedded --test embedded_core_api

#[cfg(feature = "embedded")]
mod tests {
    use allsource_core::embedded::{Config, EmbeddedCore, EventView, IngestEvent, Query};
    use serde_json::json;
    use tempfile::TempDir;

    // =========================================================================
    // Config builder tests
    // =========================================================================

    #[test]
    fn config_builder_defaults_to_in_memory_single_tenant() {
        let config = Config::builder().build().unwrap();
        // No data_dir means in-memory only
        assert!(config.data_dir().is_none());
        // Single-tenant by default
        assert!(config.single_tenant());
    }

    #[test]
    fn config_builder_with_data_dir() {
        let tmp = TempDir::new().unwrap();
        let config = Config::builder().data_dir(tmp.path()).build().unwrap();
        assert!(config.data_dir().is_some());
    }

    #[test]
    fn config_builder_wal_sync_option() {
        let config = Config::builder().wal_sync_on_write(false).build().unwrap();
        assert!(!config.wal_sync_on_write());
    }

    #[test]
    fn config_builder_multi_tenant() {
        let config = Config::builder().single_tenant(false).build().unwrap();
        assert!(!config.single_tenant());
    }

    // =========================================================================
    // EmbeddedCore::open tests
    // =========================================================================

    #[tokio::test]
    async fn open_in_memory() {
        let core = EmbeddedCore::open(Config::builder().build().unwrap())
            .await
            .expect("open in-memory should succeed");
        assert_eq!(core.stats().total_events, 0);
    }

    #[tokio::test]
    async fn open_with_persistence() {
        let tmp = TempDir::new().unwrap();
        let core = EmbeddedCore::open(Config::builder().data_dir(tmp.path()).build().unwrap())
            .await
            .expect("open with persistence should succeed");
        assert_eq!(core.stats().total_events, 0);
    }

    // =========================================================================
    // Ingest tests
    // =========================================================================

    #[tokio::test]
    async fn ingest_simple_event() {
        let core = open_in_memory_core().await;

        core.ingest(IngestEvent {
            entity_id: "order-001",
            event_type: "order.placed",
            payload: json!({"total": 99.99}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .expect("ingest should succeed");

        assert_eq!(core.stats().total_events, 1);
    }

    #[tokio::test]
    async fn ingest_with_metadata() {
        let core = open_in_memory_core().await;

        core.ingest(IngestEvent {
            entity_id: "user-001",
            event_type: "user.registered",
            payload: json!({"email": "a@b.com"}),
            metadata: Some(json!({"source": "signup"})),
            tenant_id: None,
        })
        .await
        .unwrap();

        assert_eq!(core.stats().total_events, 1);
    }

    #[tokio::test]
    async fn ingest_rejects_invalid_event_type() {
        let core = open_in_memory_core().await;

        let result = core
            .ingest(IngestEvent {
                entity_id: "e1",
                event_type: "Invalid.Type", // uppercase — should fail EventType validation
                payload: json!({}),
                metadata: None,
                tenant_id: None,
            })
            .await;

        assert!(result.is_err());
        assert_eq!(core.stats().total_events, 0);
    }

    // =========================================================================
    // Query tests
    // =========================================================================

    #[tokio::test]
    async fn query_by_entity_id() {
        let core = open_in_memory_core().await;

        for i in 0..3 {
            core.ingest(IngestEvent {
                entity_id: "order-99",
                event_type: "order.updated",
                payload: json!({"step": i}),
                metadata: None,
                tenant_id: None,
            })
            .await
            .unwrap();
        }

        let events = core
            .query(Query::new().entity_id("order-99"))
            .await
            .unwrap();

        assert_eq!(events.len(), 3);
    }

    #[tokio::test]
    async fn query_by_event_type_prefix() {
        let core = open_in_memory_core().await;

        core.ingest(IngestEvent {
            entity_id: "e1",
            event_type: "order.placed",
            payload: json!({}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();
        core.ingest(IngestEvent {
            entity_id: "e2",
            event_type: "order.shipped",
            payload: json!({}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();
        core.ingest(IngestEvent {
            entity_id: "e3",
            event_type: "user.created",
            payload: json!({}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        let events = core
            .query(Query::new().event_type_prefix("order."))
            .await
            .unwrap();
        assert_eq!(events.len(), 2);
    }

    #[tokio::test]
    async fn query_excludes_event_type_prefixes() {
        let core = open_in_memory_core().await;

        for (entity, et) in [
            ("e1", "order.placed"),
            ("e2", "audit.api_key.created"),
            ("e3", "service.heartbeat"),
            ("e4", "user.created"),
        ] {
            core.ingest(IngestEvent {
                entity_id: entity,
                event_type: et,
                payload: json!({}),
                metadata: None,
                tenant_id: None,
            })
            .await
            .unwrap();
        }

        // Drop operational namespaces; only domain events remain.
        let events = core
            .query(Query::new().exclude_event_type_prefix("audit.,service."))
            .await
            .unwrap();
        let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
        assert_eq!(events.len(), 2, "got {types:?}");
        assert!(types.contains(&"order.placed"));
        assert!(types.contains(&"user.created"));
        assert!(
            !types
                .iter()
                .any(|t| t.starts_with("audit.") || t.starts_with("service."))
        );
    }

    #[tokio::test]
    async fn exclude_applies_before_limit() {
        let core = open_in_memory_core().await;

        // 20 heartbeats then one real event — newest. With limit=1 and the
        // heartbeats excluded, the real event must survive (exclusion happens
        // before the limit, so noise can't consume the window).
        for i in 0..20 {
            core.ingest(IngestEvent {
                entity_id: "sys",
                event_type: "service.heartbeat",
                payload: json!({ "n": i }),
                metadata: None,
                tenant_id: None,
            })
            .await
            .unwrap();
        }
        core.ingest(IngestEvent {
            entity_id: "order-9",
            event_type: "order.placed",
            payload: json!({}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        let events = core
            .query(Query::new().exclude_event_type_prefix("service.").limit(1))
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "order.placed");
    }

    #[tokio::test]
    async fn query_with_limit() {
        let core = open_in_memory_core().await;

        for i in 0..10 {
            core.ingest(IngestEvent {
                entity_id: &format!("e-{i}"),
                event_type: "item.created",
                payload: json!({"i": i}),
                metadata: None,
                tenant_id: None,
            })
            .await
            .unwrap();
        }

        let events = core.query(Query::new().limit(3)).await.unwrap();
        assert_eq!(events.len(), 3);
    }

    #[tokio::test]
    async fn query_page_rejects_zero_limit() {
        let core = open_in_memory_core().await;

        let error = core
            .query_page(Query::new().limit(0))
            .await
            .expect_err("zero-size pages cannot make pagination progress");

        assert!(
            error
                .to_string()
                .contains("limit must be greater than zero")
        );
    }

    #[tokio::test]
    async fn query_returns_event_view_with_plain_strings() {
        let core = open_in_memory_core().await;

        core.ingest(IngestEvent {
            entity_id: "ent-1",
            event_type: "item.added",
            payload: json!({"qty": 5}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        let events = core.query(Query::new().entity_id("ent-1")).await.unwrap();

        assert_eq!(events.len(), 1);
        let ev: &EventView = &events[0];
        assert_eq!(ev.event_type, "item.added");
        assert_eq!(ev.entity_id, "ent-1");
        assert_eq!(ev.payload["qty"], 5);
        assert!(!ev.id.is_nil());
    }

    #[tokio::test]
    async fn query_empty_result() {
        let core = open_in_memory_core().await;

        let events = core
            .query(Query::new().entity_id("nonexistent"))
            .await
            .unwrap();

        assert!(events.is_empty());
    }

    // =========================================================================
    // Projection tests
    // =========================================================================

    #[tokio::test]
    async fn projection_returns_none_for_unknown_entity() {
        let core = open_in_memory_core().await;

        // Built-in projections exist but return None for unknown entities
        let state = core.projection("entity_snapshots", "nonexistent");
        assert!(state.is_none());
    }

    // =========================================================================
    // Single-tenant mode
    // =========================================================================

    #[tokio::test]
    async fn single_tenant_uses_default_tenant_id() {
        let core = open_in_memory_core().await;

        core.ingest(IngestEvent {
            entity_id: "e-st",
            event_type: "event.created",
            payload: json!({}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        let events = core.query(Query::new().entity_id("e-st")).await.unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tenant_id, "default");
    }

    // =========================================================================
    // Isolation — no global state
    // =========================================================================

    #[tokio::test]
    async fn multiple_instances_are_isolated() {
        let core_a = open_in_memory_core().await;
        let core_b = open_in_memory_core().await;

        core_a
            .ingest(IngestEvent {
                entity_id: "a1",
                event_type: "a.created",
                payload: json!({}),
                metadata: None,
                tenant_id: None,
            })
            .await
            .unwrap();

        assert_eq!(core_a.stats().total_events, 1);
        assert_eq!(core_b.stats().total_events, 0); // isolated
    }

    // =========================================================================
    // Shutdown
    // =========================================================================

    #[tokio::test]
    async fn shutdown_is_graceful() {
        let tmp = TempDir::new().unwrap();
        let core = EmbeddedCore::open(Config::builder().data_dir(tmp.path()).build().unwrap())
            .await
            .unwrap();

        core.ingest(IngestEvent {
            entity_id: "e1",
            event_type: "data.saved",
            payload: json!({"important": true}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        core.shutdown().await.expect("shutdown should succeed");
    }

    // =========================================================================
    // inner() escape hatch
    // =========================================================================

    #[tokio::test]
    async fn inner_provides_raw_event_store() {
        let core = open_in_memory_core().await;
        let store = core.inner();
        // Can call raw EventStore methods
        assert_eq!(store.stats().total_events, 0);
    }

    // =========================================================================
    // Phase 2: Serde support
    // =========================================================================

    #[tokio::test]
    async fn event_view_serializes_to_json() {
        let core = open_in_memory_core().await;

        core.ingest(IngestEvent {
            entity_id: "ser-1",
            event_type: "item.created",
            payload: json!({"name": "widget"}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        let events = core.query(Query::new().entity_id("ser-1")).await.unwrap();

        let json_str = serde_json::to_string(&events[0]).unwrap();
        assert!(json_str.contains("item.created"));
        assert!(json_str.contains("widget"));
    }

    #[tokio::test]
    async fn event_view_deserializes_from_json() {
        let core = open_in_memory_core().await;

        core.ingest(IngestEvent {
            entity_id: "deser-1",
            event_type: "item.created",
            payload: json!({"x": 1}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        let events = core.query(Query::new().entity_id("deser-1")).await.unwrap();

        let json_str = serde_json::to_string(&events[0]).unwrap();
        let round_tripped: EventView = serde_json::from_str(&json_str).unwrap();
        assert_eq!(round_tripped.entity_id, "deser-1");
        assert_eq!(round_tripped.event_type, "item.created");
        assert_eq!(round_tripped.payload["x"], 1);
    }

    // =========================================================================
    // Phase 2: Multi-tenant support via tenant_id on IngestEvent
    // =========================================================================

    #[tokio::test]
    async fn ingest_with_explicit_tenant_id() {
        let core = EmbeddedCore::open(Config::builder().single_tenant(false).build().unwrap())
            .await
            .unwrap();

        core.ingest(IngestEvent {
            entity_id: "mt-1",
            event_type: "order.placed",
            payload: json!({}),
            metadata: None,
            tenant_id: Some("tenant-acme"),
        })
        .await
        .unwrap();

        let events = core.query(Query::new().entity_id("mt-1")).await.unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tenant_id, "tenant-acme");
    }

    #[tokio::test]
    async fn ingest_without_tenant_id_in_multi_tenant_uses_default() {
        let core = EmbeddedCore::open(Config::builder().single_tenant(false).build().unwrap())
            .await
            .unwrap();

        core.ingest(IngestEvent {
            entity_id: "mt-2",
            event_type: "order.placed",
            payload: json!({}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        let events = core.query(Query::new().entity_id("mt-2")).await.unwrap();

        assert_eq!(events[0].tenant_id, "default");
    }

    #[tokio::test]
    async fn single_tenant_ignores_explicit_tenant_id() {
        let core = open_in_memory_core().await; // single_tenant=true by default

        core.ingest(IngestEvent {
            entity_id: "st-override",
            event_type: "order.placed",
            payload: json!({}),
            metadata: None,
            tenant_id: Some("ignored-tenant"),
        })
        .await
        .unwrap();

        let events = core
            .query(Query::new().entity_id("st-override"))
            .await
            .unwrap();

        // In single-tenant mode, tenant_id is always "default" regardless of input
        assert_eq!(events[0].tenant_id, "default");
    }

    #[tokio::test]
    async fn multi_tenant_query_filters_by_tenant() {
        let core = EmbeddedCore::open(Config::builder().single_tenant(false).build().unwrap())
            .await
            .unwrap();

        // Ingest events for two tenants
        core.ingest(IngestEvent {
            entity_id: "shared-entity",
            event_type: "data.created",
            payload: json!({"tenant": "a"}),
            metadata: None,
            tenant_id: Some("tenant-a"),
        })
        .await
        .unwrap();

        core.ingest(IngestEvent {
            entity_id: "shared-entity",
            event_type: "data.created",
            payload: json!({"tenant": "b"}),
            metadata: None,
            tenant_id: Some("tenant-b"),
        })
        .await
        .unwrap();

        // Both events stored with correct tenant IDs
        let all = core
            .query(Query::new().entity_id("shared-entity"))
            .await
            .unwrap();
        assert_eq!(all.len(), 2);

        // Verify tenant IDs were persisted correctly
        let tenants: Vec<&str> = all.iter().map(|e| e.tenant_id.as_str()).collect();
        assert!(tenants.contains(&"tenant-a"));
        assert!(tenants.contains(&"tenant-b"));
    }

    // =========================================================================
    // Phase 2: Root re-export
    // =========================================================================

    #[tokio::test]
    async fn embedded_core_accessible_from_crate_root() {
        // EmbeddedCore should be re-exported at the crate root
        let core = allsource_core::EmbeddedCore::open(Config::builder().build().unwrap())
            .await
            .unwrap();
        assert_eq!(core.stats().total_events, 0);
    }

    // =========================================================================
    // Projection backfill test
    // =========================================================================

    #[tokio::test]
    async fn register_projection_with_backfill_replays_history() {
        use allsource_core::{
            application::services::projection::Projection, domain::entities::Event,
        };
        use dashmap::DashMap;
        use std::sync::Arc;

        // Simple counting projection
        struct CountProjection {
            counts: DashMap<String, u64>,
        }

        impl Projection for CountProjection {
            fn name(&self) -> &'static str {
                "test_counter"
            }
            fn process(&self, event: &Event) -> allsource_core::error::Result<()> {
                self.counts
                    .entry(event.entity_id_str().to_string())
                    .and_modify(|c| *c += 1)
                    .or_insert(1);
                Ok(())
            }
            fn get_state(&self, entity_id: &str) -> Option<serde_json::Value> {
                self.counts.get(entity_id).map(|c| json!({ "count": *c }))
            }
            fn clear(&self) {
                self.counts.clear();
            }
        }

        let core = open_in_memory_core().await;

        // Ingest 5 events BEFORE registering the projection
        for i in 0..5 {
            core.ingest(IngestEvent {
                entity_id: "backfill-entity",
                event_type: "backfill.test",
                payload: json!({"seq": i}),
                metadata: None,
                tenant_id: None,
            })
            .await
            .unwrap();
        }

        // Register with backfill — should replay historical events
        let projection = Arc::new(CountProjection {
            counts: DashMap::new(),
        });
        let dyn_proj: Arc<dyn Projection> = projection.clone();
        core.inner()
            .register_projection_with_backfill(&dyn_proj)
            .unwrap();

        // Projection should have seen all 5 historical events
        let state = projection.get_state("backfill-entity").unwrap();
        assert_eq!(state["count"], 5, "Backfill should have replayed 5 events");

        // Future events should also be processed
        core.ingest(IngestEvent {
            entity_id: "backfill-entity",
            event_type: "backfill.test",
            payload: json!({"seq": 5}),
            metadata: None,
            tenant_id: None,
        })
        .await
        .unwrap();

        let state = projection.get_state("backfill-entity").unwrap();
        assert_eq!(state["count"], 6, "Future event should also be processed");
    }

    // =========================================================================
    // Crash recovery test
    // =========================================================================

    #[tokio::test]
    async fn events_survive_store_restart_via_wal() {
        let tmp = TempDir::new().unwrap();
        let data_dir = tmp.path().to_path_buf();

        // Phase 1: Open, ingest events, shutdown
        {
            let core = EmbeddedCore::open(Config::builder().data_dir(&data_dir).build().unwrap())
                .await
                .unwrap();

            for i in 0..10 {
                core.ingest(IngestEvent {
                    entity_id: &format!("recovery-{i}"),
                    event_type: "recovery.test",
                    payload: json!({"seq": i}),
                    metadata: None,
                    tenant_id: None,
                })
                .await
                .unwrap();
            }

            core.shutdown().await.unwrap();
            // core is dropped here
        }

        // Phase 2: Reopen with same data_dir, verify events survived
        {
            let core = EmbeddedCore::open(Config::builder().data_dir(&data_dir).build().unwrap())
                .await
                .unwrap();

            let events = core
                .query(Query::new().event_type("recovery.test").limit(100))
                .await
                .unwrap();

            assert_eq!(
                events.len(),
                10,
                "Expected 10 events after restart, got {}",
                events.len()
            );

            // Verify event data is intact
            let first = events.iter().find(|e| e.entity_id == "recovery-0").unwrap();
            assert_eq!(first.payload["seq"], 0);
        }
    }

    // =========================================================================
    // Concurrency tests
    // =========================================================================

    #[tokio::test]
    async fn concurrent_writers_no_lost_events() {
        use std::sync::Arc;

        let core = Arc::new(open_in_memory_core().await);
        let mut handles = Vec::new();

        // Spawn 10 tasks, each ingesting 100 events
        for task_id in 0..10u32 {
            let core = Arc::clone(&core);
            handles.push(tokio::spawn(async move {
                for i in 0..100u32 {
                    core.ingest(IngestEvent {
                        entity_id: &format!("task-{task_id}-entity-{i}"),
                        event_type: "concurrency.test",
                        payload: json!({"task": task_id, "seq": i}),
                        metadata: None,
                        tenant_id: None,
                    })
                    .await
                    .unwrap();
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        // All 1000 events should be present
        let events = core
            .query(Query::new().event_type("concurrency.test").limit(2000))
            .await
            .unwrap();
        assert_eq!(
            events.len(),
            1000,
            "Expected 1000 events from 10 writers x 100 events, got {}",
            events.len()
        );
    }

    #[tokio::test]
    async fn concurrent_readers_and_writers() {
        use std::sync::Arc;

        let core = Arc::new(open_in_memory_core().await);
        let mut handles = Vec::new();

        // 5 writers
        for task_id in 0..5u32 {
            let core = Arc::clone(&core);
            handles.push(tokio::spawn(async move {
                for i in 0..50u32 {
                    core.ingest(IngestEvent {
                        entity_id: &format!("rw-{task_id}-{i}"),
                        event_type: "rw.test",
                        payload: json!({"task": task_id, "seq": i}),
                        metadata: None,
                        tenant_id: None,
                    })
                    .await
                    .unwrap();
                }
            }));
        }

        // 5 concurrent readers — each reads multiple times
        for _ in 0..5u32 {
            let core = Arc::clone(&core);
            handles.push(tokio::spawn(async move {
                let mut prev_count = 0;
                for _ in 0..20 {
                    let events = core
                        .query(Query::new().event_type("rw.test").limit(500))
                        .await
                        .unwrap();
                    // Event count should be monotonically non-decreasing
                    assert!(
                        events.len() >= prev_count,
                        "Event count decreased: {} -> {}",
                        prev_count,
                        events.len()
                    );
                    prev_count = events.len();
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        // Final count: 5 writers x 50 events = 250
        let events = core
            .query(Query::new().event_type("rw.test").limit(500))
            .await
            .unwrap();
        assert_eq!(events.len(), 250);
    }

    // =========================================================================
    // Helper
    // =========================================================================

    async fn open_in_memory_core() -> EmbeddedCore {
        EmbeddedCore::open(Config::builder().build().unwrap())
            .await
            .unwrap()
    }
}
