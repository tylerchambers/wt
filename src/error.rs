use std::io;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("not inside a Git repository")]
    NotGitRepository,

    #[error("invalid session name '{0}'")]
    InvalidSessionName(String),

    #[error("invalid branch name '{0}'")]
    InvalidBranchName(String),

    #[error("session '{0}' already exists")]
    SessionAlreadyExists(String),

    #[error("session '{0}' was not found")]
    SessionNotFound(String),

    #[error("branch '{0}' already exists")]
    BranchAlreadyExists(String),

    #[error("worktree path '{}' already exists", .0.display())]
    WorktreePathExists(PathBuf),

    #[error("worktree '{0}' has uncommitted changes; use --force-worktree or --force")]
    WorktreeDirty(String),

    #[error(
        "branch '{branch}' contains commits not merged into '{base}'; use --force-branch or --force"
    )]
    BranchNotMerged { branch: String, base: String },

    #[error("worktree '{0}' is locked")]
    WorktreeLocked(String),

    #[error("worktree '{0}' is missing; run 'wt prune' to remove stale Git metadata")]
    WorktreeMissing(String),

    #[error("the main worktree cannot be removed")]
    CannotRemoveMain,

    #[error("cannot determine the base for '{0}'; configure default_base or use --force-branch")]
    BaseUnknown(String),

    #[error("configuration error: {0}")]
    Configuration(String),

    #[error("git command failed: git {args}: {stderr}")]
    GitCommandFailed { args: String, stderr: String },

    #[error("{action}: {source}")]
    Io {
        action: String,
        #[source]
        source: io::Error,
    },

    #[error("invalid UTF-8 in Git output")]
    InvalidGitOutput(#[source] std::string::FromUtf8Error),

    #[error("could not encode JSON output: {0}")]
    Json(#[from] serde_json::Error),
}

impl Error {
    pub fn io(action: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            action: action.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
