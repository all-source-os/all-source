//! Spike for plan slice 11: two nodes, a backdated correction, both time axes.
//!
//! Records what each side sends and answers. Wire sync carries valid time;
//! the follower still files record time at receive (plan section 7, Q5).

#![cfg(feature = "embedded-sync")]

use std::{thread::sleep, time::Duration};

use allsource_core::embedded::{Config, EmbeddedCore, IngestEvent};
use chrono::{DateTime, Utc};
use serde_json::json;

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn tick() -> DateTime<Utc> {
    sleep(Duration::from_millis(3));
    Utc::now()
}

async fn open(node: u32) -> EmbeddedCore {
    EmbeddedCore::open(Config::builder().node_id(node).build().unwrap())
        .await
        .unwrap()
}

async fn put(core: &EmbeddedCore, event_type: &str, payload: serde_json::Value, valid_from: &str) {
    // Distinct millisecond per write, so record order is unambiguous.
    sleep(Duration::from_millis(2));
    core.ingest_with_valid_time(
        IngestEvent {
            entity_id: "doc-1",
            event_type,
            payload,
            metadata: None,
            tenant_id: None,
        },
        Some(at(valid_from)),
        None,
    )
    .await
    .unwrap();
}

/// Status of `doc-1`, or the error variant when the fold has no events.
fn status(
    core: &EmbeddedCore,
    as_of_recorded: Option<DateTime<Utc>>,
    as_of_valid: Option<DateTime<Utc>>,
) -> String {
    match core
        .inner()
        .entity_state_at("doc-1", as_of_recorded, as_of_valid, None, false)
    {
        Ok(v) => v["current_state"]["status"]
            .as_str()
            .unwrap_or("?")
            .to_string(),
        Err(e) => format!("err:{e:?}"),
    }
}

async fn seed_leader(a: &EmbeddedCore) -> DateTime<Utc> {
    put(
        a,
        "doc.created",
        json!({"status": "draft"}),
        "2026-01-01T00:00:00Z",
    )
    .await;
    put(
        a,
        "doc.updated",
        json!({"status": "final"}),
        "2026-03-01T00:00:00Z",
    )
    .await;
    tick()
}

#[tokio::test]
async fn two_nodes_answer_both_axes_after_backdated_correction() {
    let a = open(1).await;
    let b = open(2).await;

    let r1 = seed_leader(&a).await;
    b.sync_to(&a).await.unwrap();
    a.sync_to(&b).await.unwrap();
    assert_eq!(a.event_count(), 2);
    assert_eq!(b.event_count(), 2);

    put(
        &b,
        "doc.updated",
        json!({"status": "corrected"}),
        "2026-02-01T00:00:00Z",
    )
    .await;
    let r2 = tick();

    let b_sends = b.events_for_sync(&a.version_vector()).await.unwrap();
    assert_eq!(
        b_sends.len(),
        3,
        "B re-sends its relayed A events plus the correction"
    );

    b.sync_to(&a).await.unwrap();
    a.sync_to(&b).await.unwrap();
    assert_eq!(a.event_count(), 3);
    assert_eq!(b.event_count(), 3);

    let feb_15 = at("2026-02-15T00:00:00Z");
    let mar_15 = at("2026-03-15T00:00:00Z");
    let cases = [
        ("believed at r1", Some(r1), None, "final"),
        ("believed at r2", Some(r2), None, "corrected"),
        ("true feb15 given r2", Some(r2), Some(feb_15), "corrected"),
        ("true feb15 given r1", Some(r1), Some(feb_15), "draft"),
        ("true mar15 given r2", Some(r2), Some(mar_15), "final"),
    ];
    for (label, rec, valid, expected) in cases {
        assert_eq!(status(&a, rec, valid), expected, "A: {label}");
        assert_eq!(status(&b, rec, valid), expected, "B: {label}");
    }
}

#[tokio::test]
async fn wire_sync_keeps_valid_time() {
    let a = open(1).await;
    let r1 = seed_leader(&a).await;

    let sent = a
        .events_for_sync(&std::collections::BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(sent.len(), 2);
    assert!(
        sent.iter()
            .all(|e| e.event_data.get("valid_from").is_some()),
        "the wire payload carries valid_from"
    );

    let c = open(3).await;
    let (accepted, skipped) = c.receive_sync_push(sent).await.unwrap();
    assert_eq!((accepted, skipped), (2, 0));

    // Follower stamps record time at receive, so it knows nothing before r1.
    assert!(
        status(&c, Some(r1), None).starts_with("err:"),
        "follower has no events at r1: {}",
        status(&c, Some(r1), None)
    );
    let jan_15 = at("2026-01-15T00:00:00Z");
    let mar_15 = at("2026-03-15T00:00:00Z");
    let c_now = tick();
    for valid in [jan_15, mar_15] {
        assert_eq!(
            status(&c, Some(c_now), Some(valid)),
            status(&a, Some(c_now), Some(valid)),
            "follower and leader agree at valid {valid}"
        );
    }
    assert_eq!(status(&c, Some(c_now), Some(jan_15)), "draft");
    assert_eq!(status(&c, Some(c_now), Some(mar_15)), "final");
}

#[tokio::test]
async fn wire_sync_pins_an_unset_valid_from_to_the_origin_record_time() {
    let a = open(1).await;
    a.ingest(IngestEvent {
        entity_id: "doc-1",
        event_type: "doc.created",
        payload: json!({"status": "draft"}),
        metadata: None,
        tenant_id: None,
    })
    .await
    .unwrap();
    let origin = a
        .events_for_sync(&std::collections::BTreeMap::new())
        .await
        .unwrap();
    let origin_recorded: DateTime<Utc> =
        serde_json::from_value(origin[0].event_data["timestamp"].clone()).unwrap();

    tick();
    let c = open(3).await;
    c.receive_sync_push(origin).await.unwrap();

    let c_now = tick();
    assert_eq!(status(&c, Some(c_now), Some(origin_recorded)), "draft");
}
