//! A boot from a system checkpoint reaches the state a full WAL replay
//! reaches, and no checkpoint, crash or rollback loses an acknowledged write.

use allsource_core::{
    domain::{
        entities::{Tenant, TenantQuotas, UsageMeter, egress::*, query_usage::*},
        repositories::TenantRepository,
        value_objects::{TenantId, system_stream::SystemDomain},
    },
    infrastructure::{
        persistence::{
            SystemBootstrap, SystemCheckpointSettings, SystemMetadataStore, SystemRepositories,
            SystemStoreOptions, VerifyOutcome, system_checkpoint,
        },
        repositories::EventSourcedTenantRepository,
    },
};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};
use tempfile::TempDir;

const TENANTS: [&str; 3] = ["checkpoint-a", "checkpoint-b", "checkpoint-c"];

fn settings() -> SystemCheckpointSettings {
    SystemCheckpointSettings::default()
}

async fn boot(dir: &Path, settings: SystemCheckpointSettings) -> SystemRepositories {
    let repos = SystemBootstrap::initialize_with(dir.to_path_buf(), None, settings)
        .await
        .unwrap();
    if let Some(written) = repos.checkpointer.wait_pending() {
        written.unwrap();
    }
    repos
}

fn tenant_id(name: &str) -> TenantId {
    TenantId::new(name.into()).unwrap()
}

fn uuid() -> String {
    uuid::Uuid::new_v4().hyphenated().to_string()
}

/// One round of the writes production sees: usage increments, metadata
/// merges, an egress lease chain, query admissions and a config change.
async fn traffic(repos: &SystemRepositories, round: u64) {
    let repo = &repos.tenant_repository;
    for name in TENANTS {
        let id = tenant_id(name);
        if repo.find_by_id(&id).await.unwrap().is_none() {
            let mut tenant =
                Tenant::new(id.clone(), name.into(), TenantQuotas::standard()).unwrap();
            tenant.update_metadata(json!({
                "quotas": {"queries_quota": 1_000_000, "queries_used": 0, "events_used": 0},
                "subscription": {"tier": "indie", "status": "active"},
            }));
            repo.create_initialized(tenant).await.unwrap();
            repo.mutate_egress(
                &id,
                EgressCommand::Configure {
                    included_bytes: 1 << 40,
                },
            )
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        }
        for _ in 0..5 {
            repo.increment_usage(&id, UsageMeter::Events, 3)
                .await
                .unwrap()
                .unwrap();
        }
        repo.merge_metadata(&id, json!({"round": round}))
            .await
            .unwrap()
            .unwrap();
        let lease = uuid();
        repo.mutate_egress(
            &id,
            EgressCommand::Lease {
                id: lease.clone(),
                bytes: 1 << 20,
            },
        )
        .await
        .unwrap()
        .unwrap()
        .unwrap();
        repo.mutate_egress(
            &id,
            EgressCommand::Settle {
                id: lease,
                bytes: 4096,
                next: Some(LeaseRequest {
                    id: uuid(),
                    bytes: 1 << 20,
                }),
            },
        )
        .await
        .unwrap()
        .unwrap()
        .unwrap();
        let admitted = repo
            .admit_query_usage(
                &id,
                QueryUsageRequest {
                    operation_id: format!("{}:{}", chrono::Utc::now().timestamp(), uuid()),
                    fingerprint: "c".repeat(64),
                    count: 1,
                    expected_period: 0,
                },
            )
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(admitted, QueryUsageDecision::Admitted { .. }));
    }
    repos
        .config_repository
        .set(&format!("round-{round}"), json!(round), Some("test"))
        .unwrap();
}

/// The tenant fold and config a full WAL replay reaches, without the fields
/// replay stamps with the clock.
fn replayed_state(dir: &Path) -> Value {
    let store = Arc::new(
        SystemMetadataStore::open(
            dir,
            SystemStoreOptions {
                load_checkpoint: false,
                ..SystemStoreOptions::default()
            },
        )
        .unwrap(),
    );
    state_of(&store)
}

fn state_of(store: &Arc<SystemMetadataStore>) -> Value {
    let tenants = EventSourcedTenantRepository::new(Arc::clone(store));
    let config = allsource_core::infrastructure::repositories::EventSourcedConfigRepository::new(
        Arc::clone(store),
    );
    state_from(&tenants, &config)
}

/// The state of a running repository set. A fresh repository over a live
/// store would boot from the store's resident events, which a checkpoint
/// has already folded out of.
fn live_state(repos: &SystemRepositories) -> Value {
    state_from(&repos.tenant_repository, &repos.config_repository)
}

fn state_from(
    tenants: &EventSourcedTenantRepository,
    config: &allsource_core::infrastructure::repositories::EventSourcedConfigRepository,
) -> Value {
    let mut fold = tenants.export_fold().unwrap().state;
    for tenant in fold["tenants"].as_array_mut().unwrap() {
        let tenant = tenant.as_object_mut().unwrap();
        tenant.remove("updated_at");
        tenant.remove("usage");
    }
    let mut entries: Vec<(String, Value)> = config
        .list()
        .into_iter()
        .map(|e| (e.key, e.value))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    json!({"tenants": fold, "config": entries})
}

fn booted_state(dir: &Path) -> (Value, Option<u64>, usize, usize) {
    let store = Arc::new(SystemMetadataStore::new(dir).unwrap());
    let covered = store.loaded_checkpoint().map(|c| c.covered_seq);
    let (resident, total) = (store.resident_events(), store.total_events());
    (state_of(&store), covered, resident, total)
}

fn tenant_boot_source(dir: &Path) -> &'static str {
    EventSourcedTenantRepository::new(Arc::new(SystemMetadataStore::new(dir).unwrap()))
        .boot_source()
}

fn checkpoints(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(dir.join(system_checkpoint::CHECKPOINT_DIR))
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

fn events_used(state: &Value, tenant: &str) -> u64 {
    state["tenants"]["tenants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == tenant)
        .unwrap()["metadata"]["quotas"]["events_used"]
        .as_u64()
        .unwrap()
}

#[tokio::test]
async fn a_checkpoint_boot_reaches_the_state_a_full_replay_reaches() {
    let directory = TempDir::new().unwrap();
    let dir = directory.path().join("system");
    {
        let repos = boot(&dir, settings()).await;
        traffic(&repos, 1).await;
        repos.checkpointer.checkpoint().unwrap().unwrap();
        traffic(&repos, 2).await;
    }
    let (state, covered, resident, total) = booted_state(&dir);
    assert!(covered.is_some(), "the boot must use the checkpoint");
    assert!(
        resident < total,
        "folded tenant events must not be resident: {resident} of {total}"
    );
    assert_eq!(state, replayed_state(&dir));
    assert_eq!(events_used(&state, TENANTS[0]), 30);
    assert_eq!(tenant_boot_source(&dir), "checkpoint tail");
}

#[tokio::test]
async fn writes_after_a_checkpoint_boot_survive_the_next_boot() {
    let directory = TempDir::new().unwrap();
    let dir = directory.path().join("system");
    {
        let repos = boot(&dir, settings()).await;
        traffic(&repos, 1).await;
        repos.checkpointer.checkpoint().unwrap().unwrap();
    }
    for round in 2..5 {
        let repos = boot(&dir, settings()).await;
        assert!(repos.system_store.loaded_checkpoint().is_some());
        traffic(&repos, round).await;
    }
    let (state, _, _, _) = booted_state(&dir);
    assert_eq!(state, replayed_state(&dir));
    assert_eq!(events_used(&state, TENANTS[1]), 4 * 15);

    let store = SystemMetadataStore::open(
        &dir,
        SystemStoreOptions {
            load_checkpoint: false,
            ..SystemStoreOptions::default()
        },
    )
    .unwrap();
    let entries = store.wal().read_all_entries().unwrap();
    let mut sequences: Vec<u64> = entries.iter().map(|entry| entry.sequence).collect();
    let count = sequences.len();
    sequences.dedup();
    assert_eq!(sequences.len(), count, "no WAL sequence may be reused");
}

#[tokio::test]
async fn a_corrupt_newest_checkpoint_falls_back_to_the_older_one() {
    let directory = TempDir::new().unwrap();
    let dir = directory.path().join("system");
    {
        let repos = boot(&dir, settings()).await;
        traffic(&repos, 1).await;
        repos.checkpointer.checkpoint().unwrap().unwrap();
        traffic(&repos, 2).await;
        repos.checkpointer.checkpoint().unwrap().unwrap();
        traffic(&repos, 3).await;
    }
    let files = checkpoints(&dir);
    assert_eq!(files.len(), system_checkpoint::RETAINED);
    let newest = files.last().unwrap();
    let older = system_checkpoint::read(&files[0]).unwrap().covered_seq;
    let mut bytes = std::fs::read(newest).unwrap();
    bytes[10] ^= 0x20;
    std::fs::write(newest, bytes).unwrap();

    let (state, covered, _, _) = booted_state(&dir);
    assert_eq!(covered, Some(older));
    assert_eq!(state, replayed_state(&dir));
    assert_eq!(tenant_boot_source(&dir), "checkpoint tail");
}

#[tokio::test]
async fn no_valid_checkpoint_replays_the_whole_wal() {
    let directory = TempDir::new().unwrap();
    let dir = directory.path().join("system");
    {
        let repos = boot(&dir, settings()).await;
        traffic(&repos, 1).await;
        repos.checkpointer.checkpoint().unwrap().unwrap();
        traffic(&repos, 2).await;
    }
    for file in checkpoints(&dir) {
        let bytes = std::fs::read(&file).unwrap();
        std::fs::write(&file, &bytes[..bytes.len() / 2]).unwrap();
    }
    let (state, covered, resident, total) = booted_state(&dir);
    assert_eq!(covered, None);
    assert_eq!(resident, total);
    assert_eq!(state, replayed_state(&dir));
}

#[tokio::test]
async fn an_interrupted_checkpoint_write_is_ignored_and_cleaned_up() {
    let directory = TempDir::new().unwrap();
    let dir = directory.path().join("system");
    let covered = {
        let repos = boot(&dir, settings()).await;
        traffic(&repos, 1).await;
        let covered = repos
            .checkpointer
            .checkpoint()
            .unwrap()
            .unwrap()
            .covered_seq;
        traffic(&repos, 2).await;
        covered
    };
    let checkpoint_dir = dir.join(system_checkpoint::CHECKPOINT_DIR);
    let torn = checkpoint_dir.join(format!(".checkpoint-{:016x}.json.tmp", covered + 50));
    std::fs::write(&torn, b"{\"format\":1,\"covered_seq\"").unwrap();

    let (state, loaded, _, _) = booted_state(&dir);
    assert_eq!(loaded, Some(covered));
    assert_eq!(state, replayed_state(&dir));

    let repos = boot(&dir, settings()).await;
    repos.checkpointer.checkpoint().unwrap();
    assert!(
        !torn.exists(),
        "a later checkpoint write removes the torn file"
    );
}

#[tokio::test]
async fn a_wal_shorter_than_the_checkpoint_saw_is_replayed_whole() {
    let directory = TempDir::new().unwrap();
    let dir = directory.path().join("system");
    {
        let repos = boot(&dir, settings()).await;
        traffic(&repos, 1).await;
        repos.checkpointer.checkpoint().unwrap().unwrap();
    }
    let newest = checkpoints(&dir).pop().unwrap();
    let checkpoint = system_checkpoint::read(&newest).unwrap();
    let segment = checkpoint
        .segments
        .iter()
        .filter(|s| s.len > 0)
        .max_by_key(|s| s.len)
        .unwrap();
    let path = dir.join("wal").join(&segment.name);
    let bytes = std::fs::read(&path).unwrap();
    let cut = bytes[..bytes.len() - 1]
        .iter()
        .rposition(|b| *b == b'\n')
        .unwrap()
        + 1;
    std::fs::write(&path, &bytes[..cut]).unwrap();

    let (state, covered, resident, total) = booted_state(&dir);
    assert_eq!(covered, None, "a WAL that contradicts the checkpoint wins");
    assert_eq!(resident, total);
    assert_eq!(state, replayed_state(&dir));
    assert!(
        checkpoints(&dir).is_empty(),
        "the contradicted checkpoint is moved aside"
    );

    let store = SystemMetadataStore::new(&dir).unwrap();
    assert!(
        store.wal().current_sequence() >= checkpoint.covered_seq,
        "a sequence the rejected checkpoint covered is never handed out again"
    );
}

#[tokio::test]
async fn writes_from_a_binary_without_checkpoints_reach_the_next_checkpoint_boot() {
    let directory = TempDir::new().unwrap();
    let dir = directory.path().join("system");
    {
        let repos = boot(&dir, settings()).await;
        traffic(&repos, 1).await;
        repos.checkpointer.checkpoint().unwrap().unwrap();
    }
    {
        let rollback = SystemCheckpointSettings {
            enabled: false,
            ..settings()
        };
        let repos = boot(&dir, rollback).await;
        assert!(repos.system_store.loaded_checkpoint().is_none());
        traffic(&repos, 2).await;
    }
    let (state, covered, _, _) = booted_state(&dir);
    assert!(covered.is_some());
    assert_eq!(state, replayed_state(&dir));
    assert_eq!(events_used(&state, TENANTS[2]), 30);
}

#[tokio::test]
async fn checkpoints_off_writes_none() {
    let directory = TempDir::new().unwrap();
    let dir = directory.path().join("system");
    let off = SystemCheckpointSettings {
        enabled: false,
        ..settings()
    };
    let repos = boot(&dir, off).await;
    traffic(&repos, 1).await;
    assert!(repos.checkpointer.checkpoint().unwrap().is_none());
    assert!(checkpoints(&dir).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn writes_racing_checkpoints_are_all_kept() {
    let directory = TempDir::new().unwrap();
    let dir = directory.path().join("system");
    {
        let repos = boot(&dir, settings()).await;
        traffic(&repos, 1).await;
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let checkpointer = Arc::clone(&repos.checkpointer);
        let flag = Arc::clone(&stop);
        let checkpoints = std::thread::spawn(move || {
            let mut written = 0;
            while !flag.load(std::sync::atomic::Ordering::Relaxed) {
                if checkpointer.checkpoint().unwrap().is_some() {
                    written += 1;
                }
            }
            written
        });
        let mut writers = Vec::new();
        for name in TENANTS {
            let repo = Arc::clone(&repos.tenant_repository);
            writers.push(tokio::spawn(async move {
                for _ in 0..200 {
                    repo.increment_usage(&tenant_id(name), UsageMeter::Events, 1)
                        .await
                        .unwrap()
                        .unwrap();
                }
            }));
        }
        for writer in writers {
            writer.await.unwrap();
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(checkpoints.join().unwrap() > 0);
    }
    let (state, covered, _, _) = booted_state(&dir);
    assert!(covered.is_some());
    for name in TENANTS {
        assert_eq!(events_used(&state, name), 15 + 200);
    }
    assert_eq!(state, replayed_state(&dir));
    assert_eq!(tenant_boot_source(&dir), "checkpoint tail");
}

/// Replace every checkpoint with `edit` applied to the newest one.
fn forge(dir: &Path, edit: impl FnOnce(&mut Value)) -> u64 {
    let newest = checkpoints(dir).pop().unwrap();
    let mut forged = system_checkpoint::read(&newest).unwrap();
    edit(
        &mut forged
            .folds
            .get_mut(SystemDomain::Tenant.as_str())
            .unwrap()
            .state,
    );
    for file in checkpoints(dir) {
        std::fs::remove_file(file).unwrap();
    }
    system_checkpoint::write(&dir.join(system_checkpoint::CHECKPOINT_DIR), &forged).unwrap();
    forged.covered_seq
}

async fn checkpointed_with_tail(dir: &Path) {
    let repos = boot(dir, settings()).await;
    traffic(&repos, 1).await;
    repos.checkpointer.checkpoint().unwrap().unwrap();
    traffic(&repos, 2).await;
}

#[tokio::test]
async fn a_fold_that_does_not_round_trip_is_replayed_from_the_wal() {
    let directory = TempDir::new().unwrap();
    let dir = directory.path().join("system");
    checkpointed_with_tail(&dir).await;
    forge(&dir, |state| {
        state["tenants"][0]["field_from_a_newer_binary"] = json!("kept");
        state["tenants"][0]["metadata"]["quotas"]["events_used"] = json!(999);
    });
    let (state, covered, _, _) = booted_state(&dir);
    assert!(covered.is_some(), "the checkpoint still loads");
    assert_eq!(
        state,
        replayed_state(&dir),
        "but the tenant fold is not used"
    );
    assert_eq!(tenant_boot_source(&dir), "wal after unusable checkpoint");
}

#[tokio::test]
async fn a_fold_that_ignored_what_this_binary_interprets_is_replayed_from_the_wal() {
    let directory = TempDir::new().unwrap();
    let dir = directory.path().join("system");
    checkpointed_with_tail(&dir).await;
    forge(&dir, |state| {
        state["uninterpreted"] = json!(["_system.tenant.updated#metadata"]);
        state["tenants"][0]["metadata"]["quotas"]["events_used"] = json!(999);
    });
    let (state, _, _, _) = booted_state(&dir);
    assert_eq!(state, replayed_state(&dir));
    assert_eq!(tenant_boot_source(&dir), "wal after unusable checkpoint");
}

#[tokio::test]
async fn events_this_binary_ignores_are_recorded_in_the_fold() {
    let directory = TempDir::new().unwrap();
    let dir = directory.path().join("system");
    let repos = boot(&dir, settings()).await;
    traffic(&repos, 1).await;
    repos
        .system_store
        .append_system_event(
            "_system.tenant.renamed_in_a_later_release",
            allsource_core::domain::value_objects::system_stream::system_entity_id_value(
                SystemDomain::Tenant,
                TENANTS[0],
            ),
            json!({"name": "later"}),
            None,
        )
        .unwrap();
    repos
        .system_store
        .append_system_event(
            "_system.tenant.updated",
            allsource_core::domain::value_objects::system_stream::system_entity_id_value(
                SystemDomain::Tenant,
                TENANTS[0],
            ),
            json!({"plan_from_a_later_release": "gold"}),
            None,
        )
        .unwrap();
    let path = repos
        .checkpointer
        .checkpoint()
        .unwrap()
        .unwrap()
        .path
        .unwrap();
    let fold = &system_checkpoint::read(&path).unwrap().folds[SystemDomain::Tenant.as_str()];
    assert_eq!(
        fold.state["uninterpreted"],
        json!([
            "_system.tenant.renamed_in_a_later_release",
            "_system.tenant.updated#plan_from_a_later_release"
        ])
    );
}

#[tokio::test]
async fn verify_accepts_a_true_checkpoint_and_rejects_a_forged_one() {
    let directory = TempDir::new().unwrap();
    let dir = directory.path().join("system");
    {
        let repos = boot(&dir, settings()).await;
        traffic(&repos, 1).await;
        repos.checkpointer.checkpoint().unwrap().unwrap();
        traffic(&repos, 2).await;
    }
    {
        let repos = boot(&dir, settings()).await;
        let outcome = repos.checkpointer.verify_loaded().unwrap();
        assert!(
            matches!(outcome, VerifyOutcome::Matches { .. }),
            "{outcome:?}"
        );
    }
    let forged = forge(&dir, |state| {
        state["tenants"][0]["metadata"]["quotas"]["events_used"] = json!(1);
    });

    let repos = boot(&dir, settings()).await;
    assert_eq!(
        repos.system_store.loaded_checkpoint().unwrap().covered_seq,
        forged
    );
    let outcome = repos.checkpointer.verify_loaded().unwrap();
    assert!(
        matches!(outcome, VerifyOutcome::Rejected { .. }),
        "{outcome:?}"
    );
    traffic(&repos, 3).await;
    assert!(
        repos.checkpointer.checkpoint().is_err(),
        "a rejected checkpointer writes nothing more"
    );
    assert!(checkpoints(&dir).is_empty());
    drop(repos);
    let (_, covered, _, _) = booted_state(&dir);
    assert_eq!(
        covered, None,
        "a rejected checkpoint is not booted from again"
    );
}

#[tokio::test]
async fn folded_resident_drops_at_checkpoint_persist() {
    let directory = TempDir::new().unwrap();
    let dir = directory.path().join("system");
    let store = {
        let repos = boot(&dir, settings()).await;
        traffic(&repos, 1).await;
        let store = Arc::clone(&repos.system_store);
        let tenant_events = store.count_stream(SystemDomain::Tenant);
        assert_eq!(store.resident_events(), store.total_events());
        repos.checkpointer.checkpoint().unwrap().unwrap();
        assert_eq!(
            store.resident_events(),
            store.total_events() - tenant_events,
            "only the tenant events the checkpoint folded leave memory"
        );
        assert_eq!(store.count_stream(SystemDomain::Tenant), tenant_events);
        traffic(&repos, 2).await;
        let live = live_state(&repos);
        drop(repos);
        (store, live)
    };
    let (store, live) = store;
    drop(store);
    let (state, covered, _, _) = booted_state(&dir);
    assert!(covered.is_some());
    assert_eq!(state, live);
    assert_eq!(state, replayed_state(&dir));
}

#[tokio::test]
async fn checkpoint_failure_keeps_folded_resident() {
    let directory = TempDir::new().unwrap();
    let dir = directory.path().join("system");
    let repos = boot(&dir, settings()).await;
    traffic(&repos, 1).await;
    let store = Arc::clone(&repos.system_store);
    let resident = store.resident_events();

    let checkpoint_dir = store.checkpoint_dir();
    let _ = std::fs::remove_dir_all(&checkpoint_dir);
    std::fs::write(&checkpoint_dir, b"a file where the directory belongs").unwrap();
    assert!(repos.checkpointer.checkpoint().is_err());
    assert_eq!(store.resident_events(), resident);

    std::fs::remove_file(&checkpoint_dir).unwrap();
    repos.checkpointer.checkpoint().unwrap().unwrap();
    assert!(store.resident_events() < resident);
    drop(repos);
    let (state, covered, _, _) = booted_state(&dir);
    assert!(covered.is_some());
    assert_eq!(state, replayed_state(&dir));
}
