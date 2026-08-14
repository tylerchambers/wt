mod cli;
mod config;
mod error;
mod git;
mod metadata;
mod session;

use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use cli::{Cli, Command};
use error::Result;
use serde::Serialize;
use session::{RemoveOptions, SessionManager};

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let cwd =
        env::current_dir().map_err(|source| error::Error::io("could not read cwd", source))?;
    let manager = SessionManager::discover(&cwd)?;
    match cli.command {
        Command::New(args) => {
            let created =
                manager.new_session(&args.name, args.branch.as_deref(), args.base.as_deref())?;
            if args.print_path {
                println!("{}", created.path.display());
            } else if args.json {
                print_json(&created)?;
            } else {
                println!("Created {}", created.name);
                println!("{}", created.path.display());
            }
        }
        Command::Ls(args) => {
            let sessions = manager.list()?;
            if args.json {
                print_json(&sessions)?;
            } else {
                print_sessions(&sessions);
            }
        }
        Command::Path(args) | Command::Cd(args) => {
            println!("{}", manager.path(&args.name)?.display());
        }
        Command::Status(args) => {
            let status = manager.status()?;
            if args.json {
                print_json(&status)?;
            } else {
                println!("Repository:  {}", status.repository);
                println!("Session:     {}", status.name);
                println!(
                    "Branch:      {}",
                    status.branch.as_deref().unwrap_or("detached")
                );
                println!("Base:        {}", status.base.as_deref().unwrap_or("-"));
                println!("Path:        {}", status.path.display());
                println!("HEAD:        {}", status.head);
                println!("Dirty:       {}", if status.dirty { "yes" } else { "no" });
            }
        }
        Command::Root => println!("{}", manager.root().display()),
        Command::Rm(args) => {
            let force_worktree = args.force || args.force_worktree;
            let force_branch = args.force || args.force_branch;
            let removed = manager.remove(
                &args.name,
                RemoveOptions {
                    keep_branch: args.keep_branch,
                    force_worktree,
                    force_branch,
                },
            )?;
            if args.json {
                print_json(&removed)?;
            } else {
                println!("Removed {}", removed.name);
            }
        }
        Command::Prune(args) => {
            let result = manager.prune(args.dry_run)?;
            if args.json {
                print_json(&result)?;
            } else if result.messages.is_empty() {
                println!("Nothing to prune");
            } else {
                for message in result.messages {
                    println!("{message}");
                }
            }
        }
    }
    Ok(())
}

fn print_json(value: &impl Serialize) -> Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer_pretty(&mut output, value)?;
    output
        .write_all(b"\n")
        .map_err(|source| error::Error::io("could not write stdout", source))?;
    Ok(())
}

fn print_sessions(sessions: &[session::ListedSession]) {
    let home = env::var_os("HOME").map(PathBuf::from);
    let rows = sessions
        .iter()
        .map(|session| {
            (
                session.name.as_str(),
                session.branch.as_deref().unwrap_or("(detached)"),
                session.status.to_string(),
                display_path(&session.path, home.as_deref()),
            )
        })
        .collect::<Vec<_>>();
    let session_width = rows
        .iter()
        .map(|row| row.0.len())
        .chain(["SESSION".len()])
        .max()
        .unwrap_or(7);
    let branch_width = rows
        .iter()
        .map(|row| row.1.len())
        .chain(["BRANCH".len()])
        .max()
        .unwrap_or(6);
    let status_width = rows
        .iter()
        .map(|row| row.2.len())
        .chain(["STATUS".len()])
        .max()
        .unwrap_or(6);
    println!(
        "{:<session_width$}  {:<branch_width$}  {:<status_width$}  PATH",
        "SESSION", "BRANCH", "STATUS"
    );
    for (name, branch, status, path) in rows {
        println!(
            "{name:<session_width$}  {branch:<branch_width$}  {status:<status_width$}  {path}"
        );
    }
}

fn display_path(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home
        && let Ok(relative) = path.strip_prefix(home)
    {
        return Path::new("~").join(relative).display().to_string();
    }
    path.display().to_string()
}
