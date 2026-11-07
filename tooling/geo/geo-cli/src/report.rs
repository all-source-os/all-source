//! `geo report` — read the `geo.*` family back out of AllSource and print a
//! per-layer count table.
//!
//! The layer mapping lives here, not in `geo-core`: the emitter is
//! transport-thin and has no opinion about measurement layers. This is the
//! only place that groups event types into layers.

use std::collections::{BTreeMap, BTreeSet};

use allsource::QueryEventsParams;
use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration, SecondsFormat, Utc};
use geo_core::{
    BotCategory, EVENT_TYPE_PREFIX, GeoConfig, GeoEventType, IngestEnvelope, JudgeVerdict,
    TAXONOMY_VERSION, samples,
};
use serde_json::Value;

use crate::layer3::{InterrogationRow, Layer3, SovRow};

/// One measurement layer: a named group of event types and the slice that
/// produces them.
struct Layer {
    id: &'static str,
    name: &'static str,
    produced_by: &'static str,
    types: &'static [GeoEventType],
}

/// The layer map. Prompt numbers are the plan of record; if a slice lands
/// under a different number, fix it here and in
/// `docs/contracts/geo-events/README.md`.
const LAYERS: &[Layer] = &[
    Layer {
        id: "1",
        name: "Referral attribution",
        produced_by: "prompt 024",
        types: &[GeoEventType::ReferralObserved],
    },
    Layer {
        id: "2",
        name: "Crawl diagnostics",
        produced_by: "prompt 024",
        types: &[GeoEventType::CrawlObserved],
    },
    Layer {
        id: "3a",
        name: "Share of voice",
        produced_by: "prompt 025",
        types: &[GeoEventType::SovProbed],
    },
    Layer {
        id: "3b",
        name: "Interrogation",
        produced_by: "prompt 025",
        types: &[GeoEventType::InterrogationProbed],
    },
    Layer {
        id: "4",
        name: "Self-report",
        produced_by: "prompt 026",
        types: &[GeoEventType::SelfReportCaptured],
    },
    Layer {
        id: "5",
        name: "Experiments",
        produced_by: "prompt 027",
        types: &[
            GeoEventType::ExperimentStarted,
            GeoEventType::ExperimentScored,
        ],
    },
];

/// Events and distinct entities seen for one event type.
#[derive(Default)]
struct TypeTally {
    events: u64,
    entities: BTreeSet<String>,
}

/// Everything a report needs from the stream.
#[derive(Default)]
struct Tally {
    by_type: BTreeMap<&'static str, TypeTally>,
    unknown_types: BTreeMap<String, u64>,
    total_events: u64,
    truncated: bool,
    crawl: CrawlTally,
    referral: ReferralTally,
    layer3: Layer3,
    selfreport: SelfReportTally,
}

impl Tally {
    fn record_event(&mut self, event_type: &str, entity_id: &str, payload: &Value) {
        self.record(event_type, entity_id);
        match GeoEventType::parse(event_type) {
            Some(GeoEventType::CrawlObserved) => self.crawl.record(entity_id, payload),
            Some(GeoEventType::ReferralObserved) => self.referral.record(entity_id, payload),
            Some(GeoEventType::SovProbed) => record_sov(&mut self.layer3, payload),
            Some(GeoEventType::InterrogationProbed) => {
                record_interrogation(&mut self.layer3, payload);
            }
            Some(GeoEventType::SelfReportCaptured) => self.selfreport.record(entity_id, payload),
            _ => {}
        }
    }

    fn record(&mut self, event_type: &str, entity_id: &str) {
        self.total_events += 1;
        if let Some(known) = GeoEventType::parse(event_type) {
            let slot = self.by_type.entry(known.as_str()).or_default();
            slot.events += 1;
            slot.entities.insert(entity_id.to_string());
        } else {
            // A `geo.*` type this build does not know about: report it rather
            // than silently dropping it, so a newer producer is visible.
            let seen = self
                .unknown_types
                .entry(event_type.to_string())
                .or_default();
            *seen += 1;
        }
    }

    fn events_for(&self, t: GeoEventType) -> u64 {
        self.by_type.get(t.as_str()).map_or(0, |s| s.events)
    }

    fn entities_for(&self, t: GeoEventType) -> u64 {
        self.by_type
            .get(t.as_str())
            .map_or(0, |s| s.entities.len() as u64)
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Layer 2 — crawl, per category, per week
// ───────────────────────────────────────────────────────────────────────────

/// How many trailing weeks the rolling median smooths over.
///
/// A **median**, not a mean: a single overnight crawl burst — one vendor
/// re-reading the whole site in an hour — moves a 4-week mean by a quarter and
/// leaves a median untouched. The framework asks for median smoothing for
/// exactly that reason, and the report obeys it.
const MEDIAN_WEEKS: usize = 4;

/// The Monday 00:00 UTC that starts `ts`'s week. Weeks are the reporting unit
/// because crawl volume is bursty within a day and meaningless per-hour.
fn week_start(ts: DateTime<Utc>) -> DateTime<Utc> {
    let days_since_monday = i64::from(ts.weekday().num_days_from_monday());
    (ts - Duration::days(days_since_monday))
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map_or(ts, |naive| naive.and_utc())
}

/// Hits and paths for one (category, week) cell.
#[derive(Default, Clone)]
struct WeekCell {
    hits: u64,
    paths: BTreeMap<String, u64>,
}

/// The fields the layer-2 section reads off one stored crawl row.
#[derive(Clone)]
struct CrawlRow {
    observed_at: DateTime<Utc>,
    category: String,
    path: String,
    hits: u64,
    verified: bool,
    aggregated: bool,
}

/// Everything the layer-2 section needs, kept strictly per category.
///
/// There is deliberately no all-categories total anywhere in this struct. A
/// training crawler, a search indexer and a user fetcher answer three
/// different questions; a blended number answers none of them.
///
/// Rows are folded **by `entity_id`, last version wins**, which is what makes
/// the counts replay-safe. Every observation's `entity_id` embeds its
/// idempotency key, so re-ingesting a log window appends version 2 to the same
/// entity. Summing raw events would double a re-ingested week; folding by
/// entity does not. (This is exactly the `EVENTS` vs `ENTITIES` distinction in
/// the inventory table above, applied to the hit counts.)
#[derive(Default)]
struct CrawlTally {
    rows: BTreeMap<String, CrawlRow>,
}

impl CrawlTally {
    fn record(&mut self, entity_id: &str, payload: &Value) {
        let Some(observed_at) = payload
            .get("observed_at")
            .and_then(Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
        else {
            return;
        };
        // Core returns events ordered by (timestamp, version), so a plain
        // insert leaves the newest version of each entity in place.
        self.rows.insert(
            entity_id.to_string(),
            CrawlRow {
                observed_at,
                category: payload
                    .get("category")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                path: payload
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or("(unknown)")
                    .to_string(),
                hits: payload.get("hits").and_then(Value::as_u64).unwrap_or(1),
                verified: payload
                    .get("verified")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                aggregated: payload.get("aggregation").and_then(Value::as_str) != Some("hit"),
            },
        );
    }

    fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Verified hits by category then week.
    fn by_category(&self) -> BTreeMap<&str, BTreeMap<DateTime<Utc>, WeekCell>> {
        let mut out: BTreeMap<&str, BTreeMap<DateTime<Utc>, WeekCell>> = BTreeMap::new();
        for row in self.rows.values() {
            if !row.verified || BotCategory::parse(&row.category).is_none() {
                continue;
            }
            let cell = out
                .entry(row.category.as_str())
                .or_default()
                .entry(week_start(row.observed_at))
                .or_default();
            cell.hits += row.hits;
            *cell.paths.entry(row.path.clone()).or_default() += row.hits;
        }
        out
    }

    /// Hits stored with `verified: false`, never counted as crawl volume.
    fn unverified_hits(&self) -> u64 {
        self.rows
            .values()
            .filter(|r| !r.verified)
            .map(|r| r.hits)
            .sum()
    }

    /// Verified rows carrying a category this binary's taxonomy does not know.
    fn unknown_categories(&self) -> BTreeMap<&str, u64> {
        let mut out: BTreeMap<&str, u64> = BTreeMap::new();
        for row in self.rows.values() {
            if row.verified && BotCategory::parse(&row.category).is_none() {
                *out.entry(row.category.as_str()).or_default() += row.hits;
            }
        }
        out
    }

    /// How many stored rows are aggregated buckets rather than single hits.
    fn aggregated_rows(&self) -> usize {
        self.rows.values().filter(|r| r.aggregated).count()
    }

    /// The contiguous week axis, so a week with no crawl reads as a zero
    /// rather than vanishing and flattering the median.
    fn weeks(&self, window: Window) -> Vec<DateTime<Utc>> {
        let first = self.rows.values().map(|r| r.observed_at).min();
        let last = self.rows.values().map(|r| r.observed_at).max();
        let start = week_start(first.unwrap_or(window.since).min(window.since));
        let end = last.unwrap_or(window.until).max(window.until);
        let mut out = Vec::new();
        let mut cursor = start;
        while cursor < end {
            out.push(cursor);
            cursor += Duration::weeks(1);
        }
        out
    }
}

/// Median of the trailing `window` values, one output per input.
///
/// Fewer than `window` values available (the start of a series) uses whatever
/// is there rather than emitting nothing — a short series is still a series.
fn rolling_median(values: &[u64], window: usize) -> Vec<f64> {
    (0..values.len())
        .map(|i| {
            let start = i.saturating_sub(window - 1);
            let mut slice: Vec<u64> = values[start..=i].to_vec();
            slice.sort_unstable();
            let mid = slice.len() / 2;
            if slice.len() % 2 == 1 {
                slice[mid] as f64
            } else {
                f64::midpoint(slice[mid - 1] as f64, slice[mid] as f64)
            }
        })
        .collect()
}

/// Layer 1 counters, read off `geo.referral.observed` payloads.
///
/// Folded by `entity_id`, last version wins — which is doing real work here
/// and not just replay safety: a conversion is emitted as a *second version*
/// of the arrival's entity with `converted: true`, so the last version is the
/// current state of that session.
#[derive(Default)]
struct ReferralTally {
    rows: BTreeMap<String, (String, bool)>,
}

impl ReferralTally {
    fn record(&mut self, entity_id: &str, payload: &Value) {
        let surface = payload
            .get("surface")
            .and_then(Value::as_str)
            .unwrap_or("(unknown)")
            .to_string();
        let converted = payload.get("converted").and_then(Value::as_bool) == Some(true);
        self.rows.insert(entity_id.to_string(), (surface, converted));
    }

    fn by_surface(&self) -> BTreeMap<&str, u64> {
        let mut out: BTreeMap<&str, u64> = BTreeMap::new();
        for (surface, _) in self.rows.values() {
            *out.entry(surface.as_str()).or_default() += 1;
        }
        out
    }

    fn converted(&self) -> u64 {
        self.rows.values().filter(|(_, c)| *c).count() as u64
    }

    fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Layer 4 — self-report
// ───────────────────────────────────────────────────────────────────────────

/// Tiers that mean money is changing hands.
///
/// `trial` and `self-host` are deliberately absent: a trial is not revenue and
/// a self-host tenant is not billed. An unrecognised tier is counted as
/// *unknown*, never as paid — inflating the paid share is the one error in
/// this report that would be repeated in a deck.
const PAID_TIERS: &[&str] = &["indie", "studio", "scale", "enterprise"];

/// One self-report capture, folded to its latest version.
#[derive(Clone, Default)]
struct SelfReportRow {
    /// Discovery-source id — what the human said sent them.
    surface: String,
    /// Capture-path id — which signup path collected the answer.
    capture_path: String,
    /// The buyer's literal prompt, when they gave one.
    verbatim: Option<String>,
    /// Tier at capture; `None` when the capturing path could not resolve one.
    tier: Option<String>,
}

/// Layer 4 counters, read off `geo.selfreport.captured` payloads.
///
/// Folded by `entity_id`, last version wins — the same rule layer 1 uses, and
/// for the same reason: a later re-emit (a tier correction, a re-submitted
/// answer) is the same capture restated, not a second signup.
#[derive(Default)]
struct SelfReportTally {
    rows: BTreeMap<String, SelfReportRow>,
}

impl SelfReportTally {
    fn record(&mut self, entity_id: &str, payload: &Value) {
        let text = |key: &str| {
            payload
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        self.rows.insert(
            entity_id.to_string(),
            SelfReportRow {
                surface: text("surface").unwrap_or_else(|| "(unknown)".to_string()),
                capture_path: text("source").unwrap_or_else(|| "(unknown)".to_string()),
                verbatim: text("verbatim"),
                tier: text("tier"),
            },
        );
    }

    fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    fn total(&self) -> u64 {
        self.rows.len() as u64
    }

    /// Captures per discovery source, descending by count then id.
    fn by_surface(&self) -> Vec<(String, u64)> {
        let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
        for row in self.rows.values() {
            *counts.entry(row.surface.as_str()).or_default() += 1;
        }
        let mut ranked: Vec<(String, u64)> = counts
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        ranked
    }

    fn by_capture_path(&self) -> BTreeMap<&str, u64> {
        let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
        for row in self.rows.values() {
            *counts.entry(row.capture_path.as_str()).or_default() += 1;
        }
        counts
    }

    /// Captures whose discovery source counts as AI.
    fn ai_sourced(&self) -> u64 {
        self.rows
            .values()
            .filter(|r| geo_core::is_ai_source(&r.surface))
            .count() as u64
    }

    /// Captures on one capture path.
    fn on_path(&self, path: &str) -> u64 {
        self.rows
            .values()
            .filter(|r| r.capture_path == path)
            .count() as u64
    }

    /// AI-sourced captures on one capture path.
    fn ai_on_path(&self, path: &str) -> u64 {
        self.rows
            .values()
            .filter(|r| r.capture_path == path && geo_core::is_ai_source(&r.surface))
            .count() as u64
    }

    /// `(paid captures, AI-sourced paid captures, captures with no tier recorded)`.
    fn paid_split(&self) -> (u64, u64, u64) {
        let mut paid = 0;
        let mut ai_paid = 0;
        let mut untiered = 0;
        for row in self.rows.values() {
            match row.tier.as_deref() {
                None => untiered += 1,
                Some(tier) if PAID_TIERS.contains(&tier) => {
                    paid += 1;
                    if geo_core::is_ai_source(&row.surface) {
                        ai_paid += 1;
                    }
                }
                Some(_) => {}
            }
        }
        (paid, ai_paid, untiered)
    }

    /// Every free-text answer, with the source that framed it. These are the
    /// raw material for the prompt set — they are printed verbatim, never
    /// summarised.
    fn verbatims(&self) -> Vec<(&str, &str)> {
        let mut out: Vec<(&str, &str)> = self
            .rows
            .values()
            .filter_map(|r| r.verbatim.as_deref().map(|v| (r.surface.as_str(), v)))
            .collect();
        out.sort_unstable();
        out
    }

    /// Discovery-source ids this build's vocabulary does not know.
    fn unknown_sources(&self) -> BTreeMap<&str, u64> {
        let mut out: BTreeMap<&str, u64> = BTreeMap::new();
        for row in self.rows.values() {
            if geo_core::discovery_source(&row.surface).is_none() {
                *out.entry(row.surface.as_str()).or_default() += 1;
            }
        }
        out
    }
}

/// `n/d` as a percentage, or `None` when the denominator is zero.
///
/// Returning `None` rather than `0.0` is the point: "0% of nothing" reads as a
/// measured zero and it is not one.
fn percent(numerator: u64, denominator: u64) -> Option<f64> {
    (denominator > 0).then(|| numerator as f64 * 100.0 / denominator as f64)
}

fn percent_str(numerator: u64, denominator: u64) -> String {
    percent(numerator, denominator).map_or_else(|| "   n/a".to_string(), |p| format!("{p:5.1}%"))
}

/// Read one stored `geo.sov.probed` payload into the layer-3 aggregate.
///
/// Deliberately tolerant of missing fields: this stream outlives the binary
/// reading it, and a row written by a newer producer must degrade to a
/// partially-known row rather than take the report down.
fn record_sov(layer: &mut Layer3, payload: &Value) {
    let text = |key: &str| {
        payload
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let urls = |key: &str| {
        payload
            .get(key)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    layer.sov.push(SovRow {
        engine: text("engine"),
        prompt_id: text("prompt_id"),
        // Older rows predate the stored intent; they are reported under
        // "(unclassified)" rather than silently folded into a real class.
        intent: payload
            .get("intent")
            .and_then(Value::as_str)
            .unwrap_or("(unclassified)")
            .to_string(),
        mentioned: payload
            .get("mentioned")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        rank: payload
            .get("rank")
            .and_then(Value::as_u64)
            .and_then(|r| u32::try_from(r).ok()),
        competitors: urls("competitors"),
        cited_urls: urls("cited_urls"),
        score: payload.get("score").and_then(Value::as_f64).unwrap_or(0.0),
        // The stored payload carries no answer text, so a hedge cannot be
        // re-derived at read time. Left None rather than guessed.
        hedge: None,
    });
}

/// Read one stored `geo.interrogation.probed` payload into the aggregate.
fn record_interrogation(layer: &mut Layer3, payload: &Value) {
    let text = |key: &str| {
        payload
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    layer.interrogation.push(InterrogationRow {
        engine: text("engine"),
        prompt_id: text("prompt_id"),
        claim_id: text("claim_id"),
        // An unknown verdict string becomes `Unscored`, which is excluded from
        // every accuracy denominator — never guessed into a neighbour.
        verdict: JudgeVerdict::parse(&text("verdict")).unwrap_or(JudgeVerdict::Unscored),
        excerpt: text("answer_excerpt"),
        reasoning: text("reasoning"),
        judge_model: text("judge_model"),
        cited_urls: payload
            .get("cited_urls")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    });
}

/// Layers 3a and 3b, rendered by the shared reporter.
fn print_layer3_section(tally: &Tally, window: Window, markdown_out: Option<&std::path::Path>) {
    println!();
    println!("== LAYERS 3a/3b — what the engines say about us =====================");
    if tally.layer3.is_empty() {
        println!();
        println!(
            "No geo.sov.probed or geo.interrogation.probed events in this window. Run \
             `geo probe` (see docs/runbooks/GEO_MEASUREMENT.md)."
        );
        return;
    }
    let mut layer = Layer3 {
        provenance: format!(
            "Read back from Core over {} → {}. Answer texts are not stored, so observed \
             vocabulary is only available from a fresh `geo probe` sweep.",
            window.since_str(),
            window.until_str()
        ),
        ..Default::default()
    };
    layer.sov.clone_from(&tally.layer3.sov);
    layer.interrogation.clone_from(&tally.layer3.interrogation);
    let rendered = layer.render();
    println!();
    println!("{rendered}");
    if let Some(path) = markdown_out {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        match std::fs::write(path, &rendered) {
            Ok(()) => println!("wrote {}", path.display()),
            Err(e) => eprintln!("WARNING: could not write {}: {e}", path.display()),
        }
    }
}

/// The window a report covers.
#[derive(Clone, Copy)]
pub struct Window {
    pub since: DateTime<Utc>,
    pub until: DateTime<Utc>,
}

impl Window {
    /// RFC 3339 start, second resolution, UTC.
    pub fn since_str(self) -> String {
        self.since.to_rfc3339_opts(SecondsFormat::Secs, true)
    }

    /// RFC 3339 end, second resolution, UTC.
    pub fn until_str(self) -> String {
        self.until.to_rfc3339_opts(SecondsFormat::Secs, true)
    }
}

/// Run a live report against the gateway.
pub async fn run_live(
    window: Window,
    max_events: u64,
    page_size: u32,
    markdown_out: Option<&std::path::Path>,
) -> Result<()> {
    let config = GeoConfig::from_env()?;
    let client = config
        .query_client()
        .with_context(|| format!("could not build a gateway client for {}", config.api_url()))?;

    let params = QueryEventsParams::new()
        .event_type_prefix(EVENT_TYPE_PREFIX)
        .since(&window.since_str())
        .until(&window.until_str())
        .limit(page_size);

    let mut pages = client.query_events_paged(params);
    let mut tally = Tally::default();

    while let Some(page) = pages
        .next_page()
        .await
        .context("gateway query for geo.* events failed")?
    {
        for event in page {
            tally.record_event(&event.event_type, &event.entity_id, &event.payload);
        }
        if tally.total_events >= max_events {
            tally.truncated = true;
            break;
        }
    }

    println!(
        "GEO report — {} → {}",
        window.since_str(),
        window.until_str()
    );
    println!("source: {} (Control Plane gateway)", config.api_url());
    println!();
    print_table(&tally);
    print_referral_section(&tally.referral);
    print_crawl_section(&tally.crawl, window);
    print_layer3_section(&tally, window, markdown_out);
    print_selfreport_section(&tally.selfreport);
    print_reconciliation(&tally.referral, &tally.selfreport);
    Ok(())
}

/// Run a dry-run report: no gateway, no credential.
///
/// Prints the canonical example of every event type through the emitter's
/// dry-run path — the exact JSON a live emit would POST — then tallies those
/// same events. That makes this a self-contained smoke test of contract →
/// emitter → report with nothing to burn.
pub fn run_dry_run(window: Window, markdown_out: Option<&std::path::Path>) -> Result<()> {
    println!("GEO report — DRY RUN (no gateway call, no API key needed)");
    println!(
        "would query: GET /api/v1/events/query?event_type_prefix={EVENT_TYPE_PREFIX}&since={}&until={}",
        window.since_str(),
        window.until_str()
    );
    println!();
    println!("-- envelopes the emitter would POST ------------------------------");

    let events = samples::canonical_events();
    let mut tally = Tally::default();
    for event in &events {
        let envelope = IngestEnvelope::build(event)?;
        println!("{}", envelope.to_pretty_json()?);
        tally.record_event(&envelope.event_type, &envelope.entity_id, &envelope.payload);
    }

    println!();
    println!("-- tally over the sample above -----------------------------------");
    println!();
    print_table(&tally);
    print_referral_section(&tally.referral);
    print_crawl_section(&tally.crawl, window);
    print_layer3_section(&tally, window, markdown_out);
    print_selfreport_section(&tally.selfreport);
    print_reconciliation(&tally.referral, &tally.selfreport);
    Ok(())
}

/// Layer 1. Small on purpose, and caveated on purpose.
fn print_referral_section(tally: &ReferralTally) {
    println!();
    println!("== LAYER 1 — arrivals from AI surfaces ==============================");
    if tally.is_empty() {
        println!("No geo.referral.observed events in this window.");
        println!(
            "Either nobody arrived from an AI surface, or — far more likely — every arrival \
             misattributed as Direct. See the caveat below."
        );
    } else {
        println!("{:<28} {:>10}", "SURFACE", "ARRIVALS");
        println!("{}", "-".repeat(40));
        for (surface, count) in tally.by_surface() {
            println!("{surface:<28} {count:>10}");
        }
        println!("{}", "-".repeat(40));
        println!("{:<28} {:>10}", "converted", tally.converted());
    }
    println!();
    println!(
        "CAVEAT — this is a FLOOR, not a measurement. ChatGPT, Claude and most assistant\n\
         surfaces strip or omit the referrer, so the majority of AI-referred sessions land\n\
         in 'Direct' and never appear above. Treat a small number here as 'the referrer\n\
         survived rarely', never as 'the channel is small'. Layer 4 (self-report, prompt\n\
         026) is the only layer that sees through a stripped referrer."
    );
}

/// Layer 2. Three categories, never blended, median-smoothed.
fn print_crawl_section(tally: &CrawlTally, window: Window) {
    println!();
    println!("== LAYER 2 — verified AI bot hits, by category ======================");
    println!("taxonomy v{TAXONOMY_VERSION} · verified hits only · weeks start Monday 00:00 UTC");

    if tally.is_empty() {
        println!();
        println!(
            "No geo.crawl.observed events in this window. Run `geo crawl` over the edge logs \
             (see docs/runbooks/GEO_MEASUREMENT.md)."
        );
        return;
    }

    let weeks = tally.weeks(window);
    let by_category = tally.by_category();
    for category in BotCategory::ALL {
        let series = by_category.get(category.as_str());
        let counts: Vec<u64> = weeks
            .iter()
            .map(|w| series.and_then(|s| s.get(w)).map_or(0, |c| c.hits))
            .collect();
        let medians = rolling_median(&counts, MEDIAN_WEEKS);
        let total: u64 = counts.iter().sum();

        println!();
        println!("-- {} --", category.label());
        println!("   {}", category.means());
        println!(
            "   {:<12} {:>8} {:>8} {:>14}",
            "WEEK OF", "HITS", "PATHS", "MEDIAN(4w)"
        );
        for (i, week) in weeks.iter().enumerate() {
            let cell = series.and_then(|s| s.get(week));
            println!(
                "   {:<12} {:>8} {:>8} {:>14.1}",
                week.format("%Y-%m-%d"),
                counts[i],
                cell.map_or(0, |c| c.paths.len()),
                medians[i]
            );
        }
        println!("   {:<12} {total:>8}", "TOTAL");

        // Top paths across the whole window for this category only.
        let mut paths: BTreeMap<&str, u64> = BTreeMap::new();
        if let Some(series) = series {
            for cell in series.values() {
                for (path, hits) in &cell.paths {
                    *paths.entry(path.as_str()).or_default() += hits;
                }
            }
        }
        if paths.is_empty() {
            println!("   top paths: (none)");
        } else {
            let mut ranked: Vec<(&str, u64)> = paths.into_iter().collect();
            ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
            println!("   top paths:");
            for (path, hits) in ranked.iter().take(5) {
                println!("     {hits:>6}  {path}");
            }
        }
    }

    println!();
    println!(
        "NOTE: there is deliberately no all-bots total. A training crawler, a search indexer\n\
         and a user-triggered fetcher answer three different questions — infrastructure\n\
         readiness, citation eligibility, and live human demand — and their volumes differ by\n\
         orders of magnitude. A blended number answers none of them."
    );
    let unverified_hits = tally.unverified_hits();
    if unverified_hits > 0 {
        println!();
        println!(
            "{unverified_hits} unverified hit(s) are stored in this window and are excluded from\n\
             every count above (they were ingested with --include-unverified). A user agent is a\n\
             string the client chose; only IP-verified hits are counted as AI crawl volume."
        );
    }
    let aggregated_rows = tally.aggregated_rows();
    if aggregated_rows > 0 {
        println!();
        println!(
            "{aggregated_rows} row(s) in this window are aggregated buckets, not single hits —\n\
             HITS above sums the payloads' `hits` field, so the totals are still hit counts."
        );
    }
    let unknown_categories = tally.unknown_categories();
    if !unknown_categories.is_empty() {
        println!();
        println!("categories this binary's taxonomy does not know (a newer ingest wrote them):");
        for (name, hits) in &unknown_categories {
            println!("  {name:<40} {hits:>8}");
        }
    }
}

/// Layer 4. The only layer that survives a stripped referrer.
fn print_selfreport_section(tally: &SelfReportTally) {
    println!();
    println!("== LAYER 4 — self-report: what people say sent them =================");

    if tally.is_empty() {
        println!();
        println!(
            "No geo.selfreport.captured events in this window. Either nobody has signed up,\n\
             or the capture is not wired: check the onboarding question in apps/web and the\n\
             optional discovery_source field on POST /api/v1/onboard/start\n\
             (see docs/runbooks/GEO_MEASUREMENT.md)."
        );
        return;
    }

    let total = tally.total();
    let ai = tally.ai_sourced();

    println!();
    println!("{:<34} {:>8} {:>8}  AI?", "DISCOVERY SOURCE", "SIGNUPS", "SHARE");
    println!("{}", "-".repeat(62));
    for (surface, count) in tally.by_surface() {
        println!(
            "{:<34} {:>8} {:>8}  {}",
            geo_core::discovery_label(&surface),
            count,
            percent_str(count, total),
            if geo_core::is_ai_source(&surface) {
                "AI"
            } else {
                ""
            },
        );
    }
    println!("{}", "-".repeat(62));
    println!(
        "{:<34} {:>8} {:>8}",
        "AI-SOURCED",
        ai,
        percent_str(ai, total)
    );
    println!("{:<34} {:>8}", "captures in window", total);

    // Which path collected them. Never blended into one number: the API path
    // is the one that reaches agents and headless signups, and "is it
    // capturing anything at all" is a question a blended total cannot answer.
    println!();
    println!("-- by capture path --");
    let by_path = tally.by_capture_path();
    for path in geo_core::capture_path::ALL {
        let count = by_path.get(*path).copied().unwrap_or(0);
        println!(
            "   {:<32} {:>8} captures, {:>8} AI-sourced ({})",
            geo_core::capture_path::label(path),
            count,
            tally.ai_on_path(path),
            percent_str(tally.ai_on_path(path), count),
        );
    }
    for (path, count) in &by_path {
        if !geo_core::capture_path::ALL.contains(path) {
            println!("   {path:<32} {count:>8} captures  (unknown capture path)");
        }
    }

    // Paid conversions. This is the line that turns GEO from a traffic story
    // into a revenue story, so it is reported with its denominator visible.
    let (paid, ai_paid, untiered) = tally.paid_split();
    println!();
    println!("-- paid conversions by discovery source --");
    if paid == 0 {
        println!(
            "   No capture in this window carries a paid tier ({}). Nothing to report —\n   \
             this is not 0% AI-sourced revenue, it is no observed revenue.",
            PAID_TIERS.join(" / ")
        );
    } else {
        println!(
            "   {ai_paid} of {paid} paid signups ({}) said an AI sent them.",
            percent_str(ai_paid, paid).trim()
        );
    }
    if untiered > 0 {
        println!(
            "   {untiered} capture(s) carry no tier and are excluded from both sides above."
        );
    }

    // The free text. Printed whole, in the buyer's words, because this is the
    // only first-party record of how buyers actually phrase the problem — and
    // it is what prompt 027 folds into the probe set.
    let verbatims = tally.verbatims();
    println!();
    println!("-- what they asked the assistant (verbatim) --");
    if verbatims.is_empty() {
        println!("   (nobody has filled the optional free-text field yet)");
    } else {
        for (surface, text) in &verbatims {
            println!("   [{}] {text}", geo_core::discovery_label(surface));
        }
    }

    let unknown = tally.unknown_sources();
    if !unknown.is_empty() {
        println!();
        println!(
            "discovery sources this binary's vocabulary does not know (a newer producer wrote\n\
             them). They are NOT counted as AI-sourced above — rebuild rather than assume:"
        );
        for (id, count) in &unknown {
            println!("  {id:<40} {count:>8}");
        }
    }
}

/// Layers 1 and 4 on the same line. The gap between them *is* the finding.
fn print_reconciliation(referral: &ReferralTally, selfreport: &SelfReportTally) {
    println!();
    println!("== LAYER 1 vs LAYER 4 — the attribution gap ========================");

    let answered = selfreport.total();
    if answered == 0 {
        println!();
        println!(
            "No self-reports in this window, so there is nothing to reconcile against.\n\
             Layer 1 alone is a floor and must not be read as a channel size."
        );
        return;
    }

    // Both numbers are stated over the SAME denominator — the signups that
    // answered the question — because a gap between two differently-normalised
    // percentages is not a gap, it is an arithmetic accident.
    let referred_conversions = referral.converted();
    let ai_reported = selfreport.ai_sourced();

    println!();
    println!(
        "   analytics (layer 1, referrer survived) : {:>5} of {answered}  {}",
        referred_conversions,
        percent_str(referred_conversions, answered)
    );
    println!(
        "   humans    (layer 4, they told us)      : {:>5} of {answered}  {}",
        ai_reported,
        percent_str(ai_reported, answered)
    );

    match (
        percent(referred_conversions, answered),
        percent(ai_reported, answered),
    ) {
        (Some(analytics), Some(humans)) if analytics > 0.0 => {
            println!();
            println!(
                "   Humans report {:.1}x what the referrer shows.",
                humans / analytics
            );
        }
        (Some(_), Some(humans)) if humans > 0.0 => {
            println!();
            println!(
                "   The referrer survived ZERO times while {humans:.1}% of people said an AI\n   \
                 sent them. That is the undercount at its most extreme, not an absence of\n   \
                 AI traffic."
            );
        }
        _ => {}
    }

    // The web-only slice, because layer 1 structurally cannot see the API path
    // — an agent calling /onboard/start never had a browser session to strip a
    // referrer from, so including it would overstate the gap.
    let web = geo_core::capture_path::WEB;
    let web_answered = selfreport.on_path(web);
    if web_answered > 0 && web_answered != answered {
        println!();
        println!(
            "   web-signup captures only            : {:>5} of {web_answered}  {} said an AI sent them",
            selfreport.ai_on_path(web),
            percent_str(selfreport.ai_on_path(web), web_answered)
        );
        println!(
            "   (layer 1 cannot see the {} captures from {} — an agent\n   \
             calling the API never had a browser session to strip a referrer from.)",
            answered - web_answered,
            geo_core::capture_path::label(geo_core::capture_path::API)
        );
    }

    println!();
    println!(
        "HOW TO READ THIS. The two rows share a denominator (signups that answered the\n\
         question) so they can sit on one line, but they are not the same measurement:\n\
         the top row counts sessions whose AI referrer survived AND that were seen to\n\
         convert; the bottom counts people who said so. The gap is the referrer\n\
         undercount, and it is the reason layer 4 exists. Published work on this\n\
         repeatedly finds self-report in double digits where analytics reports under 1%.\n\
         Neither row is a channel size: the top is a floor, the bottom is a sample of\n\
         the people who chose to answer an optional question."
    );
}

fn print_table(tally: &Tally) {
    println!("-- event inventory (stored rows, not bot hits) ---------------------");
    println!(
        "{:<5} {:<22} {:<28} {:>7} {:>9}  PRODUCED BY",
        "LAYER", "NAME", "EVENT TYPE", "EVENTS", "ENTITIES"
    );
    println!("{}", "-".repeat(92));

    for layer in LAYERS {
        for (i, &event_type) in layer.types.iter().enumerate() {
            let (id, name) = if i == 0 {
                (layer.id, layer.name)
            } else {
                ("", "")
            };
            println!(
                "{:<5} {:<22} {:<28} {:>7} {:>9}  {}",
                id,
                name,
                event_type.as_str(),
                tally.events_for(event_type),
                tally.entities_for(event_type),
                if i == 0 { layer.produced_by } else { "" },
            );
        }
    }

    println!("{}", "-".repeat(92));
    println!(
        "{:<5} {:<22} {:<28} {:>7} {:>9}",
        "", "TOTAL", "", tally.total_events, ""
    );

    if !tally.unknown_types.is_empty() {
        println!();
        println!("unrecognised geo.* types (a newer producer is writing to this stream):");
        for (name, count) in &tally.unknown_types {
            println!("  {name:<40} {count:>7}");
        }
    }

    if tally.truncated {
        println!();
        println!("NOTE: hit --max-events; counts above are a truncated prefix of the window.");
    }

    if tally.total_events == 0 {
        println!();
        println!(
            "No geo.* events in this window. Nothing has been instrumented yet — the layer \
             slices (prompts 024/025/026) are what start filling this table."
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_type_belongs_to_exactly_one_layer() {
        for event_type in GeoEventType::ALL {
            let hits = LAYERS
                .iter()
                .filter(|l| l.types.contains(&event_type))
                .count();
            assert_eq!(hits, 1, "{event_type} is in {hits} layers, expected 1");
        }
    }

    #[test]
    fn the_layer_map_invents_no_event_types() {
        for layer in LAYERS {
            for t in layer.types {
                assert!(GeoEventType::ALL.contains(t), "{t} is not in the contract");
            }
        }
    }

    #[test]
    fn tally_counts_events_and_distinct_entities_separately() {
        let mut tally = Tally::default();
        // Same entity twice — a replay. Two events, one entity.
        tally.record("geo.crawl.observed", "geo:crawl:abc");
        tally.record("geo.crawl.observed", "geo:crawl:abc");
        tally.record("geo.crawl.observed", "geo:crawl:def");
        assert_eq!(tally.events_for(GeoEventType::CrawlObserved), 3);
        assert_eq!(tally.entities_for(GeoEventType::CrawlObserved), 2);
    }

    fn selfreport(surface: &str, path: &str, tier: Option<&str>, verbatim: Option<&str>) -> Value {
        serde_json::json!({
            "schema_version": 1,
            "observed_at": "2026-08-11T11:30:00Z",
            "source": path,
            "surface": surface,
            "verbatim": verbatim,
            "contact_ref": "tenant-x",
            "tier": tier,
        })
    }

    #[test]
    fn selfreport_folds_by_entity_so_a_restate_is_not_a_second_signup() {
        let mut tally = SelfReportTally::default();
        let web = geo_core::capture_path::WEB;
        // Same entity twice: the second is a tier correction, not a new signup.
        tally.record("geo:selfreport:a", &selfreport("chatgpt", web, Some("trial"), None));
        tally.record("geo:selfreport:a", &selfreport("chatgpt", web, Some("indie"), None));
        tally.record("geo:selfreport:b", &selfreport("github", web, Some("indie"), None));
        assert_eq!(tally.total(), 2);
        assert_eq!(tally.ai_sourced(), 1);
        // Latest version wins, so the corrected tier is the one counted.
        assert_eq!(tally.paid_split(), (2, 1, 0));
    }

    #[test]
    fn an_unknown_discovery_source_is_never_counted_as_ai() {
        let mut tally = SelfReportTally::default();
        let web = geo_core::capture_path::WEB;
        tally.record("geo:selfreport:a", &selfreport("chatgpt-6", web, None, None));
        assert_eq!(tally.ai_sourced(), 0);
        assert_eq!(tally.unknown_sources().get("chatgpt-6"), Some(&1));
    }

    #[test]
    fn a_trial_tier_is_not_revenue_and_a_missing_tier_is_not_a_free_signup() {
        let mut tally = SelfReportTally::default();
        let web = geo_core::capture_path::WEB;
        tally.record("geo:selfreport:a", &selfreport("chatgpt", web, Some("trial"), None));
        tally.record("geo:selfreport:b", &selfreport("chatgpt", web, None, None));
        tally.record("geo:selfreport:c", &selfreport("chatgpt", web, Some("scale"), None));
        // 1 paid, 1 of them AI-sourced, 1 with no tier recorded at all.
        assert_eq!(tally.paid_split(), (1, 1, 1));
    }

    #[test]
    fn capture_paths_are_counted_separately() {
        let mut tally = SelfReportTally::default();
        tally.record(
            "geo:selfreport:a",
            &selfreport("chatgpt", geo_core::capture_path::WEB, None, None),
        );
        tally.record(
            "geo:selfreport:b",
            &selfreport("claude", geo_core::capture_path::API, None, None),
        );
        tally.record(
            "geo:selfreport:c",
            &selfreport("github", geo_core::capture_path::API, None, None),
        );
        assert_eq!(tally.on_path(geo_core::capture_path::WEB), 1);
        assert_eq!(tally.on_path(geo_core::capture_path::API), 2);
        assert_eq!(tally.ai_on_path(geo_core::capture_path::API), 1);
    }

    #[test]
    fn verbatims_are_kept_whole() {
        let mut tally = SelfReportTally::default();
        let web = geo_core::capture_path::WEB;
        tally.record(
            "geo:selfreport:a",
            &selfreport("chatgpt", web, None, Some("best event store for agent memory?")),
        );
        // An empty string is not an answer and must not render as a blank row.
        tally.record("geo:selfreport:b", &selfreport("claude", web, None, Some("   ")));
        assert_eq!(
            tally.verbatims(),
            vec![("chatgpt", "best event store for agent memory?")]
        );
    }

    #[test]
    fn a_zero_denominator_is_not_zero_percent() {
        assert_eq!(percent(0, 0), None);
        assert_eq!(percent_str(0, 0).trim(), "n/a");
        assert_eq!(percent(1, 4), Some(25.0));
    }

    #[test]
    fn the_selfreport_tally_is_fed_by_record_event() {
        let mut tally = Tally::default();
        tally.record_event(
            "geo.selfreport.captured",
            "geo:selfreport:a",
            &selfreport("chatgpt", geo_core::capture_path::WEB, Some("trial"), None),
        );
        assert_eq!(tally.selfreport.total(), 1);
        assert_eq!(tally.entities_for(GeoEventType::SelfReportCaptured), 1);
    }

    #[test]
    fn unknown_types_are_surfaced_not_dropped() {
        let mut tally = Tally::default();
        tally.record("geo.brand.new", "geo:brand:1");
        assert_eq!(tally.total_events, 1);
        assert_eq!(tally.unknown_types.get("geo.brand.new"), Some(&1));
    }

    // ── layer 2 reporting ───────────────────────────────────────────────

    fn crawl_payload(observed_at: &str, category: &str, path: &str, verified: bool) -> Value {
        serde_json::json!({
            "schema_version": 1,
            "observed_at": observed_at,
            "bot": "gptbot",
            "category": category,
            "taxonomy_version": TAXONOMY_VERSION,
            "verified": verified,
            "user_agent": "GPTBot/1.2",
            "path": path,
            "status": 200,
            "source": "file:test",
            "aggregation": "hit",
            "hits": 1,
            "window_end": null,
            "request_id": null,
        })
    }

    #[test]
    fn weeks_start_on_monday_utc() {
        // 2026-08-11 is a Tuesday.
        let tuesday = DateTime::parse_from_rfc3339("2026-08-11T23:59:59Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            week_start(tuesday).to_rfc3339(),
            "2026-08-10T00:00:00+00:00"
        );
        // A Monday is its own week start.
        let monday = DateTime::parse_from_rfc3339("2026-08-10T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(week_start(monday), monday);
    }

    #[test]
    fn a_burst_moves_a_mean_but_not_the_median() {
        // The whole reason the report smooths with a median: week 4 is a
        // single crawl burst. The mean would nearly double; the median holds.
        let counts = [10, 12, 11, 400];
        let medians = rolling_median(&counts, 4);
        let mean = counts.iter().sum::<u64>() as f64 / 4.0;
        assert_eq!(medians[3], 11.5);
        assert!(mean > 100.0, "{mean}");
    }

    #[test]
    fn a_rolling_median_uses_a_short_prefix_rather_than_emitting_nothing() {
        let medians = rolling_median(&[5], 4);
        assert_eq!(medians, vec![5.0]);
        let medians = rolling_median(&[4, 8], 4);
        assert_eq!(medians, vec![4.0, 6.0]);
    }

    #[test]
    fn the_rolling_window_forgets_old_weeks() {
        let medians = rolling_median(&[100, 100, 100, 1, 1, 1, 1], 4);
        assert_eq!(*medians.last().unwrap(), 1.0);
    }

    #[test]
    fn categories_are_tallied_separately_and_never_summed() {
        let mut tally = CrawlTally::default();
        tally.record("e1", &crawl_payload("2026-08-10T01:00:00Z", "training_crawler", "/a", true));
        tally.record("e2", &crawl_payload("2026-08-10T02:00:00Z", "training_crawler", "/b", true));
        tally.record("e3", &crawl_payload("2026-08-10T03:00:00Z", "user_fetcher", "/a", true));

        let by_category = tally.by_category();
        assert_eq!(by_category.len(), 2);
        let week = week_start(
            DateTime::parse_from_rfc3339("2026-08-10T01:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        );
        assert_eq!(by_category["training_crawler"][&week].hits, 2);
        assert_eq!(by_category["training_crawler"][&week].paths.len(), 2);
        assert_eq!(by_category["user_fetcher"][&week].hits, 1);
    }

    #[test]
    fn a_re_ingested_window_does_not_double_the_hit_count() {
        // The bug this fold exists to prevent: re-running `geo crawl` over the
        // same log window writes version 2 of the SAME entity, so summing raw
        // events would report twice the traffic that actually happened.
        let mut tally = CrawlTally::default();
        let payload = crawl_payload("2026-08-10T01:00:00Z", "training_crawler", "/a", true);
        tally.record("geo:crawl:abc", &payload);
        tally.record("geo:crawl:abc", &payload);

        let week = week_start(
            DateTime::parse_from_rfc3339("2026-08-10T01:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        );
        assert_eq!(tally.by_category()["training_crawler"][&week].hits, 1);
    }

    #[test]
    fn unverified_rows_never_reach_a_category() {
        let mut tally = CrawlTally::default();
        tally.record("e1", &crawl_payload("2026-08-10T01:00:00Z", "training_crawler", "/a", false));
        assert!(tally.by_category().is_empty());
        assert_eq!(tally.unverified_hits(), 1);
    }

    #[test]
    fn an_unknown_category_is_surfaced_not_folded_into_a_neighbour() {
        let mut tally = CrawlTally::default();
        tally.record("e1", &crawl_payload("2026-08-10T01:00:00Z", "shopping_agent", "/a", true));
        assert!(tally.by_category().is_empty());
        assert_eq!(tally.unknown_categories().get("shopping_agent"), Some(&1));
    }

    #[test]
    fn an_aggregated_row_contributes_its_hit_count_not_one() {
        let mut tally = CrawlTally::default();
        let mut payload = crawl_payload("2026-08-10T00:00:00Z", "search_indexer", "/docs", true);
        payload["aggregation"] = Value::from("daily");
        payload["hits"] = Value::from(42);
        payload["window_end"] = Value::from("2026-08-11T00:00:00Z");
        tally.record("e1", &payload);

        let week = week_start(
            DateTime::parse_from_rfc3339("2026-08-10T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        );
        assert_eq!(tally.by_category()["search_indexer"][&week].hits, 42);
        assert_eq!(tally.aggregated_rows(), 1);
    }

    #[test]
    fn the_week_axis_is_contiguous_so_a_quiet_week_reads_as_zero() {
        let mut tally = CrawlTally::default();
        tally.record("e1", &crawl_payload("2026-07-06T00:00:00Z", "training_crawler", "/a", true));
        let window = Window {
            since: DateTime::parse_from_rfc3339("2026-07-06T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            until: DateTime::parse_from_rfc3339("2026-08-03T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };
        let weeks = tally.weeks(window);
        assert_eq!(weeks.len(), 4, "{weeks:?}");
        for pair in weeks.windows(2) {
            assert_eq!(pair[1] - pair[0], Duration::weeks(1));
        }
    }

    #[test]
    fn a_referral_tally_counts_surfaces_and_conversions() {
        let mut tally = ReferralTally::default();
        tally.record("e1", &serde_json::json!({"surface": "chatgpt.com", "converted": false}));
        tally.record("e2", &serde_json::json!({"surface": "chatgpt.com", "converted": false}));
        tally.record("e3", &serde_json::json!({"surface": "perplexity.ai", "converted": false}));
        // e2 converts: a SECOND VERSION of the same entity, not a new session.
        tally.record("e2", &serde_json::json!({"surface": "chatgpt.com", "converted": true}));

        assert_eq!(tally.by_surface()["chatgpt.com"], 2);
        assert_eq!(tally.by_surface()["perplexity.ai"], 1);
        assert_eq!(tally.converted(), 1);
    }

    #[test]
    fn the_layer_map_attributes_referral_and_crawl_to_this_slice() {
        // Both layer 1 and layer 2 ship in prompt 024; an earlier draft
        // mis-attributed referral to 026 and the runbook table with it.
        let by_id = |id: &str| LAYERS.iter().find(|l| l.id == id).expect("layer exists");
        assert_eq!(by_id("1").produced_by, "prompt 024");
        assert_eq!(by_id("2").produced_by, "prompt 024");
        assert_eq!(by_id("4").produced_by, "prompt 026");
    }
}
