use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "wt", version, about = "Fast, safe Git worktree sessions")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a worktree session.
    New(NewArgs),
    /// List repository worktrees.
    Ls(ListArgs),
    /// Print a session's absolute worktree path.
    Path(SessionArg),
    /// Print a session's absolute worktree path for shell integration.
    Cd(SessionArg),
    /// Show the current worktree session.
    Status(StatusArgs),
    /// Print the current worktree root.
    Root,
    /// Safely remove a worktree session.
    Rm(RemoveArgs),
    /// Prune stale Git worktree metadata.
    Prune(PruneArgs),
}

#[derive(Debug, Args)]
pub struct NewArgs {
    pub name: String,

    /// Revision from which to create the branch.
    #[arg(long)]
    pub base: Option<String>,

    /// Full branch name instead of the configured name.
    #[arg(long)]
    pub branch: Option<String>,

    /// Emit a JSON object.
    #[arg(long, conflicts_with = "print_path")]
    pub json: bool,

    /// Print only the absolute worktree path.
    #[arg(long, conflicts_with = "json")]
    pub print_path: bool,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Emit a JSON array.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SessionArg {
    pub name: String,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    /// Emit a JSON object.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct RemoveArgs {
    pub name: String,

    /// Remove the worktree but retain its branch.
    #[arg(long, conflicts_with_all = ["force_branch", "force"])]
    pub keep_branch: bool,

    /// Permit discarding uncommitted worktree changes.
    #[arg(long)]
    pub force_worktree: bool,

    /// Permit deleting a branch with unmerged commits.
    #[arg(long)]
    pub force_branch: bool,

    /// Permit both worktree and branch data loss.
    #[arg(long, conflicts_with = "keep_branch")]
    pub force: bool,

    /// Emit a JSON object.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct PruneArgs {
    /// Report stale metadata without removing it.
    #[arg(long)]
    pub dry_run: bool,

    /// Emit a JSON object.
    #[arg(long)]
    pub json: bool,
}
