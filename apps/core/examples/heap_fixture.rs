//! Writes the heap replay fixture: synthetic events sized from `shape.toml`.
//!
//! Tenant counts and the payload-size distribution come from the shape file
//! and nothing else. Events are written through `ParquetStorage`, which encodes
//! every row with `event_codec`, so the fixture is in the production format.
//!
//! Usage:
//!   cargo run --example heap_fixture -p allsource-core --features community -- \
//!       --shape <abs path to shape.toml> --out <abs empty dir>
//!
//! The output directory holds one tenant-partitioned Parquet tree and a
//! `fixture.json` with per-tenant counts, the estimated total, the bounding box
//! for the geo query, and a content digest over every Parquet file.

use allsource_core::{Event, ParquetStorage};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const CHUNK_EVENTS: u64 = 50_000;
const EVENT_TYPES: [&str; 3] = ["replay.created", "replay.updated", "replay.closed"];
const BASE_TIMESTAMP: i64 = 1_735_689_600; // 2025-01-01T00:00:00Z
const LAT_RANGE: (f64, f64) = (-60.0, 60.0);
const LNG_RANGE: (f64, f64) = (-170.0, 170.0);
const MIN_PAYLOAD_BYTES: u64 = 96;
const MAX_PAYLOAD_BYTES: u64 = 200_000;

#[derive(Deserialize)]
struct Shape {
    seed: u64,
    tenants: Tenants,
    payload: Payload,
    entities: Entities,
}

#[derive(Deserialize)]
struct Tenants {
    counts: Vec<u64>,
}

#[derive(Deserialize)]
struct Payload {
    mean_bytes: u64,
    sigma_milli: u64,
}

#[derive(Deserialize)]
struct Entities {
    events_per_entity: u64,
}

#[derive(Serialize)]
struct Manifest {
    schema: &'static str,
    seed: u64,
    shape_sha256: String,
    tenants: Vec<TenantEntry>,
    total_events: u64,
    estimated_total_bytes: u64,
    payload_mean_bytes: u64,
    bbox: BBox,
    content_sha256: String,
}

#[derive(Serialize)]
struct TenantEntry {
    id: String,
    events: u64,
    entities: u64,
    estimated_bytes: u64,
}

#[derive(Serialize, Clone, Copy)]
struct BBox {
    min_lat: f64,
    min_lng: f64,
    max_lat: f64,
    max_lng: f64,
}

/// splitmix64: a fixed, portable generator. No clock, no OS randomness.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in (0, 1].
    fn unit(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64 + 1.0) / (1u64 << 53) as f64
    }

    /// Standard normal by Box-Muller.
    fn normal(&mut self) -> f64 {
        let (u1, u2) = (self.unit(), self.unit());
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

/// Log-normal payload sizes with the given arithmetic mean and sigma.
struct SizeDraw {
    mu: f64,
    sigma: f64,
}

impl SizeDraw {
    fn new(mean_bytes: u64, sigma_milli: u64) -> Self {
        let sigma = sigma_milli as f64 / 1000.0;
        Self {
            mu: (mean_bytes as f64).ln() - sigma * sigma / 2.0,
            sigma,
        }
    }

    fn draw(&self, rng: &mut Rng) -> u64 {
        let size = (self.mu + self.sigma * rng.normal()).exp().round() as u64;
        size.clamp(MIN_PAYLOAD_BYTES, MAX_PAYLOAD_BYTES)
    }
}

/// A payload of about `target` bytes of JSON text, carrying a location.
fn payload(seq: u64, target: u64, lat: f64, lng: f64) -> serde_json::Value {
    let overhead = 120;
    let pad = target.saturating_sub(overhead) as usize;
    serde_json::json!({
        "seq": seq,
        "location": { "lat": lat, "lng": lng },
        "pad": "x".repeat(pad),
    })
}

fn tenant_id(index: usize) -> String {
    format!("tenant-{:02}", index + 1)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let shape_path = flag(&args, "--shape").expect("--shape <path> is required");
    let out = PathBuf::from(flag(&args, "--out").expect("--out <dir> is required"));
    let shape_bytes = std::fs::read(&shape_path).expect("read shape");
    let shape: Shape = toml::from_str(std::str::from_utf8(&shape_bytes).expect("utf8 shape"))
        .expect("parse shape");

    std::fs::create_dir_all(&out).expect("create out dir");
    let storage = ParquetStorage::new(&out).expect("storage");

    let sizes = SizeDraw::new(shape.payload.mean_bytes, shape.payload.sigma_milli);
    let bbox = BBox {
        min_lat: LAT_RANGE.0,
        min_lng: LNG_RANGE.0,
        max_lat: LAT_RANGE.1,
        max_lng: LNG_RANGE.1,
    };

    let mut rng = Rng(shape.seed);
    let mut tenants = Vec::new();
    let mut total_payload = 0u64;
    let mut total_events = 0u64;
    let mut estimated_total = 0u64;

    for (index, count) in shape.tenants.counts.iter().copied().enumerate() {
        let tenant = tenant_id(index);
        let entities = count.div_ceil(shape.entities.events_per_entity).max(1);
        let mut tenant_estimate = 0u64;
        let mut written = 0u64;
        let mut chunk_index = 0u32;
        while written < count {
            let take = (count - written).min(CHUNK_EVENTS);
            let mut events = Vec::with_capacity(take as usize);
            for offset in 0..take {
                let seq = written + offset;
                let size = sizes.draw(&mut rng);
                let lat = LAT_RANGE.0 + (LAT_RANGE.1 - LAT_RANGE.0) * rng.unit();
                let lng = LNG_RANGE.0 + (LNG_RANGE.1 - LNG_RANGE.0) * rng.unit();
                let event = event_at(&tenant, seq, entities, payload(seq, size, lat, lng));
                tenant_estimate += event.estimated_size_bytes();
                total_payload += size;
                events.push(event);
            }
            storage
                .write_atomic_parquet(&tenant, &format!("events-replay-{chunk_index:03}"), &events)
                .expect("write parquet");
            written += take;
            chunk_index += 1;
        }
        total_events += count;
        estimated_total += tenant_estimate;
        tenants.push(TenantEntry {
            id: tenant,
            events: count,
            entities,
            estimated_bytes: tenant_estimate,
        });
    }

    let manifest = Manifest {
        schema: "heap-fixture.v1",
        seed: shape.seed,
        shape_sha256: hex_sha256(&shape_bytes),
        tenants,
        total_events,
        estimated_total_bytes: estimated_total,
        payload_mean_bytes: total_payload / total_events.max(1),
        bbox,
        content_sha256: content_digest(&out),
    };
    let text = serde_json::to_string_pretty(&manifest).expect("manifest json");
    std::fs::write(out.join("fixture.json"), format!("{text}\n")).expect("write manifest");
    println!(
        "fixture events={} tenants={} estimated_total_bytes={}",
        manifest.total_events,
        manifest.tenants.len(),
        manifest.estimated_total_bytes
    );
}

/// One fixture event. Ids and timestamps are functions of the tenant and the
/// sequence number, so every write produces the same rows.
fn event_at(tenant: &str, seq: u64, entities: u64, payload: serde_json::Value) -> Event {
    let tenant_no = tenant
        .rsplit('-')
        .next()
        .and_then(|n| n.parse::<u128>().ok())
        .unwrap_or(0);
    let id = uuid::Uuid::from_u128((tenant_no << 64) | u128::from(seq) | (1u128 << 127));
    let timestamp =
        DateTime::<Utc>::from_timestamp(BASE_TIMESTAMP + seq as i64, 0).expect("timestamp");
    Event::reconstruct_from_strings(
        id,
        EVENT_TYPES[(seq % 3) as usize].to_string(),
        format!("e{:06}", seq % entities),
        tenant.to_string(),
        payload,
        timestamp,
        None,
        1,
    )
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn hex_sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// SHA-256 over every file under `root` (except `fixture.json`), in path order.
fn content_digest(root: &Path) -> String {
    let mut files = Vec::new();
    collect(root, root, &mut files);
    files.sort();
    let mut hasher = Sha256::new();
    for relative in files {
        let bytes = std::fs::read(root.join(&relative)).expect("read fixture file");
        hasher.update(relative.as_bytes());
        hasher.update(b"\n");
        hasher.update(Sha256::digest(&bytes));
        hasher.update(b"\n");
    }
    hex::encode(hasher.finalize())
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).expect("read fixture dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, out);
        } else {
            let relative = path
                .strip_prefix(root)
                .expect("under root")
                .to_string_lossy()
                .into_owned();
            if relative != "fixture.json" {
                out.push(relative);
            }
        }
    }
}
