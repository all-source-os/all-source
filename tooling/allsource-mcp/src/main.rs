use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

mod diagnostics;
mod protocol;
mod stores;
mod tools;
mod transport;

use diagnostics::{AccessProfile, DiagnosticPolicy};
use stores::StoreRegistry;
use transport::StdioTransport;

#[derive(Parser)]
#[command(
    name = "allsource-mcp",
    about = "MCP server for local AllSource debugging (stdio transport)"
)]
struct Cli {
    /// Path to `AllSource` data directory containing storage/ and wal/
    #[arg(long, env = "ALLSOURCE_DATA_DIR")]
    data_dir: PathBuf,

    /// Additional named store, `name=/path/to/allsource`, repeatable.
    ///
    /// A tool call selects one with `store`; the directory from `--data-dir` is
    /// `default`. Names are fixed at startup so a request can never name a path.
    #[arg(long = "store", value_parser = parse_store, value_name = "NAME=PATH")]
    stores: Vec<(String, PathBuf)>,

    /// Access profile. Hosted tenant mode fails closed without --tenant-id.
    #[arg(
        long,
        env = "ALLSOURCE_MCP_PROFILE",
        value_enum,
        default_value = "local"
    )]
    profile: AccessProfile,

    /// Immutable tenant binding for this MCP process.
    #[arg(long, env = "ALLSOURCE_MCP_TENANT_ID")]
    tenant_id: Option<String>,

    /// Safe source label returned in diagnostic provenance.
    #[arg(
        long,
        env = "ALLSOURCE_MCP_SOURCE_ID",
        default_value = "allsource-local"
    )]
    source_id: String,
}

/// Parse a `name=path` store argument.
fn parse_store(raw: &str) -> Result<(String, PathBuf), String> {
    let (name, path) = raw
        .split_once('=')
        .ok_or_else(|| format!("expected NAME=PATH, got '{raw}'"))?;
    if name.is_empty() || path.is_empty() {
        return Err(format!("expected NAME=PATH, got '{raw}'"));
    }
    Ok((name.to_string(), PathBuf::from(path)))
}

#[tokio::main]
/// Start the read-only MCP server over standard input and output.
async fn main() -> Result<()> {
    // Log to stderr so stdout is reserved for MCP JSON-RPC
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let policy = DiagnosticPolicy::new(cli.profile, cli.tenant_id, &cli.source_id)?;

    tracing::info!("Opening AllSource data at {:?}", cli.data_dir);

    let registry = StoreRegistry::open(&cli.data_dir, &cli.stores).await?;
    for (name, store) in registry.iter() {
        tracing::info!(
            store = name.as_str(),
            path = %store.path.display(),
            events = store.core.event_count(),
            "AllSource store opened"
        );
    }

    let mut transport = StdioTransport::new(registry, policy);
    transport.run().await?;

    Ok(())
}
