//! Valid time on a verbatim event survives a checkpoint write and read, and a
//! checkpoint whose events carry no valid time reads back unset.

use allsource_core::{
    domain::entities::Event,
    infrastructure::persistence::system_checkpoint::{self, SystemCheckpoint},
};
use chrono::{Duration, Utc};
use serde_json::json;
use std::collections::BTreeMap;
use tempfile::TempDir;
use uuid::Uuid;

fn event(valid: Option<(chrono::DateTime<Utc>, Option<chrono::DateTime<Utc>>)>) -> Event {
    let mut event = Event::reconstruct_from_strings(
        Uuid::new_v4(),
        "_system.tenant.created".to_string(),
        "tenant-valid-time".to_string(),
        "default".to_string(),
        json!({"name": "valid-time"}),
        Utc::now(),
        None,
        1,
    );
    if let Some((from, to)) = valid {
        event.valid_from = Some(from);
        event.valid_to = to;
    }
    event
}

fn checkpoint_with(verbatim: Vec<(u64, Event)>) -> SystemCheckpoint {
    SystemCheckpoint {
        format: system_checkpoint::format(),
        covered_seq: verbatim.last().map_or(0, |(seq, _)| *seq),
        created_at: Utc::now(),
        segments: Vec::new(),
        domain_counts: BTreeMap::new(),
        verbatim,
        folds: BTreeMap::new(),
    }
}

fn write_and_read(checkpoint: &SystemCheckpoint) -> SystemCheckpoint {
    let directory = TempDir::new().unwrap();
    let dir = directory.path().join(system_checkpoint::CHECKPOINT_DIR);
    std::fs::create_dir_all(&dir).unwrap();
    let path = system_checkpoint::write(&dir, checkpoint).unwrap();
    system_checkpoint::read(&path).unwrap()
}

#[test]
fn valid_time_round_trips_through_a_checkpoint() {
    let from = Utc::now() - Duration::days(7);
    let to = Utc::now() + Duration::days(7);
    let open = event(Some((from, None)));
    let closed = event(Some((from, Some(to))));
    let read = write_and_read(&checkpoint_with(vec![
        (1, open.clone()),
        (2, closed.clone()),
    ]));

    assert_eq!(read.verbatim.len(), 2);
    assert_eq!(read.verbatim[0].1, open);
    assert_eq!(read.verbatim[1].1, closed);
    assert_eq!(read.verbatim[1].1.valid_to, Some(to));
}

#[test]
fn a_checkpoint_without_valid_time_reads_back_unset() {
    let plain = event(None);
    let read = write_and_read(&checkpoint_with(vec![(1, plain.clone())]));

    assert_eq!(read.verbatim[0].1, plain);
    assert_eq!(read.verbatim[0].1.valid_from, None);
    assert_eq!(read.verbatim[0].1.valid_to, None);
}
