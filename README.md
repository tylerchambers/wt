# wt

A fast, single-binary CLI for Git worktree development sessions.

`wt` makes “one task = one isolated checkout” cheap and predictable. Git remains authoritative for repositories, branches, and worktrees; `wt` provides deterministic paths, concise output, and conservative cleanup.

## ⚡ Quickstart

Add the built-in integration to your Bash or Zsh startup file:

```bash
# ~/.bashrc
eval "$(wt shell-init bash)"

# ~/.zshrc
eval "$(wt shell-init zsh)"
```

Then, from anywhere inside a Git repository:

```bash
wt new fix-auth-race --cd
```

This creates the branch and linked worktree, then changes the initialized calling shell to
the new worktree directory.

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
wt new <name> --base main --cd
```

Base precedence:

1. `--base`
2. configured `default_base`
3. the current branch, or the current commit when detached

For interactive Bash and Zsh sessions, initialize the shell once and use `--cd`:

```bash
eval "$(wt shell-init zsh)" # use bash in ~/.bashrc
wt new fix-auth-race --base main --cd
```

The shell function delegates ordinary `wt` commands unchanged. It handles only
`wt new ... --cd`, creating the session first and changing directory only after success.
`--cd` cannot be combined with `--json` or `--print-path`.

For scripts and agents, keep using `--print-path` to print only the absolute path:

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

For interactive creation and entry, use the built-in Bash or Zsh helper rather than a
custom alias or function. Add one line to the matching startup file:

```bash
# ~/.bashrc
eval "$(wt shell-init bash)"

# ~/.zshrc
eval "$(wt shell-init zsh)"
```

After restarting the shell (or evaluating the line in the current shell),
`wt new <name> --cd` enters the exact path created by `wt new`. Existing sessions can
still be resolved with `wt path` or `wt cd`; those commands remain path-printing helpers.

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

Preview the same safety checks and planned worktree/branch actions without changing the
worktree, branch, lock state, or `wt` metadata:

```bash
wt rm fix-auth-race --dry-run
wt rm fix-auth-race --dry-run --json
```

Dry-run accepts the same `--keep-branch` and force options as real removal. It refuses
dirty, locked, missing, main-worktree, unknown-base, and unmerged-branch cases under the
same rules. Force-enabled previews report whether each destructive authorization was
provided and whether the current state requires it; they never perform the authorized
action.

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
