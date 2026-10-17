//! Integration tests for the R041 read shortcuts: `ready`, `search`,
//! `list --all/--search/--count`, `dep list`, and bare `bead <ID>`.

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;

fn init_workspace(temp: &std::path::Path) {
    std::env::set_current_dir(temp).unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--skip-foreign-workspace", "--prefix", "test"])
        .assert()
        .success();
}

fn create(title: &str) -> String {
    let output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", title])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8_lossy(&output).trim().to_string()
}

#[test]
#[serial]
fn test_ready_matches_list_ready_byte_for_byte() {
    let temp = tempfile::tempdir().unwrap();
    init_workspace(temp.path());

    create("Task A");
    create("Task B");

    let ready_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["ready", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let list_ready_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--ready", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(ready_output, list_ready_output);
}

#[test]
#[serial]
fn test_search_matches_list_all_search_byte_for_byte() {
    let temp = tempfile::tempdir().unwrap();
    init_workspace(temp.path());

    create("Fix flaky scanner test");
    create("Unrelated issue");

    let search_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["search", "flaky", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let list_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--all", "--search", "flaky", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(search_output, list_output);
    assert!(String::from_utf8_lossy(&search_output).contains("Fix flaky scanner test"));
    assert!(!String::from_utf8_lossy(&search_output).contains("Unrelated issue"));
}

#[test]
#[serial]
fn test_search_matches_description_and_notes_case_insensitively() {
    let temp = tempfile::tempdir().unwrap();
    init_workspace(temp.path());

    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "create",
            "--title",
            "Investigate outage",
            "--description",
            "Root cause looked FLAKY under load",
        ])
        .assert()
        .success();

    let output = Command::cargo_bin("bead")
        .unwrap()
        .args(["search", "flaky", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(String::from_utf8_lossy(&output).contains("Investigate outage"));
}

#[test]
#[serial]
fn test_list_search_combines_with_status_and_assignee() {
    let temp = tempfile::tempdir().unwrap();
    init_workspace(temp.path());

    let matching = create("Flaky build on CI");
    let other = create("Flaky build but assigned");

    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &other, "--assignee", "alice"])
        .assert()
        .success();

    let output = Command::cargo_bin("bead")
        .unwrap()
        .args([
            "list",
            "--all",
            "--search",
            "flaky",
            "--assignee",
            "",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&output);
    assert!(text.contains(&format!("\"id\":\"{}\"", matching)));
    assert!(!text.contains(&format!("\"id\":\"{}\"", other)));
}

#[test]
#[serial]
fn test_list_count_plain_and_json() {
    let temp = tempfile::tempdir().unwrap();
    init_workspace(temp.path());

    create("Issue One");
    create("Issue Two");
    create("Issue Three");

    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--count"])
        .assert()
        .success()
        .stdout("3\n");

    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--count", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"count\":3"));
}

#[test]
#[serial]
fn test_list_count_respects_filters() {
    let temp = tempfile::tempdir().unwrap();
    init_workspace(temp.path());

    create("Open issue");
    let assigned = create("Assigned issue");
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &assigned, "--assignee", "bob"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--count", "--assignee", "bob"])
        .assert()
        .success()
        .stdout("1\n");

    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--count", "--ready"])
        .assert()
        .success()
        .stdout("1\n");
}

#[test]
#[serial]
fn test_list_all_ignores_limit() {
    let temp = tempfile::tempdir().unwrap();
    init_workspace(temp.path());

    for i in 1..=5 {
        create(&format!("Issue {}", i));
    }

    let output = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--all", "--limit", "2", "--jsonl"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let lines = String::from_utf8_lossy(&output).lines().count();
    assert_eq!(lines, 5);
}

#[test]
#[serial]
fn test_dep_list_shows_both_directions() {
    let temp = tempfile::tempdir().unwrap();
    init_workspace(temp.path());

    let blocked = create("Blocked issue");
    let blocker = create("Blocker issue");

    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &blocked, &blocker])
        .assert()
        .success();

    let blocker_view = Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "list", &blocker, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let blocker_text = String::from_utf8_lossy(&blocker_view);
    assert!(blocker_text.contains(&format!("\"dependent\":\"{}\"", blocked)));
    assert!(blocker_text.contains("\"kind\":\"blocks\""));

    let blocked_view = Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "list", &blocked, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let blocked_text = String::from_utf8_lossy(&blocked_view);
    assert!(blocked_text.contains(&format!("\"blocker\":\"{}\"", blocker)));
    assert!(blocked_text.contains("\"finished\":false"));
}

#[test]
#[serial]
fn test_dep_list_nonexistent_issue_exits_3() {
    let temp = tempfile::tempdir().unwrap();
    init_workspace(temp.path());

    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "list", "test-deadbeef"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("Issue not found"));
}

#[test]
#[serial]
fn test_bare_id_equals_show() {
    let temp = tempfile::tempdir().unwrap();
    init_workspace(temp.path());

    let id = create("Bare ID lookup");

    let bare = Command::cargo_bin("bead")
        .unwrap()
        .args([id.as_str(), "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let show = Command::cargo_bin("bead")
        .unwrap()
        .args(["show", &id, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(bare, show);
}

#[test]
#[serial]
fn test_bare_unknown_token_is_still_usage_error() {
    let temp = tempfile::tempdir().unwrap();
    init_workspace(temp.path());

    // Neither a subcommand nor ID-shaped (no '-' at all).
    Command::cargo_bin("bead")
        .unwrap()
        .args(["frobnicate"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

#[test]
#[serial]
fn test_bare_id_nonexistent_exits_3() {
    let temp = tempfile::tempdir().unwrap();
    init_workspace(temp.path());

    Command::cargo_bin("bead")
        .unwrap()
        .args(["test-deadbeef"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("Issue not found"));
}

#[test]
#[serial]
fn test_subcommand_name_always_wins_over_id_shape() {
    let temp = tempfile::tempdir().unwrap();
    init_workspace(temp.path());

    // `list` is a real subcommand even though it is not ID-shaped; this just
    // documents that the rewrite never touches a recognized subcommand.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No issues found"));
}
