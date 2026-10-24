use allsource_core::{
    domain::entities::Event,
    store::{
        EventStore, EventStoreConfig,
        read_model::{MAX_PAGE_BYTES, ModelCommit},
    },
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn append(store: &EventStore, tenant: &str, entity: &str) {
    store
        .ingest(
            &Event::from_strings(
                "item.changed".into(),
                entity.into(),
                tenant.into(),
                json!({"id":entity}),
                None,
            )
            .unwrap(),
        )
        .unwrap();
}

fn commit(store: &EventStore, revision: u64, changes: BTreeMap<String, Option<Value>>) -> u64 {
    let head = store.tenant_feed("alice", None, 0).unwrap().head;
    store
        .commit_read_model(
            "alice",
            "board",
            ModelCommit {
                expected_revision: revision,
                checkpoint: head,
                ready: true,
                reset: false,
                changes,
            },
        )
        .unwrap()
}

#[test]
fn feed_is_tenant_scoped_bounded_and_ignores_projection_writes() {
    let store = EventStore::new();
    append(&store, "bob", "private");
    append(&store, "alice", "one");
    append(&store, "alice", "two");
    let first = store.tenant_feed("alice", None, 1).unwrap();
    assert_eq!(first.events.len(), 1);
    assert_eq!(first.events[0].entity_id, "one");
    assert!(!first.caught_up);
    let second = store.tenant_feed("alice", Some(first.cursor), 1).unwrap();
    assert_eq!(second.events[0].entity_id, "two");
    assert!(second.caught_up);
    let head = second.head;
    commit(
        &store,
        0,
        BTreeMap::from([("q/one".into(), Some(json!({"status":"queued"})))]),
    );
    assert_eq!(store.tenant_feed("alice", None, 0).unwrap().head, head);
    assert!(
        store
            .tenant_feed("alice", Some(head), 500)
            .unwrap()
            .events
            .is_empty()
    );
    assert!(store.tenant_feed("alice", None, 501).is_err());
    assert!(
        store
            .read_model_page("bob", "board", "", None, 100, None)
            .unwrap()
            .rows
            .is_empty()
    );
}

#[test]
fn checkpoint_and_projection_commit_atomically_and_reject_stale_writers() {
    let store = EventStore::new();
    append(&store, "alice", "one");
    let head = store.tenant_feed("alice", None, 0).unwrap().head;
    commit(
        &store,
        0,
        BTreeMap::from([("q/one".into(), Some(json!(1)))]),
    );
    let rejected = ModelCommit {
        expected_revision: 0,
        checkpoint: head,
        ready: true,
        reset: false,
        changes: BTreeMap::from([("q/one".into(), None)]),
    };
    assert!(store.commit_read_model("alice", "board", rejected).is_err());
    let page = store
        .read_model_page("alice", "board", "q/", None, 100, None)
        .unwrap();
    assert_eq!(page.rows["q/one"], 1);
    assert_eq!(page.checkpoint, Some(head));
    assert_eq!(page.revision, 1);
    let oversized = ModelCommit {
        expected_revision: 1,
        checkpoint: head,
        ready: true,
        reset: false,
        changes: BTreeMap::from([
            ("q/one".into(), None),
            ("q/two".into(), Some(json!("x".repeat(128 * 1024)))),
        ]),
    };
    assert!(
        store
            .commit_read_model("alice", "board", oversized)
            .is_err()
    );
    assert_eq!(
        store
            .read_model_page("alice", "board", "q/", None, 100, None)
            .unwrap()
            .rows["q/one"],
        1
    );
}

#[test]
fn pages_bound_bytes_and_checkpoint_only_updates_preserve_content_revision() {
    let store = EventStore::new();
    append(&store, "alice", "one");
    let rows = (0..40)
        .map(|i| (format!("q/{i:03}"), Some(json!("x".repeat(4000)))))
        .collect();
    commit(&store, 0, rows);
    let page = store
        .read_model_page("alice", "board", "q/", None, 500, None)
        .unwrap();
    assert!(serde_json::to_vec(&page).unwrap().len() <= MAX_PAGE_BYTES);
    assert!(page.next.is_some());
    let next = store
        .read_model_page("alice", "board", "q/", page.next.as_deref(), 500, None)
        .unwrap();
    assert_eq!(page.rows.len() + next.rows.len(), 40);
    append(&store, "alice", "two");
    commit(&store, 1, BTreeMap::new());
    let same = store
        .read_model_page(
            "alice",
            "board",
            "q/",
            None,
            500,
            Some(page.content_revision),
        )
        .unwrap();
    assert!(same.unchanged);
    assert!(same.rows.is_empty());
    assert_eq!(same.revision, 2);
    commit(&store, 2, BTreeMap::from([("q/000".into(), None)]));
    assert!(
        !store
            .read_model_page(
                "alice",
                "board",
                "q/",
                None,
                500,
                Some(page.content_revision)
            )
            .unwrap()
            .unchanged
    );
}

#[test]
fn durable_journal_survives_restart_and_old_feed_cursor_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let old_head;
    {
        let store = EventStore::with_config(EventStoreConfig::with_persistence(dir.path()));
        append(&store, "alice", "one");
        commit(
            &store,
            0,
            BTreeMap::from([("q/one".into(), Some(json!({"status":"queued"})))]),
        );
        old_head = store.tenant_feed("alice", None, 0).unwrap().head;
        store.checkpoint().unwrap();
    }
    let store = EventStore::with_config(EventStoreConfig::with_persistence(dir.path()));
    let page = store
        .read_model_page("alice", "board", "q/", None, 100, None)
        .unwrap();
    assert_eq!(page.rows["q/one"]["status"], "queued");
    assert_eq!(page.checkpoint, Some(old_head));
    assert!(store.tenant_feed("alice", Some(old_head), 100).is_err());
    let head = store.tenant_feed("alice", None, 0).unwrap().head;
    assert_ne!(head.epoch, old_head.epoch);
}

#[test]
fn evicting_one_tenant_preserves_other_tenants_feed_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let store = EventStore::with_config(EventStoreConfig::with_persistence(dir.path()));
    append(&store, "alice", "alice-one");
    append(&store, "bob", "bob-one");
    store.checkpoint().unwrap();
    store.flush_storage().unwrap();
    let alice = store.tenant_feed("alice", None, 0).unwrap().head;
    let bob = store.tenant_feed("bob", None, 0).unwrap().head;
    assert!(alice.epoch < 1_u64 << 53);
    store.evict_tenant("alice");
    assert!(!store.is_tenant_loaded("alice"));
    append(&store, "bob", "bob-two");
    let next = store.tenant_feed("bob", Some(bob), 100).unwrap();
    assert_eq!(next.events.len(), 1);
    assert_eq!(next.events[0].entity_id, "bob-two");
    assert!(store.tenant_feed("alice", Some(alice), 100).is_err());
}

#[test]
fn model_cache_eviction_restores_durable_state() {
    let store = EventStore::new();
    append(&store, "alice", "one");
    let checkpoint = store.tenant_feed("alice", None, 0).unwrap().head;
    for index in 0..40 {
        store
            .commit_read_model(
                "alice",
                &format!("board-{index}"),
                ModelCommit {
                    expected_revision: 0,
                    checkpoint,
                    ready: true,
                    reset: false,
                    changes: BTreeMap::from([("q/one".into(), Some(json!(index)))]),
                },
            )
            .unwrap();
    }
    assert_eq!(
        store
            .read_model_page("alice", "board-0", "q/", None, 10, None)
            .unwrap()
            .rows["q/one"],
        0
    );
    assert_eq!(
        store
            .read_model_page("alice", "board-39", "q/", None, 10, None)
            .unwrap()
            .rows["q/one"],
        39
    );
}

#[test]
fn same_model_name_has_independent_tenant_write_revisions() {
    let store = EventStore::new();
    for tenant in ["alice", "bob"] {
        append(&store, tenant, &format!("{tenant}-one"));
        let checkpoint = store.tenant_feed(tenant, None, 0).unwrap().head;
        let result = store.commit_read_model(
            tenant,
            "board",
            ModelCommit {
                expected_revision: 0,
                checkpoint,
                ready: true,
                reset: false,
                changes: BTreeMap::from([("owner".into(), Some(json!(tenant)))]),
            },
        );
        assert_eq!(result.unwrap(), 1, "independent revision for {tenant}");
        assert_eq!(
            store
                .read_model_page(tenant, "board", "", None, 10, None)
                .unwrap()
                .rows["owner"],
            tenant
        );
    }
}
