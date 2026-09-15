use clap::{Parser, Subcommand, ValueEnum};

use crate::domain::task::{Priority, TaskStatus, TaskType};

#[derive(Parser)]
#[command(
    name = "cn",
    version,
    about = "Chronis — the agent-native task CLI",
    long_about = "Chronis — the agent-native task CLI. Event-sourced, TOON-optimized.\n\n\
        Inspired by beads_rust. Every action is an immutable event.\n\
        State is derived from projections over the event stream.\n\n\
        TOON OUTPUT (the key differentiator):\n  \
          Pass --toon to ANY command for TOON (Token-Oriented Object Notation).\n  \
          ~50% fewer tokens than JSON — pipe-delimited, no braces, no quotes.\n  \
          Lists:    [id|type|title|pri|status|...]\\n val|val|val\n  \
          Details:  key:value lines\n  \
          Actions:  ok:verb:id (single-line ack)\n\n\
        AGENT WORKFLOW (4 commands, ~20 tokens overhead):\n  \
          cn ready --toon          → pick a task from TOON table\n  \
          cn claim <id>  --toon    → ok:claimed:<id>\n  \
          cn done <id>   --toon    → ok:done:<id>\n  \
          cn list --toon           → verify final state\n\n\
        QUICK REFERENCE:\n  \
          cn task create <title>   Create task (-p priority, -t type, --parent, --blocked-by, -d desc)\n  \
          cn task edit <id>        Edit a task (--title, -d desc, --append-description, -p, -t)\n  \
          cn list [filters]        List tasks (--status, --priority, --type, --claimed-by,\n  \
                                   --unclaimed, --parent [--depth all], --no-blockers,\n  \
                                   --archived, --all, --format toon|json|tsv|table)\n  \
          cn ready                 Open + unblocked tasks\n  \
          cn show <id>             Task detail + event timeline\n  \
          cn claim <id>            Claim task (--cascade for epics)\n  \
          cn release <id>          Release a claimed task back to open (--reason)\n  \
          cn done <id>             Complete task (--reason, --cascade)\n  \
          cn approve <id>          Approve task\n  \
          cn archive [ids..]       Archive tasks (--all-done, --done-before <days>)\n  \
          cn unarchive [ids..]     Restore archived tasks\n  \
          cn dep add|remove        Manage blockers\n  \
          cn sync                  Sync events with remote Core\n  \
          cn tui                   Interactive TUI\n  \
          cn serve                 Web viewer\n\n\
        MIGRATE FROM BEADS:\n  \
          cn migrate-beads         Import .beads/issues.jsonl → chronis events"
)]
pub struct Cli {
    /// Output TOON format (Token-Oriented Object Notation) for agents/scripts — ~50% fewer tokens
    #[arg(long, global = true)]
    pub toon: bool,

    /// Seconds a non-interactive command may run before it gives up (0 disables).
    ///
    /// A killed caller does not kill `cn`: the process is reparented to init and
    /// keeps working for an answer nobody will read. On a loaded machine those
    /// orphans accumulate and compete with the work that made them slow. A
    /// status query that cannot answer in time is more useful failing.
    #[arg(long, global = true, default_value_t = 30, env = "CN_TIMEOUT")]
    pub timeout: u64,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Initialize a new .chronis workspace
    Init(InitArgs),

    /// Task management
    Task(TaskArgs),

    /// List tasks
    #[command(visible_alias = "ls")]
    List(ListArgs),

    /// Show task details and timeline
    #[command(visible_alias = "s")]
    Show(ShowArgs),

    /// Show tasks that are ready to work on (open + unblocked)
    #[command(visible_alias = "r")]
    Ready,

    /// Claim a task
    #[command(visible_alias = "c")]
    Claim(ClaimArgs),

    /// Release a claimed task back to the pool
    Release(ReleaseArgs),

    /// Mark a task as done
    #[command(visible_alias = "d")]
    Done(DoneArgs),

    /// Approve a task
    Approve(ApproveArgs),

    /// Archive tasks (hide from default listings)
    Archive(ArchiveArgs),

    /// Unarchive a task
    Unarchive(UnarchiveArgs),

    /// Manage task dependencies
    Dep(DepArgs),

    /// Migrate issues from .beads/ to chronis
    MigrateBeads(MigrateBeadsArgs),

    /// Sync events with a remote Core (embedded mode only), or with git via --git
    Sync(SyncArgs),

    /// Configure AllSource Prime (agent memory) for this project
    Prime(PrimeArgs),

    /// Launch interactive TUI dashboard
    Tui,

    /// Start embedded web viewer
    Serve(ServeArgs),
}

#[derive(clap::Args)]
pub struct InitArgs {
    /// Remote AllSource URL for team sync (e.g. https://api.all-source.xyz)
    #[arg(long)]
    pub remote: Option<String>,

    /// API key for authenticating with AllSource
    #[arg(long)]
    pub api_key: Option<String>,
}

#[derive(clap::Args)]
pub struct SyncArgs {
    /// Commit and push `.chronis/` through git instead of syncing to a Core.
    ///
    /// Stages only `.chronis/`, never the rest of the working tree, and refuses
    /// to commit if anything else is already staged. Does not force-push.
    #[arg(long)]
    pub git: bool,
}

#[derive(clap::Args)]
pub struct TaskArgs {
    #[command(subcommand)]
    pub subcommand: TaskCommands,
}

#[derive(Subcommand)]
pub enum TaskCommands {
    /// Create a new task
    Create(CreateArgs),

    /// Edit an existing task's title, description, priority, or type
    Edit(EditArgs),
}

#[derive(clap::Args)]
pub struct EditArgs {
    /// Task ID to edit
    pub id: String,

    /// New title
    #[arg(long)]
    pub title: Option<String>,

    /// New description (replaces the existing one; `-` reads from stdin)
    #[arg(short = 'd', long)]
    pub description: Option<String>,

    /// Append to the existing description instead of replacing it
    #[arg(long, conflicts_with = "description")]
    pub append_description: Option<String>,

    /// New priority (p0-p3)
    #[arg(short, long)]
    pub priority: Option<Priority>,

    /// New task type (task, epic, bug, feature)
    #[arg(short = 't', long = "type")]
    pub task_type: Option<TaskType>,
}

#[derive(clap::Args)]
pub struct CreateArgs {
    /// Task title
    pub title: String,

    /// Priority (p0-p3)
    #[arg(short, long, default_value = "p2")]
    pub priority: Priority,

    /// Task IDs that block this task
    #[arg(long, value_delimiter = ',')]
    pub blocked_by: Vec<String>,

    /// Task type (task, epic, bug, feature)
    #[arg(short = 't', long = "type", default_value = "task")]
    pub task_type: TaskType,

    /// Parent task ID (for hierarchy)
    #[arg(long)]
    pub parent: Option<String>,

    /// Description
    #[arg(short = 'd', long)]
    pub description: Option<String>,
}

/// How far down the hierarchy `--parent` reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum Depth {
    /// Direct children only
    #[default]
    Direct,
    /// All descendants (children, grandchildren, …)
    All,
}

/// Output format for `cn list` / `cn ready`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Pipe-delimited TOON (token-optimized for agents)
    Toon,
    /// JSON array of task objects (for `jq` / downstream tooling)
    Json,
    /// Tab-separated values with a header row
    Tsv,
    /// Rounded human-readable table
    Table,
}

#[derive(clap::Args)]
pub struct ListArgs {
    /// Filter by status — comma-separated, OR'd (e.g. open,in-progress)
    #[arg(short, long, value_delimiter = ',')]
    pub status: Vec<TaskStatus>,

    /// Filter by priority — comma-separated, OR'd (e.g. p0,p1)
    #[arg(short, long, value_delimiter = ',')]
    pub priority: Vec<Priority>,

    /// Filter by type — comma-separated, OR'd (e.g. bug,feature)
    #[arg(short = 't', long = "type", value_delimiter = ',')]
    pub task_type: Vec<TaskType>,

    /// Only tasks claimed by this agent label (e.g. human, claude:f132c66c)
    #[arg(long, value_name = "AGENT")]
    pub claimed_by: Option<String>,

    /// Only claimed tasks
    #[arg(long, conflicts_with = "unclaimed")]
    pub claimed: bool,

    /// Only unclaimed tasks
    #[arg(long)]
    pub unclaimed: bool,

    /// Only tasks under this parent (see --depth)
    #[arg(long, value_name = "ID")]
    pub parent: Option<String>,

    /// How far --parent reaches: direct children or all descendants
    #[arg(long, value_enum, default_value_t = Depth::Direct, requires = "parent")]
    pub depth: Depth,

    /// Only tasks blocked by this task ID
    #[arg(long, value_name = "ID")]
    pub blocked_by: Option<String>,

    /// Only unblocked tasks (all blockers done) — composable `cn ready`
    #[arg(long)]
    pub no_blockers: bool,

    /// Show only archived tasks
    #[arg(long)]
    pub archived: bool,

    /// Show all tasks (including archived)
    #[arg(long)]
    pub all: bool,

    /// Output format (overrides the global --toon flag)
    #[arg(long, value_enum)]
    pub format: Option<OutputFormat>,
}

#[derive(clap::Args)]
pub struct ShowArgs {
    /// Task ID
    pub id: String,
}

#[derive(clap::Args)]
pub struct ClaimArgs {
    /// Task ID
    pub id: String,

    /// Also claim all children (for epics)
    #[arg(long)]
    pub cascade: bool,
}

#[derive(clap::Args)]
pub struct ReleaseArgs {
    /// Task ID
    pub id: String,

    /// Why the claim is being released (e.g. "session died")
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(clap::Args)]
pub struct DoneArgs {
    /// Task ID
    pub id: String,

    /// Reason or summary for completion
    #[arg(long)]
    pub reason: Option<String>,

    /// Also mark all children as done (for epics)
    #[arg(long)]
    pub cascade: bool,
}

#[derive(clap::Args)]
pub struct ApproveArgs {
    /// Task ID
    pub id: String,
}

#[derive(clap::Args)]
pub struct ArchiveArgs {
    /// Task IDs to archive
    pub ids: Vec<String>,

    /// Archive all done tasks
    #[arg(long)]
    pub all_done: bool,

    /// Archive tasks done before N days ago (e.g., 30)
    #[arg(long, value_name = "DAYS")]
    pub done_before: Option<u64>,
}

#[derive(clap::Args)]
pub struct UnarchiveArgs {
    /// Task IDs to unarchive
    pub ids: Vec<String>,
}

#[derive(clap::Args)]
pub struct DepArgs {
    #[command(subcommand)]
    pub subcommand: DepCommands,
}

#[derive(Subcommand)]
pub enum DepCommands {
    /// Add a dependency (blocker) to a task
    Add(DepAddArgs),
    /// Remove a dependency from a task
    Remove(DepRemoveArgs),
}

#[derive(clap::Args)]
pub struct DepAddArgs {
    /// Task ID that is blocked
    pub task_id: String,
    /// Task ID that blocks it
    pub blocker_id: String,
}

#[derive(clap::Args)]
pub struct DepRemoveArgs {
    /// Task ID to remove dependency from
    pub task_id: String,
    /// Blocker task ID to remove
    pub blocker_id: String,
}

#[derive(clap::Args)]
pub struct MigrateBeadsArgs {
    /// Path to .beads/ directory (default: .beads/ in current directory)
    #[arg(long, default_value = ".beads")]
    pub beads_dir: String,
}

#[derive(clap::Args)]
pub struct PrimeArgs {
    #[command(subcommand)]
    pub subcommand: PrimeCommands,
}

#[derive(Subcommand)]
pub enum PrimeCommands {
    /// Wire AllSource Prime into Claude Code for this project (.mcp.json)
    Setup(PrimeSetupArgs),
}

#[derive(clap::Args)]
pub struct PrimeSetupArgs {
    /// Override the `allsource-prime` binary path (default: $PATH lookup)
    #[arg(long, value_name = "PATH")]
    pub bin: Option<String>,

    /// Override the Prime data directory (default: <workspace>/.chronis/prime)
    #[arg(long, value_name = "DIR")]
    pub data_dir: Option<std::path::PathBuf>,
}

#[derive(clap::Args)]
pub struct ServeArgs {
    /// Port to bind the web viewer
    #[arg(short, long, default_value = "3905")]
    pub port: u16,

    /// Auto-open browser after starting
    #[arg(long)]
    pub open: bool,
}
