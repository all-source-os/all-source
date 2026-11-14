//! `PUT /api/v1/pipelines/{id}/reset` returns `pipeline.windows` and
//! `pipeline.state` to zero in the resident ledger. The handler is called
//! directly, so the assertion is on the ledger the route itself leaves behind.
#![cfg(feature = "server")]

use allsource_core::{
    application::services::pipeline::{PipelineConfig, PipelineOperator, WindowConfig, WindowType},
    domain::entities::Event,
    infrastructure::{
        memory::ledger::{Owner, resident},
        web::api::reset_pipeline,
    },
    store::EventStore,
};
use axum::extract::{Path, State};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

fn event(seq: i64) -> Event {
    Event::from_strings(
        "order.placed".into(),
        format!("order-{seq}"),
        "tenant-a".into(),
        json!({"amount": seq}),
        None,
    )
    .unwrap()
}

#[tokio::test]
async fn pipeline_reset_releases() {
    assert_eq!(resident().owner_bytes(Owner::PipelineWindows), 0);
    assert_eq!(resident().owner_bytes(Owner::PipelineState), 0);

    let store = Arc::new(EventStore::new());
    let id = Uuid::new_v4();
    store.pipeline_manager().register(PipelineConfig {
        id,
        name: "windowed-sum".into(),
        description: None,
        source_event_types: vec!["order.placed".into()],
        operators: vec![PipelineOperator::Window {
            config: WindowConfig {
                window_type: WindowType::Tumbling,
                size_seconds: 3_600,
                slide_seconds: None,
                session_timeout_seconds: None,
            },
            aggregation: Box::new(PipelineOperator::Reduce {
                field: "amount".into(),
                function: "sum".into(),
                group_by: None,
            }),
        }],
        enabled: true,
        output: "order-totals".into(),
    });

    for seq in 0..200 {
        store.ingest(&event(seq)).unwrap();
    }
    assert!(resident().owner_bytes(Owner::PipelineWindows) > 0);
    assert!(resident().owner_bytes(Owner::PipelineState) > 0);

    let response = reset_pipeline(State(Arc::clone(&store)), Path(id))
        .await
        .unwrap();
    assert_eq!(response.0["pipeline_id"], json!(id.to_string()));

    assert_eq!(resident().owner_bytes(Owner::PipelineWindows), 0);
    assert_eq!(resident().owner_bytes(Owner::PipelineState), 0);
}
