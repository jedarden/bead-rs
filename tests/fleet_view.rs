use bead_rs::fleet::{run, FleetAction, FleetOptions};
use bead_rs::store::identity::{FORMAT_UUID, FORMAT_VERSION};
use rusqlite::{params, Connection};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

fn make_store(path: &Path, uuid: &str, wal: bool) -> Connection {
    let beads = path.join(".beads");
    fs::create_dir_all(&beads).unwrap();
    fs::write(beads.join("config.json"), format!(r#"{{"uuid":"{uuid}"}}"#)).unwrap();
    let conn = Connection::open(beads.join("beads.db")).unwrap();
    if wal {
        let mode: String = conn
            .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
    }
    conn.execute_batch(
        "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY);
         CREATE TABLE workspace(id INTEGER PRIMARY KEY, uuid TEXT NOT NULL, format_uuid TEXT, format_version INTEGER);
         CREATE TABLE issues(id TEXT PRIMARY KEY, title TEXT NOT NULL, description TEXT, notes TEXT,
             priority INTEGER NOT NULL, base_status TEXT NOT NULL, manual_blocked INTEGER, assignee TEXT,
             created_at TEXT NOT NULL);
         CREATE TABLE dependencies(blocked_issue_id TEXT, blocker_issue_id TEXT, kind TEXT);
         CREATE TABLE issue_resource_keys(issue_id TEXT, resource_key TEXT);
         CREATE TABLE resource_locks(resource_key TEXT, issue_id TEXT, lease_fencing_token INTEGER);
         CREATE TABLE leases(issue_id TEXT, fencing_token INTEGER, expires_at TEXT);
         CREATE TABLE events(sequence INTEGER PRIMARY KEY, issue_id TEXT, kind TEXT, time TEXT, detail TEXT);",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO workspace(id, uuid, format_uuid, format_version) VALUES (1, ?1, ?2, ?3)",
        params![uuid, FORMAT_UUID, FORMAT_VERSION],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO issues(id,title,description,notes,priority,base_status,manual_blocked,assignee,created_at)
         VALUES ('x-1','Literal % item','search target',NULL,1,'open',0,NULL,'2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO issues(id,title,description,notes,priority,base_status,manual_blocked,assignee,created_at)
         VALUES ('x-2','Blocked item',NULL,NULL,2,'open',1,NULL,'2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute_batch(
        r#"INSERT INTO issues(id,title,description,notes,priority,base_status,manual_blocked,assignee,created_at)
             VALUES ('x-3','Stale claim',NULL,NULL,2,'in_progress',0,'worker','2025-01-01T00:00:00Z');
         INSERT INTO issues(id,title,description,notes,priority,base_status,manual_blocked,assignee,created_at)
             VALUES ('x-4','Leased claim',NULL,NULL,2,'in_progress',0,'worker','2025-01-01T00:00:00Z');
         INSERT INTO events(sequence,issue_id,kind,time,detail)
             VALUES (1,'x-3','claimed','2025-01-01T00:00:00Z','{}');
         INSERT INTO events(sequence,issue_id,kind,time,detail)
             VALUES (2,'x-4','claimed','2025-01-01T00:00:00Z','{"with_lease":true}');"#,
    )
    .unwrap();
    conn
}

fn options(root: &Path, include_copies: bool) -> FleetOptions {
    FleetOptions {
        root: root.to_path_buf(),
        max_depth: 1,
        include_copies,
    }
}

fn paths(records: &Value) -> Vec<String> {
    records
        .as_array()
        .unwrap()
        .iter()
        .map(|record| record["path"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn groups_copies_and_reports_skips_and_errors() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let primary = root.join("project");
    let copy = root.join("project-verify");
    let dot_copy = root.join(".scratch");
    let _primary_conn = make_store(&primary, "same-uuid", false);
    let _copy_conn = make_store(&copy, "same-uuid", false);
    let _dot_conn = make_store(&dot_copy, "same-uuid", false);

    let bf = root.join("bf/.beads");
    fs::create_dir_all(&bf).unwrap();
    fs::write(bf.join("config.yaml"), "backend: bf\n").unwrap();
    let unknown = root.join("unknown/.beads");
    fs::create_dir_all(&unknown).unwrap();
    let uninit = root.join("uninit/.beads");
    fs::create_dir_all(&uninit).unwrap();
    fs::write(uninit.join("config.json"), r#"{"uuid":"fresh"}"#).unwrap();
    let foreign = root.join("foreign/.beads");
    fs::create_dir_all(&foreign).unwrap();
    fs::write(foreign.join("config.json"), r#"{"uuid":"foreign"}"#).unwrap();
    let foreign_conn = Connection::open(foreign.join("beads.db")).unwrap();
    foreign_conn
        .execute_batch("CREATE TABLE alien(id INTEGER);")
        .unwrap();
    let corrupt = root.join("corrupt/.beads");
    fs::create_dir_all(&corrupt).unwrap();
    fs::write(corrupt.join("config.json"), r#"{"uuid":"corrupt"}"#).unwrap();
    fs::write(corrupt.join("beads.db"), b"not a database").unwrap();

    let before = all_bead_files(root);
    let status = run(FleetAction::Status, options(root, false)).unwrap();
    assert_eq!(
        paths(&status["workspaces"]),
        vec![primary.to_string_lossy()]
    );
    assert_eq!(
        paths(&status["copies"]),
        vec![dot_copy.to_string_lossy(), copy.to_string_lossy()]
    );
    assert_eq!(status["copies"][0]["sequence"], 1);
    assert_eq!(status["copies"][1]["sequence"], 2);
    assert_eq!(status["copies"][0]["selected"], false);
    assert_eq!(status["totals"]["ready_count"], 1);
    assert_eq!(status["totals"]["counts"]["open"], 2);
    assert_eq!(status["totals"]["counts"]["in_progress"], 2);
    assert_eq!(status["totals"]["stale_in_progress_count"], 1);
    let skipped = status["skipped"].as_array().unwrap();
    assert!(skipped.iter().any(|item| item["reason"] == "bf_store"));
    assert!(skipped.iter().any(|item| item["reason"] == "unrecognized"));
    assert!(skipped.iter().any(|item| item["reason"] == "uninitialized"));
    assert!(skipped
        .iter()
        .any(|item| item["reason"] == "foreign_schema"));
    assert_eq!(
        paths(&status["errors"]),
        vec![root.join("corrupt").to_string_lossy()]
    );

    let with_copies = run(FleetAction::Status, options(root, true)).unwrap();
    assert_eq!(with_copies["totals"]["ready_count"], 3);
    assert_eq!(with_copies["copies"][0]["selected"], true);
    assert_eq!(all_bead_files(root), before);
}

fn all_bead_files(root: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    fn visit(path: &Path, files: &mut Vec<(PathBuf, String)>) {
        let mut entries = fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        entries.sort();
        for entry in entries {
            if entry.is_dir() && entry.file_name().unwrap() == ".beads" {
                files.extend(bead_files(&entry));
            } else if entry.is_dir() {
                visit(&entry, files);
            }
        }
    }
    visit(root, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

fn bead_files(beads: &Path) -> Vec<(PathBuf, String)> {
    let mut result = Vec::new();
    collect_files(beads, &mut result);
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

fn collect_files(dir: &Path, result: &mut Vec<(PathBuf, String)>) {
    let mut entries = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_file() {
            let digest = Sha256::digest(fs::read(&path).unwrap());
            result.push((path, hex::encode(digest)));
        } else if path.is_dir() {
            collect_files(&path, result);
        }
    }
}

#[test]
fn reads_uncheckpointed_wal_without_changing_source_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let workspace = root.join("wal");
    let conn = make_store(&workspace, "wal-uuid", true);
    // Keep the writer connection open so the issue rows exist only in the WAL.
    let beads = workspace.join(".beads");
    fs::create_dir_all(beads.join("nested")).unwrap();
    fs::write(beads.join("nested/keep.txt"), "keep this file").unwrap();
    assert!(beads.join("beads.db-wal").exists());
    let before = bead_files(&beads);

    let ready = run(FleetAction::Ready, options(root, false)).unwrap();
    assert_eq!(ready["totals"]["ready_count"], 1, "{ready}");
    assert_eq!(ready["ready"][0]["id"], "x-1");
    assert_eq!(bead_files(&beads), before);

    let search = run(FleetAction::Search("%".into()), options(root, false)).unwrap();
    assert_eq!(search["matches"].as_array().unwrap().len(), 1);
    assert_eq!(search["matches"][0]["id"], "x-1");
    assert_eq!(bead_files(&beads), before);

    let doctor = run(FleetAction::Doctor, options(root, false)).unwrap();
    assert_eq!(doctor["workspaces"][0]["doctor"]["healthy"], true);
    assert_eq!(bead_files(&beads), before);
    drop(conn);
}
