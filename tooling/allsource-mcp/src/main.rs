use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

mod diagnostics;
mod protocol;
mod tools;
mod transport;

use diagnostics::{AccessProfile, DiagnosticPolicy};
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

    let core = allsource_core::embedded::EmbeddedCore::open(
        allsource_core::embedded::Config::builder()
            .data_dir(&cli.data_dir)
            .single_tenant(false)
            .read_only(true)
            .build()?,
    )
    .await?;

    tracing::info!("AllSource Core opened: {} events", core.event_count());

    let mut transport = StdioTransport::new(core, policy);
    transport.run().await?;

    Ok(())
}
