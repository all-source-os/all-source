//! Measures the system-store boot replay on a synthetic tenant stream shaped
//! like production's: one usage counter `UPDATED` event per increment, each
//! carrying the tenant's whole metadata blob. Run by hand, in release:
//!
//! ```sh
//! SYSTEM_REPLAY_EVENTS=300000 SYSTEM_REPLAY_METADATA_BYTES=1024 SYSTEM_REPLAY_SETTLES=0 \
//!   cargo test --release --test system_replay_cost -- --ignored --nocapture
//! ```

use allsource_core::{
    domain::{
        entities::{
            Event,
            egress::{EgressCommand, EgressEvent, LeaseRequest},
        },
        repositories::TenantRepository,
        value_objects::{
            TenantId,
            system_stream::{
                SystemDomain, system_entity_id_value, system_event_type, system_tenant_id,
                tenant_events,
            },
        },
    },
    infrastructure::{
        persistence::{SystemMetadataStore, WALConfig, WriteAheadLog},
        repositories::EventSourcedTenantRepository,
    },
};
use chrono::Datelike;
use serde_json::json;
use std::{sync::Arc, time::Instant};

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn lease_id(tenant: usize, round: usize) -> String {
    uuid::Uuid::from_u64_pair(tenant as u64, round as u64)
        .hyphenated()
        .to_string()
}

#[test]
#[ignore = "measurement, not a correctness check"]
fn tenant_stream_replay_cost() {
    let events = env_usize("SYSTEM_REPLAY_EVENTS", 300_000);
    let metadata_bytes = env_usize("SYSTEM_REPLAY_METADATA_BYTES", 1024);
    let tenants = env_usize("SYSTEM_REPLAY_TENANTS", 20);

    let dir = tempfile::tempdir().unwrap();
    let system_dir = dir.path().join("__system");
    let wal = WriteAheadLog::new(
        system_dir.join("wal"),
        WALConfig {
            max_file_size: 16 * 1024 * 1024,
            sync_on_write: false,
            max_wal_files: 5,
            compress: false,
            ..WALConfig::default()
        },
    )
    .unwrap();
    let settles = env_usize("SYSTEM_REPLAY_SETTLES", 0);
    let filler = "x".repeat(metadata_bytes);
    for t in 0..tenants {
        let id = format!("tenant-{t}");
        wal.append(Event::new(
            system_event_type(tenant_events::CREATED),
            system_entity_id_value(SystemDomain::Tenant, &id),
            system_tenant_id(),
            json!({"name": id, "quotas": {}, "metadata": {}}),
        ))
        .unwrap();
    }
    let started = Instant::now();
    for n in 0..events {
        let id = format!("tenant-{}", n % tenants);
        wal.append(Event::new(
            system_event_type(tenant_events::UPDATED),
            system_entity_id_value(SystemDomain::Tenant, &id),
            system_tenant_id(),
            json!({
                "name": id,
                "description": null,
                "is_demo": false,
                "metadata": {"events_used": n, "queries_used": n / 2, "notes": filler},
            }),
        ))
        .unwrap();
    }
    // One settle per second per tenant, each opening the next lease, as a
    // gateway draining a lease does; settle receipts are kept for an hour.
    let now = chrono::Utc::now();
    let period = u32::try_from(now.year()).unwrap() * 12 + now.month();
    let egress = |tenant: &str, at: i64, command: EgressCommand| {
        Event::new(
            system_event_type(tenant_events::UPDATED),
            system_entity_id_value(SystemDomain::Tenant, tenant),
            system_tenant_id(),
            json!({ "egress_ledger_v1": EgressEvent { command, at, period } }),
        )
    };
    let start_at = now.timestamp() - i64::try_from(settles.div_ceil(tenants)).unwrap() - 2;
    for t in 0..tenants.min(settles) {
        let id = format!("tenant-{t}");
        let configure = EgressCommand::Configure {
            included_bytes: 100_000_000_000_000,
        };
        wal.append(egress(&id, start_at, configure)).unwrap();
        let lease = EgressCommand::Lease {
            id: lease_id(t, 0),
            bytes: 1024 * 1024,
        };
        wal.append(egress(&id, start_at, lease)).unwrap();
    }
    for n in 0..settles {
        let t = n % tenants;
        let round = n / tenants;
        let settle = EgressCommand::Settle {
            id: lease_id(t, round),
            bytes: 4096,
            next: Some(LeaseRequest {
                id: lease_id(t, round + 1),
                bytes: 1024 * 1024,
            }),
        };
        let at = start_at + i64::try_from(round).unwrap() + 1;
        wal.append(egress(&format!("tenant-{t}"), at, settle))
            .unwrap();
    }
    wal.sync().unwrap();
    drop(wal);
    eprintln!(
        "wrote {events} updates and {settles} settles in {:?}",
        started.elapsed()
    );

    let started = Instant::now();
    let store = Arc::new(SystemMetadataStore::new(&system_dir).unwrap());
    let stage1 = started.elapsed();
    let started = Instant::now();
    let repository = EventSourcedTenantRepository::new(store.clone());
    let stage2 = started.elapsed();
    if settles > 0 {
        let snapshot = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(repository.egress_snapshot(&TenantId::new("tenant-0".into()).unwrap()))
            .unwrap()
            .expect("replayed egress account is valid");
        assert_eq!(snapshot.used_bytes, 4096 * settles.div_ceil(tenants) as u64);
    }
    drop(repository);

    eprintln!(
        "events={} metadata_bytes={metadata_bytes} stage1_recover={stage1:?} stage2_tenant_rebuild={stage2:?} per_event_stage2={:?}",
        store.total_events(),
        stage2 / u32::try_from(store.total_events().max(1)).unwrap()
    );
}
