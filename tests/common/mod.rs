#![allow(dead_code)]

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

pub struct TestRepo {
    temp: TempDir,
    pub root: PathBuf,
    pub home: PathBuf,
}

impl TestRepo {
    pub fn new() -> Self {
        Self::with_name("project")
    }

    pub fn with_name(name: &str) -> Self {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let root = temp.path().join(name);
        let home = temp.path().join("home");
        fs::create_dir_all(&home).expect("create test home");
        let home = fs::canonicalize(home).expect("canonicalize test home");

        git_at(temp.path(), ["init", "-b", "main", path_str(&root)]);
        git_at(&root, ["config", "user.name", "WT Test"]);
        git_at(&root, ["config", "user.email", "wt@example.com"]);
        fs::write(root.join("README.txt"), "initial\n").expect("write initial file");
        git_at(&root, ["add", "README.txt"]);
        git_at(&root, ["commit", "-m", "initial"]);

        let root = fs::canonicalize(root).expect("canonicalize repository root");
        Self { temp, root, home }
    }

    pub fn wt<I, S>(&self, cwd: &Path, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Command::new(env!("CARGO_BIN_EXE_wt"))
            .args(args)
            .current_dir(cwd)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("LC_ALL", "C")
            .output()
            .expect("run wt")
    }

    pub fn git<I, S>(&self, cwd: &Path, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("HOME", &self.home)
            .env("LC_ALL", "C")
            .output()
            .expect("run git")
    }

    pub fn write_config(&self, config: &str) {
        let path = self.home.join(".config/wt/config.toml");
        fs::create_dir_all(path.parent().expect("config parent")).expect("create config dir");
        fs::write(path, config).expect("write config");
    }

    pub fn temp_path(&self, name: &str) -> PathBuf {
        fs::canonicalize(self.temp.path())
            .expect("canonicalize temporary root")
            .join(name)
    }
}

pub fn git_at<I, S>(cwd: &Path, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("LC_ALL", "C")
        .output()
        .expect("run git");
    assert_success(&output);
    output
}

pub fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        stdout(output),
        stderr(output)
    );
}

pub fn assert_failure(output: &Output, message: &str) {
    assert!(!output.status.success(), "command unexpectedly succeeded");
    assert!(
        stderr(output).contains(message),
        "stderr did not contain {message:?}:\n{}",
        stderr(output)
    );
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

pub fn path_str(path: &Path) -> &str {
    path.to_str().expect("test path is UTF-8")
}
