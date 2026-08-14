use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::git::Repository;

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub worktree_dir: Option<String>,
    pub branch_prefix: String,
    pub default_base: Option<String>,
    pub terminal: Terminal,
    pub delete_merged_branches: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            worktree_dir: None,
            branch_prefix: String::new(),
            default_base: None,
            terminal: Terminal::None,
            delete_merged_branches: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Terminal {
    #[default]
    None,
    Tmux,
    Cmux,
}

impl Config {
    pub fn load() -> Result<Self> {
        let Some(path) = global_config_path() else {
            return Ok(Self::default());
        };
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => {
                return Err(Error::io(
                    format!("could not read configuration '{}'", path.display()),
                    source,
                ));
            }
        };
        toml::from_str(&contents).map_err(|error| {
            Error::Configuration(format!("could not parse '{}': {error}", path.display()))
        })
    }

    pub fn branch_for(&self, session: &str) -> String {
        format!("{}{session}", self.branch_prefix)
    }

    pub fn session_from_branch<'a>(&self, branch: &'a str) -> &'a str {
        if self.branch_prefix.is_empty() {
            branch
        } else {
            branch.strip_prefix(&self.branch_prefix).unwrap_or(branch)
        }
    }

    pub fn worktree_path(
        &self,
        repository: &Repository,
        session: &str,
        branch: &str,
    ) -> Result<PathBuf> {
        let Some(template) = &self.worktree_dir else {
            let parent = repository.main_root.parent().ok_or_else(|| {
                Error::Configuration(format!(
                    "repository root '{}' has no parent directory",
                    repository.main_root.display()
                ))
            })?;
            return Ok(parent
                .join(".worktrees")
                .join(&repository.name)
                .join(session));
        };

        validate_template(template)?;
        let expanded = expand_home(template)?
            .replace("{repo}", &repository.name)
            .replace("{session}", session)
            .replace("{branch}", branch);
        let path = PathBuf::from(expanded);
        if !path.is_absolute() {
            return Err(Error::Configuration(
                "worktree_dir must expand to an absolute path".to_owned(),
            ));
        }
        Ok(path)
    }
}

fn global_config_path() -> Option<PathBuf> {
    if let Some(directory) = env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(directory).join("wt/config.toml"));
    }
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/wt/config.toml"))
}

fn validate_template(template: &str) -> Result<()> {
    let remainder = template
        .replace("{repo}", "")
        .replace("{session}", "")
        .replace("{branch}", "");
    if remainder.contains('{') || remainder.contains('}') {
        return Err(Error::Configuration(format!(
            "worktree_dir contains an unknown placeholder: {template}"
        )));
    }
    if !template.contains("{session}") && !template.contains("{branch}") {
        return Err(Error::Configuration(
            "worktree_dir must contain {session} or {branch}".to_owned(),
        ));
    }
    Ok(())
}

fn expand_home(template: &str) -> Result<String> {
    if template == "~" {
        return home_as_string();
    }
    if let Some(remainder) = template.strip_prefix("~/") {
        let home = home_as_string()?;
        return Ok(Path::new(&home)
            .join(remainder)
            .to_string_lossy()
            .into_owned());
    }
    Ok(template.to_owned())
}

fn home_as_string() -> Result<String> {
    env::var_os("HOME")
        .map(|home| home.to_string_lossy().into_owned())
        .ok_or_else(|| Error::Configuration("HOME is not set for '~' expansion".to_owned()))
}
