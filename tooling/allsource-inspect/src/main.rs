mod filter;
mod lifecycle;
mod stores;

use allsource_core::{
    embedded::{DurabilityStatus, EmbeddedCore, EventView, Query},
    store::StoreStats,
};
use anyhow::Context;
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::filter::PayloadShape;
use crate::lifecycle::LifecycleSpec;

/// Every command opens the store read-only, so it is safe to point at a
/// directory a running application is writing.
#[derive(Parser)]
#[command(
    name = "allsource-inspect",
    about = "Read AllSource Core storage (WAL + Parquet) without a server, read-only",
    version
)]
struct Cli {
    /// Path to the store's data directory (holds `storage/` and `wal/`)
    #[arg(long, env = "ALLSOURCE_DATA_DIR", global = true)]
    data_dir: Option<PathBuf>,

    /// Output format
    #[arg(long, default_value = "table", global = true)]
    format: OutputFormat,

    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    Json,
    Table,
}

#[derive(Subcommand)]
enum Command {
    /// Query events with filters
    Events {
        /// Filter by entity ID (exact match)
        #[arg(long)]
        entity_id: Option<String>,

        /// Filter by event type (exact match)
        #[arg(long)]
        event_type: Option<String>,

        /// Filter by event type prefix (e.g. "order." matches "order.placed")
        #[arg(long)]
        event_type_prefix: Option<String>,

        /// Filter events after this time (RFC 3339)
        #[arg(long)]
        since: Option<String>,

        /// Filter events before this time (RFC 3339)
        #[arg(long)]
        until: Option<String>,

        /// Maximum number of events to return (oldest first)
        #[arg(long, default_value = "100")]
        limit: usize,

        /// Keep events whose payload text contains this (repeatable; all must match)
        #[arg(long)]
        contains: Vec<String>,

        /// Print only these payload fields, as dot paths (e.g. `steps.4.approval`)
        #[arg(long, value_delimiter = ',')]
        fields: Vec<String>,

        /// Print the payload's top-level keys instead of the payload
        #[arg(long)]
        keys: bool,
    },
    /// Show storage summary and stats
    Summary,
    /// Show WAL status and the events still in the WAL
    Wal {
        /// Accepted for compatibility; not implemented yet
        #[arg(long)]
        wal_only: bool,

        /// Maximum number of WAL events to display
        #[arg(long, default_value = "50")]
        limit: usize,
    },
    /// List the store data directories under a root (opens none of them)
    Stores {
        /// Directory to search, e.g. an application's support directory
        #[arg(long)]
        root: PathBuf,

        /// How many directory levels to descend
        #[arg(long, default_value = "6")]
        max_depth: usize,
    },
    /// Fold each entity's lifecycle events into one line: current state, when, how many events
    Lifecycle {
        /// Event type prefix shared by the lifecycle, e.g. `workflow_run.`
        #[arg(long)]
        event_type_prefix: String,

        /// Suffixes that set state, e.g. `started,completed,failed`
        #[arg(long, value_delimiter = ',', required = true)]
        states: Vec<String>,

        /// Only entities currently in this state
        #[arg(long)]
        state: Option<String>,

        /// Payload field naming the entity, when one logical entity spans entity ids
        #[arg(long)]
        key_field: Option<String>,

        /// Payload fields (dot paths) to print from each entity's state-setting event
        #[arg(long, value_delimiter = ',')]
        fields: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if let Command::Stores { root, max_depth } = &cli.command {
        for dir in stores::find(root, *max_depth)? {
            println!("{}", stores::describe(root, &dir));
        }
        return Ok(());
    }

    let data_dir = cli
        .data_dir
        .as_deref()
        .context("--data-dir (or ALLSOURCE_DATA_DIR) is required for this command")?;
    eprintln!("reading {} (read-only)", data_dir.display());
    let core = stores::open_read_only(data_dir).await?;

    match cli.command {
        Command::Stores { .. } => {}
        Command::Events {
            entity_id,
            event_type,
            event_type_prefix,
            since,
            until,
            limit,
            contains,
            fields,
            keys,
        } => {
            let since_dt = since
                .as_deref()
                .map(str::parse::<DateTime<Utc>>)
                .transpose()?;
            let until_dt = until
                .as_deref()
                .map(str::parse::<DateTime<Utc>>)
                .transpose()?;

            let mut query = Query::new();
            if let Some(ref id) = entity_id {
                query = query.entity_id(id);
            }
            if let Some(ref et) = event_type {
                query = query.event_type(et);
            }
            if let Some(ref prefix) = event_type_prefix {
                query = query.event_type_prefix(prefix);
            }
            if let Some(dt) = since_dt {
                query = query.since(dt);
            }
            if let Some(dt) = until_dt {
                query = query.until(dt);
            }
            // The text filter must see every match before the limit applies.
            if contains.is_empty() {
                query = query.limit(limit);
            }

            let mut events = core.query(query).await?;
            events.retain(|event| filter::payload_contains(&event.payload, &contains));
            events.truncate(limit);
            print_events(
                &cli.format,
                &events,
                &PayloadShape::from_flags(keys, &fields),
            );
        }
        Command::Summary => {
            cmd_summary(&core, &cli.format);
        }
        Command::Wal { wal_only, limit } => {
            cmd_wal(&core, &cli.format, wal_only, limit);
        }
        Command::Lifecycle {
            event_type_prefix,
            states,
            state,
            key_field,
            fields,
        } => {
            let events = core
                .query(Query::new().event_type_prefix(&event_type_prefix))
                .await?;
            let spec = LifecycleSpec {
                prefix: &event_type_prefix,
                states: &states,
                key_field: key_field.as_deref(),
                fields: &fields,
            };
            let lines: Vec<_> = lifecycle::fold(&events, &spec)
                .into_iter()
                .filter(|line| state.as_deref().is_none_or(|s| line["state"] == s))
                .collect();
            print_lifecycle(&cli.format, &lines);
        }
    }

    Ok(())
}

fn print_lifecycle(format: &OutputFormat, lines: &[serde_json::Value]) {
    match format {
        OutputFormat::Json => {
            for line in lines {
                println!("{line}");
            }
        }
        OutputFormat::Table => {
            if lines.is_empty() {
                println!("No entities reached a named state.");
                return;
            }
            let mut table = comfy_table::Table::new();
            table.set_header(vec!["Key", "State", "State at", "Events", "Fields"]);
            for line in lines {
                let text = |key: &str| {
                    line[key]
                        .as_str()
                        .map_or_else(|| line[key].to_string(), str::to_string)
                };
                table.add_row(vec![
                    text("key"),
                    text("state"),
                    text("state_at"),
                    line["events"].to_string(),
                    line["fields"].to_string(),
                ]);
            }
            println!("{table}");
            println!("\n{} entit(ies)", lines.len());
        }
    }
}

fn print_events(format: &OutputFormat, events: &[EventView], shape: &PayloadShape<'_>) {
    match format {
        OutputFormat::Json => {
            for event in events {
                println!("{}", filter::render(event, shape));
            }
        }
        OutputFormat::Table => {
            if events.is_empty() {
                println!("No events found.");
                return;
            }
            let mut table = comfy_table::Table::new();
            table.set_header(vec![
                "ID",
                "Entity ID",
                "Event Type",
                "Timestamp",
                "Payload (truncated)",
            ]);
            for event in events {
                let payload_str = shape.apply(&event.payload).to_string();
                let truncated = if payload_str.chars().count() > 80 {
                    format!("{}…", payload_str.chars().take(80).collect::<String>())
                } else {
                    payload_str
                };
                table.add_row(vec![
                    event.id.to_string(),
                    event.entity_id.clone(),
                    event.event_type.clone(),
                    event.timestamp.to_rfc3339(),
                    truncated,
                ]);
            }
            println!("{table}");
            println!("\n{} event(s) returned", events.len());
        }
    }
}

fn cmd_summary(core: &EmbeddedCore, format: &OutputFormat) {
    let status = core.durability_status();
    let stats = core.stats();
    let store = core.inner();
    let streams = store.list_streams();
    let event_types = store.list_event_types();

    // Find date range from streams
    let all_dates: Vec<DateTime<Utc>> = streams.iter().filter_map(|s| s.last_event_at).collect();
    let earliest = all_dates.iter().min().copied();
    let latest = all_dates.iter().max().copied();

    // Top 10 event types by count
    let mut sorted_types = event_types.clone();
    sorted_types.sort_by(|a, b| b.event_count.cmp(&a.event_count));
    sorted_types.truncate(10);

    match format {
        OutputFormat::Json => {
            let top_types: Vec<serde_json::Value> = sorted_types
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "event_type": t.event_type,
                        "count": t.event_count,
                    })
                })
                .collect();
            let summary = serde_json::json!({
                "total_events": stats.total_events,
                "total_entities": stats.total_entities,
                "total_event_types": stats.total_event_types,
                "date_range": {
                    "earliest": earliest.map(|d| d.to_rfc3339()),
                    "latest": latest.map(|d| d.to_rfc3339()),
                },
                "top_event_types": top_types,
                "durability": {
                    "durable": status.durable,
                    "memory_events": status.memory_events,
                    "wal_enabled": status.wal_enabled,
                    "wal_entries": status.wal_entries,
                    "wal_bytes": status.wal_bytes,
                    "wal_sequence": status.wal_sequence,
                    "parquet_enabled": status.parquet_enabled,
                    "parquet_files": status.parquet_files,
                    "parquet_bytes": status.parquet_bytes,
                    "parquet_pending_batch": status.parquet_pending_batch,
                },
                "warnings": status.warnings,
            });
            println!("{}", serde_json::to_string_pretty(&summary).unwrap());
        }
        OutputFormat::Table => {
            print_summary_table(&stats, &status, earliest, latest, &sorted_types);
        }
    }
}

fn print_summary_table(
    stats: &StoreStats,
    status: &DurabilityStatus,
    earliest: Option<DateTime<Utc>>,
    latest: Option<DateTime<Utc>>,
    top_types: &[allsource_core::store::EventTypeInfo],
) {
    println!("AllSource Core Storage Summary");
    println!("==============================\n");

    println!("Events:      {}", stats.total_events);
    println!("Entities:    {}", stats.total_entities);
    println!("Event Types: {}", stats.total_event_types);

    match (earliest, latest) {
        (Some(e), Some(l)) => println!("Date Range:  {} → {}", e.to_rfc3339(), l.to_rfc3339()),
        _ => println!("Date Range:  (no events)"),
    }

    println!(
        "\nDurability:  {}",
        if status.durable { "YES" } else { "NO" }
    );
    println!("Memory:      {} events", status.memory_events);

    println!(
        "\nWAL:         {}",
        if status.wal_enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    if status.wal_enabled {
        println!("  Entries:   {}", status.wal_entries);
        println!("  Bytes:     {}", format_bytes(status.wal_bytes));
        println!("  Sequence:  {}", status.wal_sequence);
    }

    println!(
        "\nParquet:     {}",
        if status.parquet_enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    if status.parquet_enabled {
        println!("  Files:     {}", status.parquet_files);
        println!("  Bytes:     {}", format_bytes(status.parquet_bytes));
        println!("  Pending:   {} events", status.parquet_pending_batch);
    }

    if !top_types.is_empty() {
        println!("\nTop Event Types:");
        let mut table = comfy_table::Table::new();
        table.set_header(vec!["Event Type", "Count", "Last Seen"]);
        for t in top_types {
            table.add_row(vec![
                t.event_type.clone(),
                t.event_count.to_string(),
                t.last_event_at
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_else(|| "-".to_string()),
            ]);
        }
        println!("{table}");
    }

    if !status.warnings.is_empty() {
        println!("\nWarnings:");
        for w in &status.warnings {
            println!("  - {w}");
        }
    }
}

fn cmd_wal(core: &EmbeddedCore, format: &OutputFormat, _wal_only: bool, limit: usize) {
    let status = core.durability_status();
    let store = core.inner();

    match format {
        OutputFormat::Json => {
            let mut wal_info = serde_json::json!({
                "enabled": status.wal_enabled,
                "entries": status.wal_entries,
                "bytes": status.wal_bytes,
                "sequence": status.wal_sequence,
            });

            // Recover WAL events to show them
            if status.wal_enabled
                && let Some(wal) = store.wal()
            {
                match wal.recover() {
                    Ok(events) => {
                        let capped: Vec<&_> = events.iter().take(limit).collect();
                        let event_views: Vec<EventView> =
                            capped.iter().map(|e| EventView::from(*e)).collect();
                        wal_info["recovered_events"] = serde_json::json!(event_views.len());
                        for ev in &event_views {
                            println!("{}", serde_json::to_string(ev).unwrap());
                        }
                        return;
                    }
                    Err(e) => {
                        wal_info["recovery_error"] = serde_json::json!(e.to_string());
                    }
                }
            }

            println!("{}", serde_json::to_string_pretty(&wal_info).unwrap());
        }
        OutputFormat::Table => {
            if !status.wal_enabled {
                println!("WAL is not enabled for this data directory.");
                return;
            }
            println!("WAL Status");
            println!("==========\n");
            println!("Entries:   {}", status.wal_entries);
            println!("Bytes:     {}", format_bytes(status.wal_bytes));
            println!("Sequence:  {}", status.wal_sequence);

            // Recover and display WAL events
            if let Some(wal) = store.wal() {
                match wal.recover() {
                    Ok(events) => {
                        if events.is_empty() {
                            println!("\nNo events in WAL.");
                            return;
                        }
                        println!("\nRecovered {} event(s) from WAL:", events.len());
                        let capped: Vec<&_> = events.iter().take(limit).collect();
                        let event_views: Vec<EventView> =
                            capped.iter().map(|e| EventView::from(*e)).collect();
                        print_events(&OutputFormat::Table, &event_views, &PayloadShape::Full);
                    }
                    Err(e) => {
                        println!("\nWAL recovery failed: {e}");
                    }
                }
            }
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
