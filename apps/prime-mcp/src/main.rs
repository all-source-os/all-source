// mimalloc chosen over jemallocator for Docker build simplicity — jemalloc's
// autoconf-based C build needs more tooling than the slim runtime image has.
// For this workload (stdio-dominated + occasional HTTP) the allocator choice
// is secondary; mimalloc is fine for long-running servers and builds cleanly.
#[cfg(not(feature = "dhat-heap"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;

mod analytics;
mod core_writer;
mod dispatch;
mod doc_extract;
mod email_ingester;
mod embed_backfill;
mod export;
mod hosted_dispatch;
#[cfg(test)]
mod hosted_test_core;
mod hound;
mod http;
mod install;
mod pr;
mod profiling;
mod projection_registry;
mod protocol;
mod report;
mod sync;
mod templates;
mod tools;
mod toon;
mod transport;
mod wire;

use transport::StdioTransport;

#[derive(Clone, Debug, clap::ValueEnum)]
enum Mode {
    /// MCP server over stdio (newline-delimited JSON-RPC, per the MCP spec)
    Mcp,
    /// HTTP REST API server
    Http,
    /// Preflight the embedder model, then exit. Run once with network access to
    /// populate the fastembed cache (or with `PRIME_EMBED_MODEL_DIR` set to verify
    /// an offline vendored model). Exits non-zero if the embedder can't load —
    /// suitable as a CI canary against a fresh, cache-less container.
    Warm,
    /// One-shot: extract a codebase into the local Prime graph (Tree-sitter,
    /// on-device, no LLM), then exit. Pass the source tree as a positional
    /// argument: `allsource-prime --mode hound --data-dir <dir> <PATH>`.
    Hound,
    /// Write a Prime Hound usage skill into an AI assistant's config, then exit.
    /// Use `--platform <claude-code|cursor|agents|all>` and `--global`.
    Install,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum Format {
    /// Pretty-printed JSON (default — maximally compatible)
    Json,
    /// Token-Oriented Object Notation — compact, ~40-60% fewer tokens on
    /// uniform result arrays. Encodes the tool-result payload only; the
    /// JSON-RPC envelope is always standard JSON.
    Toon,
}

#[derive(Parser)]
#[command(
    name = "allsource-prime",
    about = "AllSource Prime — unified agent memory engine. Supports MCP (stdio) and HTTP modes.",
    version
)]
// A CLI flags struct legitimately has many booleans — one per --flag. Restructuring
// clap args to satisfy struct_excessive_bools would make the interface worse, not
// better.
#[allow(clippy::struct_excessive_bools)]
struct Cli {
    /// Path to Prime data directory (WAL, Parquet, projection checkpoints).
    /// Defaults to the standard `~/.prime/memory`; not needed for `--mode install`.
    #[arg(long, env = "PRIME_DATA_DIR", default_value = "~/.prime/memory")]
    data_dir: PathBuf,

    /// Server mode: mcp (stdio) or http
    #[arg(long, env = "PRIME_MODE", default_value = "mcp")]
    mode: Mode,

    /// HTTP port (only used in http mode)
    #[arg(long, env = "PRIME_PORT", default_value = "3905")]
    port: u16,

    /// Tool-result payload encoding: json or toon (mcp mode only)
    #[arg(long, env = "PRIME_RESULT_FORMAT", default_value = "json")]
    format: Format,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, env = "PRIME_LOG_LEVEL", default_value = "info")]
    log_level: String,

    /// Auto-inject compressed index as MCP resource (zer0dex-style pre-message injection)
    #[arg(long, env = "PRIME_AUTO_INJECT")]
    auto_inject: bool,

    /// Max tokens for auto-injected index (default: 1000)
    #[arg(long, env = "PRIME_AUTO_INJECT_MAX_TOKENS", default_value = "1000")]
    auto_inject_max_tokens: usize,

    /// Remote `AllSource` Core base URL to push `prime.*` events to.
    /// When set with `--api-key`, spawns a background sync loop that ships
    /// events to your tenant so the web panel's Memory tab can show them.
    #[arg(long, env = "PRIME_SYNC_TO")]
    sync_to: Option<String>,

    /// Tenant API key for sync. Required if `--sync-to` is set.
    #[arg(long, env = "PRIME_API_KEY")]
    api_key: Option<String>,

    /// Sync flush interval in milliseconds (default: 1000ms).
    #[arg(long, env = "PRIME_SYNC_INTERVAL_MS", default_value = "1000")]
    sync_interval_ms: u64,

    /// Source tree to ingest in `--mode hound` (Tree-sitter code graph).
    /// Ignored in other modes.
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,

    /// In `--mode hound`, also embed each file/symbol (in-process, no LLM) so the
    /// code graph is searchable by meaning via `prime_recall`. Opt-in: runs the
    /// embedder once per node.
    #[arg(long, env = "PRIME_HOUND_EMBED")]
    embed: bool,

    /// In `--mode hound`, write a human-readable `GRAPH_REPORT.md` to this path
    /// after ingest (Graphify-style). Omit to skip.
    #[arg(long, value_name = "PATH")]
    report: Option<PathBuf>,

    /// In `--mode hound`, delete the existing code graph before ingesting so a
    /// re-run replaces it instead of duplicating (idempotent — what the git hook
    /// uses). Leaves non-code (memory) nodes alone.
    #[arg(long)]
    rebuild: bool,

    /// In `--mode hound`, after ingest export the graph to this format:
    /// cypher, graphml, mermaid, obsidian, or wiki. Pair with --export-out.
    #[arg(long, value_name = "FORMAT")]
    export: Option<String>,

    /// Output for --export: a file (cypher/graphml/mermaid) or a directory
    /// (obsidian/wiki). Defaults to ./hound-export-<format>.
    #[arg(long, value_name = "PATH")]
    export_out: Option<PathBuf>,

    /// In `--mode hound`, also extract a knowledge graph from docs (markdown/
    /// text) under this path via the configured LLM (`PRIME_LLM_ENDPOINT`). Prose
    /// only; code still comes from the positional PATH.
    #[arg(long, value_name = "PATH")]
    docs: Option<PathBuf>,

    /// In `--mode install`, which assistant to install the Hound skill for:
    /// claude-code, cursor, agents, or all.
    #[arg(long, default_value = "all")]
    platform: String,

    /// In `--mode install`, write into your home (~) instead of the current repo.
    #[arg(long)]
    global: bool,
}

/// Expand a leading `~`/`~/` and `$HOME` / `${HOME}` references in a path.
///
/// The Claude Desktop DXT (and similar MCP launchers) substitute their own
/// `${user_config.*}` placeholders but pass the resulting value to the binary
/// verbatim — they do NOT run shell/env expansion. So a user-config value of
/// `${HOME}/.prime/memory` reaches us as a literal string, and without this we
/// would create a `${HOME}` directory relative to the process cwd. Expand the
/// common `$HOME` forms (the only ones that bite in practice) so the data dir
/// resolves to the intended absolute location.
fn expand_home_path(raw: &Path) -> PathBuf {
    let s = raw.to_string_lossy();
    let Ok(home) = std::env::var("HOME") else {
        return raw.to_path_buf();
    };
    // Braced before unbraced so `${HOME}` is consumed first. (`$HOME` is not a
    // substring of `${HOME}`, so order is not strictly required, but explicit.)
    let mut out = s.replace("${HOME}", &home).replace("$HOME", &home);
    if out == "~" {
        out = home;
    } else if let Some(rest) = out.strip_prefix("~/") {
        out = format!("{home}/{rest}");
    }
    PathBuf::from(out)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Log to stderr so stdout is reserved for MCP JSON-RPC
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&cli.log_level)),
        )
        .with_writer(std::io::stderr)
        .init();

    // Expand `~`, `$HOME`, and `${HOME}` in the data dir. The Claude Desktop
    // DXT passes `--data-dir` values verbatim — it does NOT expand shell/env
    // syntax — so a user who configures the natural-looking `${HOME}/.prime/memory`
    // would otherwise have Prime create a literal `${HOME}` directory under its
    // cwd (often `/`, where it can't be written at all). Expand the common cases
    // here so the data dir lands where the user meant.
    let data_dir = expand_home_path(&cli.data_dir);
    if data_dir != cli.data_dir {
        tracing::info!(
            raw = %cli.data_dir.display(),
            expanded = %data_dir.display(),
            "Expanded data dir (~ / $HOME / ${{HOME}})"
        );
    }

    // ── Skill install (one-shot; needs no store) ─────────────────────────
    if matches!(cli.mode, Mode::Install) {
        // A git hook is per-repo and needs the binary + data-dir baked in, so it
        // has its own path (always the current repo, never --global).
        if cli.platform.eq_ignore_ascii_case("git-hook") {
            let repo_root = std::env::current_dir()?;
            let exe = std::env::current_exe()?;
            // Resolve where git actually looks for hooks — respects core.hooksPath
            // so we don't write a hook git will never run.
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo_root)
                .args(["rev-parse", "--git-path", "hooks"])
                .output();
            let hooks_dir = match out {
                Ok(o) if o.status.success() => {
                    let rel = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    let p = PathBuf::from(&rel);
                    if p.is_absolute() {
                        p
                    } else {
                        repo_root.join(p)
                    }
                }
                _ => anyhow::bail!(
                    "not a git repository (git rev-parse failed) — run from the repo root"
                ),
            };
            // A per-repo hook must not land in a shared/global hooks dir
            // (core.hooksPath), where it would fire for every repo.
            if !hooks_dir.starts_with(&repo_root) {
                anyhow::bail!(
                    "git uses a hooks directory outside this repo ({}) — likely a global \
                     core.hooksPath. A per-repo hook doesn't belong there. Add this line to \
                     that post-commit yourself (guard it by repo):\n  \
                     \"{}\" --mode hound --rebuild --data-dir \"{}\" \"{}\"",
                    hooks_dir.display(),
                    exe.display(),
                    data_dir.display(),
                    repo_root.display()
                );
            }
            let hook = install::git_hook(&hooks_dir, &repo_root, &exe, &data_dir)?;
            println!("installed git post-commit hook → {}", hook.display());
            println!("Each commit now refreshes the code graph (idempotent rebuild).");
            return Ok(());
        }
        let root = if cli.global {
            PathBuf::from(std::env::var("HOME").map_err(|_| anyhow::anyhow!("HOME is not set"))?)
        } else {
            std::env::current_dir()?
        };
        let written = install::run(&root, &cli.platform)?;
        for p in &written {
            println!("installed Prime Hound skill → {}", p.display());
        }
        println!(
            "Next: ensure the allsource-prime MCP server is configured, then ask your \
             assistant to ingest this repo (hound_ingest path=\".\")."
        );
        return Ok(());
    }

    // ── Stateless hosted HTTP mode ────────────────────────────────────────
    //
    // When running as the hosted `allsource-prime` app (CORE_URL + PRIME_API_KEY
    // set, http mode), the app owns NO durable store: every request is served
    // tenant-scoped through `HostedPrime` over the remote Core. Detect that
    // before opening any embedded Prime so the `/data` volume is never touched —
    // this is what lets the Fly volume be dropped. The stdio (Mcp) and Warm
    // paths, and local/dev http (no CORE_URL), still open the embedded store
    // below, unchanged.
    if matches!(cli.mode, Mode::Http) {
        let api_key_set = std::env::var("PRIME_API_KEY").is_ok_and(|s| !s.is_empty());
        let core_url_set = std::env::var("CORE_URL").ok().filter(|s| !s.is_empty());
        if core_url_set.is_some() && !api_key_set {
            tracing::error!(
                "CORE_URL is set but PRIME_API_KEY is not — hosted tenant-scoped Prime is \
                 DISABLED. The HTTP endpoints would be unauthenticated, letting any caller \
                 spoof X-Tenant-Id and read another tenant's memory. Set PRIME_API_KEY to \
                 enable hosted mode (falling back to the embedded store for now)."
            );
        }
        if let Some(core_url) = core_url_set.filter(|_| api_key_set) {
            let cap = std::env::var("PRIME_TENANT_CACHE_CAP")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(64);
            let ttl_secs = std::env::var("PRIME_TENANT_CACHE_TTL_SECS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(300);
            tracing::info!(
                core_url = %core_url,
                cache_cap = cap,
                cache_ttl_secs = ttl_secs,
                "Hosted tenant-scoped Prime ENABLED — running STATELESS (no embedded store, no \
                 /data volume). All requests with X-Tenant-Id route to the remote Core."
            );
            tools::set_sync_status(tools::SyncStatus {
                enabled: false,
                remote_url: None,
            });
            let hosted = Arc::new(allsource_core::prime::hosted::HostedPrime::connect(
                core_url,
                std::env::var("CORE_API_KEY").ok(),
                cap,
                std::time::Duration::from_secs(ttl_secs),
            ));
            let state = Arc::new(http::AppState {
                prime: None,
                recall: None,
                hosted: Some(hosted),
            });
            tracing::info!("Starting HTTP server on port {}", cli.port);
            http::serve(state, cli.port).await?;
            return Ok(());
        }
    }

    tracing::info!("Opening Prime at {:?}", data_dir);

    let prime = Arc::new(allsource_core::prime::Prime::open(&data_dir).await?);

    tracing::info!(
        "Prime ready — {} nodes, {} edges",
        prime.stats().total_nodes,
        prime.stats().total_edges
    );

    // Warm mode: preflight the embedder and exit. This is the "make works-offline
    // true at a moment of the user's choosing" path from #200 — and the CI canary
    // hook: run it in a fresh container with no cache to catch model-fetch
    // regressions before they ship. Skips sync/recall setup entirely.
    if matches!(cli.mode, Mode::Warm) {
        tracing::info!("Warming embedder model (this may download ~25 MB on first run)…");
        match prime.embed_text("warm") {
            Ok(v) => {
                tracing::info!(
                    "Embedder ready — produced a {}-dim vector. Model is cached/loaded; \
                     prime_embed and prime_recall will work offline from here.",
                    v.len()
                );
                return Ok(());
            }
            Err(e) => {
                // The error already carries actionable recovery steps (cache dir,
                // PRIME_EMBED_MODEL_DIR, HF_ENDPOINT, the bring-your-own-vector
                // escape hatch). Surface it and exit non-zero for CI.
                anyhow::bail!("embedder warm-up failed:\n{e}");
            }
        }
    }

    // Hound mode: one-shot codebase → graph ingest, then exit (like Warm). Runs
    // before recall/sync setup — it neither serves requests nor needs the
    // embedder; it only writes prime.node/edge events into the embedded store.
    if matches!(cli.mode, Mode::Hound) {
        let Some(raw_path) = cli.path.as_ref() else {
            anyhow::bail!(
                "hound mode requires a PATH argument: \
                 allsource-prime --mode hound --data-dir <dir> <PATH>"
            );
        };
        let path = expand_home_path(raw_path);
        tracing::info!(
            embed = cli.embed,
            rebuild = cli.rebuild,
            "Hound: extracting code graph from {:?}",
            path
        );
        let summary = hound::ingest(&prime, &path, cli.embed, cli.rebuild).await?;
        tracing::info!(
            files = summary.files,
            nodes = summary.nodes,
            edges = summary.edges,
            defines = summary.defines,
            calls = summary.calls,
            ambiguous = summary.ambiguous,
            unresolved = summary.unresolved,
            embedded = summary.embedded,
            deleted_stale = summary.deleted_stale,
            "Hound: ingest complete"
        );
        println!(
            "hound: {} files → {} nodes, {} edges \
             ({} defines / {} calls / {} ambiguous, {} unresolved); \
             {} embedded, {} stale removed into {}",
            summary.files,
            summary.nodes,
            summary.edges,
            summary.defines,
            summary.calls,
            summary.ambiguous,
            summary.unresolved,
            summary.embedded,
            summary.deleted_stale,
            data_dir.display()
        );

        // Opt-in: LLM extraction of prose docs under --docs, into the same graph.
        if let Some(docs_raw) = cli.docs.as_ref() {
            let docs_path = expand_home_path(docs_raw);
            tracing::info!(
                "Hound: extracting docs from {:?} via the configured LLM",
                docs_path
            );
            let d = doc_extract::extract_docs(&prime, &docs_path, cli.embed).await?;
            tracing::info!(
                files = d.files,
                chunks = d.chunks,
                entities = d.entities,
                relationships = d.relationships,
                embedded = d.embedded,
                "Hound: doc extraction complete"
            );
            println!(
                "hound docs: {} files, {} chunks → {} entities, {} relationships, \
                 {} embedded, {} skipped; {} LLM calls, {} tokens",
                d.files,
                d.chunks,
                d.entities,
                d.relationships,
                d.embedded,
                d.skipped,
                d.llm_calls,
                d.total_tokens
            );
        }

        // Opt-in: write the human-readable GRAPH_REPORT.md (Graphify-style).
        if let Some(report_path) = cli.report.as_ref() {
            let graph = prime.full_graph(None, None, None);
            let md = report::compute(&graph, 25).to_markdown();
            let path = expand_home_path(report_path);
            std::fs::write(&path, md)?;
            println!("hound: wrote report → {}", path.display());
        }

        // Opt-in: export the graph to an interchange format.
        if let Some(fmt) = cli.export.as_ref() {
            let graph = prime.full_graph(None, None, None);
            let out = expand_home_path(
                cli.export_out
                    .as_ref()
                    .unwrap_or(&PathBuf::from(format!("hound-export-{fmt}"))),
            );
            if let Some(text) = export::render_text(&graph, fmt, 60) {
                std::fs::write(&out, text)?;
                println!("hound: exported {fmt} → {}", out.display());
            } else if let Some(files) = export::render_files(&graph, fmt) {
                for (rel, content) in files {
                    let p = out.join(&rel);
                    if let Some(parent) = p.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&p, content)?;
                }
                println!("hound: exported {fmt} vault → {}/", out.display());
            } else {
                anyhow::bail!(
                    "unknown export format '{fmt}' — choose one of: {}",
                    export::format_keys()
                );
            }
        }

        // Opt-in: with --sync-to + --api-key, push the freshly-extracted code
        // graph to the hosted tenant Core, then exit. One-shot drain (not the
        // forever loop the server modes spawn). Same trim-as-unset handling as
        // the server path so a DXT-cleared `--sync-to ""` is treated as local.
        let sync_to = cli
            .sync_to
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let api_key = cli
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        match (sync_to, api_key) {
            (Some(url), Some(key)) => {
                let sync_config = sync::SyncConfig {
                    remote_url: url.to_string(),
                    api_key: key.to_string(),
                    interval: std::time::Duration::from_millis(cli.sync_interval_ms),
                };
                tracing::info!(remote_url = %url, "Hound: pushing code graph to hosted Core…");
                let pushed = sync::flush_all(&prime, &sync_config, &data_dir).await?;
                tracing::info!(pushed, "Hound: sync complete");
                println!("hound: synced {pushed} events to {url}");
            }
            (Some(_), None) | (None, Some(_)) => {
                anyhow::bail!(
                    "--sync-to and --api-key must be supplied together (or neither). \
                     Get a tenant API key at https://www.all-source.xyz/connect."
                );
            }
            (None, None) => {
                tracing::info!(
                    "Hound: local-only graph. Pass --sync-to https://api.all-source.xyz \
                     and --api-key <tenant key> to push it to your hosted tenant graph."
                );
            }
        }

        return Ok(());
    }

    let recall_config = allsource_core::prime::recall::IndexConfig::default();
    let recall =
        allsource_core::prime::recall::RecallEngine::with_deps(prime.recall_deps(), &recall_config);

    tools::set_result_format(match cli.format {
        Format::Json => tools::ResultFormat::Json,
        Format::Toon => tools::ResultFormat::Toon,
    });

    // Hydrate the projection registry cache from the durable event log.
    // The event log (prime.projection.defined events) is the source of
    // truth; this just primes the in-memory accelerator. Failure is
    // tolerable — agents can re-register projections — but a warn is
    // logged so the operator notices.
    match prime.load_projection_defs().await {
        Ok(defs) => {
            let count = defs.len();
            projection_registry::hydrate(defs);
            if count > 0 {
                tracing::info!("Hydrated {count} projection definition(s) from event log");
            }
        }
        Err(e) => tracing::warn!(
            error = %e,
            "failed to hydrate projection registry from event log — agents will need to re-register"
        ),
    }

    // Unconditional: the deficit it closes is local and exists whether or not
    // the store syncs anywhere.
    let backfill_prime = Arc::clone(&prime);
    tokio::spawn(embed_backfill::run_embed_backfill_loop(
        backfill_prime,
        std::time::Duration::from_secs(30),
    ));

    // Spawn the push-only sync loop when both --sync-to and --api-key are set.
    // Mismatched flags are a user error worth surfacing rather than silently
    // dropping sync.
    //
    // Treat blank strings as "not set". The Claude Desktop DXT manifest always
    // passes `--sync-to ${user_config.sync_to}` / `--api-key ${...}`, so a user
    // who clears the sync URL in the extension settings hands us `--sync-to ""`
    // rather than omitting the flag. Without this guard that empty string would
    // be taken as a real remote and every push would fail against `/api/v1/events`.
    let sync_to = cli
        .sync_to
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let api_key = cli
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match (sync_to, api_key) {
        (Some(url), Some(key)) => {
            tracing::info!(
                remote_url = %url,
                "Prime sync ENABLED — prime.* events will be shipped to your AllSource tenant \
                 and appear on the dashboard Memory tab (all-source.xyz/dashboard/memory)."
            );
            tools::set_sync_status(tools::SyncStatus {
                enabled: true,
                remote_url: Some(url.to_string()),
            });
            // Give tool handlers (inbox_draft) a writer to the remote Core.
            core_writer::set_core_writer(url, key);
            let sync_config = sync::SyncConfig {
                remote_url: url.to_string(),
                api_key: key.to_string(),
                interval: std::time::Duration::from_millis(cli.sync_interval_ms),
            };
            let sync_prime = Arc::clone(&prime);
            let sync_data_dir = data_dir.clone();
            tokio::spawn(sync::run_sync_loop(sync_prime, sync_config, sync_data_dir));

            // Also pull email.* events from the same remote Core and fold them
            // into the local graph as interaction nodes (P0 AI inbox).
            let email_config = email_ingester::EmailIngestConfig {
                remote_url: url.to_string(),
                api_key: key.to_string(),
                interval: std::time::Duration::from_millis(cli.sync_interval_ms),
            };
            let email_prime = Arc::clone(&prime);
            let email_data_dir = data_dir.clone();
            tokio::spawn(email_ingester::run_email_ingest_loop(
                email_prime,
                email_config,
                email_data_dir,
            ));
        }
        (Some(_), None) | (None, Some(_)) => {
            anyhow::bail!(
                "--sync-to and --api-key must be supplied together (or neither). \
                 Get a tenant API key at https://www.all-source.xyz/connect."
            );
        }
        (None, None) => {
            tools::set_sync_status(tools::SyncStatus {
                enabled: false,
                remote_url: None,
            });
            // Loud, unmissable: a silent local-only mode is the exact failure
            // that makes the dashboard show "No memory yet" while writes look
            // like they're succeeding. Surface it at WARN on every startup.
            tracing::warn!(
                "Prime running LOCAL-ONLY — writes will NOT appear in your AllSource dashboard \
                 (all-source.xyz/dashboard/memory). Pass --sync-to https://api.all-source.xyz \
                 and --api-key <your tenant key> to sync. Get a key at \
                 https://www.all-source.xyz/connect. Call prime_stats to confirm sync state."
            );
        }
    }

    match cli.mode {
        Mode::Mcp => {
            tracing::info!(
                "Starting MCP server (stdio transport, {:?} payloads)",
                cli.format
            );
            let mut transport = StdioTransport::new(prime, recall);
            if cli.auto_inject {
                tracing::info!(
                    "Auto-inject enabled (max {} tokens)",
                    cli.auto_inject_max_tokens
                );
                transport = transport.with_auto_inject(cli.auto_inject_max_tokens);
            }
            transport.run().await?;
        }
        Mode::Http => {
            // Reaching here means http mode WITHOUT a hosted backend (no
            // CORE_URL, or CORE_URL set without PRIME_API_KEY → fall back to the
            // embedded single store). The hosted, stateless path returns earlier
            // in `main`, before any embedded Prime is opened.
            tracing::info!("Starting HTTP server on port {} (embedded store)", cli.port);
            tracing::info!(
                "Prime graph viewer: http://localhost:{}/api/v1/prime/graph.html \
                 (open in a browser to see your memory as a bubble graph + detail list)",
                cli.port
            );

            let state = Arc::new(http::AppState {
                prime: Some(prime),
                recall: Some(recall),
                hosted: None,
            });
            http::serve(state, cli.port).await?;
        }
        // Warm exits earlier, before sync/recall setup.
        Mode::Warm => unreachable!("warm mode returns before the server match"),
        // Hound is a one-shot ingest that also returns before this match.
        Mode::Hound => unreachable!("hound mode returns before the server match"),
        // Install writes skill files and returns before this match.
        Mode::Install => unreachable!("install mode returns before the server match"),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_home_path_handles_dxt_home_forms() {
        let home = std::env::var("HOME").expect("HOME is set in the test environment");
        let want = PathBuf::from(format!("{home}/.prime/memory"));

        // The exact trap the Claude Desktop DXT hits: ${HOME} passed verbatim.
        assert_eq!(expand_home_path(Path::new("${HOME}/.prime/memory")), want);
        assert_eq!(expand_home_path(Path::new("$HOME/.prime/memory")), want);
        assert_eq!(expand_home_path(Path::new("~/.prime/memory")), want);
        assert_eq!(expand_home_path(Path::new("~")), PathBuf::from(&home));

        // Absolute and unrelated paths are left untouched.
        assert_eq!(
            expand_home_path(Path::new("/Users/x/.prime/memory")),
            PathBuf::from("/Users/x/.prime/memory")
        );
        // A tilde that is not the path prefix must not be expanded.
        assert_eq!(
            expand_home_path(Path::new("/a/~/b")),
            PathBuf::from("/a/~/b")
        );
    }
}
