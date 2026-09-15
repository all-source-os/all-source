#[cfg(not(feature = "dhat-heap"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use chronis::{
    infrastructure::workspace::Workspace,
    presentation::{
        cli::{Cli, Command},
        dispatch,
    },
};
use clap::Parser;
use tracing_subscriber::EnvFilter;

/// Distinct from 1 so a caller can tell "ran out of time" from "the command failed".
const EXIT_TIMEOUT: i32 = 75;

#[cfg_attr(feature = "hotpath", hotpath::main)]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let cli = Cli::parse();

    match &cli.command {
        Command::Init(args) => {
            dispatch::dispatch_init(args)?;
        }
        Command::Prime(args) => {
            dispatch::dispatch_prime(args, cli.toon)?;
        }
        Command::Tui => {
            let ws = Workspace::open().await?;
            let repo = ws.repo();
            chronis::presentation::tui::run(repo).await?;
            // repo is moved into tui::run, no need to drop
            ws.shutdown().await?;
        }
        Command::Serve(args) => {
            let ws = Workspace::open().await?;
            let repo = ws.repo();
            chronis::presentation::web::run(repo, args.port, args.open).await?;
            ws.shutdown().await?;
        }
        // Tui and Serve are long-running by design and are deliberately not
        // bounded; everything below answers a question and then exits.
        _ => {
            let work = async {
                let ws = Workspace::open().await?;
                let repo = ws.repo();
                let result =
                    dispatch::dispatch(&cli.command, &repo, &ws.root, &ws.config, cli.toon).await;
                drop(repo);
                ws.shutdown().await?;
                result
            };

            if cli.timeout == 0 {
                work.await?;
            } else {
                let limit = std::time::Duration::from_secs(cli.timeout);
                match tokio::time::timeout(limit, work).await {
                    Ok(result) => result?,
                    Err(_) => {
                        eprintln!(
                            "cn: gave up after {}s. The machine is likely loaded; \
                             raise the ceiling with --timeout <seconds> or CN_TIMEOUT, \
                             or disable it with --timeout 0.",
                            cli.timeout
                        );
                        // Exiting rather than unwinding: the point is to stop being an
                        // orphan holding CPU for an answer nobody is waiting for.
                        std::process::exit(EXIT_TIMEOUT);
                    }
                }
            }
        }
    }

    Ok(())
}
