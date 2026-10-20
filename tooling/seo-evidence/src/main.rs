use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use seo_evidence::{
    config, drift,
    ledger::{Entry, Evidence, Ledger},
    sources::{Source, bing, crux, ga4, gsc, psi, render},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
enum Format {
    Json,
    Markdown,
}

#[derive(Parser)]
#[command(
    name = "seo-evidence",
    about = "Collect SEO and AI-search evidence into a state-tagged ledger"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Report which sources are configured, without calling any of them.
    Doctor {
        #[arg(long, value_enum, default_value = "markdown")]
        format: Format,
        /// Exit 2 when any source is still awaiting setup. Use this in CI once
        /// a source is expected to be configured, so a revoked credential
        /// fails the run instead of quietly producing a thinner report.
        #[arg(long)]
        strict: bool,
    },
    /// Query every configured source and write the ledger.
    Collect {
        #[arg(long, value_enum)]
        source: Vec<Source>,
        #[arg(long, value_enum, default_value = "json")]
        format: Format,
        /// Lookback for the time-series sources.
        #[arg(long, default_value_t = 28)]
        window_days: i64,
        #[arg(long)]
        out: Option<String>,
    },
    /// Compare two ledgers and exit 2 if a source stopped answering.
    Drift {
        #[arg(long)]
        before: String,
        #[arg(long)]
        after: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Doctor { format, strict } => doctor(format, strict),
        Command::Collect {
            source,
            format,
            window_days,
            out,
        } => collect(source, format, window_days, out).await,
        Command::Drift { before, after } => run_drift(&before, &after),
    }
}

fn doctor(format: Format, strict: bool) -> Result<()> {
    let entries: Vec<Entry> = Source::ALL
        .iter()
        .map(|source| {
            let missing = config::missing(source.keys());
            let evidence = if missing.is_empty() {
                Evidence::observed(serde_json::json!({
                    "configured": true,
                    "requires_credentials": !source.keys().is_empty(),
                }))
            } else {
                Evidence::setup_required(
                    missing,
                    format!("set the listed variables for {}", source.label()),
                    config::RUNBOOK,
                )
            };
            Entry {
                source: format!("doctor.{source:?}").to_lowercase(),
                measures: source.label().to_owned(),
                evidence,
            }
        })
        .collect();

    let ledger = Ledger::new(config::site_url(), entries);
    emit(&ledger, format)?;

    if strict && ledger.coverage.setup_required > 0 {
        std::process::exit(2);
    }
    Ok(())
}

async fn collect(
    requested: Vec<Source>,
    format: Format,
    window_days: i64,
    out: Option<String>,
) -> Result<()> {
    let selected: Vec<Source> = if requested.is_empty() {
        Source::ALL.to_vec()
    } else {
        requested
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent("allsource-seo-evidence/0.1")
        .build()
        .context("building the HTTP client failed")?;

    let site = config::site_url();
    let pages = config::default_pages();
    let profile_dir = std::env::temp_dir().join("seo-evidence-chrome");

    let mut entries = Vec::new();
    for source in selected {
        let produced = match source {
            Source::Gsc => gsc::collect(&client, window_days).await,
            Source::Ga4 => ga4::collect(&client, window_days).await,
            Source::Psi => psi::collect(&client, &pages).await,
            Source::Crux => crux::collect(&client, site.trim_end_matches('/')).await,
            Source::Bing => bing::collect(&client, &site).await,
            Source::Render => render::collect(&pages, &profile_dir.to_string_lossy()).await,
        };
        entries.extend(produced?);
    }

    let ledger = Ledger::new(&site, entries);
    if let Some(path) = out {
        std::fs::write(&path, serde_json::to_string_pretty(&ledger)?)
            .with_context(|| format!("writing the ledger to {path} failed"))?;
        eprintln!(
            "wrote {path}: {} observed, {} awaiting setup, {} failed",
            ledger.coverage.observed, ledger.coverage.setup_required, ledger.coverage.failed
        );
    }
    emit(&ledger, format)?;

    // A source that was configured and then refused is a broken pipeline, and
    // the caller must be able to tell that from a source nobody set up yet.
    if ledger.coverage.failed > 0 {
        std::process::exit(2);
    }
    Ok(())
}

fn run_drift(before: &str, after: &str) -> Result<()> {
    let before_json =
        std::fs::read_to_string(before).with_context(|| format!("reading {before} failed"))?;
    let after_json =
        std::fs::read_to_string(after).with_context(|| format!("reading {after} failed"))?;
    let result = drift::compare(&before_json, &after_json)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    if result.has_regressions() {
        std::process::exit(2);
    }
    Ok(())
}

fn emit(ledger: &Ledger, format: Format) -> Result<()> {
    match format {
        Format::Json => println!("{}", serde_json::to_string_pretty(ledger)?),
        Format::Markdown => println!("{}", ledger.to_markdown()),
    }
    Ok(())
}
