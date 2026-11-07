//! `geo crawl` — layer 2. Turn edge access logs into verified
//! `geo.crawl.observed` events.
//!
//! # The pipeline
//!
//! ```text
//! log source → parse → window filter → identify bot → verify IP → categorise
//!            → aggregate → emit
//! ```
//!
//! Every stage is counted, and the counts are printed. That is not decoration:
//! a verification bug that silently drops every bot looks *identical* to "no
//! AI traffic", and that misread would derail the whole programme. If the
//! run identified 400 bot hits and verified 0, the summary says so in a line
//! you cannot miss.
//!
//! # Where logs come from
//!
//! `--file` is the workhorse and the only source that needs no credential: a
//! Vercel **log drain** writes exactly the NDJSON this parses. `--vercel-project`
//! and `--fly-app` shell out to the vendor CLIs (the only part of this that is
//! not Rust, and only to *retrieve* bytes — every decision about those bytes is
//! made here). Both vendor CLIs are best-effort by nature: `vercel logs` serves
//! a recent window, not history, so a durable layer-2 series wants a drain.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Datelike, Duration, TimeZone, Timelike, Utc};
use geo_core::{
    Aggregation, CrawlObserved, EmitMode, EmitOutcome, GeoEmitter, GeoEvent, IngestEnvelope,
    TAXONOMY_VERSION, Verdict,
    bots::{self, BotCategory, BotSpec, RangeCatalog},
};

use crate::{
    logs::{self, AccessLine, LogFormat},
    ranges::{self, RangeSource},
    report::Window,
};

/// How many envelopes `--dry-run` prints in full before switching to counts.
/// Enough to eyeball the shape, few enough to stay readable.
const DRY_RUN_SAMPLE: usize = 2;

/// Above this share of unreadable lines, the run refuses rather than reporting
/// a number built from a fraction of the log.
const MAX_UNPARSEABLE_RATIO: f64 = 0.5;

/// Where the log bytes come from.
#[derive(Debug, Clone)]
pub enum LogInput {
    /// A file on disk (a log-drain dump, or a fixture).
    File(PathBuf),
    /// `vercel logs <project> --json`.
    Vercel(String),
    /// `fly logs -a <app> --no-tail --json`.
    Fly(String),
}

impl LogInput {
    /// The value stored in `geo.crawl.observed.source`, so a reader can tell
    /// which edge a row came from.
    fn source_label(&self) -> String {
        match self {
            Self::File(path) => format!(
                "file:{}",
                path.file_name().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().to_string()
                )
            ),
            Self::Vercel(project) => format!("vercel:{project}"),
            Self::Fly(app) => format!("fly:{app}"),
        }
    }

    /// Fetch the raw log body.
    fn read(&self) -> Result<String> {
        match self {
            Self::File(path) => std::fs::read_to_string(path)
                .with_context(|| format!("could not read log file {}", path.display())),
            Self::Vercel(project) => run_cli(
                "vercel",
                &["logs", project, "--json"],
                "install it with `bun add -g vercel` and run `vercel login`",
            ),
            Self::Fly(app) => run_cli(
                "fly",
                &["logs", "-a", app, "--no-tail", "--json"],
                "install flyctl and run `fly auth login`",
            ),
        }
    }
}

/// Shell out to a vendor CLI. Retrieval only — nothing about the bytes is
/// interpreted out here.
fn run_cli(program: &str, args: &[&str], hint: &str) -> Result<String> {
    let output = Command::new(program).args(args).output().map_err(|e| {
        anyhow::anyhow!("could not run `{program} {}`: {e}\n{hint}", args.join(" "))
    })?;
    if !output.status.success() {
        bail!(
            "`{program} {}` exited {}\nstderr: {}\n{hint}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Everything `geo crawl` needs to run.
pub struct CrawlOptions {
    pub window: Window,
    pub inputs: Vec<LogInput>,
    pub format: LogFormat,
    pub aggregation: Aggregation,
    pub ranges_dir: Option<PathBuf>,
    pub offline: bool,
    pub include_unverified: bool,
    pub mode: EmitMode,
}

/// One bot hit, after identification and verification.
#[derive(Debug, Clone)]
struct Classified {
    line: AccessLine,
    spec: &'static BotSpec,
    verdict: Verdict,
    source: String,
}

/// Counts for the run summary. Every stage, so a drop is attributable.
#[derive(Debug, Default)]
struct Counters {
    parsed: usize,
    blank: usize,
    unparseable: usize,
    outside_window: usize,
    not_a_bot: usize,
    identified: usize,
    verified: usize,
    rejected: usize,
    unverifiable: BTreeMap<&'static str, usize>,
    samples: Vec<String>,
}

/// Run the ingest.
pub async fn run(options: CrawlOptions) -> Result<()> {
    if options.inputs.is_empty() {
        bail!(
            "geo crawl needs a log source: --file <path> (a Vercel log-drain dump), \
             --vercel-project <name>, or --fly-app <name>.\n\
             See docs/runbooks/GEO_MEASUREMENT.md for how to stand up the drain."
        );
    }

    // Independent I/O, so overlap it: the range lists are fetched while the
    // log bodies are read off disk / out of the vendor CLIs.
    let ranges_future = ranges::load(options.ranges_dir.as_deref(), options.offline);
    let mut counters = Counters::default();
    let mut classified_input: Vec<(String, logs::ParseReport)> = Vec::new();
    for input in &options.inputs {
        let body = input.read()?;
        classified_input.push((input.source_label(), logs::parse(&body, options.format)));
    }
    let (catalog, sources) = ranges_future.await?;

    let classified = classify(classified_input, &catalog, options.window, &mut counters);

    print_preamble(&options, &sources, &counters);

    if counters.unparseable_ratio() > MAX_UNPARSEABLE_RATIO {
        bail!(
            "{:.0}% of log lines were unreadable — refusing to report a crawl number built \
             from a fraction of the log. Check --format (tried {:?}) against the samples above.",
            counters.unparseable_ratio() * 100.0,
            options.format
        );
    }

    let events = build_events(&classified, options.aggregation, options.include_unverified);
    print_categories(&classified, options.aggregation, &events);

    if !sources.iter().any(|s| s.outcome.is_loaded()) {
        eprintln!(
            "\nWARNING: no vendor range list loaded, so nothing could be verified. \
             Zero verified hits here means 'we could not check', not 'no AI traffic'."
        );
    }
    if counters.identified > 0 && counters.verified == 0 {
        eprintln!(
            "\nWARNING: {} bot hits were identified and 0 verified. That is either a \
             genuine spoofing wave or a verification bug — do not read it as a crawl \
             number until you know which.",
            counters.identified
        );
    }

    emit(&events, options.mode).await
}

impl Counters {
    fn unparseable_ratio(&self) -> f64 {
        let considered = self.parsed + self.unparseable;
        if considered == 0 {
            return 0.0;
        }
        self.unparseable as f64 / considered as f64
    }
}

/// Parse reports → identified, verified bot hits.
fn classify(
    inputs: Vec<(String, logs::ParseReport)>,
    catalog: &RangeCatalog,
    window: Window,
    counters: &mut Counters,
) -> Vec<Classified> {
    let mut out = Vec::new();
    for (source, report) in inputs {
        counters.parsed += report.lines.len();
        counters.blank += report.blank;
        counters.unparseable += report.unparseable;
        for sample in report.samples {
            if counters.samples.len() < logs::MAX_SAMPLES {
                counters.samples.push(sample);
            }
        }

        for line in report.lines {
            if line.timestamp < window.since || line.timestamp >= window.until {
                counters.outside_window += 1;
                continue;
            }
            let Some(spec) = bots::identify(&line.user_agent) else {
                counters.not_a_bot += 1;
                continue;
            };
            counters.identified += 1;
            let verdict = catalog.verify(spec, line.client_ip);
            match &verdict {
                Verdict::Verified => counters.verified += 1,
                Verdict::Rejected => counters.rejected += 1,
                other => {
                    *counters.unverifiable.entry(other.as_str()).or_default() += 1;
                }
            }
            out.push(Classified {
                line,
                spec,
                verdict,
                source: source.clone(),
            });
        }
    }
    out
}

/// Truncate a timestamp to the start of its aggregation bucket.
fn bucket_start(ts: DateTime<Utc>, aggregation: Aggregation) -> DateTime<Utc> {
    match aggregation {
        Aggregation::Hit => ts,
        Aggregation::Hourly => Utc
            .with_ymd_and_hms(ts.year(), ts.month(), ts.day(), ts.hour(), 0, 0)
            .single()
            .unwrap_or(ts),
        Aggregation::Daily => Utc
            .with_ymd_and_hms(ts.year(), ts.month(), ts.day(), 0, 0, 0)
            .single()
            .unwrap_or(ts),
    }
}

fn bucket_end(start: DateTime<Utc>, aggregation: Aggregation) -> Option<DateTime<Utc>> {
    match aggregation {
        Aggregation::Hit => None,
        Aggregation::Hourly => Some(start + Duration::hours(1)),
        Aggregation::Daily => Some(start + Duration::days(1)),
    }
}

/// Which hits become events.
///
/// - `Verified` → always. This is the number.
/// - `Unverifiable*` → only with `--include-unverified`, and stamped
///   `verified: false` so a report can never fold them into the verified
///   count.
/// - `Rejected` → **never**. Someone wearing `GPTBot`'s user agent is not
///   OpenAI crawling us, and storing it as a crawl event would poison the
///   series. It stays in the summary above, where a human can see it.
fn emits(verdict: &Verdict, include_unverified: bool) -> bool {
    match verdict {
        Verdict::Verified => true,
        Verdict::Rejected => false,
        _ => include_unverified,
    }
}

fn build_events(
    classified: &[Classified],
    aggregation: Aggregation,
    include_unverified: bool,
) -> Vec<GeoEvent> {
    let eligible: Vec<&Classified> = classified
        .iter()
        .filter(|c| emits(&c.verdict, include_unverified))
        .collect();

    if aggregation == Aggregation::Hit {
        return eligible
            .into_iter()
            .map(|c| {
                GeoEvent::Crawl(CrawlObserved {
                    schema_version: geo_core::SCHEMA_VERSION,
                    observed_at: c.line.timestamp,
                    bot: c.spec.id.to_string(),
                    category: c.spec.category.as_str().to_string(),
                    taxonomy_version: TAXONOMY_VERSION,
                    verified: c.verdict.is_verified(),
                    user_agent: c.line.user_agent.clone(),
                    path: c.line.path.clone(),
                    status: c.line.status,
                    source: c.source.clone(),
                    aggregation: Aggregation::Hit.as_str().to_string(),
                    hits: 1,
                    window_end: None,
                    request_id: c.line.request_id.clone(),
                })
            })
            .collect();
    }

    // Bucket key: everything the payload would otherwise have to lie about.
    // `verified` is in the key so a verified and an unverified hit never share
    // a row.
    type BucketKey = (
        DateTime<Utc>,
        &'static str,
        String,
        u16,
        String,
        bool,
    );
    let mut buckets: BTreeMap<BucketKey, (u32, String)> = BTreeMap::new();
    for c in eligible {
        let start = bucket_start(c.line.timestamp, aggregation);
        let key = (
            start,
            c.spec.id,
            c.line.path.clone(),
            c.line.status,
            c.source.clone(),
            c.verdict.is_verified(),
        );
        let slot = buckets
            .entry(key)
            .or_insert_with(|| (0, c.line.user_agent.clone()));
        slot.0 += 1;
    }

    buckets
        .into_iter()
        .map(|((start, bot, path, status, source, verified), (hits, user_agent))| {
            let spec = bots::by_id(bot).expect("bucketed bot came from the taxonomy");
            GeoEvent::Crawl(CrawlObserved {
                schema_version: geo_core::SCHEMA_VERSION,
                observed_at: start,
                bot: bot.to_string(),
                category: spec.category.as_str().to_string(),
                taxonomy_version: TAXONOMY_VERSION,
                verified,
                user_agent,
                path,
                status,
                source,
                aggregation: aggregation.as_str().to_string(),
                hits,
                window_end: bucket_end(start, aggregation),
                request_id: None,
            })
        })
        .collect()
}

fn print_preamble(options: &CrawlOptions, sources: &[RangeSource], counters: &Counters) {
    println!(
        "geo crawl — {} → {}",
        options.window.since_str(),
        options.window.until_str()
    );
    println!(
        "taxonomy v{TAXONOMY_VERSION} · aggregation {} · {}",
        options.aggregation,
        match options.mode {
            EmitMode::DryRun => "DRY RUN (nothing is written)",
            EmitMode::Live => "LIVE (writing to the gateway)",
        }
    );
    println!();

    println!("-- log sources ----------------------------------------------------");
    for input in &options.inputs {
        println!("  {}", input.source_label());
    }
    println!();

    println!("-- vendor IP range lists ------------------------------------------");
    for source in sources {
        println!("  {:<62} {}", source.url, source.outcome.describe());
        println!("      verifies: {}", source.bots.join(", "));
    }
    println!();

    println!("-- ingest funnel ---------------------------------------------------");
    println!("  log lines parsed          {:>8}", counters.parsed);
    println!("  blank lines skipped       {:>8}", counters.blank);
    println!("  unreadable lines          {:>8}", counters.unparseable);
    println!("  outside the window        {:>8}", counters.outside_window);
    println!("  not a known bot           {:>8}", counters.not_a_bot);
    println!("  bot hits identified       {:>8}", counters.identified);
    println!("    verified                {:>8}", counters.verified);
    println!(
        "    REJECTED (spoofed UA)   {:>8}   <- claimed a bot, IP outside the vendor's range",
        counters.rejected
    );
    for (reason, count) in &counters.unverifiable {
        println!("    {reason:<48} {count:>4}");
    }
    if !counters.samples.is_empty() {
        println!();
        println!("  unreadable line samples:");
        for sample in &counters.samples {
            println!("    {sample}");
        }
    }
    println!();
}

fn print_categories(classified: &[Classified], aggregation: Aggregation, events: &[GeoEvent]) {
    println!("-- verified hits by category (never blended) ------------------------");
    println!(
        "{:<26} {:>6} {:>7} {:>9}  MEANS",
        "CATEGORY", "BOTS", "HITS", "PATHS"
    );
    println!("{}", "-".repeat(100));

    for category in BotCategory::ALL {
        let hits: Vec<&Classified> = classified
            .iter()
            .filter(|c| c.spec.category == category && c.verdict.is_verified())
            .collect();
        let bots_seen: BTreeSet<&str> = hits.iter().map(|c| c.spec.id).collect();
        let paths: BTreeSet<&str> = hits.iter().map(|c| c.line.path.as_str()).collect();
        println!(
            "{:<26} {:>6} {:>7} {:>9}  {}",
            category.label(),
            bots_seen.len(),
            hits.len(),
            paths.len(),
            category.means()
        );
        for bot in &bots_seen {
            let bot_hits = hits.iter().filter(|c| &c.spec.id == bot).count();
            println!("  └ {bot:<22} {bot_hits:>13}");
        }
    }
    println!("{}", "-".repeat(100));

    let rejected: Vec<&Classified> = classified
        .iter()
        .filter(|c| c.verdict == Verdict::Rejected)
        .collect();
    if rejected.is_empty() {
        println!("rejected (unverified) hits: 0");
    } else {
        println!(
            "REJECTED — claimed a bot identity the vendor's published ranges do not support:"
        );
        for c in &rejected {
            println!(
                "  {} claimed {:<18} from {:<16} {} {}",
                c.line.timestamp.to_rfc3339(),
                c.spec.id,
                c.line
                    .client_ip
                    .map_or_else(|| "?".to_string(), |ip| ip.to_string()),
                c.line.status,
                c.line.path
            );
        }
        println!(
            "  ({} rejected hit(s) — counted here, never emitted as crawl events)",
            rejected.len()
        );
    }

    println!();
    println!(
        "events to emit: {} (aggregation {aggregation})",
        events.len()
    );
}

async fn emit(events: &[GeoEvent], mode: EmitMode) -> Result<()> {
    if events.is_empty() {
        println!("nothing to emit for this window.");
        return Ok(());
    }

    let emitter = GeoEmitter::from_env(mode)?;
    match mode {
        EmitMode::DryRun => {
            println!();
            println!(
                "-- first {} envelope(s) that would be POSTed ----------------------",
                DRY_RUN_SAMPLE.min(events.len())
            );
            for event in events.iter().take(DRY_RUN_SAMPLE) {
                println!("{}", IngestEnvelope::build(event)?.to_pretty_json()?);
            }
            if events.len() > DRY_RUN_SAMPLE {
                println!("... and {} more", events.len() - DRY_RUN_SAMPLE);
            }
            println!();
            println!(
                "DRY RUN — nothing written. Re-run without --dry-run (needs ALLSOURCE_API_KEY \
                 or ALLSOURCE_CLIENT_SIGNING_KEY)."
            );
        }
        EmitMode::Live => {
            let outcomes = emitter.emit_all(events).await?;
            let ingested = outcomes
                .iter()
                .filter(|o| matches!(o, EmitOutcome::Ingested { .. }))
                .count();
            println!();
            println!(
                "emitted {ingested}/{} geo.crawl.observed events to {}",
                events.len(),
                emitter.api_url()
            );
            println!(
                "re-running this exact window is safe: every event's entity_id embeds its \
                 idempotency key, so a replay appends a version rather than a second entity."
            );
        }
    }
    Ok(())
}

/// Turn `--file`/`--vercel-project`/`--fly-app` into inputs.
pub fn inputs_from(files: &[PathBuf], vercel: Option<&str>, fly: Option<&str>) -> Vec<LogInput> {
    let mut out: Vec<LogInput> = files
        .iter()
        .map(|p| LogInput::File(Path::to_path_buf(p)))
        .collect();
    if let Some(project) = vercel {
        out.push(LogInput::Vercel(project.to_string()));
    }
    if let Some(app) = fly {
        out.push(LogInput::Fly(app.to_string()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo_core::bots::{by_id, ipv4};

    fn ts(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .expect("test timestamp")
            .with_timezone(&Utc)
    }

    fn line(path: &str, when: &str, ua: &str, ip: Option<&str>, id: Option<&str>) -> AccessLine {
        AccessLine {
            timestamp: ts(when),
            client_ip: ip.map(|s| s.parse().expect("test ip")),
            user_agent: ua.to_string(),
            path: path.to_string(),
            status: 200,
            request_id: id.map(str::to_string),
        }
    }

    fn catalog() -> RangeCatalog {
        let mut catalog = RangeCatalog::new();
        let url = by_id("gptbot").unwrap().verification.ranges_url().unwrap();
        catalog
            .insert_json(
                url,
                r#"{"creationTime":"2025-10-30T11:00:00.000000",
                    "prefixes":[{"ipv4Prefix":"132.196.86.0/24"}]}"#,
            )
            .expect("fixture loads");
        catalog
    }

    fn window() -> Window {
        Window {
            since: ts("2026-08-01T00:00:00Z"),
            until: ts("2026-09-01T00:00:00Z"),
        }
    }

    fn report(lines: Vec<AccessLine>) -> logs::ParseReport {
        logs::ParseReport {
            lines,
            ..Default::default()
        }
    }

    const GPTBOT_UA: &str = "Mozilla/5.0 (compatible; GPTBot/1.2; +https://openai.com/gptbot)";
    const HUMAN_UA: &str = "Mozilla/5.0 (Macintosh) Chrome/126 Safari/537.36";

    #[test]
    fn the_funnel_attributes_every_dropped_line() {
        let mut counters = Counters::default();
        let lines = vec![
            // verified
            line("/a", "2026-08-10T00:00:00Z", GPTBOT_UA, Some("132.196.86.9"), None),
            // spoofed — claims GPTBot from an IP OpenAI does not publish
            line("/b", "2026-08-10T00:00:01Z", GPTBOT_UA, Some("203.0.113.9"), None),
            // human
            line("/c", "2026-08-10T00:00:02Z", HUMAN_UA, Some("203.0.113.9"), None),
            // outside the window
            line("/d", "2026-07-01T00:00:00Z", GPTBOT_UA, Some("132.196.86.9"), None),
        ];
        let out = classify(
            vec![("file:test".to_string(), report(lines))],
            &catalog(),
            window(),
            &mut counters,
        );
        assert_eq!(counters.parsed, 4);
        assert_eq!(counters.outside_window, 1);
        assert_eq!(counters.not_a_bot, 1);
        assert_eq!(counters.identified, 2);
        assert_eq!(counters.verified, 1);
        assert_eq!(counters.rejected, 1);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn a_spoofed_bot_is_never_emitted() {
        assert!(!emits(&Verdict::Rejected, true));
        assert!(!emits(&Verdict::Rejected, false));
        assert!(emits(&Verdict::Verified, false));
        assert!(!emits(&Verdict::UnverifiableNoRanges, false));
        assert!(emits(&Verdict::UnverifiableNoRanges, true));
    }

    #[test]
    fn hit_level_events_carry_category_and_a_hit_count_of_one() {
        let mut counters = Counters::default();
        let classified = classify(
            vec![(
                "file:test".to_string(),
                report(vec![line(
                    "/llms.txt",
                    "2026-08-10T00:00:00Z",
                    GPTBOT_UA,
                    Some("132.196.86.9"),
                    Some("req-1"),
                )]),
            )],
            &catalog(),
            window(),
            &mut counters,
        );
        let events = build_events(&classified, Aggregation::Hit, false);
        assert_eq!(events.len(), 1);
        let GeoEvent::Crawl(payload) = &events[0] else {
            panic!("expected a crawl event");
        };
        assert_eq!(payload.bot, "gptbot");
        assert_eq!(payload.category, BotCategory::TrainingCrawler.as_str());
        assert_eq!(payload.hits, 1);
        assert_eq!(payload.aggregation, "hit");
        assert_eq!(payload.window_end, None);
        assert_eq!(payload.request_id.as_deref(), Some("req-1"));
        assert!(payload.verified);
    }

    #[test]
    fn two_hits_in_the_same_second_stay_two_entities() {
        // Without `request_id` in the natural key these would collapse and the
        // count would silently halve.
        let mut counters = Counters::default();
        let classified = classify(
            vec![(
                "file:test".to_string(),
                report(vec![
                    line("/x", "2026-08-10T00:00:00Z", GPTBOT_UA, Some("132.196.86.9"), Some("r1")),
                    line("/x", "2026-08-10T00:00:00Z", GPTBOT_UA, Some("132.196.86.9"), Some("r2")),
                ]),
            )],
            &catalog(),
            window(),
            &mut counters,
        );
        let events = build_events(&classified, Aggregation::Hit, false);
        let ids: BTreeSet<String> = events.iter().map(GeoEvent::entity_id).collect();
        assert_eq!(ids.len(), 2, "{ids:?}");
    }

    #[test]
    fn re_running_the_same_window_produces_the_same_entity_ids() {
        let build = || {
            let mut counters = Counters::default();
            let classified = classify(
                vec![(
                    "file:test".to_string(),
                    report(vec![line(
                        "/llms.txt",
                        "2026-08-10T00:00:00Z",
                        GPTBOT_UA,
                        Some("132.196.86.9"),
                        Some("req-1"),
                    )]),
                )],
                &catalog(),
                window(),
                &mut counters,
            );
            build_events(&classified, Aggregation::Hit, false)
                .iter()
                .map(GeoEvent::entity_id)
                .collect::<Vec<_>>()
        };
        assert_eq!(build(), build());
    }

    #[test]
    fn hourly_aggregation_collapses_a_bucket_and_says_so() {
        let mut counters = Counters::default();
        let classified = classify(
            vec![(
                "file:test".to_string(),
                report(vec![
                    line("/x", "2026-08-10T00:05:00Z", GPTBOT_UA, Some("132.196.86.9"), Some("r1")),
                    line("/x", "2026-08-10T00:45:00Z", GPTBOT_UA, Some("132.196.86.9"), Some("r2")),
                    line("/x", "2026-08-10T01:05:00Z", GPTBOT_UA, Some("132.196.86.9"), Some("r3")),
                ]),
            )],
            &catalog(),
            window(),
            &mut counters,
        );
        let events = build_events(&classified, Aggregation::Hourly, false);
        assert_eq!(events.len(), 2, "one row per hour");
        let GeoEvent::Crawl(first) = &events[0] else {
            panic!("expected a crawl event");
        };
        assert_eq!(first.hits, 2);
        assert_eq!(first.aggregation, "hourly");
        assert_eq!(first.observed_at, ts("2026-08-10T00:00:00Z"));
        assert_eq!(first.window_end, Some(ts("2026-08-10T01:00:00Z")));
        // Total hits are conserved: aggregation must never lose volume.
        let total: u32 = events
            .iter()
            .map(|e| match e {
                GeoEvent::Crawl(p) => p.hits,
                _ => 0,
            })
            .sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn an_hourly_bucket_and_a_hit_never_share_an_entity() {
        let mut counters = Counters::default();
        let classified = classify(
            vec![(
                "file:test".to_string(),
                report(vec![line(
                    "/x",
                    "2026-08-10T00:00:00Z",
                    GPTBOT_UA,
                    Some("132.196.86.9"),
                    None,
                )]),
            )],
            &catalog(),
            window(),
            &mut counters,
        );
        let hit = build_events(&classified, Aggregation::Hit, false)[0].entity_id();
        let hourly = build_events(&classified, Aggregation::Hourly, false)[0].entity_id();
        assert_ne!(hit, hourly);
    }

    #[test]
    fn bucket_starts_truncate_correctly() {
        let t = ts("2026-08-10T13:47:31Z");
        assert_eq!(bucket_start(t, Aggregation::Hit), t);
        assert_eq!(
            bucket_start(t, Aggregation::Hourly),
            ts("2026-08-10T13:00:00Z")
        );
        assert_eq!(
            bucket_start(t, Aggregation::Daily),
            ts("2026-08-10T00:00:00Z")
        );
    }

    #[test]
    fn an_unverifiable_bot_is_only_emitted_when_asked_and_is_never_marked_verified() {
        let mut counters = Counters::default();
        let ccbot = "CCBot/2.0 (https://commoncrawl.org/faq/)";
        let classified = classify(
            vec![(
                "file:test".to_string(),
                report(vec![line(
                    "/docs",
                    "2026-08-10T00:00:00Z",
                    ccbot,
                    Some("1.2.3.4"),
                    None,
                )]),
            )],
            &catalog(),
            window(),
            &mut counters,
        );
        assert_eq!(counters.verified, 0);
        assert!(build_events(&classified, Aggregation::Hit, false).is_empty());

        let events = build_events(&classified, Aggregation::Hit, true);
        assert_eq!(events.len(), 1);
        let GeoEvent::Crawl(payload) = &events[0] else {
            panic!("expected a crawl event");
        };
        assert!(!payload.verified);
    }

    #[test]
    fn source_labels_identify_the_edge() {
        assert_eq!(
            LogInput::File(PathBuf::from("/tmp/drain.ndjson")).source_label(),
            "file:drain.ndjson"
        );
        assert_eq!(
            LogInput::Vercel("allsource-web".to_string()).source_label(),
            "vercel:allsource-web"
        );
        assert_eq!(
            LogInput::Fly("allsource-control-plane".to_string()).source_label(),
            "fly:allsource-control-plane"
        );
    }

    #[test]
    fn inputs_accumulate_across_flags() {
        let inputs = inputs_from(
            &[PathBuf::from("a.ndjson"), PathBuf::from("b.ndjson")],
            Some("allsource-web"),
            Some("allsource-control-plane"),
        );
        assert_eq!(inputs.len(), 4);
    }

    #[test]
    fn an_empty_catalog_does_not_manufacture_rejections() {
        let mut counters = Counters::default();
        let _ = classify(
            vec![(
                "file:test".to_string(),
                report(vec![line(
                    "/x",
                    "2026-08-10T00:00:00Z",
                    GPTBOT_UA,
                    Some("132.196.86.9"),
                    None,
                )]),
            )],
            &RangeCatalog::new(),
            window(),
            &mut counters,
        );
        assert_eq!(counters.rejected, 0);
        assert_eq!(counters.verified, 0);
        assert_eq!(
            counters
                .unverifiable
                .get(Verdict::UnverifiableRangesUnavailable.as_str()),
            Some(&1)
        );
    }

    // ── the committed fixture ───────────────────────────────────────────
    //
    // `geo-cli/tests/fixtures/` holds a realistic five-week Vercel log-drain
    // dump plus a snapshot of the seven real vendor range lists. Together they
    // make the whole pipeline — parse → identify → verify → categorise →
    // aggregate → key — testable offline and deterministically, against the
    // vendors' actual published prefixes rather than invented ones.

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }

    /// The window the fixture was generated for.
    fn fixture_window() -> Window {
        Window {
            since: ts("2026-07-06T00:00:00Z"),
            until: ts("2026-08-11T00:00:00Z"),
        }
    }

    /// Load the committed fixture through the real pipeline, offline.
    async fn fixture_run(aggregation: Aggregation) -> (Vec<Classified>, Vec<GeoEvent>, Counters) {
        let dir = fixtures_dir();
        let body = std::fs::read_to_string(dir.join("vercel-log-drain.ndjson"))
            .expect("fixture log is committed");
        let (catalog, sources) = ranges::load(Some(&dir.join("ranges")), true)
            .await
            .expect("fixture ranges load");
        assert!(
            sources.iter().all(|s| s.outcome.is_loaded()),
            "every vendor range snapshot must be committed: {sources:#?}"
        );
        let mut counters = Counters::default();
        let classified = classify(
            vec![(
                "vercel-log-drain".to_string(),
                logs::parse(&body, LogFormat::Auto),
            )],
            &catalog,
            fixture_window(),
            &mut counters,
        );
        let events = build_events(&classified, aggregation, false);
        (classified, events, counters)
    }

    #[tokio::test]
    async fn the_fixture_exercises_all_three_categories_and_a_spoof() {
        let (classified, events, counters) = fixture_run(Aggregation::Hit).await;

        assert!(counters.parsed > 200, "{counters:?}");
        assert!(counters.not_a_bot > 0, "the fixture must contain humans");
        assert!(
            counters.unparseable > 0,
            "the fixture must contain unreadable lines so the skip path is exercised"
        );
        assert!(
            counters.rejected > 0,
            "the fixture must contain a spoofed bot, or verification is untested"
        );
        assert_eq!(
            counters.verified + counters.rejected + counters.unverifiable.values().sum::<usize>(),
            counters.identified,
            "every identified hit must land in exactly one verdict bucket"
        );

        for category in BotCategory::ALL {
            let hits = classified
                .iter()
                .filter(|c| c.spec.category == category && c.verdict.is_verified())
                .count();
            assert!(hits > 0, "{category} has no verified hits in the fixture");
        }

        // Training crawlers dominate by an order of magnitude — the shape
        // Cloudflare's crawl-to-referral ratios predict, and the reason a
        // blended number would be meaningless.
        let per_category = |c: BotCategory| {
            classified
                .iter()
                .filter(|x| x.spec.category == c && x.verdict.is_verified())
                .count()
        };
        assert!(
            per_category(BotCategory::TrainingCrawler)
                > per_category(BotCategory::UserFetcher) * 5
        );

        assert_eq!(events.len(), counters.verified);
    }

    #[tokio::test]
    async fn re_ingesting_the_fixture_yields_the_same_entity_ids() {
        // The idempotency guarantee, over real data rather than a single row.
        let first: Vec<String> = fixture_run(Aggregation::Hit)
            .await
            .1
            .iter()
            .map(GeoEvent::entity_id)
            .collect();
        let second: Vec<String> = fixture_run(Aggregation::Hit)
            .await
            .1
            .iter()
            .map(GeoEvent::entity_id)
            .collect();
        assert_eq!(first, second);
        let distinct: BTreeSet<&String> = first.iter().collect();
        assert_eq!(
            distinct.len(),
            first.len(),
            "two hits collapsed onto one entity — the natural key is too coarse"
        );
    }

    #[tokio::test]
    async fn daily_aggregation_conserves_the_fixture_hit_count() {
        let (_, hits, _) = fixture_run(Aggregation::Hit).await;
        let (_, daily, _) = fixture_run(Aggregation::Daily).await;
        let sum = |events: &[GeoEvent]| -> u32 {
            events
                .iter()
                .map(|e| match e {
                    GeoEvent::Crawl(p) => p.hits,
                    _ => 0,
                })
                .sum()
        };
        assert_eq!(sum(&hits), sum(&daily));
        assert!(daily.len() < hits.len(), "daily must actually collapse rows");
    }

    /// Regenerates `tests/fixtures/vercel-log-drain.ndjson`.
    ///
    /// A generator, not a check — so the fixture is always a real log-drain
    /// shape produced by one deterministic function, never a file hand-edited
    /// until the assertions passed. Client IPs are taken from the committed
    /// vendor range snapshots, so "verified" in this fixture means verified
    /// against OpenAI's, Anthropic's and Perplexity's actual published
    /// prefixes.
    ///
    /// ```text
    /// cargo test -p geo-cli -- --ignored regenerate_crawl_fixture
    /// ```
    #[test]
    #[ignore = "generator, not a check — run explicitly to refresh the fixture"]
    fn regenerate_crawl_fixture() {
        /// Deterministic LCG, so a regeneration is byte-identical.
        struct Rng(u64);
        impl Rng {
            fn next(&mut self) -> u64 {
                self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
                self.0 >> 33
            }
            fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
                &items[(self.next() as usize) % items.len()]
            }
        }

        const PATHS: &[&str] = &[
            "/",
            "/pricing",
            "/docs",
            "/llms.txt",
            "/blog/agent-memory",
            "/vs/mem0",
            "/compare",
            "/prime",
            "/sitemap.xml",
        ];

        // (bot ua, client ip from the vendor's published list, weekly volumes)
        struct Plan {
            ua: &'static str,
            ip: &'static str,
            weekly: [usize; 5],
        }
        let plans = [
            Plan {
                ua: "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko); compatible; \
                     GPTBot/1.2; +https://openai.com/gptbot",
                ip: "132.196.86.17",
                weekly: [10, 13, 12, 52, 12], // week 4 is a crawl burst
            },
            Plan {
                ua: "Mozilla/5.0 (compatible; ClaudeBot/1.0; +claudebot@anthropic.com)",
                ip: "216.73.216.44",
                weekly: [15, 18, 16, 17, 20],
            },
            Plan {
                ua: "CCBot/2.0 (https://commoncrawl.org/faq/)",
                ip: "98.51.100.4",
                weekly: [2, 0, 0, 3, 0], // no published ranges -> unverifiable
            },
            Plan {
                ua: "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko); compatible; \
                     OAI-SearchBot/1.0; +https://openai.com/searchbot",
                ip: "135.234.64.19",
                weekly: [3, 4, 3, 3, 4],
            },
            Plan {
                ua: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                     (KHTML, like Gecko) PerplexityBot/1.0; +https://perplexity.ai/perplexitybot",
                ip: "3.224.62.45",
                weekly: [2, 2, 2, 1, 3],
            },
            Plan {
                ua: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, \
                     like Gecko) Chrome/120.0.0.0 Safari/537.36; compatible; ChatGPT-User/1.0; \
                     +https://openai.com/bot",
                ip: "104.210.139.196",
                weekly: [1, 2, 1, 2, 3],
            },
            Plan {
                ua: "Mozilla/5.0 (compatible; Claude-User/1.0; +claudebot@anthropic.com)",
                ip: "34.162.230.222",
                weekly: [0, 1, 1, 1, 2],
            },
            Plan {
                ua: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                     (KHTML, like Gecko) Perplexity-User/1.0; \
                     +https://perplexity.ai/perplexity-user",
                ip: "44.208.221.197",
                weekly: [0, 0, 1, 1, 1],
            },
            Plan {
                // The spoof: claims GPTBot from an address OpenAI does not
                // publish (203.0.113.0/24 is RFC 5737 documentation space).
                ua: "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko); compatible; \
                     GPTBot/1.2; +https://openai.com/gptbot",
                ip: "203.0.113.77",
                weekly: [1, 1, 1, 1, 1],
            },
            Plan {
                ua: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                     (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
                ip: "198.51.100.23",
                weekly: [3, 3, 3, 3, 3], // humans
            },
        ];

        let week_zero = ts("2026-07-06T00:00:00Z");
        let mut rng = Rng(0x5EED_1234_ABCD_0001);
        let mut rows: Vec<(DateTime<Utc>, String)> = Vec::new();

        for (plan_index, plan) in plans.iter().enumerate() {
            for (week, count) in plan.weekly.iter().enumerate() {
                for i in 0..*count {
                    let offset = Duration::seconds(
                        (rng.next() % (7 * 24 * 60 * 60)) as i64,
                    );
                    let when = week_zero + Duration::weeks(week as i64) + offset;
                    let path = rng.pick(PATHS);
                    let status = if *path == "/vs/mem0" && i % 17 == 0 {
                        404
                    } else {
                        200
                    };
                    let request_id = format!(
                        "iad1::{plan_index:02}{week}{i:04}-{}-0f1e2d3c4b5a",
                        when.timestamp_millis()
                    );
                    let value = serde_json::json!({
                        "id": request_id,
                        "type": "request",
                        "source": "static",
                        "requestId": request_id,
                        "timestampInMs": when.timestamp_millis(),
                        "proxy": {
                            "timestamp": when.timestamp_millis(),
                            "method": "GET",
                            "host": "www.all-source.xyz",
                            "path": path,
                            "statusCode": status,
                            "clientIp": plan.ip,
                            "region": "iad1",
                            "scheme": "https",
                            "referer": "",
                            "userAgent": [plan.ua],
                        },
                    });
                    rows.push((when, serde_json::to_string(&value).expect("row renders")));
                }
            }
        }

        rows.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut out: Vec<String> = rows.into_iter().map(|(_, line)| line).collect();
        // Two unreadable lines, because a real drain dump has them and the
        // skip path must be exercised by the fixture rather than only by a
        // hand-written unit test.
        out.insert(3, "{\"truncated\": ".to_string());
        out.push("<html>504 Gateway Timeout</html>".to_string());

        let dir = fixtures_dir();
        std::fs::create_dir_all(&dir).expect("fixtures dir");
        let path = dir.join("vercel-log-drain.ndjson");
        std::fs::write(&path, out.join("\n") + "\n").expect("fixture write");
        println!("wrote {} ({} lines)", path.display(), out.len());
    }

    #[test]
    fn verification_is_actually_exercised_by_the_ip() {
        // Same UA, two IPs, opposite verdicts — proof the verdict comes from
        // the address and not from the user agent.
        let catalog = catalog();
        let gptbot = by_id("gptbot").unwrap();
        assert!(catalog.verify(gptbot, Some(ipv4(132, 196, 86, 9))).is_verified());
        assert!(!catalog.verify(gptbot, Some(ipv4(203, 0, 113, 9))).is_verified());
    }
}
