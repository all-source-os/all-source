//! Reading one entity out of a tenant's archive must not miss an event.
//!
//! This is what lets a conditional publish succeed against a tenant too
//! large to be made resident. It skips row groups whose `entity_id`
//! statistics exclude the target, so the property under test is the
//! dangerous direction: a group wrongly skipped turns a present event into a
//! missing one, and no caller can tell that apart from the entity genuinely
//! having no such event (gh#321).

use allsource_core::{
    domain::entities::Event,
    infrastructure::persistence::{ParquetStorage, ParquetStorageConfig},
};
use std::path::Path;
use tempfile::TempDir;

/// Interleaves several entities across many single-batch files, so pruning
/// has both files it can skip entirely and files it must decode.
fn seed(root: &Path, tenant: &str, entities: &[&str], events_each: usize) {
    let storage = ParquetStorage::with_config(
        root,
        ParquetStorageConfig {
            batch_size: 1,
            ..Default::default()
        },
    )
    .unwrap();
    for round in 0..events_each {
        for entity in entities {
            storage
                .append_event(
                    Event::from_strings(
                        "synthetic.updated".into(),
                        (*entity).into(),
                        tenant.into(),
                        serde_json::json!({ "round": round }),
                        None,
                    )
                    .unwrap(),
                )
                .unwrap();
            storage.flush().unwrap();
        }
    }
}

fn rounds_for(root: &Path, tenant: &str, entity: &str) -> Vec<i64> {
    let (events, skipped) = ParquetStorage::new(root)
        .unwrap()
        .load_entity_events_for_tenant(tenant, entity)
        .unwrap();
    assert_eq!(skipped, 0, "no file in this fixture is unreadable");
    let mut rounds: Vec<i64> = events
        .iter()
        .map(|event| event.payload["round"].as_i64().unwrap())
        .collect();
    assert!(
        events.iter().all(|event| event.entity_id_str() == entity),
        "an entity-scoped read returned another entity's events"
    );
    rounds.sort_unstable();
    rounds
}

#[test]
fn every_event_for_the_entity_is_found_across_many_files() {
    let directory = TempDir::new().unwrap();
    let tenant = "synthetic-entity-read";
    let entities = ["alpha", "beta", "gamma", "delta"];
    seed(directory.path(), tenant, &entities, 7);

    for entity in entities {
        assert_eq!(
            rounds_for(directory.path(), tenant, entity),
            (0..7).collect::<Vec<i64>>(),
            "pruning skipped a row group that held {entity}"
        );
    }
}

#[test]
fn an_entity_with_no_events_reads_empty_rather_than_failing() {
    let directory = TempDir::new().unwrap();
    let tenant = "synthetic-entity-absent";
    seed(directory.path(), tenant, &["alpha"], 3);

    assert!(rounds_for(directory.path(), tenant, "never-written").is_empty());
}

#[test]
fn an_entity_id_that_sorts_between_two_others_is_still_found() {
    // Byte-wise bounds are what the row groups carry. An id ordering between
    // the ids around it is the case where a min/max test can admit a group
    // that does not hold it, and must still never exclude one that does.
    let directory = TempDir::new().unwrap();
    let tenant = "synthetic-entity-ordering";
    seed(directory.path(), tenant, &["aaa", "mmm", "zzz"], 4);

    assert_eq!(
        rounds_for(directory.path(), tenant, "mmm"),
        (0..4).collect::<Vec<i64>>()
    );
}

#[test]
fn an_unknown_tenant_reads_empty() {
    let directory = TempDir::new().unwrap();
    assert!(rounds_for(directory.path(), "synthetic-no-such-tenant", "alpha").is_empty());
}

#[test]
fn an_unreadable_file_is_counted_rather_than_failing_the_read() {
    let directory = TempDir::new().unwrap();
    let tenant = "synthetic-entity-corrupt";
    seed(directory.path(), tenant, &["alpha"], 5);
    std::fs::write(
        directory.path().join(tenant).join("events-bad.parquet"),
        "not a parquet file",
    )
    .unwrap();

    let (events, skipped) = ParquetStorage::new(directory.path())
        .unwrap()
        .load_entity_events_for_tenant(tenant, "alpha")
        .unwrap();

    assert_eq!(
        skipped, 1,
        "a corrupt file must be reported, not silently absorbed"
    );
    assert_eq!(
        events.len(),
        5,
        "the readable files must still answer the query"
    );
}
