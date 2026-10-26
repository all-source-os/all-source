//! The durable feed log on its own: what survives a crash, and what a replay
//! in a different order does to positions. Dropping a `FeedLog` without a sync
//! is the crash: unsynced records live only in memory.
use allsource_core::infrastructure::persistence::feed_log::{
    Appended, FeedLog, FeedLogRecord, FeedLogScope,
};
use std::{collections::HashSet, io::Write, path::Path};
use uuid::Uuid;

fn record(entity: &str) -> FeedLogRecord {
    FeedLogRecord::new(Uuid::new_v4(), "item.changed", entity)
}

fn open(dir: &Path) -> FeedLog {
    FeedLog::open(dir, FeedLogScope::All).unwrap()
}

fn ids(log: &FeedLog, tenant: &str) -> Vec<Uuid> {
    log.records(tenant)
        .unwrap()
        .1
        .iter()
        .map(|record| record.id)
        .collect()
}

fn log_file(dir: &Path) -> std::path::PathBuf {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "feed"))
        .unwrap()
}

#[test]
fn restart_keeps_epoch_and_positions() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b, c) = (record("a"), record("b"), record("c"));
    let epoch = {
        let log = open(dir.path());
        let epoch = log.create("alice", &[a.clone(), b.clone()]).unwrap();
        assert_eq!(log.append("alice", c.clone()).unwrap(), Appended::New);
        log.sync_tenant("alice").unwrap();
        epoch
    };

    let log = open(dir.path());
    assert_eq!(log.epoch("alice"), Some(epoch));
    assert_eq!(ids(&log, "alice"), vec![a.id, b.id, c.id]);
    assert_eq!(log.durable_len("alice"), 3);
}

#[test]
fn crash_before_fsync_never_served() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (record("a"), record("b"));
    {
        let log = open(dir.path());
        log.create("alice", std::slice::from_ref(&a)).unwrap();
        assert_eq!(log.append("alice", b).unwrap(), Appended::New);
        assert_eq!(
            log.durable_len("alice"),
            1,
            "an unsynced record is not visible"
        );
        assert!(log.has_pending("alice"));
    }

    let log = open(dir.path());
    assert_eq!(ids(&log, "alice"), vec![a.id]);
    assert_eq!(log.durable_len("alice"), 1);
}

#[test]
fn sync_all_makes_replayed_records_durable() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (record("a"), record("b"));
    {
        let log = open(dir.path());
        log.create("alice", std::slice::from_ref(&a)).unwrap();
        log.append("alice", b.clone()).unwrap();
        assert!(log.has_any_pending());
        log.sync_all().unwrap();
        assert!(!log.has_any_pending());
    }

    assert_eq!(ids(&open(dir.path()), "alice"), vec![a.id, b.id]);
}

#[test]
fn recover_order_differs_no_reorder() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b, c, d) = (record("a"), record("b"), record("c"), record("d"));
    let log = open(dir.path());
    log.create("alice", &[a.clone(), b.clone(), c.clone()])
        .unwrap();

    let outcomes: Vec<_> = [c.clone(), a.clone(), d.clone(), b.clone()]
        .into_iter()
        .map(|record| log.append("alice", record).unwrap())
        .collect();
    assert_eq!(
        outcomes,
        vec![
            Appended::Duplicate,
            Appended::Duplicate,
            Appended::New,
            Appended::Duplicate
        ]
    );
    log.sync_tenant("alice").unwrap();
    let expected = vec![a.id, b.id, c.id, d.id];
    assert_eq!(ids(&log, "alice"), expected);
    drop(log);

    assert_eq!(ids(&open(dir.path()), "alice"), expected);
}

#[test]
fn torn_tail_is_dropped_and_appends_continue_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b, c) = (record("a"), record("b"), record("c"));
    {
        let log = open(dir.path());
        log.create("alice", &[a.clone(), b.clone()]).unwrap();
    }
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(log_file(dir.path()))
        .unwrap();
    file.write_all(br#"{"id":"0000"#).unwrap();
    drop(file);

    {
        let log = open(dir.path());
        assert_eq!(ids(&log, "alice"), vec![a.id, b.id]);
        log.append("alice", c.clone()).unwrap();
        log.sync_tenant("alice").unwrap();
    }

    assert_eq!(ids(&open(dir.path()), "alice"), vec![a.id, b.id, c.id]);
}

#[test]
fn an_unreadable_log_is_set_aside_and_the_tenant_starts_a_new_epoch() {
    let dir = tempfile::tempdir().unwrap();
    let epoch = open(dir.path()).create("alice", &[record("a")]).unwrap();
    std::fs::write(log_file(dir.path()), b"not a header\n").unwrap();

    let log = open(dir.path());
    assert!(!log.is_open("alice"));
    assert!(std::fs::read_dir(dir.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .path()
            .to_string_lossy()
            .ends_with(".feed.corrupt")
    }));
    assert_ne!(log.create("alice", &[]).unwrap(), epoch);
}

#[test]
fn tenants_outside_the_scope_are_never_logged() {
    let dir = tempfile::tempdir().unwrap();
    let scope = FeedLogScope::Tenants(HashSet::from(["alice".to_owned()]));
    let log = FeedLog::open(dir.path(), scope).unwrap();
    assert!(log.create("bob", &[]).is_err());
    assert_eq!(log.append("bob", record("a")).unwrap(), Appended::NotOpen);
    assert_eq!(log.durable_len("bob"), 0);
}

#[test]
fn scope_is_off_by_default_and_parses_all_or_an_allowlist() {
    for off in [
        None,
        Some(""),
        Some("0"),
        Some("false"),
        Some("off"),
        Some(" , "),
    ] {
        assert_eq!(FeedLogScope::parse(off), None, "{off:?}");
    }
    for all in ["1", "true", "on", "*", "all", "ALL"] {
        assert_eq!(FeedLogScope::parse(Some(all)), Some(FeedLogScope::All));
    }
    let scope = FeedLogScope::parse(Some(" alice, bob ")).unwrap();
    assert!(scope.includes("alice") && scope.includes("bob"));
    assert!(!scope.includes("carol"));
}
