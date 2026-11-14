//! Deterministic in-process replay of the v1 API over a fixture volume.
//!
//! The router is `build_v1_router`, the same stack `serve_v1` serves. Requests
//! go through it with `tower::oneshot`, on a current-thread runtime, with no
//! checkpoint loop and no memory upkeep task.
//!
//! Usage (run on a private clone of the fixture, never on the fixture itself):
//!   heap_replay --volume <abs dir> --scenario main|eviction [--heap-profile <abs file>]
//!
//! `--heap-profile` needs the `heap-profile` feature on linux.
//!
//! Prints one line: `{"heap_replay": {...}}`. Built by the evaluator in
//! `tooling/core-heap-replay`; see that crate for the gates.

use allsource_core::{
    AuthManager, Event, ParquetStorage, RateLimiter, Role,
    api_v1::NodeRole,
    infrastructure::{
        di::ContainerBuilder, repositories::InMemoryTenantRepository, web::api_v1::build_v1_router,
    },
    rate_limit::RateLimitConfig,
    store::{EventStore, EventStoreConfig},
};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
use tower::ServiceExt;

#[cfg(not(all(feature = "heap-profile", target_os = "linux")))]
#[global_allocator]
static GLOBAL: cap::Cap<std::alloc::System> = cap::Cap::new(std::alloc::System, usize::MAX);

#[cfg(all(feature = "heap-profile", target_os = "linux"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(all(feature = "heap-profile", target_os = "linux"))]
#[allow(non_upper_case_globals)]
#[expect(
    unsafe_code,
    reason = "jemalloc reads malloc_conf from the binary's symbol table"
)]
#[unsafe(export_name = "malloc_conf")]
pub static malloc_conf: &[u8] = b"prof:true,prof_active:true,lg_prof_sample:19\0";

#[cfg(not(all(feature = "heap-profile", target_os = "linux")))]
fn heap_live_bytes() -> u64 {
    GLOBAL.allocated() as u64
}

#[cfg(not(all(feature = "heap-profile", target_os = "linux")))]
fn peak_live_bytes() -> u64 {
    GLOBAL.max_allocated() as u64
}

#[cfg(all(feature = "heap-profile", target_os = "linux"))]
fn heap_live_bytes() -> u64 {
    allsource_core::infrastructure::observability::heap::jemalloc_live_bytes()
}

// The profiling build reports attribution only; peak comes from the default build.
#[cfg(all(feature = "heap-profile", target_os = "linux"))]
fn peak_live_bytes() -> u64 {
    0
}

#[cfg(all(feature = "heap-profile", target_os = "linux"))]
async fn write_heap_profile(path: &Path) {
    let bytes = allsource_core::infrastructure::observability::heap::heap_profile_pprof()
        .await
        .expect("heap profile dump");
    std::fs::write(path, bytes).expect("heap profile write");
}

#[cfg(not(all(feature = "heap-profile", target_os = "linux")))]
async fn write_heap_profile(path: &Path) {
    panic!(
        "--heap-profile {} needs the heap-profile feature on linux",
        path.display()
    );
}

/// 2 GiB: today's production budget, so the main scenario evicts nothing.
const MAIN_BUDGET_BYTES: u64 = 2_147_483_648;
const SEED_TENANT: &str = "replay-ingest";
const SEED_EVENTS: u64 = 2_000;
const READ_MODEL: &str = "replay-model";
/// Core's `MODEL_EVENT` (`store_feed.rs`), which the replay must never digest.
const MODEL_EVENT: &str = "allsource.read_model.committed";
/// JSON numbers above 2^52 lose integer precision, so a larger epoch would not
/// survive the digest. The driver asserts this bound on every feed page.
const EPOCH_LIMIT: u64 = 1 << 52;
const ENTITIES_CHECKED: usize = 20;
const FIXTURE_BASE: i64 = 1_735_689_600; // 2025-01-01T00:00:00Z
const EVENT_TYPES: [&str; 3] = ["replay.created", "replay.updated", "replay.closed"];

#[derive(Deserialize)]
struct Manifest {
    tenants: Vec<TenantEntry>,
    estimated_total_bytes: u64,
    bbox: Value,
}

#[derive(Deserialize)]
struct TenantEntry {
    id: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scenario {
    Main,
    Eviction,
}

struct Args {
    volume: PathBuf,
    scenario: Scenario,
    /// One line per request: status, method, target, body SHA-256.
    trace: Option<PathBuf>,
    /// pprof dump of the live heap, written after the replay.
    heap_profile: Option<PathBuf>,
}

fn parse_args() -> Args {
    let argv: Vec<String> = std::env::args().collect();
    let flag = |name: &str| {
        argv.iter()
            .position(|a| a == name)
            .and_then(|i| argv.get(i + 1).cloned())
    };
    let volume = PathBuf::from(flag("--volume").expect("--volume <dir> is required"));
    let scenario = match flag("--scenario").as_deref() {
        Some("main") => Scenario::Main,
        Some("eviction") => Scenario::Eviction,
        other => panic!("--scenario must be main or eviction, got {other:?}"),
    };
    let trace = flag("--trace").map(PathBuf::from);
    let heap_profile = flag("--heap-profile").map(PathBuf::from);
    Args {
        volume,
        scenario,
        trace,
        heap_profile,
    }
}

/// Digests and counters for one process.
#[derive(Default)]
struct Ledger {
    query: Sha256,
    geo: Sha256,
    requests: u64,
    status_mismatches: u64,
    epoch_mismatches: u64,
    model_event_leaks: u64,
}

struct Driver {
    app: Router,
    keys: HashMap<String, String>,
    ledger: Ledger,
    revisions: HashMap<String, u64>,
    trace: Option<std::fs::File>,
}

/// Which digest a response feeds. Geo has its own digest.
#[derive(Clone, Copy)]
enum Lane {
    Query,
    Geo,
}

impl Driver {
    /// Send one request through the router and count its status. Nothing is
    /// digested here: the caller records the body it wants pinned.
    async fn request(
        &mut self,
        method: Method,
        target: &str,
        key: &str,
        body: Option<Value>,
    ) -> (StatusCode, Vec<u8>) {
        let request_body = match &body {
            Some(value) => Body::from(serde_json::to_vec(value).expect("request json")),
            None => Body::empty(),
        };
        let mut builder = Request::builder()
            .method(method.clone())
            .uri(target)
            .header("authorization", format!("Bearer {key}"));
        if body.is_some() {
            builder = builder.header("content-type", "application/json");
        }
        let request = builder.body(request_body).expect("request");
        let response = self
            .app
            .clone()
            .oneshot(request)
            .await
            .expect("infallible router");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 512 << 20)
            .await
            .expect("response body")
            .to_vec();
        self.ledger.requests += 1;
        if let Some(trace) = self.trace.as_mut() {
            let line = format!(
                "{} {method} {target} {}\n",
                status.as_u16(),
                hex::encode(Sha256::digest(&bytes))
            );
            let _ = trace.write_all(line.as_bytes());
        }
        (status, bytes)
    }

    /// Check a status against the expected one. `may_413` marks the 10,000-row
    /// page, which sits near the egress cap: Core answers 413 when the page is
    /// over it and 200 when under. The status is digested, so either is pinned.
    fn check_status(&mut self, method: &Method, target: &str, status: StatusCode, may_413: bool) {
        let expected =
            status == StatusCode::OK || (may_413 && status == StatusCode::PAYLOAD_TOO_LARGE);
        if !expected {
            self.ledger.status_mismatches += 1;
            eprintln!(
                "unexpected status {} for {method} {target}",
                status.as_u16()
            );
        }
    }

    fn digest(
        &mut self,
        lane: Lane,
        method: &Method,
        target: &str,
        status: StatusCode,
        body: &[u8],
    ) {
        let digest = match lane {
            Lane::Query => &mut self.ledger.query,
            Lane::Geo => &mut self.ledger.geo,
        };
        digest.update(method.as_str().as_bytes());
        digest.update(b"\n");
        digest.update(target.as_bytes());
        digest.update(b"\n");
        digest.update(status.as_u16().to_be_bytes());
        digest.update((body.len() as u64).to_be_bytes());
        digest.update(body);
    }

    /// One request, checked and digested.
    async fn call(
        &mut self,
        lane: Lane,
        method: Method,
        target: &str,
        key: &str,
        body: Option<Value>,
        may_413: bool,
    ) -> (StatusCode, Vec<u8>) {
        let (status, bytes) = self.request(method.clone(), target, key, body).await;
        self.check_status(&method, target, status, may_413);
        self.digest(lane, &method, target, status, &bytes);
        (status, bytes)
    }

    /// Feed page with epochs zeroed for the digest. Returns the page as sent
    /// plus the raw head cursor, which the read-model commit needs. Raw epochs
    /// are random per process, so the raw bytes are never digested.
    fn normalize_feed(&mut self, bytes: &[u8]) -> Option<(Vec<u8>, Value)> {
        let mut value: Value = serde_json::from_slice(bytes).ok()?;
        let cursor = value.get("cursor")?.get("epoch")?.as_u64()?;
        let head = value.get("head")?.get("epoch")?.as_u64()?;
        if cursor != head || head >= EPOCH_LIMIT {
            self.ledger.epoch_mismatches += 1;
        }
        let raw_head = value.get("head")?.clone();
        for key in ["cursor", "head"] {
            if let Some(epoch) = value.get_mut(key).and_then(|c| c.get_mut("epoch")) {
                *epoch = json!(0);
            }
        }
        Some((serde_json::to_vec(&value).ok()?, raw_head))
    }

    fn key(&self, tenant: &str) -> String {
        self.keys.get(tenant).expect("key for tenant").clone()
    }
}

/// The per-tenant replay set, in the order the design fixes.
async fn replay_tenant(driver: &mut Driver, tenant: &str, bbox: &Value) {
    let key = driver.key(tenant);

    for entity in ["e000000", "e000001", "e000002"] {
        let target =
            format!("/api/v1/events/query?tenant_id={tenant}&entity_id={entity}&limit=100");
        let (_, body) = driver
            .call(Lane::Query, Method::GET, &target, &key, None, false)
            .await;
        check_no_model_event(driver, &body);
    }

    let target =
        format!("/api/v1/events/query?tenant_id={tenant}&event_type=replay.updated&limit=10000");
    let (_, body) = driver
        .call(Lane::Query, Method::GET, &target, &key, None, true)
        .await;
    check_no_model_event(driver, &body);

    let target = format!(
        "/api/v1/events/query?tenant_id={tenant}&since=2025-01-01T00:00:00Z&until=2025-01-31T00:00:00Z&limit=500"
    );
    let (_, body) = driver
        .call(Lane::Query, Method::GET, &target, &key, None, false)
        .await;
    check_no_model_event(driver, &body);

    for index in 0..ENTITIES_CHECKED {
        let entity = format!("e{index:06}");
        let state = format!("/api/v1/entities/{entity}/state");
        driver
            .call(Lane::Query, Method::GET, &state, &key, None, false)
            .await;
        let snapshot = format!("/api/v1/entities/{entity}/snapshot");
        driver
            .call(Lane::Query, Method::GET, &snapshot, &key, None, false)
            .await;
    }

    let geo_body = json!({ "bbox": bbox });
    driver
        .call(
            Lane::Geo,
            Method::POST,
            "/api/v1/geospatial/query",
            &key,
            Some(geo_body),
            false,
        )
        .await;

    let feed_target = format!("/api/v1/event-feed?tenant_id={tenant}");
    let (status, feed_bytes) = driver.request(Method::GET, &feed_target, &key, None).await;
    driver.check_status(&Method::GET, &feed_target, status, false);
    let Some((normalized, head)) = driver.normalize_feed(&feed_bytes) else {
        driver.ledger.epoch_mismatches += 1;
        return;
    };
    driver.digest(Lane::Query, &Method::GET, &feed_target, status, &normalized);

    let expected_revision = driver.revisions.get(tenant).copied().unwrap_or(0);
    let commit = json!({
        "expected_revision": expected_revision,
        "checkpoint": head,
        "ready": true,
        "changes": { "summary": { "tenant": tenant } },
    });
    let commit_target = format!("/api/v1/read-models/{READ_MODEL}?tenant_id={tenant}");
    let (status, body) = driver
        .call(
            Lane::Query,
            Method::PUT,
            &commit_target,
            &key,
            Some(commit),
            false,
        )
        .await;
    if status == StatusCode::OK
        && let Some(revision) = serde_json::from_slice::<Value>(&body)
            .ok()
            .and_then(|v| v.get("revision").and_then(Value::as_u64))
    {
        driver.revisions.insert(tenant.to_string(), revision);
    }
}

fn check_no_model_event(driver: &mut Driver, body: &[u8]) {
    if body
        .windows(MODEL_EVENT.len())
        .any(|w| w == MODEL_EVENT.as_bytes())
    {
        driver.ledger.model_event_leaks += 1;
    }
}

/// Fixed-id events for the seeded tenant. Outside the pinned shape.
fn seed_events() -> Vec<Event> {
    (0..SEED_EVENTS)
        .map(|seq| {
            let id =
                uuid::Uuid::from_u128(0x5eed_0000_0000_0000_0000_0000_0000_0000 | u128::from(seq));
            let timestamp =
                DateTime::<Utc>::from_timestamp(FIXTURE_BASE + seq as i64, 0).expect("timestamp");
            Event::reconstruct_from_strings(
                id,
                EVENT_TYPES[(seq % 3) as usize].to_string(),
                format!("seed-{}", seq % 10),
                SEED_TENANT.to_string(),
                json!({ "seq": seq }),
                timestamp,
                None,
                1,
            )
        })
        .collect()
}

/// SHA-256 over the bytes of a 1,000-event Parquet write.
fn codec_digest(volume: &Path) -> String {
    let dir = volume.join("codec-probe");
    let storage = ParquetStorage::new(&dir).expect("codec storage");
    let events: Vec<Event> = (0..1_000u64)
        .map(|seq| {
            let id =
                uuid::Uuid::from_u128(0xc0de_c0de_0000_0000_0000_0000_0000_0000 | u128::from(seq));
            let timestamp =
                DateTime::<Utc>::from_timestamp(FIXTURE_BASE + seq as i64, 0).expect("timestamp");
            Event::reconstruct_from_strings(
                id,
                EVENT_TYPES[(seq % 3) as usize].to_string(),
                format!("codec-{}", seq % 17),
                "codec-probe".to_string(),
                json!({ "seq": seq, "pad": "y".repeat((seq % 40) as usize) }),
                timestamp,
                Some(json!({ "source": "codec" })),
                1,
            )
        })
        .collect();
    storage
        .write_atomic_parquet("codec-probe", "events-codec", &events)
        .expect("codec write");
    let mut files = storage
        .list_parquet_files_for_tenant("codec-probe")
        .expect("codec list");
    files.sort();
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(std::fs::read(file).expect("codec file"));
    }
    hex::encode(hasher.finalize())
}

fn read_manifest(volume: &Path) -> Manifest {
    let text = std::fs::read_to_string(volume.join("fixture.json")).expect("fixture.json");
    serde_json::from_str(&text).expect("fixture manifest")
}

async fn run(args: Args) -> Value {
    let manifest = read_manifest(&args.volume);
    let budget = match args.scenario {
        Scenario::Main => MAIN_BUDGET_BYTES,
        Scenario::Eviction => manifest.estimated_total_bytes / 2,
    };

    let config = EventStoreConfig {
        storage_dir: Some(args.volume.clone()),
        cache_byte_budget: Some(budget),
        ..EventStoreConfig::default()
    };
    let store = Arc::new(EventStore::with_config(config));

    let auth = Arc::new(AuthManager::default());
    let mut keys = HashMap::new();
    let mut tenant_ids: Vec<String> = manifest.tenants.iter().map(|t| t.id.clone()).collect();
    tenant_ids.push(SEED_TENANT.to_string());
    for tenant in &tenant_ids {
        let (_, key) = auth.create_api_key(
            format!("replay-{tenant}"),
            tenant.clone(),
            Role::ServiceAccount,
            None,
        );
        keys.insert(tenant.clone(), key);
    }

    let app = build_v1_router(
        Arc::clone(&store),
        Arc::clone(&auth),
        Arc::new(InMemoryTenantRepository::new()),
        Arc::new(RateLimiter::new(RateLimitConfig::dev_mode())),
        ContainerBuilder::new()
            .with_in_memory_repositories()
            .build(),
        NodeRole::Leader,
        None,
        None,
        3910,
        None,
        None,
    )
    .await;

    for event in seed_events() {
        store.ingest(&event).expect("seed ingest");
    }

    let trace = args
        .trace
        .as_deref()
        .map(|path| std::fs::File::create(path).expect("trace file"));
    let mut driver = Driver {
        app,
        keys,
        ledger: Ledger::default(),
        revisions: HashMap::new(),
        trace,
    };
    let mut evictions: Vec<String> = Vec::new();
    let mut cold_load_ms = 0u64;

    let passes = match args.scenario {
        Scenario::Main => 2,
        Scenario::Eviction => 1,
    };
    for pass in 0..passes {
        let mut resident: Vec<bool> = manifest
            .tenants
            .iter()
            .map(|t| store.is_tenant_loaded(&t.id))
            .collect();
        for (index, tenant) in manifest.tenants.iter().enumerate() {
            let started = Instant::now();
            replay_tenant(&mut driver, &tenant.id, &manifest.bbox).await;
            if pass == 0 && index == 0 {
                cold_load_ms = started.elapsed().as_millis() as u64;
            }
            if args.scenario == Scenario::Eviction {
                // Pending events pin a tenant in memory, so flush each tenant's
                // commit before the next load can pick it as a victim.
                store.flush_storage().expect("flush at fixed step");
                let flags: Vec<bool> = manifest
                    .tenants
                    .iter()
                    .map(|other| store.is_tenant_loaded(&other.id))
                    .collect();
                eprintln!("residency after {}: {flags:?}", tenant.id);
                for ((other, was_loaded), now) in
                    manifest.tenants.iter().zip(resident.iter_mut()).zip(flags)
                {
                    if *was_loaded && !now {
                        evictions.push(other.id.clone());
                    }
                    *was_loaded = now;
                }
            }
        }
        if pass == 0 && args.scenario == Scenario::Main {
            store.flush_storage().expect("flush at fixed step");
        }
    }

    if let Some(path) = &args.heap_profile {
        write_heap_profile(path).await;
    }
    let steady = heap_live_bytes();
    let peak = peak_live_bytes();
    let codec = codec_digest(&args.volume);
    let resident = allsource_core::infrastructure::memory::ledger::resident();
    let ledger_bytes = resident.total_bytes().max(0) as u64;
    let rows_decoded = resident.rows_decoded_total();
    let owner_bytes: serde_json::Map<String, Value> =
        allsource_core::infrastructure::memory::ledger::Owner::ALL
            .iter()
            .map(|owner| {
                (
                    owner.label().to_string(),
                    json!(resident.owner_bytes(*owner)),
                )
            })
            .collect();

    let ledger = driver.ledger;
    json!({
        "heap_replay": {
            "scenario": match args.scenario { Scenario::Main => "main", Scenario::Eviction => "eviction" },
            "hasher": if cfg!(feature = "deterministic-hash") { "deterministic" } else { "production" },
            "peak_live_bytes": peak,
            "steady_live_bytes": steady,
            "ledger_bytes": ledger_bytes,
            "ledger_owner_bytes": owner_bytes,
            "rows_decoded": rows_decoded,
            "query_digest": hex::encode(ledger.query.finalize()),
            "geo_digest": hex::encode(ledger.geo.finalize()),
            "codec_digest": codec,
            "evictions": evictions,
            "requests": ledger.requests,
            "status_mismatches": ledger.status_mismatches,
            "epoch_mismatches": ledger.epoch_mismatches,
            "model_event_leaks": ledger.model_event_leaks,
            "cold_load_largest_ms": cold_load_ms,
        }
    })
}

fn main() {
    let args = parse_args();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let report = runtime.block_on(run(args));
    println!("{report}");
}
