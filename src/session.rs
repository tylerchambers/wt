use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::git::{Head, Repository, Worktree};
use crate::metadata::{MetadataStore, SessionMetadata};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionName(String);

impl SessionName {
    pub fn parse(value: &str) -> Result<Self> {
        let valid_characters = value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '/')
        });
        let valid_components = !value.is_empty()
            && !value.starts_with('/')
            && !value.ends_with('/')
            && !value.contains("//")
            && !value.contains("..")
            && Path::new(value)
                .components()
                .all(|component| matches!(component, Component::Normal(_)));
        if valid_characters && valid_components {
            Ok(Self(value.to_owned()))
        } else {
            Err(Error::InvalidSessionName(value.to_owned()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorktreeState {
    Clean,
    Dirty,
    Detached,
    Locked,
    Missing,
}

impl fmt::Display for WorktreeState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Clean => "clean",
            Self::Dirty => "dirty",
            Self::Detached => "detached",
            Self::Locked => "locked",
            Self::Missing => "missing",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Serialize)]
pub struct CreatedSession {
    pub name: String,
    pub branch: String,
    pub path: PathBuf,
    pub base: String,
}

#[derive(Debug, Serialize)]
pub struct ListedSession {
    pub name: String,
    pub branch: Option<String>,
    pub path: PathBuf,
    pub head: String,
    pub dirty: bool,
    pub locked: bool,
    pub status: WorktreeState,
}

#[derive(Debug, Serialize)]
pub struct CurrentSession {
    pub repository: String,
    pub name: String,
    pub branch: Option<String>,
    pub base: Option<String>,
    pub path: PathBuf,
    pub head: String,
    pub dirty: bool,
    pub locked: bool,
    pub status: WorktreeState,
}

#[derive(Debug, Serialize)]
pub struct RemovedSession {
    pub name: String,
    pub branch: Option<String>,
    pub path: PathBuf,
    pub branch_deleted: bool,
}

#[derive(Debug, Serialize)]
pub struct PruneResult {
    pub dry_run: bool,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct RemoveOptions {
    pub keep_branch: bool,
    pub force_worktree: bool,
    pub force_branch: bool,
}

pub struct SessionManager {
    repository: Repository,
    config: Config,
    metadata: MetadataStore,
}

impl SessionManager {
    pub fn discover(cwd: &Path) -> Result<Self> {
        let repository = Repository::discover(cwd)?;
        let config = Config::load()?;
        let metadata = MetadataStore::new(&repository.common_git_dir);
        Ok(Self {
            repository,
            config,
            metadata,
        })
    }

    pub fn new_session(
        &self,
        name: &str,
        explicit_branch: Option<&str>,
        explicit_base: Option<&str>,
    ) -> Result<CreatedSession> {
        let name = SessionName::parse(name)?;
        if self.resolve_optional(&name)?.is_some() {
            return Err(Error::SessionAlreadyExists(name.to_string()));
        }

        let branch = explicit_branch
            .map(str::to_owned)
            .unwrap_or_else(|| self.config.branch_for(name.as_str()));
        self.repository.validate_branch(&branch)?;
        if self.repository.branch_exists(&branch)? {
            return Err(Error::BranchAlreadyExists(branch));
        }

        let base = match explicit_base {
            Some(base) => base.to_owned(),
            None => match &self.config.default_base {
                Some(base) => base.clone(),
                None => match self.repository.current_branch()? {
                    Some(branch) => branch,
                    None => self.repository.current_revision()?,
                },
            },
        };
        let configured_path =
            self.config
                .worktree_path(&self.repository, name.as_str(), &branch)?;
        if configured_path.exists() {
            return Err(Error::WorktreePathExists(configured_path));
        }
        let parent = configured_path.parent().ok_or_else(|| {
            Error::Configuration(format!(
                "worktree path '{}' has no parent",
                configured_path.display()
            ))
        })?;
        fs::create_dir_all(parent).map_err(|source| {
            Error::io(
                format!("could not create worktree parent '{}'", parent.display()),
                source,
            )
        })?;
        let parent = fs::canonicalize(parent).map_err(|source| {
            Error::io(
                format!("could not resolve worktree parent '{}'", parent.display()),
                source,
            )
        })?;
        let leaf = configured_path.file_name().ok_or_else(|| {
            Error::Configuration(format!(
                "worktree path '{}' has no final component",
                configured_path.display()
            ))
        })?;
        let path = parent.join(leaf);

        self.repository.add_worktree(&branch, &path, &base)?;
        let metadata = SessionMetadata::new(name.to_string(), branch.clone(), base.clone());
        if let Err(error) = self.metadata.write(&metadata) {
            let _ = self.repository.remove_worktree(&path, true);
            let _ = self.repository.delete_branch(&branch, true);
            return Err(error);
        }

        Ok(CreatedSession {
            name: name.to_string(),
            branch,
            path,
            base,
        })
    }

    pub fn list(&self) -> Result<Vec<ListedSession>> {
        let metadata = self.metadata_by_branch()?;
        self.repository
            .worktrees()?
            .into_iter()
            .map(|worktree| self.listed_session(worktree, &metadata))
            .collect()
    }

    pub fn path(&self, name: &str) -> Result<PathBuf> {
        let name = SessionName::parse(name)?;
        self.resolve(&name).map(|worktree| worktree.path)
    }

    pub fn root(&self) -> &Path {
        &self.repository.current_root
    }

    pub fn status(&self) -> Result<CurrentSession> {
        let worktree = self
            .repository
            .worktrees()?
            .into_iter()
            .find(|worktree| worktree.path == self.repository.current_root)
            .ok_or_else(|| {
                Error::SessionNotFound(self.repository.current_root.display().to_string())
            })?;
        let metadata = self.metadata_by_branch()?;
        let listed = self.listed_session(worktree.clone(), &metadata)?;
        let base = if worktree.path == self.repository.main_root {
            None
        } else {
            match &worktree.checkout {
                Head::Branch(branch) => metadata
                    .get(branch)
                    .map(|record| record.base.clone())
                    .or_else(|| self.config.default_base.clone())
                    .or(self.repository.repository_default_branch()?),
                Head::Detached => None,
            }
        };
        Ok(CurrentSession {
            repository: self.repository.name.clone(),
            name: listed.name,
            branch: listed.branch,
            base,
            path: listed.path,
            head: listed.head,
            dirty: listed.dirty,
            locked: listed.locked,
            status: listed.status,
        })
    }

    pub fn remove(&self, name: &str, options: RemoveOptions) -> Result<RemovedSession> {
        let name = SessionName::parse(name)?;
        let worktree = self.resolve(&name)?;
        if worktree.path == self.repository.main_root {
            return Err(Error::CannotRemoveMain);
        }
        if !worktree.path.exists() || worktree.prunable {
            return Err(Error::WorktreeMissing(name.to_string()));
        }
        if worktree.locked && !options.force_worktree {
            return Err(Error::WorktreeLocked(name.to_string()));
        }
        let dirty = self.repository.dirty(&worktree.path)?;
        if dirty && !options.force_worktree {
            return Err(Error::WorktreeDirty(name.to_string()));
        }

        let branch = match &worktree.checkout {
            Head::Branch(branch) => Some(branch.clone()),
            Head::Detached => None,
        };
        let delete_branch =
            branch.is_some() && self.config.delete_merged_branches && !options.keep_branch;
        if let Some(branch) = branch.as_deref().filter(|_| delete_branch) {
            if !options.force_branch {
                let base = self
                    .base_for(&name, branch)?
                    .ok_or_else(|| Error::BaseUnknown(name.to_string()))?;
                if !self.repository.is_ancestor(branch, &base)? {
                    return Err(Error::BranchNotMerged {
                        branch: branch.to_owned(),
                        base,
                    });
                }
            }
        }

        if worktree.locked {
            self.repository.unlock_worktree(&worktree.path)?;
        }
        self.repository
            .remove_worktree(&worktree.path, options.force_worktree)?;
        self.metadata.remove(name.as_str())?;

        if let Some(branch) = branch.as_deref().filter(|_| delete_branch) {
            self.repository
                .delete_branch(branch, options.force_branch)?;
        }

        Ok(RemovedSession {
            name: name.to_string(),
            branch,
            path: worktree.path,
            branch_deleted: delete_branch,
        })
    }

    pub fn prune(&self, dry_run: bool) -> Result<PruneResult> {
        Ok(PruneResult {
            dry_run,
            messages: self.repository.prune(dry_run)?,
        })
    }

    fn resolve(&self, name: &SessionName) -> Result<Worktree> {
        self.resolve_optional(name)?
            .ok_or_else(|| Error::SessionNotFound(name.to_string()))
    }

    fn resolve_optional(&self, name: &SessionName) -> Result<Option<Worktree>> {
        let worktrees = self.repository.worktrees()?;
        if name.as_str() == "main" {
            return Ok(worktrees
                .into_iter()
                .find(|worktree| worktree.path == self.repository.main_root));
        }

        if let Some(metadata) = self.metadata.read(name.as_str())?
            && let Some(worktree) = worktrees.iter().find(|worktree| {
                matches!(&worktree.checkout, Head::Branch(branch) if branch == &metadata.branch)
            })
        {
            return Ok(Some(worktree.clone()));
        }

        let expected_branch = self.config.branch_for(name.as_str());
        if let Some(worktree) = worktrees.iter().find(|worktree| {
            matches!(&worktree.checkout, Head::Branch(branch) if branch == &expected_branch)
        }) {
            return Ok(Some(worktree.clone()));
        }

        let expected_path =
            self.config
                .worktree_path(&self.repository, name.as_str(), &expected_branch)?;
        if let Some(worktree) = worktrees
            .iter()
            .find(|worktree| worktree.path == expected_path)
        {
            return Ok(Some(worktree.clone()));
        }

        Ok(worktrees.into_iter().find(|worktree| {
            worktree.path.file_name().and_then(|value| value.to_str()) == Some(name.as_str())
        }))
    }

    fn listed_session(
        &self,
        worktree: Worktree,
        metadata: &HashMap<String, SessionMetadata>,
    ) -> Result<ListedSession> {
        let dirty = if worktree.path.exists() {
            self.repository.dirty(&worktree.path)?
        } else {
            false
        };
        let status = if !worktree.path.exists() || worktree.prunable {
            WorktreeState::Missing
        } else if worktree.locked {
            WorktreeState::Locked
        } else if matches!(worktree.checkout, Head::Detached) {
            WorktreeState::Detached
        } else if dirty {
            WorktreeState::Dirty
        } else {
            WorktreeState::Clean
        };
        let branch = match &worktree.checkout {
            Head::Branch(branch) => Some(branch.clone()),
            Head::Detached => None,
        };
        let name = if worktree.path == self.repository.main_root {
            "main".to_owned()
        } else if let Some(record) = branch.as_ref().and_then(|branch| metadata.get(branch)) {
            record.name.clone()
        } else if let Some(branch) = &branch {
            self.config.session_from_branch(branch).to_owned()
        } else {
            worktree
                .path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("detached")
                .to_owned()
        };
        Ok(ListedSession {
            name,
            branch,
            path: worktree.path,
            head: short_head(&worktree.head),
            dirty,
            locked: worktree.locked,
            status,
        })
    }

    fn metadata_by_branch(&self) -> Result<HashMap<String, SessionMetadata>> {
        Ok(self
            .metadata
            .all()?
            .into_iter()
            .map(|metadata| (metadata.branch.clone(), metadata))
            .collect())
    }

    fn base_for(&self, name: &SessionName, branch: &str) -> Result<Option<String>> {
        if let Some(metadata) = self.metadata.read(name.as_str())?
            && metadata.branch == branch
        {
            return Ok(Some(metadata.base));
        }
        if let Some(metadata) = self
            .metadata
            .all()?
            .into_iter()
            .find(|metadata| metadata.branch == branch)
        {
            return Ok(Some(metadata.base));
        }
        if self.config.default_base.is_some() {
            return Ok(self.config.default_base.clone());
        }
        self.repository.repository_default_branch()
    }
}

fn short_head(head: &str) -> String {
    head.chars().take(7).collect()
}

#[cfg(test)]
mod tests {
    use super::SessionName;

    #[test]
    fn session_names_accept_safe_branch_and_path_characters() {
        for valid in ["fix-auth", "Agent_3", "release.1", "team/task"] {
            assert!(
                SessionName::parse(valid).is_ok(),
                "expected {valid:?} to be valid"
            );
        }
    }

    #[test]
    fn session_names_reject_traversal_and_ambiguous_paths() {
        for invalid in ["", ".", "..", "a/../b", "/absolute", "a//b", "a b", "a\\b"] {
            assert!(
                SessionName::parse(invalid).is_err(),
                "expected {invalid:?} to be invalid"
            );
        }
    }
}
