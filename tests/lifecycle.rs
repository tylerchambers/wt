mod common;

use std::fs;
use std::process::{Command, Stdio};

use common::{TestRepo, assert_failure, assert_success, path_str, stderr, stdout};
use serde_json::Value;

#[test]
fn creation_resolution_status_and_removal_form_a_complete_scriptable_workflow() {
    let repo = TestRepo::new();
    let nested = repo.root.join("src/nested");
    fs::create_dir_all(&nested).expect("create nested directory");

    let created = repo.wt(
        &nested,
        ["new", "experiment", "--base", "main", "--print-path"],
    );
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    assert_eq!(stdout(&created), format!("{path}\n"));
    assert!(stderr(&created).is_empty(), "unexpected diagnostics");
    assert!(std::path::Path::new(&path).is_absolute());
    assert!(std::path::Path::new(&path).is_dir());

    let branch = repo.git(&repo.root, ["branch", "--show-current"]);
    assert_success(&branch);
    assert_eq!(stdout(&branch), "main\n");
    let session_branch = repo.git(std::path::Path::new(&path), ["branch", "--show-current"]);
    assert_success(&session_branch);
    assert_eq!(stdout(&session_branch), "experiment\n");

    let resolved = repo.wt(&repo.root, ["path", "experiment"]);
    assert_success(&resolved);
    assert_eq!(stdout(&resolved), format!("{path}\n"));

    let cd = repo.wt(&repo.root, ["cd", "experiment"]);
    assert_success(&cd);
    assert_eq!(stdout(&cd), format!("{path}\n"));

    let session_nested = std::path::Path::new(&path).join("one/two");
    fs::create_dir_all(&session_nested).expect("create worktree nested directory");
    let root = repo.wt(&session_nested, ["root"]);
    assert_success(&root);
    assert_eq!(stdout(&root), format!("{path}\n"));

    let status = repo.wt(&session_nested, ["status", "--json"]);
    assert_success(&status);
    let status: Value = serde_json::from_slice(&status.stdout).expect("valid status JSON");
    assert_eq!(status["repository"], "project");
    assert_eq!(status["name"], "experiment");
    assert_eq!(status["branch"], "experiment");
    assert_eq!(status["base"], "main");
    assert_eq!(status["path"], path);
    assert_eq!(status["dirty"], false);

    let listed = repo.wt(&repo.root, ["ls", "--json"]);
    assert_success(&listed);
    let listed: Value = serde_json::from_slice(&listed.stdout).expect("valid list JSON");
    let sessions = listed.as_array().expect("list array");
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0]["name"], "main");
    assert_eq!(sessions[1]["name"], "experiment");
    assert_eq!(sessions[1]["path"], path);
    assert_eq!(sessions[1]["dirty"], false);
    assert_eq!(sessions[1]["locked"], false);

    let removed = repo.wt(&repo.root, ["rm", "experiment"]);
    assert_success(&removed);
    assert!(!std::path::Path::new(&path).exists());
    let branch = repo.git(
        &repo.root,
        ["show-ref", "--verify", "refs/heads/experiment"],
    );
    assert!(
        !branch.status.success(),
        "merged session branch was retained"
    );
}

#[test]
fn configuration_controls_storage_branch_prefix_and_default_base() {
    let repo = TestRepo::new();
    repo.write_config(
        r#"
worktree_dir = "~/sessions/{repo}/{session}"
branch_prefix = "work/"
default_base = "main"
delete_merged_branches = true
terminal = "none"
"#,
    );

    let created = repo.wt(&repo.root, ["new", "configured", "--json"]);
    assert_success(&created);
    let created: Value = serde_json::from_slice(&created.stdout).expect("valid creation JSON");
    let expected_path = repo.home.join("sessions/project/configured");
    assert_eq!(created["name"], "configured");
    assert_eq!(created["branch"], "work/configured");
    assert_eq!(created["path"], path_str(&expected_path));
    assert_eq!(created["base"], "main");

    let branch = repo.git(&expected_path, ["branch", "--show-current"]);
    assert_success(&branch);
    assert_eq!(stdout(&branch), "work/configured\n");

    let custom = repo.wt(
        &repo.root,
        [
            "new",
            "custom-name",
            "--branch",
            "tyler/custom-branch",
            "--base",
            "main",
            "--json",
        ],
    );
    assert_success(&custom);
    let custom: Value = serde_json::from_slice(&custom.stdout).expect("valid creation JSON");
    assert_eq!(custom["name"], "custom-name");
    assert_eq!(custom["branch"], "tyler/custom-branch");

    let resolved = repo.wt(&repo.root, ["path", "custom-name"]);
    assert_success(&resolved);
    assert_eq!(stdout(&resolved).trim(), custom["path"].as_str().unwrap());
}

#[test]
fn creation_rejects_invalid_names_duplicate_sessions_and_duplicate_branches() {
    let repo = TestRepo::new();
    for invalid in [
        "",
        "../escape",
        "/absolute",
        "two words",
        "a//b",
        ".",
        "a\\b",
    ] {
        let output = repo.wt(&repo.root, ["new", invalid]);
        assert_failure(&output, "invalid session name");
    }

    let first = repo.wt(&repo.root, ["new", "first", "--print-path"]);
    assert_success(&first);
    let duplicate = repo.wt(&repo.root, ["new", "first"]);
    assert_failure(&duplicate, "session 'first' already exists");

    let duplicate_branch = repo.wt(
        &repo.root,
        ["new", "second", "--branch", "first", "--base", "main"],
    );
    assert_failure(&duplicate_branch, "branch 'first' already exists");

    let first_path = stdout(&first).trim().to_owned();
    let nested = std::path::Path::new(&first_path).join("nested");
    fs::create_dir_all(&nested).expect("nested linked worktree path");
    let from_linked = repo.wt(&nested, ["new", "from-linked", "--print-path"]);
    assert_success(&from_linked);
    assert!(std::path::Path::new(stdout(&from_linked).trim()).is_dir());
}

#[test]
fn removal_refuses_dirty_work_and_requires_a_separate_explicit_force() {
    let repo = TestRepo::new();
    let created = repo.wt(&repo.root, ["new", "dirty", "--print-path"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    fs::write(
        std::path::Path::new(&path).join("dirty.txt"),
        "not committed\n",
    )
    .expect("write dirty file");

    let refused = repo.wt(&repo.root, ["rm", "dirty"]);
    assert_failure(&refused, "worktree 'dirty' has uncommitted changes");
    assert!(std::path::Path::new(&path).exists());

    let branch_only = repo.wt(&repo.root, ["rm", "dirty", "--force-branch"]);
    assert_failure(&branch_only, "worktree 'dirty' has uncommitted changes");

    let removed = repo.wt(&repo.root, ["rm", "dirty", "--force-worktree"]);
    assert_success(&removed);
    assert!(!std::path::Path::new(&path).exists());
}

#[test]
fn dry_run_previews_clean_merged_removal_without_mutation() {
    let repo = TestRepo::new();
    let created = repo.wt(&repo.root, ["new", "preview", "--print-path"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    let common_dir = repo.git(
        &repo.root,
        ["rev-parse", "--path-format=absolute", "--git-common-dir"],
    );
    assert_success(&common_dir);
    let metadata =
        std::path::Path::new(stdout(&common_dir).trim()).join("wt/sessions/70726576696577.json");
    let metadata_before = fs::read(&metadata).expect("read preview metadata");

    let preview = repo.wt(&repo.root, ["rm", "preview", "--dry-run"]);

    assert_success(&preview);
    assert_eq!(
        stdout(&preview),
        format!(
            "Would remove preview\nPath: {path}\nBranch: preview (would delete)\nWorktree force: not authorized (required: not required)\nBranch merge: merged\nBranch force: not authorized (required: not required)\nNothing changed (dry run)\n"
        )
    );
    assert!(std::path::Path::new(&path).is_dir());
    let branch = repo.git(&repo.root, ["show-ref", "--verify", "refs/heads/preview"]);
    assert_success(&branch);
    assert_eq!(
        fs::read(&metadata).expect("reread preview metadata"),
        metadata_before
    );
    let listed = repo.wt(&repo.root, ["path", "preview"]);
    assert_success(&listed);
    assert_eq!(stdout(&listed), format!("{path}\n"));
}

#[test]
fn dry_run_keep_branch_previews_retention_without_mutation() {
    let repo = TestRepo::new();
    let created = repo.wt(&repo.root, ["new", "kept-preview", "--print-path"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();

    let preview = repo.wt(
        &repo.root,
        ["rm", "kept-preview", "--dry-run", "--keep-branch"],
    );

    assert_success(&preview);
    assert!(stdout(&preview).contains("Branch: kept-preview (would retain because --keep-branch)"));
    assert!(stdout(&preview).contains("Branch merge: not applicable"));
    assert!(stdout(&preview).contains("Nothing changed (dry run)"));
    assert!(std::path::Path::new(&path).is_dir());
    let branch = repo.git(
        &repo.root,
        ["show-ref", "--verify", "refs/heads/kept-preview"],
    );
    assert_success(&branch);
    let resolved = repo.wt(&repo.root, ["path", "kept-preview"]);
    assert_success(&resolved);
    assert_eq!(stdout(&resolved), format!("{path}\n"));
}

#[test]
fn dry_run_dirty_refusal_requires_worktree_force_without_mutation() {
    let repo = TestRepo::new();
    let created = repo.wt(&repo.root, ["new", "dirty-preview", "--print-path"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    fs::write(
        std::path::Path::new(&path).join("dirty.txt"),
        "not committed\n",
    )
    .expect("write dirty preview file");

    let refused = repo.wt(
        &repo.root,
        ["rm", "dirty-preview", "--dry-run", "--force-branch"],
    );

    assert_failure(
        &refused,
        "worktree 'dirty-preview' has uncommitted changes; use --force-worktree or --force",
    );
    assert!(std::path::Path::new(&path).join("dirty.txt").is_file());
    let branch = repo.git(
        &repo.root,
        ["show-ref", "--verify", "refs/heads/dirty-preview"],
    );
    assert_success(&branch);
    let resolved = repo.wt(&repo.root, ["path", "dirty-preview"]);
    assert_success(&resolved);
    assert_eq!(stdout(&resolved), format!("{path}\n"));
}

#[test]
fn dry_run_unmerged_refusal_requires_branch_force_without_mutation() {
    let repo = TestRepo::new();
    let created = repo.wt(&repo.root, ["new", "unmerged-preview", "--print-path"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    fs::write(
        std::path::Path::new(&path).join("feature.txt"),
        "committed only on feature\n",
    )
    .expect("write feature file");
    assert_success(&repo.git(std::path::Path::new(&path), ["add", "feature.txt"]));
    assert_success(&repo.git(
        std::path::Path::new(&path),
        ["commit", "-m", "unmerged preview"],
    ));

    let refused = repo.wt(
        &repo.root,
        ["rm", "unmerged-preview", "--dry-run", "--force-worktree"],
    );

    assert_failure(
        &refused,
        "branch 'unmerged-preview' contains commits not merged into 'main'; use --force-branch or --force",
    );
    assert!(std::path::Path::new(&path).join("feature.txt").is_file());
    let branch = repo.git(
        &repo.root,
        ["show-ref", "--verify", "refs/heads/unmerged-preview"],
    );
    assert_success(&branch);
    let resolved = repo.wt(&repo.root, ["path", "unmerged-preview"]);
    assert_success(&resolved);
    assert_eq!(stdout(&resolved), format!("{path}\n"));
}

#[test]
fn dry_run_force_reports_destructive_authorization_without_mutation() {
    let repo = TestRepo::new();
    let created = repo.wt(&repo.root, ["new", "forced-preview", "--print-path"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    fs::write(
        std::path::Path::new(&path).join("feature.txt"),
        "unmerged commit\n",
    )
    .expect("write feature file");
    assert_success(&repo.git(std::path::Path::new(&path), ["add", "feature.txt"]));
    assert_success(&repo.git(
        std::path::Path::new(&path),
        ["commit", "-m", "unmerged forced preview"],
    ));
    fs::write(
        std::path::Path::new(&path).join("dirty.txt"),
        "uncommitted work\n",
    )
    .expect("write dirty file");

    let preview = repo.wt(&repo.root, ["rm", "forced-preview", "--dry-run", "--force"]);

    assert_success(&preview);
    let output = stdout(&preview);
    assert!(output.contains("Worktree force: authorized (required: dirty)"));
    assert!(output.contains("Branch merge: unmerged"));
    assert!(output.contains("Branch force: authorized (required: unmerged)"));
    assert!(output.contains("Nothing changed (dry run)"));
    assert!(std::path::Path::new(&path).join("dirty.txt").is_file());
    let branch = repo.git(
        &repo.root,
        ["show-ref", "--verify", "refs/heads/forced-preview"],
    );
    assert_success(&branch);
    let resolved = repo.wt(&repo.root, ["path", "forced-preview"]);
    assert_success(&resolved);
    assert_eq!(stdout(&resolved), format!("{path}\n"));
}

#[test]
fn dry_run_json_reports_a_stable_inspectable_plan_without_mutation() {
    let repo = TestRepo::new();
    let created = repo.wt(&repo.root, ["new", "json-preview", "--print-path"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();

    let preview = repo.wt(&repo.root, ["rm", "json-preview", "--dry-run", "--json"]);

    assert_success(&preview);
    let preview: Value =
        serde_json::from_slice(&preview.stdout).expect("valid removal preview JSON");
    assert_eq!(preview["dry_run"], true);
    assert_eq!(preview["name"], "json-preview");
    assert_eq!(preview["path"], path);
    assert_eq!(preview["branch"], "json-preview");
    assert_eq!(preview["base"], "main");
    assert_eq!(preview["branch_deleted"], true);
    assert_eq!(preview["branch_retained"], false);
    assert_eq!(preview["force_worktree_authorized"], false);
    assert_eq!(preview["force_worktree_required"], false);
    assert_eq!(preview["force_branch_authorized"], false);
    assert_eq!(preview["force_branch_required"], false);
    assert_eq!(preview["branch_merge_status"], "merged");
    assert_eq!(preview["worktree_dirty"], false);
    assert_eq!(preview["worktree_locked"], false);
    assert!(std::path::Path::new(&path).is_dir());
    let branch = repo.git(
        &repo.root,
        ["show-ref", "--verify", "refs/heads/json-preview"],
    );
    assert_success(&branch);
    let resolved = repo.wt(&repo.root, ["path", "json-preview"]);
    assert_success(&resolved);
    assert_eq!(stdout(&resolved), format!("{path}\n"));
}

#[test]
fn force_branch_bypasses_a_deleted_base_for_preview_and_actual_removal() {
    let repo = TestRepo::new();
    assert_success(&repo.git(&repo.root, ["branch", "deleted-base"]));
    let created = repo.wt(
        &repo.root,
        [
            "new",
            "stale-base",
            "--base",
            "deleted-base",
            "--print-path",
        ],
    );
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    assert_success(&repo.git(&repo.root, ["branch", "-D", "deleted-base"]));

    let preview = repo.wt(
        &repo.root,
        ["rm", "stale-base", "--dry-run", "--force-branch"],
    );
    assert_success(&preview);
    assert!(stdout(&preview).contains("Nothing changed (dry run)"));
    assert!(std::path::Path::new(&path).is_dir());

    let removed = repo.wt(&repo.root, ["rm", "stale-base", "--force-branch"]);
    assert_success(&removed);
    assert!(!std::path::Path::new(&path).exists());
    let branch = repo.git(
        &repo.root,
        ["show-ref", "--verify", "refs/heads/stale-base"],
    );
    assert!(!branch.status.success(), "forced branch was retained");
}

#[test]
fn force_branch_bypasses_base_discovery_errors_for_preview_and_actual_removal() {
    let repo = TestRepo::new();
    let created = repo.wt(&repo.root, ["new", "preview", "--print-path"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    let common_dir = repo.git(
        &repo.root,
        ["rev-parse", "--path-format=absolute", "--git-common-dir"],
    );
    assert_success(&common_dir);
    let metadata_dir = std::path::Path::new(stdout(&common_dir).trim()).join("wt/sessions");
    fs::remove_file(metadata_dir.join("70726576696577.json")).expect("remove target metadata");
    fs::write(metadata_dir.join("unrelated.json"), b"not valid JSON\n")
        .expect("write malformed unrelated metadata");

    let refused = repo.wt(&repo.root, ["rm", "preview", "--dry-run"]);
    assert_failure(&refused, "expected ident");
    assert_eq!(
        stderr(&refused),
        "error: could not encode JSON output: expected ident at line 1 column 2\n"
    );
    assert!(std::path::Path::new(&path).is_dir());

    let preview = repo.wt(
        &repo.root,
        ["rm", "preview", "--dry-run", "--force-branch", "--json"],
    );
    assert_success(&preview);
    let preview: Value =
        serde_json::from_slice(&preview.stdout).expect("valid removal preview JSON");
    assert_eq!(preview["base"], Value::Null);
    assert_eq!(preview["branch_merge_status"], "unknown");
    assert_eq!(preview["force_branch_authorized"], true);
    assert_eq!(preview["force_branch_required"], true);
    assert!(std::path::Path::new(&path).is_dir());

    let removed = repo.wt(&repo.root, ["rm", "preview", "--force-branch"]);
    assert_success(&removed);
    assert!(!std::path::Path::new(&path).exists());
    let branch = repo.git(&repo.root, ["show-ref", "--verify", "refs/heads/preview"]);
    assert!(!branch.status.success(), "forced branch was retained");
}

#[test]
fn forced_preview_reports_unknown_merge_status_for_an_unresolvable_base() {
    let repo = TestRepo::new();
    assert_success(&repo.git(&repo.root, ["branch", "deleted-preview-base"]));
    let created = repo.wt(
        &repo.root,
        [
            "new",
            "unknown-merge",
            "--base",
            "deleted-preview-base",
            "--print-path",
        ],
    );
    assert_success(&created);
    assert_success(&repo.git(&repo.root, ["branch", "-D", "deleted-preview-base"]));

    let human = repo.wt(
        &repo.root,
        ["rm", "unknown-merge", "--dry-run", "--force-branch"],
    );
    assert_success(&human);
    assert!(stdout(&human).contains("Branch merge: unknown"));
    assert!(stdout(&human).contains("Branch force: authorized (required: unknown)"));
    assert!(!stdout(&human).contains("required: unmerged"));

    let json = repo.wt(
        &repo.root,
        [
            "rm",
            "unknown-merge",
            "--dry-run",
            "--force-branch",
            "--json",
        ],
    );
    assert_success(&json);
    let json: Value = serde_json::from_slice(&json.stdout).expect("valid removal preview JSON");
    assert_eq!(json["branch_merge_status"], "unknown");
    assert_eq!(json["force_branch_required"], true);
}

#[test]
fn removal_refuses_unmerged_commits_but_supports_keep_branch_and_force_branch() {
    let repo = TestRepo::new();
    let created = repo.wt(&repo.root, ["new", "unmerged", "--print-path"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    fs::write(std::path::Path::new(&path).join("feature.txt"), "feature\n").expect("write feature");
    let commit = repo.git(std::path::Path::new(&path), ["add", "feature.txt"]);
    assert_success(&commit);
    let commit = repo.git(
        std::path::Path::new(&path),
        ["commit", "-m", "unmerged feature"],
    );
    assert_success(&commit);

    let refused = repo.wt(&repo.root, ["rm", "unmerged"]);
    assert_failure(
        &refused,
        "branch 'unmerged' contains commits not merged into 'main'",
    );
    assert!(std::path::Path::new(&path).exists());

    let kept = repo.wt(&repo.root, ["rm", "unmerged", "--keep-branch"]);
    assert_success(&kept);
    assert!(!std::path::Path::new(&path).exists());
    let branch = repo.git(&repo.root, ["show-ref", "--verify", "refs/heads/unmerged"]);
    assert_success(&branch);

    let deleted = repo.git(&repo.root, ["branch", "-D", "unmerged"]);
    assert_success(&deleted);

    let second = repo.wt(
        &repo.root,
        ["new", "forced", "--base", "main", "--print-path"],
    );
    assert_success(&second);
    let forced_path = stdout(&second).trim().to_owned();
    fs::write(
        std::path::Path::new(&forced_path).join("forced.txt"),
        "unmerged\n",
    )
    .expect("write forced branch file");
    let add = repo.git(std::path::Path::new(&forced_path), ["add", "forced.txt"]);
    assert_success(&add);
    let commit = repo.git(
        std::path::Path::new(&forced_path),
        ["commit", "-m", "forced unmerged feature"],
    );
    assert_success(&commit);
    let forced = repo.wt(&repo.root, ["rm", "forced", "--force-branch"]);
    assert_success(&forced);
    assert!(!std::path::Path::new(&forced_path).exists());
    let branch = repo.git(&repo.root, ["show-ref", "--verify", "refs/heads/forced"]);
    assert!(!branch.status.success(), "force did not delete branch");
}

#[test]
fn forced_dry_run_preserves_a_worktree_lock() {
    let repo = TestRepo::new();
    let created = repo.wt(&repo.root, ["new", "lock-preserved", "--print-path"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();
    assert_success(&repo.git(&repo.root, ["worktree", "lock", &path]));

    let preview = repo.wt(
        &repo.root,
        [
            "rm",
            "lock-preserved",
            "--dry-run",
            "--force-worktree",
            "--keep-branch",
        ],
    );
    assert_success(&preview);
    assert!(stdout(&preview).contains("Worktree force: authorized (required: locked)"));

    let listed = repo.wt(&repo.root, ["ls", "--json"]);
    assert_success(&listed);
    let listed: Value = serde_json::from_slice(&listed.stdout).expect("valid list JSON");
    let session = listed
        .as_array()
        .expect("session list")
        .iter()
        .find(|session| session["name"] == "lock-preserved")
        .expect("locked session remains listed");
    assert_eq!(session["locked"], true);
    assert_eq!(session["status"], "locked");

    let refused = repo.wt(&repo.root, ["rm", "lock-preserved", "--keep-branch"]);
    assert_failure(&refused, "worktree 'lock-preserved' is locked");
    assert!(std::path::Path::new(&path).is_dir());
}

#[test]
fn locked_missing_detached_and_spaced_worktrees_have_explicit_states() {
    let repo = TestRepo::with_name("project with spaces");
    let created = repo.wt(&repo.root, ["new", "locked", "--print-path"]);
    assert_success(&created);
    let locked_path = stdout(&created).trim().to_owned();
    let lock = repo.git(&repo.root, ["worktree", "lock", &locked_path]);
    assert_success(&lock);

    let refused = repo.wt(&repo.root, ["rm", "locked"]);
    assert_failure(&refused, "worktree 'locked' is locked");

    let listed = repo.wt(&repo.root, ["ls", "--json"]);
    assert_success(&listed);
    let listed: Value = serde_json::from_slice(&listed.stdout).expect("valid list JSON");
    let locked = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["name"] == "locked")
        .expect("locked session listed");
    assert_eq!(locked["status"], "locked");
    assert_eq!(locked["locked"], true);
    assert!(
        locked["path"]
            .as_str()
            .unwrap()
            .contains("project with spaces")
    );

    let removed = repo.wt(
        &repo.root,
        ["rm", "locked", "--force-worktree", "--keep-branch"],
    );
    assert_success(&removed);

    let detached_path = repo.temp_path("detached");
    let detached = repo.git(
        &repo.root,
        [
            "worktree",
            "add",
            "--detach",
            path_str(&detached_path),
            "HEAD",
        ],
    );
    assert_success(&detached);

    let missing_created = repo.wt(&repo.root, ["new", "missing", "--print-path"]);
    assert_success(&missing_created);
    let missing_path = stdout(&missing_created).trim().to_owned();
    fs::remove_dir_all(&missing_path).expect("remove worktree externally");

    let listed = repo.wt(&repo.root, ["ls", "--json"]);
    assert_success(&listed);
    let listed: Value = serde_json::from_slice(&listed.stdout).expect("valid list JSON");
    let sessions = listed.as_array().unwrap();
    let detached = sessions
        .iter()
        .find(|session| session["path"] == path_str(&detached_path))
        .expect("detached worktree listed");
    assert_eq!(detached["branch"], Value::Null);
    assert_eq!(detached["status"], "detached");

    let detached_child = repo.wt(&detached_path, ["new", "from-detached", "--json"]);
    assert_success(&detached_child);
    let detached_child: Value =
        serde_json::from_slice(&detached_child.stdout).expect("valid detached creation JSON");
    let base = detached_child["base"].as_str().expect("detached base");
    assert_eq!(base.len(), 40);
    assert!(base.chars().all(|character| character.is_ascii_hexdigit()));

    let removed_detached = repo.wt(&repo.root, ["rm", "detached"]);
    assert_success(&removed_detached);
    assert!(!detached_path.exists());
    let removed_child = repo.wt(&repo.root, ["rm", "from-detached"]);
    assert_success(&removed_child);
    let missing = sessions
        .iter()
        .find(|session| session["name"] == "missing")
        .expect("missing worktree listed");
    assert_eq!(missing["status"], "missing");

    let dry_run = repo.wt(&repo.root, ["prune", "--dry-run", "--json"]);
    assert_success(&dry_run);
    let dry_run: Value = serde_json::from_slice(&dry_run.stdout).expect("valid prune JSON");
    assert_eq!(dry_run["dry_run"], true);
    assert!(!dry_run["messages"].as_array().unwrap().is_empty());

    let pruned = repo.wt(&repo.root, ["prune"]);
    assert_success(&pruned);
    assert!(stdout(&pruned).contains("Removing"));
    let listed = repo.wt(&repo.root, ["ls", "--json"]);
    assert_success(&listed);
    assert!(!stdout(&listed).contains(&missing_path));
}

#[test]
fn status_identifies_the_main_worktree_without_assuming_its_directory_name() {
    let repo = TestRepo::with_name("not-the-repository-branch");
    let status = repo.wt(&repo.root, ["status", "--json"]);
    assert_success(&status);
    let status: Value = serde_json::from_slice(&status.stdout).expect("valid status JSON");
    assert_eq!(status["name"], "main");
    assert_eq!(status["branch"], "main");
    assert_eq!(status["path"], path_str(&repo.root));
    assert_eq!(status["base"], Value::Null);
}
#[test]
fn main_only_listing_and_human_creation_output_are_concise() {
    let repo = TestRepo::new();
    let listed = repo.wt(&repo.root, ["ls"]);
    assert_success(&listed);
    let listed = stdout(&listed);
    assert!(listed.starts_with("SESSION"));
    assert!(listed.contains("main"));
    assert_eq!(listed.lines().count(), 2);

    let created = repo.wt(&repo.root, ["new", "human", "--base", "main"]);
    assert_success(&created);
    assert!(stderr(&created).is_empty());
    let lines = stdout(&created)
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "Created human");
    assert!(std::path::Path::new(&lines[1]).is_absolute());
}

#[test]
fn missing_metadata_keeps_the_worktree_usable_and_cleanup_conservative() {
    let repo = TestRepo::new();
    let created = repo.wt(&repo.root, ["new", "metadata-loss", "--print-path"]);
    assert_success(&created);
    let path = stdout(&created).trim().to_owned();

    let common_dir = repo.git(
        &repo.root,
        ["rev-parse", "--path-format=absolute", "--git-common-dir"],
    );
    assert_success(&common_dir);
    let metadata_directory = std::path::Path::new(stdout(&common_dir).trim()).join("wt/sessions");
    for entry in fs::read_dir(metadata_directory).expect("list metadata") {
        fs::remove_file(entry.expect("metadata entry").path()).expect("remove metadata");
    }

    let resolved = repo.wt(&repo.root, ["path", "metadata-loss"]);
    assert_success(&resolved);
    assert_eq!(stdout(&resolved), format!("{path}\n"));
    let listed = repo.wt(&repo.root, ["ls", "--json"]);
    assert_success(&listed);
    assert!(stdout(&listed).contains("metadata-loss"));

    let refused = repo.wt(&repo.root, ["rm", "metadata-loss"]);
    assert_failure(&refused, "cannot determine the base for 'metadata-loss'");
    assert!(std::path::Path::new(&path).exists());

    let removed = repo.wt(&repo.root, ["rm", "metadata-loss", "--keep-branch"]);
    assert_success(&removed);
    let branch = repo.git(
        &repo.root,
        ["show-ref", "--verify", "refs/heads/metadata-loss"],
    );
    assert_success(&branch);
}

#[test]
fn concurrent_creation_has_one_winner_and_one_clean_failure() {
    let repo = TestRepo::new();
    let executable = env!("CARGO_BIN_EXE_wt");
    let commands = (0..2)
        .map(|_| {
            Command::new(executable)
                .args(["new", "race", "--base", "main", "--print-path"])
                .current_dir(&repo.root)
                .env("HOME", &repo.home)
                .env("XDG_CONFIG_HOME", repo.home.join(".config"))
                .env("LC_ALL", "C")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn concurrent wt")
        })
        .collect::<Vec<_>>();
    let outputs = commands
        .into_iter()
        .map(|child| child.wait_with_output().expect("wait for concurrent wt"))
        .collect::<Vec<_>>();

    assert_eq!(
        outputs
            .iter()
            .filter(|output| output.status.success())
            .count(),
        1,
        "expected one successful creation: {:?}",
        outputs
            .iter()
            .map(|output| (output.status, stdout(output), stderr(output)))
            .collect::<Vec<_>>()
    );
    let loser = outputs
        .iter()
        .find(|output| !output.status.success())
        .expect("one creation should fail");
    assert!(
        stderr(loser).contains("already exists") || stderr(loser).contains("git command failed"),
        "unexpected concurrent failure: {}",
        stderr(loser)
    );

    let listed = repo.wt(&repo.root, ["ls", "--json"]);
    assert_success(&listed);
    let listed: Value = serde_json::from_slice(&listed.stdout).expect("valid list JSON");
    assert_eq!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .filter(|session| session["name"] == "race")
            .count(),
        1
    );
}
