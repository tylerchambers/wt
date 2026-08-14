use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const METADATA_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub version: u8,
    pub name: String,
    pub branch: String,
    pub base: String,
}

impl SessionMetadata {
    pub fn new(name: String, branch: String, base: String) -> Self {
        Self {
            version: METADATA_VERSION,
            name,
            branch,
            base,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MetadataStore {
    directory: PathBuf,
}

impl MetadataStore {
    pub fn new(common_git_dir: &Path) -> Self {
        Self {
            directory: common_git_dir.join("wt/sessions"),
        }
    }

    pub fn read(&self, name: &str) -> Result<Option<SessionMetadata>> {
        let path = self.path(name);
        let contents = match fs::read(&path) {
            Ok(contents) => contents,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(Error::io(
                    format!("could not read session metadata '{}'", path.display()),
                    source,
                ));
            }
        };
        let metadata: SessionMetadata = serde_json::from_slice(&contents)?;
        validate_version(&metadata, &path)?;
        Ok(Some(metadata))
    }

    pub fn all(&self) -> Result<Vec<SessionMetadata>> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(Error::io(
                    format!(
                        "could not list session metadata '{}'",
                        self.directory.display()
                    ),
                    source,
                ));
            }
        };
        let mut metadata = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| {
                Error::io(
                    format!(
                        "could not read session metadata directory '{}'",
                        self.directory.display()
                    ),
                    source,
                )
            })?;
            if entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                != Some("json")
            {
                continue;
            }
            let contents = fs::read(entry.path()).map_err(|source| {
                Error::io(
                    format!(
                        "could not read session metadata '{}'",
                        entry.path().display()
                    ),
                    source,
                )
            })?;
            let record: SessionMetadata = serde_json::from_slice(&contents)?;
            validate_version(&record, &entry.path())?;
            metadata.push(record);
        }
        Ok(metadata)
    }

    pub fn write(&self, metadata: &SessionMetadata) -> Result<()> {
        fs::create_dir_all(&self.directory).map_err(|source| {
            Error::io(
                format!(
                    "could not create session metadata directory '{}'",
                    self.directory.display()
                ),
                source,
            )
        })?;
        let destination = self.path(&metadata.name);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = self
            .directory
            .join(format!(".{}.{}.tmp", std::process::id(), nonce));
        let bytes = serde_json::to_vec_pretty(metadata)?;
        let write_result = (|| -> Result<()> {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)
                .map_err(|source| {
                    Error::io(
                        format!(
                            "could not create temporary session metadata '{}'",
                            temporary.display()
                        ),
                        source,
                    )
                })?;
            file.write_all(&bytes).map_err(|source| {
                Error::io(
                    format!("could not write session metadata '{}'", temporary.display()),
                    source,
                )
            })?;
            file.write_all(b"\n").map_err(|source| {
                Error::io(
                    format!("could not write session metadata '{}'", temporary.display()),
                    source,
                )
            })?;
            file.sync_all().map_err(|source| {
                Error::io(
                    format!("could not sync session metadata '{}'", temporary.display()),
                    source,
                )
            })?;
            fs::rename(&temporary, &destination).map_err(|source| {
                Error::io(
                    format!(
                        "could not install session metadata '{}'",
                        destination.display()
                    ),
                    source,
                )
            })?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result
    }

    pub fn remove(&self, name: &str) -> Result<()> {
        let path = self.path(name);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(Error::io(
                format!("could not remove session metadata '{}'", path.display()),
                source,
            )),
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.directory.join(format!("{}.json", encode_name(name)))
    }
}

fn encode_name(name: &str) -> String {
    let mut encoded = String::with_capacity(name.len() * 2);
    for byte in name.as_bytes() {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn validate_version(metadata: &SessionMetadata, path: &Path) -> Result<()> {
    if metadata.version == METADATA_VERSION {
        Ok(())
    } else {
        Err(Error::Configuration(format!(
            "unsupported session metadata version {} in '{}'",
            metadata.version,
            path.display()
        )))
    }
}
