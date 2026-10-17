//! Integration tests for `bead list` command

use assert_cmd::Command;
use serial_test::serial;

#[test]
#[serial]
fn test_list_empty_workspace() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--skip-foreign-workspace", "--prefix", "test"])
        .assert()
        .success();

    // List issues (should be empty)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("No issues found"));
}

#[test]
#[serial]
fn test_list_with_issues() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--skip-foreign-workspace", "--prefix", "test"])
        .assert()
        .success();

    // Create some issues
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "First Issue"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Second Issue"])
        .assert()
        .success();

    // List issues
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("First Issue"))
        .stdout(predicates::str::contains("Second Issue"));
}

#[test]
#[serial]
fn test_list_json() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--skip-foreign-workspace", "--prefix", "test"])
        .assert()
        .success();

    // Create an issue
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "JSON Test"])
        .assert()
        .success();

    // List with JSON output
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"id\""))
        .stdout(predicates::str::contains("\"title\""))
        .stdout(predicates::str::contains("JSON Test"));
}

#[test]
#[serial]
fn test_list_with_limit() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--skip-foreign-workspace", "--prefix", "test"])
        .assert()
        .success();

    // Create multiple issues
    for i in 1..=5 {
        Command::cargo_bin("bead")
            .unwrap()
            .args(["create", "--title", &format!("Issue {}", i)])
            .assert()
            .success();
    }

    // List with limit
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--limit", "3"])
        .assert()
        .success();
}

#[test]
#[serial]
fn test_list_invalid_limit() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--skip-foreign-workspace", "--prefix", "test"])
        .assert()
        .success();

    // List with invalid limit
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--limit", "1000000"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "Limit must be between 0 and 999999",
        ));
}

#[test]
#[serial]
fn test_list_invalid_comments() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--skip-foreign-workspace", "--prefix", "test"])
        .assert()
        .success();

    // List with invalid comments option
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--comments", "invalid"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "Comments must be one of: none, unresolved, all",
        ));
}

#[test]
#[serial]
fn test_list_ready_excludes_blocked_issues() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--skip-foreign-workspace", "--prefix", "test"])
        .assert()
        .success();

    // Create three issues: A, B, and C
    let task_a_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Task A"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let task_a = String::from_utf8_lossy(&task_a_output).trim().to_string();

    let task_b_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Task B"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let task_b = String::from_utf8_lossy(&task_b_output).trim().to_string();

    let task_c_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Task C"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let task_c = String::from_utf8_lossy(&task_c_output).trim().to_string();

    // Add blocking dependency: Task B blocks Task C
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &task_c, &task_b])
        .assert()
        .success();

    // List ready issues - should only include Task A and Task B (not Task C)
    let ready_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--ready", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let ready_str = String::from_utf8_lossy(&ready_output);

    // Task A and Task B should be present, Task C should NOT be present
    assert!(ready_str.contains(&format!("\"id\":\"{}\"", task_a)));
    assert!(ready_str.contains(&format!("\"id\":\"{}\"", task_b)));
    assert!(!ready_str.contains(&format!("\"id\":\"{}\"", task_c)));
}

#[test]
#[serial]
fn test_list_ready_includes_after_blocker_closed() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--skip-foreign-workspace", "--prefix", "test"])
        .assert()
        .success();

    // Create two issues: A and B
    let task_a_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Task A"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let task_a = String::from_utf8_lossy(&task_a_output).trim().to_string();

    let task_b_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Task B"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let task_b = String::from_utf8_lossy(&task_b_output).trim().to_string();

    // Add blocking dependency: Task A blocks Task B
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &task_b, &task_a])
        .assert()
        .success();

    // List ready issues - should only include Task A (not Task B)
    let ready_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--ready", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let ready_str = String::from_utf8_lossy(&ready_output);

    // Task A should be present, Task B should NOT be ready
    assert!(ready_str.contains(&format!("\"id\":\"{}\"", task_a)));
    assert!(!ready_str.contains(&format!("\"id\":\"{}\"", task_b)));

    // Close Task A (the blocker)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", &task_a, "--reason", "Completed"])
        .assert()
        .success();

    // Now Task B should be ready since its blocker is closed
    let ready_after_close = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--ready", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let ready_after_str = String::from_utf8_lossy(&ready_after_close);

    // Task B should now be ready
    assert!(ready_after_str.contains(&format!("\"id\":\"{}\"", task_b)));
}

// --json is a single-document mode: one JSON array whether the list matched
// zero, one, or many beads, so a consumer can always decode one document.
#[test]
#[serial]
fn test_list_json_is_one_array_for_every_result_count() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--skip-foreign-workspace", "--prefix", "test"])
        .assert()
        .success();

    // Zero results: still exactly one document, an empty array
    let empty_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let empty_str = String::from_utf8_lossy(&empty_output);
    assert_eq!(
        empty_str.trim().lines().count(),
        1,
        "zero results must still print one line, got: {empty_str:?}"
    );
    let parsed: serde_json::Value = serde_json::from_str(empty_str.trim())
        .expect("zero results must still be one valid JSON document");
    assert!(
        parsed.is_array(),
        "--json must emit an array for zero results, got: {empty_str:?}"
    );
    assert_eq!(parsed.as_array().unwrap().len(), 0);

    // One result: one array holding exactly that bead
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Solo Issue"])
        .assert()
        .success();

    let one_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let one_str = String::from_utf8_lossy(&one_output);
    assert_eq!(
        one_str.trim().lines().count(),
        1,
        "one result must print one line, got: {one_str:?}"
    );
    let parsed: serde_json::Value = serde_json::from_str(one_str.trim())
        .expect("one result must be one valid JSON document");
    let items = parsed
        .as_array()
        .unwrap_or_else(|| panic!("--json must emit an array for one result, got: {one_str:?}"));
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Solo Issue");

    // Many results: still one array, every bead present as an element
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Second Issue"])
        .assert()
        .success();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Third Issue"])
        .assert()
        .success();

    let many_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let many_str = String::from_utf8_lossy(&many_output);
    assert_eq!(
        many_str.trim().lines().count(),
        1,
        "many results must print one line, got: {many_str:?}"
    );
    let parsed: serde_json::Value = serde_json::from_str(many_str.trim())
        .expect("many results must be one valid JSON document");
    let items = parsed
        .as_array()
        .unwrap_or_else(|| panic!("--json must emit an array for many results, got: {many_str:?}"));
    assert_eq!(items.len(), 3);
    let titles: Vec<_> = items
        .iter()
        .map(|item| item["title"].as_str().unwrap())
        .collect();
    assert!(titles.contains(&"Solo Issue"));
    assert!(titles.contains(&"Second Issue"));
    assert!(titles.contains(&"Third Issue"));
}

// --jsonl is the explicit streaming mode: one JSON object per line for
// nonempty results and no bytes at all when nothing matched.
#[test]
#[serial]
fn test_list_jsonl_streams_one_object_per_line_and_nothing_when_empty() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--skip-foreign-workspace", "--prefix", "test"])
        .assert()
        .success();

    // Empty results write no output
    let empty_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--jsonl"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(
        empty_output.is_empty(),
        "--jsonl must write nothing for empty results, got: {:?}",
        String::from_utf8_lossy(&empty_output)
    );

    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Stream One"])
        .assert()
        .success();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Stream Two"])
        .assert()
        .success();

    let stream_output = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--jsonl"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stream_str = String::from_utf8_lossy(&stream_output);
    let lines: Vec<&str> = stream_str.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "one object per line expected, got: {stream_str:?}"
    );
    let mut titles = Vec::new();
    for line in &lines {
        let parsed: serde_json::Value = serde_json::from_str(line)
            .expect("each --jsonl line must be a standalone JSON object");
        assert!(
            parsed.is_object(),
            "each --jsonl line must be an object, not a wrapped array, got: {line}"
        );
        titles.push(parsed["title"].as_str().unwrap().to_string());
    }
    titles.sort();
    assert_eq!(titles, vec!["Stream One", "Stream Two"]);
}

#[test]
#[serial]
fn test_list_json_and_jsonl_are_mutually_exclusive() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();

    // Initialize workspace
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--skip-foreign-workspace", "--prefix", "test"])
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--json", "--jsonl"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "--jsonl cannot be combined with --json",
        ));
}
