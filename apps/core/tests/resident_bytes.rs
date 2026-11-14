//! `Event::resident_bytes()` against the live-byte delta the global allocator
//! records for the same event (design decision 1, `resident_bytes_accuracy`).
//!
//! The sample is the heap fixture's size distribution: the same splitmix64
//! generator, Box-Muller draw and log-normal parameters as
//! `apps/core/examples/heap_fixture.rs`, with the shape's seed, mean and sigma.
//! The shape numbers are copied, not read from `tooling/`, so the crate builds
//! without the tooling tree.
use allsource_core::domain::entities::Event;
use serde_json::{Map, Value, json};
use std::alloc::System;

#[global_allocator]
static COUNTED: cap::Cap<System> = cap::Cap::new(System, usize::MAX);

const SHAPE_SEED: u64 = 20_251_101;
const SHAPE_MEAN_BYTES: u64 = 3_892;
const SHAPE_SIGMA_MILLI: u64 = 900;
const MIN_PAYLOAD_BYTES: u64 = 96;
const MAX_PAYLOAD_BYTES: u64 = 200_000;
const SAMPLES: usize = 2_000;
const PAYLOAD_OVERHEAD: u64 = 120;

fn event(payload: Value, metadata: Option<Value>) -> Event {
    Event::from_strings(
        "order.placed".into(),
        "order-synthetic-1".into(),
        "tenant-synthetic".into(),
        payload,
        metadata,
    )
    .unwrap()
}

/// Build an event and box it. The live-byte delta covers the payload, the
/// metadata, the owned strings and the box itself, which is what
/// `resident_bytes()` is defined to count.
fn measured(build: impl FnOnce() -> Event) -> (Box<Event>, usize) {
    let before = COUNTED.allocated();
    let boxed = Box::new(build());
    let delta = COUNTED.allocated() - before;
    (boxed, delta)
}

/// Inserts from empty, as `serde_json` decoding and `json!` do.
fn wide_object(keys: usize) -> Value {
    let mut map = Map::new();
    for i in 0..keys {
        map.insert(format!("field_{i:03}"), json!(i));
    }
    Value::Object(map)
}

/// splitmix64, as in the fixture generator.
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

/// The fixture's payload: sequence number, location, and padding to `target`
/// bytes of JSON text.
fn fixture_payload(seq: u64, target: u64, lat: f64, lng: f64) -> Value {
    let pad = target.saturating_sub(PAYLOAD_OVERHEAD) as usize;
    json!({
        "seq": seq,
        "location": { "lat": lat, "lng": lng },
        "pad": "x".repeat(pad),
    })
}

/// Object maps of every size class are charged exactly what they allocate,
/// whichever map type `serde_json` was compiled with. Called from the one test
/// so no other test thread allocates during a measurement.
fn object_map_bytes_are_exact() {
    let (_, empty) = measured(|| event(json!({}), None));
    let empty_resident = event(json!({}), None).resident_bytes() as usize;
    for keys in 1..=150 {
        let (boxed, allocated) = measured(|| event(wide_object(keys), None));
        assert_eq!(
            boxed.resident_bytes() as usize - empty_resident,
            allocated - empty,
            "{keys} keys"
        );
    }
}

#[test]
fn resident_bytes_accuracy() {
    object_map_bytes_are_exact();

    type Build = Box<dyn Fn() -> Event>;
    let cases: Vec<(&str, Build)> = vec![
        ("one key", Box::new(|| event(json!({"k": "v"}), None))),
        (
            "few keys with array",
            Box::new(|| {
                event(
                    json!({"name": "Alice", "amount": 1200, "currency": "EUR", "tags": ["a", "b", "c"]}),
                    None,
                )
            }),
        ),
        (
            "nested lines",
            Box::new(|| {
                event(
                    json!({"order": {
                        "lines": [{"sku": "A-1", "qty": 2}, {"sku": "B-2", "qty": 1}],
                        "note": "x".repeat(300)
                    }}),
                    None,
                )
            }),
        ),
        (
            "large string",
            Box::new(|| event(json!({"body": "y".repeat(10_000)}), None)),
        ),
        (
            "with metadata",
            Box::new(|| {
                event(
                    json!({"amount": 7}),
                    Some(json!({"source": "api", "correlation_id": "abc-123-def-456"})),
                )
            }),
        ),
        ("forty keys", Box::new(|| event(wide_object(40), None))),
        (
            "array of records",
            Box::new(|| {
                let rows: Vec<Value> = (0..200)
                    .map(|i| json!({"id": i, "label": format!("row-{i}"), "ok": true}))
                    .collect();
                event(json!({"rows": rows}), None)
            }),
        ),
    ];

    for (name, build) in &cases {
        let (boxed, allocated) = measured(build);
        let resident = boxed.resident_bytes() as usize;
        let diff = resident.abs_diff(allocated);
        assert!(
            diff * 10 <= allocated,
            "{name}: resident_bytes {resident} vs allocated {allocated} is outside 10%"
        );
    }

    let sigma = SHAPE_SIGMA_MILLI as f64 / 1000.0;
    let mu = (SHAPE_MEAN_BYTES as f64).ln() - sigma * sigma / 2.0;
    let mut rng = Rng(SHAPE_SEED);
    let mut size_total = 0u64;
    let (mut resident_total, mut allocated_total) = (0usize, 0usize);
    for seq in 0..SAMPLES as u64 {
        let target = ((mu + sigma * rng.normal()).exp().round() as u64)
            .clamp(MIN_PAYLOAD_BYTES, MAX_PAYLOAD_BYTES);
        let lat = -60.0 + 120.0 * rng.unit();
        let lng = -170.0 + 340.0 * rng.unit();
        size_total += target;

        let (boxed, allocated) = measured(|| event(fixture_payload(seq, target, lat, lng), None));
        let resident = boxed.resident_bytes() as usize;
        resident_total += resident;
        allocated_total += allocated;
        assert!(
            resident.abs_diff(allocated) * 10 <= allocated,
            "fixture sample {seq} (target {target} B): resident_bytes {resident} vs allocated {allocated} is outside 10%"
        );
    }

    assert!(
        resident_total.abs_diff(allocated_total) * 50 <= allocated_total,
        "fixture sample: resident_bytes total {resident_total} vs allocated {allocated_total} is outside 2%"
    );

    let mean = size_total as f64 / SAMPLES as f64;
    let expected = SHAPE_MEAN_BYTES as f64;
    assert!(
        (mean - expected).abs() * 10.0 <= expected,
        "sampled mean {mean:.0} B is outside 10% of the shape mean {expected:.0} B"
    );
}
