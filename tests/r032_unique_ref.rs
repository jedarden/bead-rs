//! R032: idempotent create by unique external reference.

use std::process::{Command, Output, Stdio};

fn run(workspace: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bead"))
        .current_dir(workspace)
        .arg("--skip-foreign-workspace")
        .args(args)
        .output()
        .expect("bead command should start")
}

fn setup() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().unwrap();
    let output = run(workspace.path(), &["init", "--prefix", "r032"]);
    assert!(output.status.success(), "init failed: {output:?}");
    workspace
}

#[test]
fn unique_ref_reuses_open_issue_and_records_r011_projection() {
    let workspace = setup();

    let first = run(
        workspace.path(),
        &[
            "create",
            "--title",
            "first materialization",
            "--unique-ref",
            "github:issue-42",
        ],
    );
    assert!(first.status.success(), "first create failed: {first:?}");
    let first_id = String::from_utf8_lossy(&first.stdout).trim().to_string();
    assert!(!first_id.starts_with("EXISTING"));

    let second = run(
        workspace.path(),
        &[
            "create",
            "--title",
            "duplicate materialization",
            "--unique-ref",
            "github:issue-42",
        ],
    );
    assert!(second.status.success(), "second create failed: {second:?}");
    assert_eq!(
        String::from_utf8_lossy(&second.stdout).trim(),
        format!("EXISTING {first_id}")
    );

    let conn = rusqlite::Connection::open(workspace.path().join(".beads/beads.db")).unwrap();
    let issue_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0))
        .unwrap();
    assert_eq!(issue_count, 1);

    let projection: (String, String, String) = conn
        .query_row(
            "SELECT namespace, key, value FROM external_references WHERE issue_id = ?1",
            [&first_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        projection,
        ("github".into(), "unique-ref".into(), "issue-42".into())
    );

    let binding: String = conn
        .query_row(
            "SELECT issue_id FROM unique_reference_bindings
             WHERE namespace = 'github' AND key = 'issue-42'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(binding, first_id);
}

#[test]
fn unique_ref_hit_on_closed_issue_is_explicit() {
    let workspace = setup();
    let first = run(
        workspace.path(),
        &[
            "create",
            "--title",
            "work that finishes",
            "--unique-ref",
            "jira:PROJ-7",
        ],
    );
    assert!(first.status.success());
    let issue_id = String::from_utf8_lossy(&first.stdout).trim().to_string();

    let closed = run(
        workspace.path(),
        &["close", &issue_id, "--reason", "completed"],
    );
    assert!(closed.status.success(), "close failed: {closed:?}");

    let retry = run(
        workspace.path(),
        &[
            "create",
            "--title",
            "retry completed work",
            "--unique-ref",
            "jira:PROJ-7",
        ],
    );
    assert!(retry.status.success(), "closed ref hit failed: {retry:?}");
    assert_eq!(
        String::from_utf8_lossy(&retry.stdout).trim(),
        format!("EXISTING_CLOSED {issue_id}")
    );
}

#[test]
fn concurrent_unique_ref_creates_leave_exactly_one_issue() {
    let workspace = setup();
    let mut children = Vec::new();
    for _ in 0..8 {
        let mut command = Command::new(env!("CARGO_BIN_EXE_bead"));
        command
            .current_dir(workspace.path())
            .args([
                "create",
                "--title",
                "racing materialization",
                "--unique-ref",
                "source:ticket-99",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        children.push(command.spawn().expect("concurrent create should start"));
    }

    let outputs: Vec<Output> = children
        .into_iter()
        .map(|child| child.wait_with_output().expect("child should finish"))
        .collect();
    assert!(
        outputs.iter().all(|output| output.status.success()),
        "at least one concurrent create failed: {outputs:?}"
    );

    let values: Vec<String> = outputs
        .iter()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .collect();
    let fresh: Vec<&String> = values
        .iter()
        .filter(|value| !value.starts_with("EXISTING"))
        .collect();
    assert_eq!(fresh.len(), 1, "outputs were {values:?}");
    let winner = fresh[0].clone();
    assert!(
        values
            .iter()
            .filter(|value| value.as_str() == format!("EXISTING {winner}"))
            .count()
            == 7,
        "outputs were {values:?}"
    );

    let conn = rusqlite::Connection::open(workspace.path().join(".beads/beads.db")).unwrap();
    let issue_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0))
        .unwrap();
    let binding_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM unique_reference_bindings
             WHERE namespace = 'source' AND key = 'ticket-99'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(issue_count, 1);
    assert_eq!(binding_count, 1);
}

#[test]
fn unique_ref_requires_namespace_colon_key_form() {
    let workspace = setup();
    let output = run(
        workspace.path(),
        &[
            "create",
            "--title",
            "invalid",
            "--unique-ref",
            "missing-colon",
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("NAMESPACE:KEY"));
}
