# wt

A fast, single-binary CLI for Git worktree development sessions.

`wt` makes “one task = one isolated checkout” cheap and predictable. Git remains authoritative for repositories, branches, and worktrees; `wt` provides deterministic paths, concise output, and conservative cleanup.

## ⚡ Quickstart

From anywhere inside a Git repository:

```bash
wt new fix-auth-race
```

This creates a branch and linked worktree, then prints the worktree path. Enter it with:

```bash
cd "$(wt path fix-auth-race)"
```

Create multiple isolated sessions:

```bash
wt new feature-a --base main
wt new feature-b --base main
wt ls
```

Inspect the current session from any nested directory:

```bash
wt status
wt root
```

Remove a clean session whose branch is merged into its recorded base:

```bash
wt rm feature-a
```

`wt rm` refuses to discard dirty worktrees, locked worktrees, or unmerged branches.

## 📦 Installation

### From a local checkout

Requires a Rust toolchain to build and Git at runtime:

```bash
git clone <repository-url> wt
cd wt
cargo install --path .
```

Ensure Cargo's binary directory is on `PATH`:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

Verify the installation:

```bash
wt --version
```

### Build without installing

```bash
cargo build --release
./target/release/wt --help
```

The installed binary has no runtime dependency beyond `git`.

## Commands

### Create a session

```bash
wt new <name>
wt new <name> --base main
wt new <name> --branch tyler/custom-branch
```

Base precedence:

1. `--base`
2. configured `default_base`
3. the current branch, or the current commit when detached

For scripts and agents, print only the absolute path:

```bash
DIR="$(wt new agent-auth-fix --base main --print-path)"
cd "$DIR"
```

Request structured output with:

```bash
wt new experiment --json
```

### List sessions

```bash
wt ls
wt ls --json
```

The list includes the main worktree and reports `clean`, `dirty`, `detached`, `locked`, or `missing` state.

### Resolve and enter sessions

```bash
wt path fix-auth-race
wt cd fix-auth-race
```

Both commands print exactly the absolute worktree path. A subprocess cannot change its parent shell directory, so `wt cd` deliberately prints rather than pretending to change directories.

Optional shell helper:

```bash
wtcd() {
    cd "$(wt path "$1")"
}
```

### Inspect the current session

```bash
wt status
wt status --json
wt root
```

`wt root` prints only the current worktree root, making it suitable for command substitution.

### Remove a session

```bash
wt rm fix-auth-race
```

Keep the branch after removing its worktree:

```bash
wt rm fix-auth-race --keep-branch
```

Explicit destructive overrides are granular:

```bash
wt rm fix-auth-race --force-worktree  # discard uncommitted worktree changes
wt rm fix-auth-race --force-branch    # delete an unmerged branch
wt rm fix-auth-race --force           # allow both forms of data loss
```

Use force flags only when the data loss is intentional. No other option implies force.

### Prune stale Git metadata

```bash
wt prune --dry-run
wt prune
```

This wraps `git worktree prune`; it does not delete active directories merely because they look old.

## ⚙️ Configuration

Configuration is optional. The global file is:

```text
~/.config/wt/config.toml
```

`XDG_CONFIG_HOME` is respected when set.

Example:

```toml
worktree_dir = "~/dev/.worktrees/{repo}/{session}"
branch_prefix = "work/"
default_base = "main"
terminal = "none"
delete_merged_branches = true
```

Available worktree path placeholders:

- `{repo}` — main worktree directory name
- `{session}` — validated session name
- `{branch}` — complete branch name

Without configuration, worktrees use:

```text
<main-worktree-parent>/.worktrees/<repository>/<session>
```

## 🛡️ Safety model

Git is the source of truth. `wt` derives live worktree, branch, HEAD, lock, and dirty state from Git rather than maintaining a separate registry.

Minimal metadata records the session name, branch, and creation base under the repository's common Git directory. It exists only to make branch cleanup safer. If metadata disappears, worktrees remain discoverable; cleanup refuses branch deletion when it cannot establish a safe base.

Session names allow ASCII letters, numbers, `-`, `_`, `.`, and `/`. Empty names, absolute paths, traversal, control characters, and ambiguous path components are rejected.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```
