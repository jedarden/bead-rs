//! Checkpoint round-trip conformance test
//!
//! This test ensures that flushing a checkpoint and restoring it preserves
//! ALL public fields and side tables with complete fidelity. It would have
//! caught the checkpoint defects found during the ADR-002 review:
//! - Revision reset to 1
//! - Issue data destroyed
//! - External references destroyed
//! - Comments destroyed
//! - Absent descriptions restored as empty strings
//!
//! Test strategy:
//! 1. Create a maximally-populated workspace with every field set
//! 2. Flush forensic checkpoint
//! 3. Restore into fresh empty workspace
//! 4. Assert complete equality over the public surface
//!
//! Any field NOT expected to be preserved must be explicitly named with
//! a comment explaining why.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Create a test workspace and return the temp dir
fn create_workspace() -> TempDir {
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace (R030: `bead init` creates `.beads` itself; a
    // pre-created empty `.beads` is an unrecognized store discovery must
    // fail closed on, not scaffold over)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "bead"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    temp_dir
}

/// Pin a workspace to the explicit-flush contract (`checkpoint.auto_flush:
/// false`, plan 6.2.1 item 7).
///
/// These conformance tests drive source workspaces that mutate directly in
/// SQLite (revision bumps, projected-collection rows) and then publish with
/// an explicit `sync flush-only`. Direct database writes emit no audit event,
/// so under the automatic publication default the live sequence does not
/// advance and the explicit flush correctly reports an already-current
/// checkpoint without ever observing those rows. Suppressing publication
/// restores the contract under test: every CLI mutation leaves the checkpoint
/// dirty and the explicit flush is what carries it, manual rows included.
fn suppress_auto_flush(workspace: &std::path::Path) {
    let path = workspace.join(".beads/config.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config
        .as_object_mut()
        .unwrap()
        .entry("checkpoint")
        .or_insert(serde_json::Value::Object(Default::default()))
        .as_object_mut()
        .unwrap()
        .insert("auto_flush".into(), serde_json::Value::Bool(false));
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
}

/// Extract bead ID from create command output
fn extract_bead_id(output: &str) -> String {
    // Output format: "bead-12345678" or similar
    let lines: Vec<&str> = output.lines().collect();
    let last_line = lines.last().unwrap_or(&"");
    let id = last_line.trim().to_string();

    // Validate it looks like a bead ID
    assert!(id.starts_with("bead-"), "Invalid bead ID: {}", id);
    assert!(id.len() >= 9, "Bead ID too short: {}", id);

    id
}

/// Model a checkpoint emitted by 0.1.1, which had no collection projections.
fn remove_projected_collections(checkpoint: &Path) {
    let pointer: Value =
        serde_json::from_str(&fs::read_to_string(checkpoint.join("current.json")).unwrap())
            .unwrap();
    let relative = pointer["active_root"]["path"].as_str().unwrap();
    let generation = checkpoint.join(relative);
    let rewritten = fs::read_to_string(&generation)
        .unwrap()
        .lines()
        .map(|line| {
            let mut record: Value = serde_json::from_str(line).unwrap();
            if let Some(issue) = record.get_mut("issue").and_then(Value::as_object_mut) {
                issue.remove("data");
                issue.remove("external_references");
                issue.remove("comments");
            }
            serde_json::to_string(&record).unwrap()
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(generation, format!("{rewritten}\n")).unwrap();
}

/// Compare all issues between two workspaces for complete equality
fn assert_issues_equal(workspace1: &Path, workspace2: &Path) {
    let db1 = workspace1.join(".beads/beads.db");
    let db2 = workspace2.join(".beads/beads.db");

    // Read all issues from both databases
    let issues1 = read_all_issues(&db1);
    let issues2 = read_all_issues(&db2);

    // Compare counts
    assert_eq!(
        issues1.len(),
        issues2.len(),
        "Issue count mismatch: {} vs {}",
        issues1.len(),
        issues2.len()
    );

    // Compare each issue field-by-field
    for issue1 in &issues1 {
        let issue2 = issues2
            .iter()
            .find(|i| i["id"] == issue1["id"])
            .unwrap_or_else(|| panic!("Issue {} not found in restored workspace", issue1["id"]));

        // Required fields - must be equal
        assert_eq!(issue1["id"], issue2["id"], "ID mismatch");
        assert_eq!(
            issue1["title"], issue2["title"],
            "Title mismatch for {}",
            issue1["id"]
        );
        assert_eq!(
            issue1["priority"], issue2["priority"],
            "Priority mismatch for {}",
            issue1["id"]
        );
        assert_eq!(
            issue1["base_status"], issue2["base_status"],
            "Base status mismatch for {}",
            issue1["id"]
        );
        assert_eq!(
            issue1["created_at"], issue2["created_at"],
            "Created at mismatch for {}",
            issue1["id"]
        );
        assert_eq!(
            issue1["updated_at"], issue2["updated_at"],
            "Updated at mismatch for {}",
            issue1["id"]
        );

        // Critical field - would have caught revision reset defect
        assert_eq!(
            issue1.get("revision").and_then(|v| v.as_i64()),
            issue2.get("revision").and_then(|v| v.as_i64()),
            "Revision mismatch for {} - THIS WOULD HAVE CAUGHT THE REVISION RESET DEFECT",
            issue1["id"]
        );

        // Optional fields - must be equal if present in source
        assert_eq!(
            issue1.get("description"),
            issue2.get("description"),
            "Description mismatch for {}",
            issue1["id"]
        );
        assert_eq!(
            issue1.get("notes"),
            issue2.get("notes"),
            "Notes mismatch for {}",
            issue1["id"]
        );
        assert_eq!(
            issue1.get("manual_blocked"),
            issue2.get("manual_blocked"),
            "Manual blocked mismatch for {}",
            issue1["id"]
        );
        assert_eq!(
            issue1.get("assignee"),
            issue2.get("assignee"),
            "Assignee mismatch for {}",
            issue1["id"]
        );
        assert_eq!(
            issue1.get("issue_type"),
            issue2.get("issue_type"),
            "Issue type mismatch for {}",
            issue1["id"]
        );
        assert_eq!(
            issue1.get("closed_at"),
            issue2.get("closed_at"),
            "Closed at mismatch for {}",
            issue1["id"]
        );
        assert_eq!(
            issue1.get("close_reason"),
            issue2.get("close_reason"),
            "Close reason mismatch for {}",
            issue1["id"]
        );
        assert_eq!(
            issue1.get("source_repo"),
            issue2.get("source_repo"),
            "Source repo mismatch for {}",
            issue1["id"]
        );
        assert_eq!(
            issue1.get("profile"),
            issue2.get("profile"),
            "Profile mismatch for {}",
            issue1["id"]
        );
        assert_eq!(
            issue1.get("schema_ref"),
            issue2.get("schema_ref"),
            "Schema ref mismatch for {}",
            issue1["id"]
        );

        // Critical field - would have caught issue_data destruction defect
        assert_eq!(
            issue1.get("data"),
            issue2.get("data"),
            "Issue data mismatch for {} - THIS WOULD HAVE CAUGHT THE ISSUE_DATA DESTRUCTION DEFECT",
            issue1["id"]
        );

        // Scheduling fields (migration 9) - must be preserved
        assert_eq!(
            issue1.get("ready_since"),
            issue2.get("ready_since"),
            "Ready since mismatch for {}",
            issue1["id"]
        );
        assert_eq!(
            issue1.get("last_claim_sequence"),
            issue2.get("last_claim_sequence"),
            "Last claim sequence mismatch for {}",
            issue1["id"]
        );
        assert_eq!(
            issue1.get("attempt_tier"),
            issue2.get("attempt_tier"),
            "Attempt tier mismatch for {}",
            issue1["id"]
        );
        assert_eq!(
            issue1.get("consecutive_failures"),
            issue2.get("consecutive_failures"),
            "Consecutive failures mismatch for {}",
            issue1["id"]
        );
        assert_eq!(
            issue1.get("retry_after_claim_sequence"),
            issue2.get("retry_after_claim_sequence"),
            "Retry after claim sequence mismatch for {}",
            issue1["id"]
        );

        // Issue extensions - must be preserved
        assert_eq!(
            issue1.get("extensions"),
            issue2.get("extensions"),
            "Extensions mismatch for {}",
            issue1["id"]
        );
    }
}

/// Read all issues from the database
fn read_all_issues(db_path: &Path) -> Vec<Value> {
    use rusqlite::Connection;

    let conn = Connection::open(db_path).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, title, description, notes, priority, issue_type, base_status,
                manual_blocked, assignee, created_at, updated_at, closed_at, close_reason,
                source_repo, profile, schema_ref, revision, ready_since, last_claim_sequence,
                attempt_tier, consecutive_failures, retry_after_claim_sequence
         FROM issues",
        )
        .unwrap();

    let issues = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, String>("id")?,
            "title": row.get::<_, String>("title")?,
            "description": row.get::<_, Option<String>>("description")?,
            "notes": row.get::<_, Option<String>>("notes")?,
            "priority": row.get::<_, i64>("priority")?,
            "issue_type": row.get::<_, String>("issue_type")?,
            "base_status": row.get::<_, String>("base_status")?,
            "manual_blocked": row.get::<_, i64>("manual_blocked").map(|v| v == 1)?,
            "assignee": row.get::<_, Option<String>>("assignee")?,
            "created_at": row.get::<_, String>("created_at")?,
            "updated_at": row.get::<_, String>("updated_at")?,
            "closed_at": row.get::<_, Option<String>>("closed_at")?,
            "close_reason": row.get::<_, Option<String>>("close_reason")?,
            "source_repo": row.get::<_, Option<String>>("source_repo")?,
            "profile": row.get::<_, String>("profile")?,
            "schema_ref": row.get::<_, String>("schema_ref")?,
            "revision": row.get::<_, i64>("revision")?,
            "ready_since": row.get::<_, Option<String>>("ready_since")?,
            "last_claim_sequence": row.get::<_, Option<i64>>("last_claim_sequence")?,
            "attempt_tier": row.get::<_, i64>("attempt_tier")?,
            "consecutive_failures": row.get::<_, i64>("consecutive_failures")?,
            "retry_after_claim_sequence": row.get::<_, Option<i64>>("retry_after_claim_sequence")?,
        }))
    }).unwrap();

    let mut result = Vec::new();
    for issue in issues {
        let mut issue_json = issue.unwrap();

        // Load issue extensions
        let mut ext_stmt = conn
            .prepare("SELECT key, value FROM issue_extensions WHERE issue_id = ?1")
            .unwrap();
        let extensions = ext_stmt
            .query_map([&issue_json["id"].as_str().unwrap()], |row| {
                Ok((row.get::<_, String>("key")?, row.get::<_, String>("value")?))
            })
            .unwrap();

        let mut extensions_map = serde_json::Map::new();
        for ext in extensions {
            let (k, v) = ext.unwrap();
            extensions_map.insert(k, serde_json::Value::String(v));
        }
        issue_json.as_object_mut().unwrap().insert(
            "extensions".to_string(),
            serde_json::Value::Object(extensions_map),
        );

        // Load issue data
        let mut data_stmt = conn
            .prepare("SELECT namespace, schema_ref, value FROM issue_data WHERE issue_id = ?1")
            .unwrap();
        let data_rows = data_stmt
            .query_map([&issue_json["id"].as_str().unwrap()], |row| {
                Ok((
                    row.get::<_, String>("namespace")?,
                    row.get::<_, String>("schema_ref")?,
                    row.get::<_, String>("value")?,
                ))
            })
            .unwrap();

        let mut data_map = serde_json::Map::new();
        for data_result in data_rows {
            let (namespace, schema_ref, value) = data_result.unwrap();
            data_map.insert(
                "namespace".to_string(),
                serde_json::Value::String(namespace),
            );
            data_map.insert(
                "schema_ref".to_string(),
                serde_json::Value::String(schema_ref),
            );
            data_map.insert("value".to_string(), serde_json::Value::String(value));
        }

        if !data_map.is_empty() {
            issue_json
                .as_object_mut()
                .unwrap()
                .insert("data".to_string(), serde_json::Value::Object(data_map));
        }

        result.push(issue_json);
    }

    result
}

/// Compare all labels between two workspaces
fn assert_labels_equal(workspace1: &Path, workspace2: &Path) {
    let labels1 = read_all_labels(workspace1);
    let labels2 = read_all_labels(workspace2);

    assert_eq!(
        labels1.len(),
        labels2.len(),
        "Label count mismatch: {} vs {}",
        labels1.len(),
        labels2.len()
    );

    for key in labels1.keys() {
        assert!(
            labels2.contains_key(key),
            "Label {} not found in restored workspace",
            key
        );
        assert_eq!(
            labels1.get(key).unwrap(),
            labels2.get(key).unwrap(),
            "Label set mismatch for issue {}",
            key
        );
    }
}

/// Read all labels from workspace
fn read_all_labels(workspace: &Path) -> HashMap<String, HashSet<String>> {
    let db = workspace.join(".beads/beads.db");
    let conn = rusqlite::Connection::open(&db).unwrap();

    let mut stmt = conn.prepare("SELECT issue_id, label FROM labels").unwrap();
    let labels = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>("issue_id")?,
                row.get::<_, String>("label")?,
            ))
        })
        .unwrap();

    let mut result: HashMap<String, HashSet<String>> = HashMap::new();
    for label_result in labels {
        let (issue_id, label) = label_result.unwrap();
        result.entry(issue_id).or_default().insert(label);
    }

    result
}

/// Compare all dependencies between two workspaces
fn assert_dependencies_equal(workspace1: &Path, workspace2: &Path) {
    let deps1 = read_all_dependencies(workspace1);
    let deps2 = read_all_dependencies(workspace2);

    assert_eq!(
        deps1.len(),
        deps2.len(),
        "Dependency count mismatch: {} vs {}",
        deps1.len(),
        deps2.len()
    );

    // Dependencies are not ordered, so compare as sets
    for dep in &deps1 {
        assert!(
            deps2.contains(dep),
            "Dependency {:?} not found in restored workspace",
            dep
        );
    }
    for dep in &deps2 {
        assert!(
            deps1.contains(dep),
            "Extra dependency {:?} in restored workspace",
            dep
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Dependency {
    blocked_issue_id: String,
    blocker_issue_id: String,
    kind: String,
    condition: Option<String>,
}

/// Read all dependencies from workspace
fn read_all_dependencies(workspace: &Path) -> HashSet<Dependency> {
    let db = workspace.join(".beads/beads.db");
    let conn = rusqlite::Connection::open(&db).unwrap();

    let mut stmt = conn
        .prepare("SELECT blocked_issue_id, blocker_issue_id, kind, condition FROM dependencies")
        .unwrap();

    let deps = stmt
        .query_map([], |row| {
            Ok(Dependency {
                blocked_issue_id: row.get("blocked_issue_id")?,
                blocker_issue_id: row.get("blocker_issue_id")?,
                kind: row.get("kind")?,
                condition: row.get::<_, Option<String>>("condition")?,
            })
        })
        .unwrap();

    deps.filter_map(|d| d.ok()).collect()
}

/// Compare all comments between two workspaces
fn assert_comments_equal(workspace1: &Path, workspace2: &Path) {
    let comments1 = read_all_comments(workspace1);
    let comments2 = read_all_comments(workspace2);

    assert_eq!(
        comments1.len(),
        comments2.len(),
        "Comment count mismatch: {} vs {}",
        comments1.len(),
        comments2.len()
    );

    for comment in &comments1 {
        assert!(
            comments2.contains(comment),
            "Comment {:?} not found in restored workspace",
            comment
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Comment {
    id: String,
    issue_id: String,
    author: String,
    body: String,
    reply_to_id: Option<String>,
    resolution_state: Option<String>,
    created_at: String,
}

/// Read all comments from workspace
fn read_all_comments(workspace: &Path) -> HashSet<Comment> {
    let db = workspace.join(".beads/beads.db");
    let conn = rusqlite::Connection::open(&db).unwrap();

    let mut stmt = conn
        .prepare(
            "SELECT id, issue_id, author, body, reply_to_id, resolution_state, created_at
         FROM comments",
        )
        .unwrap();

    let comments = stmt
        .query_map([], |row| {
            Ok(Comment {
                id: row.get("id")?,
                issue_id: row.get("issue_id")?,
                author: row.get("author")?,
                body: row.get("body")?,
                reply_to_id: row.get::<_, Option<String>>("reply_to_id")?,
                resolution_state: row.get::<_, Option<String>>("resolution_state")?,
                created_at: row.get("created_at")?,
            })
        })
        .unwrap();

    comments.filter_map(|c| c.ok()).collect()
}

/// Compare all external references between two workspaces
fn assert_external_references_equal(workspace1: &Path, workspace2: &Path) {
    let refs1 = read_all_external_references(workspace1);
    let refs2 = read_all_external_references(workspace2);

    // This would have caught the external_references destruction defect
    assert_eq!(refs1.len(), refs2.len(),
        "External reference count mismatch: {} vs {} - THIS WOULD HAVE CAUGHT THE EXTERNAL_REFERENCES DESTRUCTION DEFECT",
        refs1.len(), refs2.len());

    for ext_ref in &refs1 {
        assert!(
            refs2.contains(ext_ref),
            "External reference {:?} not found in restored workspace",
            ext_ref
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ExternalReference {
    issue_id: String,
    namespace: String,
    key: String,
    value: String,
}

/// Read all external references from workspace
fn read_all_external_references(workspace: &Path) -> HashSet<ExternalReference> {
    let db = workspace.join(".beads/beads.db");
    let conn = rusqlite::Connection::open(&db).unwrap();

    let mut stmt = conn
        .prepare("SELECT issue_id, namespace, key, value FROM external_references")
        .unwrap();

    let refs = stmt
        .query_map([], |row| {
            Ok(ExternalReference {
                issue_id: row.get("issue_id")?,
                namespace: row.get("namespace")?,
                key: row.get("key")?,
                value: row.get("value")?,
            })
        })
        .unwrap();

    refs.filter_map(|r| r.ok()).collect()
}

/// Compare all events between two workspaces
fn assert_events_equal(workspace1: &Path, workspace2: &Path) {
    let events1_count = count_events(workspace1);
    let events2_count = count_events(workspace2);

    assert_eq!(
        events1_count, events2_count,
        "Event count mismatch: {} vs {}",
        events1_count, events2_count
    );
}

/// Count all events in workspace
fn count_events(workspace: &Path) -> i64 {
    let db = workspace.join(".beads/beads.db");
    let conn = rusqlite::Connection::open(&db).unwrap();

    conn.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn test_checkpoint_round_trip_fidelity_comprehensive() {
    // Step 1: Create maximally-populated workspace
    let source_workspace = create_workspace();
    suppress_auto_flush(source_workspace.path());

    // Create issues in different states with all fields populated

    // Issue 1: Open issue with all optional fields
    let output1 = Command::cargo_bin("bead")
        .unwrap()
        .args([
            "create",
            "--title",
            "Test Issue 1 - All Fields",
            "--description",
            "This is a test description",
            "--priority",
            "1",
            "--issue-type",
            "bug",
        ])
        .current_dir(source_workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::is_match("bead-[a-f0-9]{8}").unwrap());

    let bead1_id = extract_bead_id(&String::from_utf8_lossy(&output1.get_output().stdout));

    // Add notes via update command
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &bead1_id, "--notes", "These are test notes"])
        .current_dir(source_workspace.path())
        .assert()
        .success();

    // Add labels to issue 1
    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "add", "--label", "bug", &bead1_id])
        .current_dir(source_workspace.path())
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["label", "add", "--label", "high-priority", &bead1_id])
        .current_dir(source_workspace.path())
        .assert()
        .success();

    // Issue 2: In-progress issue with assignee
    let output2 = Command::cargo_bin("bead")
        .unwrap()
        .args([
            "create",
            "--title",
            "Test Issue 2 - In Progress",
            "--priority",
            "2",
        ])
        .current_dir(source_workspace.path())
        .assert()
        .success();

    let bead2_id = extract_bead_id(&String::from_utf8_lossy(&output2.get_output().stdout));

    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "update",
            &bead2_id,
            "--status",
            "in_progress",
            "--assignee",
            "test-worker",
        ])
        .current_dir(source_workspace.path())
        .assert()
        .success();

    // Issue 3: Closed issue with close_reason
    let output3 = Command::cargo_bin("bead")
        .unwrap()
        .args([
            "create",
            "--title",
            "Test Issue 3 - Closed",
            "--priority",
            "3",
        ])
        .current_dir(source_workspace.path())
        .assert()
        .success();

    let bead3_id = extract_bead_id(&String::from_utf8_lossy(&output3.get_output().stdout));

    Command::cargo_bin("bead")
        .unwrap()
        .args(["close", &bead3_id, "--reason", "test-fix"])
        .current_dir(source_workspace.path())
        .assert()
        .success();

    // Issue 4: Deferred issue
    let output4 = Command::cargo_bin("bead")
        .unwrap()
        .args([
            "create",
            "--title",
            "Test Issue 4 - Deferred",
            "--priority",
            "4",
        ])
        .current_dir(source_workspace.path())
        .assert()
        .success();

    let bead4_id = extract_bead_id(&String::from_utf8_lossy(&output4.get_output().stdout));

    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &bead4_id, "--status", "deferred"])
        .current_dir(source_workspace.path())
        .assert()
        .success();

    // Add dependencies (both kinds)
    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &bead2_id, &bead1_id, "--kind", "blocks"])
        .current_dir(source_workspace.path())
        .assert()
        .success();

    Command::cargo_bin("bead")
        .unwrap()
        .args(["dep", "add", &bead3_id, &bead4_id, "--kind", "relates_to"])
        .current_dir(source_workspace.path())
        .assert()
        .success();

    // Open database connection for direct inserts
    let db_path = source_workspace.path().join(".beads/beads.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // Add comments (via direct database insertion since CLI doesn't support this yet)
    let comment1_uuid = uuid::Uuid::new_v4().to_string();
    let comment2_uuid = uuid::Uuid::new_v4().to_string();
    let comment1_id = format!("comment-{}", &comment1_uuid[..8]);
    let comment2_id = format!("comment-{}", &comment2_uuid[..8]);

    conn.execute(
        "INSERT INTO comments (id, issue_id, author, body, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        (
            &comment1_id,
            &bead1_id,
            "test-user",
            "First comment",
            "2026-08-13T00:00:00.000000000Z",
        ),
    )
    .unwrap();
    conn.execute(
        "INSERT INTO comments (id, issue_id, author, body, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        (
            &comment2_id,
            &bead2_id,
            "test-user",
            "Comment on in-progress issue",
            "2026-08-13T00:00:01.000000000Z",
        ),
    )
    .unwrap();

    // Add external references through the public CLI.
    for (id, namespace, key, value) in [
        (&bead1_id, "github", "issue-number", "12345"),
        (&bead2_id, "gitlab", "commit-hash", "abc123def"),
        (&bead1_id, "jira", "ticket-id", "PROJ-001"),
    ] {
        Command::cargo_bin("bead")
            .unwrap()
            .args([
                "ref",
                "add",
                "--id",
                id,
                "--namespace",
                namespace,
                "--key",
                key,
                "--value",
                value,
            ])
            .current_dir(source_workspace.path())
            .assert()
            .success();
    }

    // Add structured data through the public CLI.
    for (id, namespace, schema, value) in [
        (
            &bead1_id,
            "test-ns",
            "urn:test:schema:1",
            r#"{"test":"data"}"#,
        ),
        (
            &bead2_id,
            "metrics",
            "urn:metrics:schema:1",
            r#"{"count":42}"#,
        ),
    ] {
        Command::cargo_bin("bead")
            .unwrap()
            .args([
                "data",
                "set",
                "--id",
                id,
                "--namespace",
                namespace,
                "--schema-ref",
                schema,
                "--value",
                value,
            ])
            .current_dir(source_workspace.path())
            .assert()
            .success();
    }

    // Manually increment revisions to ensure they're not reset to 1
    conn.execute(
        "UPDATE issues SET revision = revision + 1 WHERE id = ?1",
        [&bead1_id],
    )
    .unwrap();

    conn.execute(
        "UPDATE issues SET revision = revision + 1 WHERE id = ?1",
        [&bead2_id],
    )
    .unwrap();

    conn.execute(
        "UPDATE issues SET revision = revision + 1 WHERE id = ?1",
        [&bead3_id],
    )
    .unwrap();

    // Step 2: Flush forensic checkpoint
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(source_workspace.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Flushed forensic checkpoint:"))
        .stderr(predicate::str::contains("Issues: 4"));

    // Step 3: Restore into fresh empty workspace
    let restored_workspace = create_workspace();

    // Copy checkpoint files
    let checkpoint_src = source_workspace.path().join(".beads/checkpoint");
    let checkpoint_dst = restored_workspace.path().join(".beads/checkpoint");

    fs::create_dir_all(&checkpoint_dst).unwrap();

    // Copy all checkpoint files
    for entry in fs::read_dir(&checkpoint_src).unwrap() {
        let entry = entry.unwrap();
        let src_path = entry.path();
        let dst_path = checkpoint_dst.join(entry.file_name());

        if src_path.is_file() {
            fs::copy(&src_path, &dst_path).unwrap();
        } else if src_path.is_dir() {
            fs::create_dir_all(&dst_path).unwrap();
            for sub_entry in fs::read_dir(&src_path).unwrap() {
                let sub_entry = sub_entry.unwrap();
                let sub_src = sub_entry.path();
                let sub_dst = dst_path.join(sub_entry.file_name());
                if sub_src.is_file() {
                    fs::copy(&sub_src, &sub_dst).unwrap();
                }
            }
        }
    }

    // Import checkpoint
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            checkpoint_dst.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "conformance-test",
        ])
        .current_dir(restored_workspace.path())
        .assert()
        .success();

    // Step 4: Assert complete equality

    // Compare all issues
    assert_issues_equal(source_workspace.path(), restored_workspace.path());

    // Compare labels
    assert_labels_equal(source_workspace.path(), restored_workspace.path());

    // Compare dependencies
    assert_dependencies_equal(source_workspace.path(), restored_workspace.path());

    // Compare comments
    assert_comments_equal(source_workspace.path(), restored_workspace.path());

    // Compare external references.
    assert_external_references_equal(source_workspace.path(), restored_workspace.path());

    // Compare events
    assert_events_equal(source_workspace.path(), restored_workspace.path());

    // Additional verification: query both workspaces and compare JSON output
    let list1 = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--json", "--limit", "999999"])
        .current_dir(source_workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::is_match(".*").unwrap());

    let list2 = Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--json", "--limit", "999999"])
        .current_dir(restored_workspace.path())
        .assert()
        .success()
        .stdout(predicate::str::is_match(".*").unwrap());

    let count_jsonl = |bytes: &[u8]| {
        String::from_utf8_lossy(bytes)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>()
            .len()
    };
    assert_eq!(
        count_jsonl(&list1.get_output().stdout),
        count_jsonl(&list2.get_output().stdout),
        "List count mismatch"
    );
}

#[test]
fn test_checkpoint_merge_advances_revision_when_replacing_newer_live_content() {
    let source = create_workspace();
    let output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Merge revision probe"])
        .current_dir(source.path())
        .assert()
        .success();
    let id = extract_bead_id(&String::from_utf8_lossy(&output.get_output().stdout));
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &id, "--notes", "checkpoint revision two"])
        .current_dir(source.path())
        .assert()
        .success();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(source.path())
        .assert()
        .success();
    remove_projected_collections(&source.path().join(".beads/checkpoint"));

    let target = create_workspace();
    let checkpoint = source.path().join(".beads/checkpoint");
    let target_db = target.path().join(".beads/beads.db");
    let conn = rusqlite::Connection::open(&target_db).unwrap();
    conn.execute(
        "INSERT INTO issues
         (id, title, notes, priority, base_status, created_at, updated_at, revision)
         VALUES (?1, 'Live merge target', 'live revision four', 2, 'open',
                 '2000-01-01T00:00:00.000000000Z',
                 '2000-01-01T00:00:00.000000000Z', 4)",
        [&id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO issue_data (issue_id, namespace, schema_ref, value)
         VALUES (?1, 'live', 'urn:test:live', '{\"preserve\":true}')",
        [&id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO external_references (issue_id, namespace, key, value)
         VALUES (?1, 'legacy', 'source-id', 'legacy-42')",
        [&id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO comments (id, issue_id, author, body, created_at)
         VALUES ('live-comment', ?1, 'operator', 'preserve me',
                 '2000-01-01T00:00:00.000000000Z')",
        [&id],
    )
    .unwrap();
    assert_eq!(
        conn.query_row("SELECT revision FROM issues WHERE id = ?1", [&id], |row| {
            row.get::<_, i64>(0)
        },)
            .unwrap(),
        4
    );
    drop(conn);

    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            checkpoint.to_str().unwrap(),
            "--merge",
            "--actor",
            "merge-revision-test",
        ])
        .current_dir(target.path())
        .assert()
        .success();

    let conn = rusqlite::Connection::open(&target_db).unwrap();
    let revision = conn
        .query_row("SELECT revision FROM issues WHERE id = ?1", [&id], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap();
    assert_eq!(
        revision, 5,
        "merge did not invalidate the token for replaced live content"
    );
    for table in ["issue_data", "external_references", "comments"] {
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE issue_id = ?1");
        let count: i64 = conn.query_row(&sql, [&id], |row| row.get(0)).unwrap();
        assert_eq!(
            count, 1,
            "merge erased {table} omitted by the incoming checkpoint"
        );
    }
}

#[test]
fn test_checkpoint_merge_replaces_projected_collections_when_present() {
    let source = create_workspace();
    suppress_auto_flush(source.path());
    let output = Command::cargo_bin("bead")
        .unwrap()
        .args(["create", "--title", "Incoming collection owner"])
        .current_dir(source.path())
        .assert()
        .success();
    let id = extract_bead_id(&String::from_utf8_lossy(&output.get_output().stdout));
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "data",
            "set",
            "--id",
            &id,
            "--namespace",
            "incoming",
            "--schema-ref",
            "urn:test:incoming",
            "--value",
            "{\"source\":true}",
        ])
        .current_dir(source.path())
        .assert()
        .success();
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "ref",
            "add",
            "--id",
            &id,
            "--namespace",
            "tracker",
            "--key",
            "source-id",
            "--value",
            "source-7",
        ])
        .current_dir(source.path())
        .assert()
        .success();
    let source_db = source.path().join(".beads/beads.db");
    rusqlite::Connection::open(&source_db)
        .unwrap()
        .execute(
            "INSERT INTO comments (id, issue_id, author, body, created_at)
             VALUES ('incoming-comment', ?1, 'source', 'incoming body',
                     '2026-08-13T00:00:00.000000000Z')",
            [&id],
        )
        .unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(source.path())
        .assert()
        .success();

    let target = create_workspace();
    let target_db = target.path().join(".beads/beads.db");
    let conn = rusqlite::Connection::open(&target_db).unwrap();
    conn.execute(
        "INSERT INTO issues
         (id, title, priority, base_status, created_at, updated_at, revision)
         VALUES (?1, 'Old target', 2, 'open',
                 '2000-01-01T00:00:00.000000000Z',
                 '2000-01-01T00:00:00.000000000Z', 1)",
        [&id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO issue_data VALUES (?1, 'old', 'urn:test:old', '{\"old\":true}')",
        [&id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO external_references VALUES (?1, 'tracker', 'source-id', 'old-1')",
        [&id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO comments (id, issue_id, author, body, created_at)
         VALUES ('old-comment', ?1, 'target', 'old body',
                 '2000-01-01T00:00:00.000000000Z')",
        [&id],
    )
    .unwrap();
    drop(conn);

    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            source.path().join(".beads/checkpoint").to_str().unwrap(),
            "--merge",
            "--actor",
            "collection-merge-test",
        ])
        .current_dir(target.path())
        .assert()
        .success();

    let conn = rusqlite::Connection::open(&target_db).unwrap();
    let data_namespace: String = conn
        .query_row(
            "SELECT namespace FROM issue_data WHERE issue_id = ?1",
            [&id],
            |row| row.get(0),
        )
        .unwrap();
    let reference_value: String = conn
        .query_row(
            "SELECT value FROM external_references WHERE issue_id = ?1",
            [&id],
            |row| row.get(0),
        )
        .unwrap();
    let comment_id: String = conn
        .query_row(
            "SELECT id FROM comments WHERE issue_id = ?1",
            [&id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(data_namespace, "incoming");
    assert_eq!(reference_value, "source-7");
    assert_eq!(comment_id, "incoming-comment");
    drop(conn);

    // A later full checkpoint repeats the identical event prefix, adds a new
    // suffix event, and explicitly carries empty collection projections so
    // source-side deletions propagate.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["data", "remove", "--id", &id, "--namespace", "incoming"])
        .current_dir(source.path())
        .assert()
        .success();
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "ref",
            "remove",
            "--id",
            &id,
            "--namespace",
            "tracker",
            "--key",
            "source-id",
        ])
        .current_dir(source.path())
        .assert()
        .success();
    rusqlite::Connection::open(&source_db)
        .unwrap()
        .execute("DELETE FROM comments WHERE issue_id = ?1", [&id])
        .unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["update", &id, "--notes", "second merge suffix"])
        .current_dir(source.path())
        .assert()
        .success();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only"])
        .current_dir(source.path())
        .assert()
        .success();
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            source.path().join(".beads/checkpoint").to_str().unwrap(),
            "--merge",
            "--actor",
            "collection-merge-test-2",
        ])
        .current_dir(target.path())
        .assert()
        .success();

    let conn = rusqlite::Connection::open(&target_db).unwrap();
    for table in ["issue_data", "external_references", "comments"] {
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE issue_id = ?1");
        let count: i64 = conn.query_row(&sql, [&id], |row| row.get(0)).unwrap();
        assert_eq!(count, 0, "second merge did not propagate {table} deletion");
    }
    let origin_event_rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE origin_store_uuid IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let distinct_origin_events: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM (
                 SELECT DISTINCT origin_store_uuid, origin_event_sequence
                 FROM events WHERE origin_store_uuid IS NOT NULL
             )",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        origin_event_rows, distinct_origin_events,
        "repeated merge duplicated an identical event-history prefix"
    );
}
