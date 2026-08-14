use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use crate::error::{Error, Result};

#[derive(Debug)]
pub struct GitOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone)]
pub struct Git {
    cwd: PathBuf,
}

impl Git {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        Self { cwd: cwd.into() }
    }

    pub fn run<I, S>(&self, args: I) -> Result<GitOutput>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args = args
            .into_iter()
            .map(|arg| arg.as_ref().to_os_string())
            .collect::<Vec<_>>();
        let output = Command::new("git")
            .args(&args)
            .current_dir(&self.cwd)
            .output()
            .map_err(|source| Error::io("could not execute git", source))?;
        Ok(GitOutput {
            status: output.status,
            stdout: String::from_utf8(output.stdout).map_err(Error::InvalidGitOutput)?,
            stderr: String::from_utf8(output.stderr).map_err(Error::InvalidGitOutput)?,
        })
    }

    pub fn checked<I, S>(&self, args: I) -> Result<GitOutput>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args = args
            .into_iter()
            .map(|arg| arg.as_ref().to_os_string())
            .collect::<Vec<_>>();
        let output = self.run(args.iter())?;
        if output.status.success() {
            return Ok(output);
        }
        Err(command_failure(&args, &output.stderr))
    }
}

fn command_failure(args: &[OsString], stderr: &str) -> Error {
    let args = args
        .iter()
        .map(|arg| shell_escape_for_diagnostic(arg))
        .collect::<Vec<_>>()
        .join(" ");
    let stderr = stderr.trim().to_owned();
    Error::GitCommandFailed { args, stderr }
}

fn shell_escape_for_diagnostic(value: &OsStr) -> String {
    let value = value.to_string_lossy();
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "-._/:=".contains(character))
    {
        value.into_owned()
    } else {
        format!("{:?}", value)
    }
}

#[derive(Debug, Clone)]
pub struct Repository {
    pub current_root: PathBuf,
    pub main_root: PathBuf,
    pub common_git_dir: PathBuf,
    pub name: String,
    git: Git,
}

impl Repository {
    pub fn discover(cwd: &Path) -> Result<Self> {
        let discovery = Git::new(cwd);
        let root = discovery.run(["rev-parse", "--show-toplevel"])?;
        if !root.status.success() {
            return Err(Error::NotGitRepository);
        }
        let current_root = path_from_line(&root.stdout)?;
        let common =
            discovery.checked(["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
        let common_git_dir = path_from_line(&common.stdout)?;
        let git = Git::new(&current_root);
        let worktrees = list_worktrees_with(&git)?;
        let main_root = worktrees
            .first()
            .map(|worktree| worktree.path.clone())
            .ok_or(Error::NotGitRepository)?;
        let name = main_root
            .file_name()
            .and_then(OsStr::to_str)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                Error::Configuration(format!(
                    "could not derive a repository name from '{}'",
                    main_root.display()
                ))
            })?
            .to_owned();
        Ok(Self {
            current_root,
            main_root,
            common_git_dir,
            name,
            git,
        })
    }

    pub fn worktrees(&self) -> Result<Vec<Worktree>> {
        list_worktrees_with(&self.git)
    }

    pub fn current_branch(&self) -> Result<Option<String>> {
        let output = self
            .git
            .run(["symbolic-ref", "--quiet", "--short", "HEAD"])?;
        match output.status.code() {
            Some(0) => Ok(Some(output.stdout.trim().to_owned())),
            Some(1) => Ok(None),
            _ => Err(command_failure(
                &[
                    OsString::from("symbolic-ref"),
                    OsString::from("--quiet"),
                    OsString::from("--short"),
                    OsString::from("HEAD"),
                ],
                &output.stderr,
            )),
        }
    }

    pub fn current_revision(&self) -> Result<String> {
        Ok(self
            .git
            .checked(["rev-parse", "HEAD"])?
            .stdout
            .trim()
            .to_owned())
    }

    pub fn validate_branch(&self, branch: &str) -> Result<()> {
        let output = self.git.run([
            OsStr::new("check-ref-format"),
            OsStr::new("--branch"),
            branch.as_ref(),
        ])?;
        if output.status.success() {
            Ok(())
        } else {
            Err(Error::InvalidBranchName(branch.to_owned()))
        }
    }

    pub fn branch_exists(&self, branch: &str) -> Result<bool> {
        let reference = format!("refs/heads/{branch}");
        let output = self.git.run([
            OsStr::new("show-ref"),
            OsStr::new("--verify"),
            OsStr::new("--quiet"),
            reference.as_ref(),
        ])?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(command_failure(
                &[
                    OsString::from("show-ref"),
                    OsString::from("--verify"),
                    OsString::from("--quiet"),
                    OsString::from(reference),
                ],
                &output.stderr,
            )),
        }
    }

    pub fn add_worktree(&self, branch: &str, path: &Path, base: &str) -> Result<()> {
        self.git.checked([
            OsStr::new("worktree"),
            OsStr::new("add"),
            OsStr::new("-b"),
            branch.as_ref(),
            path.as_os_str(),
            base.as_ref(),
        ])?;
        Ok(())
    }

    pub fn remove_worktree(&self, path: &Path, force: bool) -> Result<()> {
        let mut args = vec![OsString::from("worktree"), OsString::from("remove")];
        if force {
            args.push(OsString::from("--force"));
        }
        args.push(path.as_os_str().to_os_string());
        self.git.checked(args.iter())?;
        Ok(())
    }

    pub fn unlock_worktree(&self, path: &Path) -> Result<()> {
        self.git.checked([
            OsStr::new("worktree"),
            OsStr::new("unlock"),
            path.as_os_str(),
        ])?;
        Ok(())
    }

    pub fn delete_branch(&self, branch: &str, force: bool) -> Result<()> {
        let mode = if force { "-D" } else { "-d" };
        self.git.checked(["branch", mode, branch])?;
        Ok(())
    }

    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool> {
        let output = self.git.run([
            OsStr::new("merge-base"),
            OsStr::new("--is-ancestor"),
            ancestor.as_ref(),
            descendant.as_ref(),
        ])?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(command_failure(
                &[
                    OsString::from("merge-base"),
                    OsString::from("--is-ancestor"),
                    OsString::from(ancestor),
                    OsString::from(descendant),
                ],
                &output.stderr,
            )),
        }
    }

    pub fn dirty(&self, path: &Path) -> Result<bool> {
        let output =
            Git::new(path).checked(["status", "--porcelain", "--untracked-files=normal"])?;
        Ok(!output.stdout.is_empty())
    }

    pub fn repository_default_branch(&self) -> Result<Option<String>> {
        let output = self.git.run([
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ])?;
        match output.status.code() {
            Some(0) => Ok(Some(output.stdout.trim().to_owned())),
            Some(1) => Ok(None),
            _ => Err(command_failure(
                &[
                    OsString::from("symbolic-ref"),
                    OsString::from("--quiet"),
                    OsString::from("--short"),
                    OsString::from("refs/remotes/origin/HEAD"),
                ],
                &output.stderr,
            )),
        }
    }

    pub fn prune(&self, dry_run: bool) -> Result<Vec<String>> {
        let mut args = vec!["worktree", "prune", "--verbose"];
        if dry_run {
            args.push("--dry-run");
        }
        let output = self.git.checked(args)?;
        Ok(output
            .stdout
            .lines()
            .chain(output.stderr.lines())
            .filter(|line| !line.trim().is_empty())
            .map(str::to_owned)
            .collect())
    }
}

fn path_from_line(output: &str) -> Result<PathBuf> {
    let value = output.trim_end_matches(['\r', '\n']);
    if value.is_empty() {
        return Err(Error::NotGitRepository);
    }
    Ok(PathBuf::from(value))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    Branch(String),
    Detached,
}

#[derive(Debug, Clone)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: String,
    pub checkout: Head,
    pub locked: bool,
    pub prunable: bool,
}

fn list_worktrees_with(git: &Git) -> Result<Vec<Worktree>> {
    let output = git.checked(["worktree", "list", "--porcelain", "-z"])?;
    parse_worktrees(&output.stdout)
}

fn parse_worktrees(output: &str) -> Result<Vec<Worktree>> {
    #[derive(Default)]
    struct Builder {
        path: Option<PathBuf>,
        head: Option<String>,
        branch: Option<String>,
        detached: bool,
        locked: bool,
        prunable: bool,
    }

    fn finish(builder: Builder) -> Result<Option<Worktree>> {
        let Some(path) = builder.path else {
            return Ok(None);
        };
        let checkout = match builder.branch {
            Some(branch) => Head::Branch(branch),
            None if builder.detached => Head::Detached,
            None => Head::Detached,
        };
        Ok(Some(Worktree {
            path,
            head: builder.head.unwrap_or_default(),
            checkout,
            locked: builder.locked,
            prunable: builder.prunable,
        }))
    }

    let mut worktrees = Vec::new();
    let mut builder = Builder::default();
    for field in output.split('\0') {
        if field.is_empty() {
            if let Some(worktree) = finish(std::mem::take(&mut builder))? {
                worktrees.push(worktree);
            }
            continue;
        }
        if let Some(path) = field.strip_prefix("worktree ") {
            if builder.path.is_some()
                && let Some(worktree) = finish(std::mem::take(&mut builder))?
            {
                worktrees.push(worktree);
            }
            builder.path = Some(PathBuf::from(path));
        } else if let Some(head) = field.strip_prefix("HEAD ") {
            builder.head = Some(head.to_owned());
        } else if let Some(branch) = field.strip_prefix("branch refs/heads/") {
            builder.branch = Some(branch.to_owned());
        } else if field == "detached" {
            builder.detached = true;
        } else if field == "locked" || field.starts_with("locked ") {
            builder.locked = true;
        } else if field == "prunable" || field.starts_with("prunable ") {
            builder.prunable = true;
        }
    }
    if let Some(worktree) = finish(builder)? {
        worktrees.push(worktree);
    }
    Ok(worktrees)
}

#[cfg(test)]
mod tests {
    use super::{Head, parse_worktrees};

    #[test]
    fn parses_nul_delimited_porcelain_without_splitting_spaced_paths() {
        let input = concat!(
            "worktree /tmp/main repository\0",
            "HEAD abcdef123456\0",
            "branch refs/heads/main\0",
            "\0",
            "worktree /tmp/linked tree\0",
            "HEAD 123456abcdef\0",
            "detached\0",
            "locked reason with spaces\0",
            "\0",
        );
        let worktrees = parse_worktrees(input).expect("parse worktrees");
        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[0].path.to_string_lossy(), "/tmp/main repository");
        assert_eq!(worktrees[0].checkout, Head::Branch("main".to_owned()));
        assert_eq!(worktrees[1].path.to_string_lossy(), "/tmp/linked tree");
        assert_eq!(worktrees[1].checkout, Head::Detached);
        assert!(worktrees[1].locked);
    }
}
