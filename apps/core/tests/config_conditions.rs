//! Durable conditional config writes used by customer connection provisioning.
use allsource_core::{
    domain::value_objects::system_stream::SystemDomain,
    error::AllSourceError,
    infrastructure::{
        persistence::SystemMetadataStore,
        repositories::event_sourced_config_repository::{
            ConfigCondition, EventSourcedConfigRepository,
        },
    },
};
use serde_json::json;
use std::sync::{Arc, Barrier};
use tempfile::TempDir;

#[test]
fn concurrent_initialization_has_one_winner() {
    let directory = TempDir::new().unwrap();
    let store = Arc::new(SystemMetadataStore::new(directory.path().join("system")).unwrap());
    let repo = Arc::new(EventSourcedConfigRepository::new(store));
    let barrier = Arc::new(Barrier::new(12));
    let handles: Vec<_> = (0..12)
        .map(|value| {
            let repo = Arc::clone(&repo);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                repo.set_conditionally(
                    "workspace-owner",
                    json!({"subject": value}),
                    None,
                    Some(&ConfigCondition::Absent {}),
                )
            })
        })
        .collect();
    let mut winners = Vec::new();
    for handle in handles {
        match handle.join().unwrap() {
            Ok(entry) => winners.push(entry),
            Err(AllSourceError::ConcurrencyError(_)) => {}
            Err(error) => panic!("unexpected write failure: {error}"),
        }
    }
    assert_eq!(winners.len(), 1);
    assert_eq!(repo.get("workspace-owner").unwrap().value, winners[0].value);
}

#[test]
fn stale_revision_cannot_restore_a_consumed_or_recreated_record() {
    let directory = TempDir::new().unwrap();
    let store = Arc::new(SystemMetadataStore::new(directory.path().join("system")).unwrap());
    let repo = EventSourcedConfigRepository::new(store);
    let initial = repo
        .set_conditionally(
            "request",
            json!({"used": false}),
            None,
            Some(&ConfigCondition::Absent {}),
        )
        .unwrap();
    let condition = ConfigCondition::Revision {
        revision: initial.revision,
    };
    let consumed = repo
        .set_conditionally("request", json!({"used": true}), None, Some(&condition))
        .unwrap();
    assert_ne!(initial.revision, consumed.revision);
    assert!(matches!(
        repo.set_conditionally("request", json!({"used": false}), None, Some(&condition)),
        Err(AllSourceError::ConcurrencyError(_))
    ));

    // Legacy writes participate in the same revision boundary, even when they
    // restore the old JSON value or delete and recreate the key.
    repo.set("request", initial.value.clone(), None).unwrap();
    assert!(matches!(
        repo.set_conditionally("request", json!({"used": true}), None, Some(&condition)),
        Err(AllSourceError::ConcurrencyError(_))
    ));
    repo.delete("request", None).unwrap();
    assert!(matches!(
        repo.set_conditionally("request", json!({"used": true}), None, Some(&condition)),
        Err(AllSourceError::ConcurrencyError(_))
    ));
    repo.set("request", initial.value, None).unwrap();
    assert!(matches!(
        repo.set_conditionally("request", json!({"used": true}), None, Some(&condition)),
        Err(AllSourceError::ConcurrencyError(_))
    ));
}

#[test]
fn recovery_preserves_revision_and_failed_condition_does_not_append() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("system");
    let entry = {
        let store = Arc::new(SystemMetadataStore::new(&path).unwrap());
        let repo = EventSourcedConfigRepository::new(Arc::clone(&store));
        let entry = repo
            .set_conditionally(
                "grant-slot",
                json!(1),
                None,
                Some(&ConfigCondition::Absent {}),
            )
            .unwrap();
        assert!(
            repo.set_conditionally(
                "grant-slot",
                json!(2),
                None,
                Some(&ConfigCondition::Absent {})
            )
            .is_err()
        );
        assert_eq!(store.read_stream(SystemDomain::Config).len(), 1);
        entry
    };
    let store = Arc::new(SystemMetadataStore::new(&path).unwrap());
    assert_eq!(store.read_stream(SystemDomain::Config).len(), 1);
    let repo = EventSourcedConfigRepository::new(store);
    let recovered = repo.get("grant-slot").unwrap();
    assert_eq!(entry.revision, recovered.revision);
    assert_eq!(entry.value, recovered.value);
    let updated = repo
        .set_conditionally(
            "grant-slot",
            json!(3),
            None,
            Some(&ConfigCondition::Revision {
                revision: recovered.revision,
            }),
        )
        .unwrap();
    assert_ne!(recovered.revision, updated.revision);
}
