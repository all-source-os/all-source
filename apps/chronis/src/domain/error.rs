use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChronError {
    #[error("task {0} not found")]
    TaskNotFound(String),

    #[error(
        "could not allocate a free task ID after {0} attempts; the ID space is unexpectedly crowded"
    )]
    IdCollisionExhausted(usize),

    #[error("refusing to create task: ID {0} is already taken")]
    IdAlreadyTaken(String),

    #[error("task {id} is {current}, can only {action} open tasks")]
    InvalidTransition {
        id: String,
        current: String,
        action: String,
    },

    #[error("task {0} is already done")]
    AlreadyDone(String),

    #[error("task {0} is not claimed; nothing to release")]
    NotClaimed(String),

    #[error(
        "nothing to edit: provide at least one of --title/--description/--append-description/--priority/--type"
    )]
    NothingToEdit,

    #[error("no .chronis/ found (searched from {0:?}). Run `cn init` first.")]
    NoWorkspace(std::path::PathBuf),

    #[error(".chronis/ already exists in {0}")]
    WorkspaceExists(String),

    #[error("unknown status: {0}")]
    InvalidStatus(String),

    #[error("unknown priority: {0} (expected p0-p3)")]
    InvalidPriority(String),

    #[error("unknown task type: {0} (expected task, epic, bug, feature)")]
    InvalidTaskType(String),

    #[error("sync: {0}")]
    Sync(String),

    #[cfg(feature = "prime")]
    #[error("prime: {0}")]
    Prime(String),

    #[error(transparent)]
    Core(#[from] CoreError),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Wrapper around allsource_core errors (which are not `std::error::Error`).
#[derive(Debug, Error)]
#[error("{0}")]
pub struct CoreError(pub String);
