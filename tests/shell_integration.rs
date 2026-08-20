mod common;

use std::env;
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Output};

use common::{TestRepo, assert_failure, assert_success, path_str, stderr, stdout};

fn wt(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wt"))
        .args(args)
        .output()
        .expect("run wt")
}

fn available_shells() -> Vec<&'static str> {
    ["bash", "zsh"]
        .into_iter()
        .filter(|shell| {
            Command::new(shell)
                .arg("--version")
                .output()
                .is_ok_and(|output| output.status.success())
        })
        .collect()
}

fn run_shell(repo: &TestRepo, shell: &str, script: &str) -> Output {
    let binary = Path::new(env!("CARGO_BIN_EXE_wt"));
    let binary_dir = binary.parent().expect("wt binary directory");
    let mut paths = vec![binary_dir.to_path_buf()];
    paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    let path = env::join_paths(paths).expect("join PATH");

    let mut command = Command::new(shell);
    if shell == "bash" {
        command.args(["--noprofile", "--norc"]);
    } else {
        command.arg("-f");
    }
    command
        .args(["-c", script])
        .current_dir(&repo.root)
        .env("WT_BIN", binary)
        .env("WT_SHELL", shell)
        .env("PATH", path)
        .env("HOME", &repo.home)
        .env("XDG_CONFIG_HOME", repo.home.join(".config"))
        .env("LC_ALL", "C")
        .output()
        .expect("run shell")
}

fn initialized_hook() -> &'static str {
    r#"eval "$("$WT_BIN" shell-init "$WT_SHELL")"
set -u
"#
}

fn expected_session_path(repo: &TestRepo, name: &str) -> OsString {
    repo.root
        .parent()
        .expect("repository parent")
        .join(".worktrees")
        .join(repo.root.file_name().expect("repository directory name"))
        .join(name)
        .into_os_string()
}

#[test]
fn shell_init_is_discoverable_and_prints_static_bash_and_zsh_hooks() {
    let help = wt(&["--help"]);
    assert_success(&help);
    assert!(stdout(&help).contains("shell-init"));

    let bash = wt(&["shell-init", "bash"]);
    assert_success(&bash);
    let zsh = wt(&["shell-init", "zsh"]);
    assert_success(&zsh);

    assert_eq!(stdout(&bash), stdout(&zsh));
    assert!(stdout(&bash).contains("wt()"));
    assert!(stdout(&bash).contains("command wt"));
    assert!(stderr(&bash).is_empty());
    assert!(stderr(&zsh).is_empty());

    let unsupported = wt(&["shell-init", "fish"]);
    assert!(!unsupported.status.success());
    assert!(stderr(&unsupported).contains("invalid value 'fish'"));
}

#[test]
fn shell_init_replaces_preexisting_wt_alias_and_enters_worktree() {
    for shell in available_shells() {
        let repo = TestRepo::new();
        let script = r#"
set -e
if [ "$WT_SHELL" = "bash" ]; then
    shopt -s expand_aliases
fi
alias wt='printf alias-wt'
eval "$("$WT_BIN" shell-init "$WT_SHELL")"
set -u
wt new replaced-alias --base main --cd
printf 'PWD:%s\n' "$PWD"
"#;
        let output = run_shell(&repo, shell, script);
        assert_success(&output);

        let expected = expected_session_path(&repo, "replaced-alias");
        let expected = path_str(Path::new(&expected));
        assert_eq!(
            stdout(&output),
            format!("PWD:{expected}\n"),
            "unexpected {shell} output"
        );
        assert!(Path::new(expected).is_dir());
        assert!(stderr(&output).is_empty(), "unexpected {shell} stderr");
    }
}

#[test]
fn initialized_shell_creates_and_enters_worktree_with_spaces_in_repository_path() {
    for shell in available_shells() {
        let repo = TestRepo::with_name("project with spaces");
        let script = format!(
            r#"{}
direct="$(command wt root)"
delegated="$(wt root)"
[ "$direct" = "$delegated" ] || exit 70
wt new fix-auth --base main --cd --
printf 'ROOT:%s\nPWD:%s\n' "$delegated" "$PWD"
"#,
            initialized_hook()
        );
        let output = run_shell(&repo, shell, &script);
        assert_success(&output);

        let expected = expected_session_path(&repo, "fix-auth");
        let expected = path_str(Path::new(&expected));
        assert_eq!(
            stdout(&output),
            format!("ROOT:{}\nPWD:{expected}\n", repo.root.display()),
            "unexpected {shell} output"
        );
        assert!(Path::new(expected).is_dir());
        assert!(stderr(&output).is_empty(), "unexpected {shell} stderr");
    }
}

#[test]
fn initialized_shell_uses_builtin_cd_when_cd_function_is_shadowed() {
    for shell in available_shells() {
        let repo = TestRepo::new();
        let script = format!(
            r#"{}
cd() {{ return 0; }}
wt new shadowed-cd --base main --cd
printf 'PWD:%s\n' "$PWD"
"#,
            initialized_hook()
        );
        let output = run_shell(&repo, shell, &script);
        assert_success(&output);

        let expected = expected_session_path(&repo, "shadowed-cd");
        let expected = path_str(Path::new(&expected));
        assert_eq!(
            stdout(&output),
            format!("PWD:{expected}\n"),
            "unexpected {shell} output"
        );
        assert!(Path::new(expected).is_dir());
        assert!(stderr(&output).is_empty(), "unexpected {shell} stderr");
    }
}

#[test]
fn initialized_shell_preserves_failure_status_and_current_directory() {
    for shell in available_shells() {
        let repo = TestRepo::with_name("failed project with spaces");
        let script = format!(
            r#"{}
before="$PWD"
wt new duplicate-main --branch main --cd
command_status=$?
printf 'STATUS:%s\nBEFORE:%s\nPWD:%s\n' "$command_status" "$before" "$PWD"
"#,
            initialized_hook()
        );
        let output = run_shell(&repo, shell, &script);
        assert_success(&output);
        assert!(stderr(&output).contains("branch 'main' already exists"));
        assert!(stdout(&output).contains("STATUS:1\n"));
        assert!(stdout(&output).contains(&format!("BEFORE:{}\n", repo.root.display())));
        assert!(stdout(&output).contains(&format!("PWD:{}\n", repo.root.display())));
        assert!(!Path::new(&expected_session_path(&repo, "duplicate-main")).exists());
    }
}

#[test]
fn initialized_shell_rejects_output_conflicts_before_creation() {
    for shell in available_shells() {
        let repo = TestRepo::new();
        let script = format!(
            r#"{}
wt new conflict-json --cd --json
json_status=$?
wt new conflict-path --print-path --cd
path_status=$?
printf 'JSON:%s\nPATH:%s\nPWD:%s\n' "$json_status" "$path_status" "$PWD"
"#,
            initialized_hook()
        );
        let output = run_shell(&repo, shell, &script);
        assert_success(&output);
        assert_eq!(
            stdout(&output),
            format!("JSON:2\nPATH:2\nPWD:{}\n", repo.root.display())
        );
        assert!(stderr(&output).contains("--cd cannot be used with --json or --print-path"));
        assert!(!Path::new(&expected_session_path(&repo, "conflict-json")).exists());
        assert!(!Path::new(&expected_session_path(&repo, "conflict-path")).exists());
    }
}

#[test]
fn direct_binary_cd_refuses_with_setup_guidance_before_creation() {
    let repo = TestRepo::new();
    let help = repo.wt(&repo.root, ["new", "--help"]);
    assert_success(&help);
    assert!(stdout(&help).contains("--cd"));

    let output = repo.wt(&repo.root, ["new", "direct", "--cd"]);
    assert_failure(&output, "parent-shell directory changes require");
    assert!(stderr(&output).contains("eval \"$(wt shell-init <shell>)\""));
    assert!(!Path::new(&expected_session_path(&repo, "direct")).exists());
}

#[test]
fn direct_binary_cd_conflicts_do_not_create_worktrees() {
    let repo = TestRepo::new();
    for (name, conflict) in [("direct-json", "--json"), ("direct-path", "--print-path")] {
        let output = repo.wt(&repo.root, ["new", name, "--cd", conflict]);
        assert!(!output.status.success());
        assert!(!Path::new(&expected_session_path(&repo, name)).exists());
    }
}
