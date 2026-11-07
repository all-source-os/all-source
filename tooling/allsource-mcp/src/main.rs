use anyhow::Result;
use clap::Parser;
use std::{path::PathBuf, time::Duration};
use tracing_subscriber::EnvFilter;

mod diagnostics;
mod protocol;
mod remote;
mod stores;
mod tools;
mod transport;

use diagnostics::{AccessProfile, DiagnosticPolicy};
use remote::RemoteStore;
use stores::StoreRegistry;
use transport::StdioTransport;

#[derive(Parser)]
#[command(
    name = "allsource-mcp",
    version,
    about = "MCP server for local AllSource debugging (stdio transport)"
)]
struct Cli {
    /// Path to `AllSource` data directory containing storage/ and wal/
    #[arg(long, env = "ALLSOURCE_DATA_DIR")]
    data_dir: Option<PathBuf>,

    /// Gateway base URL, read over HTTP. The view tools need it.
    ///
    /// Selected as `default` without --data-dir, otherwise as `remote`.
    #[arg(long, value_name = "URL")]
    remote: Option<String>,

    /// Key sent as a bearer token to --remote.
    #[arg(long, env = "ALLSOURCE_API_KEY", hide_env_values = true)]
    api_key: Option<String>,

    /// P-256 PKCS#8 private key (PEM, or a path to one) registered as a client
    /// key on the tenant. When set, --remote requests carry a fresh 60-second
    /// ES256 assertion instead of --api-key.
    #[arg(long, env = "ALLSOURCE_CLIENT_SIGNING_KEY", hide_env_values = true)]
    client_signing_key: Option<String>,

    /// Registered client id; the assertion's `iss`.
    #[arg(long, env = "ALLSOURCE_CLIENT_ID")]
    client_id: Option<String>,

    /// Tenant the client key is registered on; the assertion's `sub`.
    #[arg(long, env = "ALLSOURCE_TENANT_ID")]
    client_tenant_id: Option<String>,

    /// The registered key's kid (its RFC 7638 thumbprint).
    #[arg(long, env = "ALLSOURCE_CLIENT_KID")]
    client_kid: Option<String>,

    /// Audience the gateway expects in every assertion.
    #[arg(
        long,
        env = "ALLSOURCE_ASSERTION_AUDIENCE",
        default_value = "https://api.all-source.xyz"
    )]
    assertion_audience: String,

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

    /// Minimum milliseconds between catching a store up with its writer before a
    /// read. 0 refreshes before every read.
    #[arg(long, env = "ALLSOURCE_MCP_REFRESH_MS", default_value_t = 1000)]
    refresh_ms: u64,
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

    let assertion = cli
        .client_signing_key
        .as_deref()
        .map(|signing_key| {
            remote::assertion_provider(&remote::AssertionSettings {
                signing_key,
                client_id: cli.client_id.as_deref(),
                tenant_id: cli.client_tenant_id.as_deref(),
                kid: cli.client_kid.as_deref(),
                audience: &cli.assertion_audience,
            })
        })
        .transpose()?;
    let remote = cli
        .remote
        .as_deref()
        .map(|url| {
            RemoteStore::new(url, cli.api_key.clone()).map(|store| match assertion {
                Some(provider) => store.with_assertion(provider),
                None => store,
            })
        })
        .transpose()?;
    let registry = StoreRegistry::open(
        cli.data_dir.as_deref(),
        &cli.stores,
        Duration::from_millis(cli.refresh_ms),
        remote,
    )
    .await?;
    for (name, store) in registry.iter() {
        tracing::info!(
            store = name.as_str(),
            path = %store.path.display(),
            events = store.core.event_count(),
            "AllSource store opened"
        );
    }
    for (name, remote) in registry.remotes() {
        tracing::info!(
            store = name.as_str(),
            url = remote.display_url(),
            "AllSource gateway store bound"
        );
    }

    let mut transport = StdioTransport::new(registry, policy);
    transport.run().await?;

    Ok(())
}
