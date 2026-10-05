//! Independent synthetic regression fixtures for the owner-directed gap closure.
use assert_cmd::Command;
use bead_rs::scan::{self, Field, ScanConfig};
use bead_rs::service::{issues, redaction, secret_diagnostics};
use bead_rs::store::{open_configured_connection, SqliteStore, WorkspaceConfig};
use rusqlite::types::Value as SqlValue;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn bead(root: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command.current_dir(root);
    command
}
fn workspace() -> (tempfile::TempDir, SqliteStore) {
    let root = tempfile::Builder::new()
        .prefix("bead-secret-gaps-")
        .tempdir_in("/var/tmp")
        .unwrap();
    bead(root.path())
        .args([
            "init",
            "--skip-foreign-workspace",
            "--prefix",
            "gap",
            "--no-auto-flush",
        ])
        .assert()
        .success();
    let store = SqliteStore::from_conn(
        open_configured_connection(&root.path().join(".beads/beads.db")).unwrap(),
    );
    (root, store)
}
fn provider() -> String {
    ["AK", "IA", "7Q9W2E4R6T8Y1U3I"].concat()
}
fn mixed(length: usize) -> String {
    let alphabet = b"a1B2c3D4e5F6g7H8j9K0m1N2p3Q4r5S6t7U8v9W0x1Y2z3A4";
    (0..length)
        .map(|index| alphabet[(index * 7 + 3) % alphabet.len()] as char)
        .collect()
}
fn base64(text: &str) -> String {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::new();
    let mut accumulator = 0u32;
    let mut bits = 0;
    for byte in text.bytes() {
        accumulator = (accumulator << 8) | byte as u32;
        bits += 8;
        while bits >= 6 {
            bits -= 6;
            encoded.push(alphabet[((accumulator >> bits) & 63) as usize] as char);
        }
    }
    if bits != 0 {
        encoded.push(alphabet[((accumulator << (6 - bits)) & 63) as usize] as char);
    }
    while encoded.len() % 4 != 0 {
        encoded.push('=');
    }
    encoded
}
fn insert(conn: &rusqlite::Connection, description: &str) {
    conn.execute("INSERT INTO issues (id,title,description,priority,issue_type,base_status,created_at,updated_at,revision)
        VALUES ('gap-1','stable title',?1,2,'task','open','2026-10-03T00:00:00Z','2026-10-03T00:00:00Z',1)",[description]).unwrap();
}
fn fingerprints(conn: &rusqlite::Connection) -> Vec<String> {
    secret_diagnostics::scan_live_findings(conn)
        .unwrap()
        .into_iter()
        .filter(|finding| finding.rule_id == "aws-access-key-id" && finding.is_blocking_match())
        .map(|finding| finding.fingerprint)
        .collect()
}
fn stored(conn: &rusqlite::Connection) -> String {
    conn.query_row(
        "SELECT description FROM issues WHERE id='gap-1'",
        [],
        |row| row.get(0),
    )
    .unwrap()
}

fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&source_path, &target_path);
        } else {
            fs::copy(source_path, target_path).unwrap();
        }
    }
}

fn isolated_workspace_root(prefix: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in("/var/tmp")
        .unwrap()
}

fn recovery_snapshot(conn: &rusqlite::Connection) -> (String, i64, i64, i64, i64, i64) {
    conn.query_row(
        "SELECT (SELECT uuid FROM workspace),
                (SELECT COUNT(*) FROM issues),
                (SELECT COUNT(*) FROM events),
                (SELECT COUNT(*) FROM provenance_receipts),
                (SELECT COUNT(*) FROM checkpoint_state),
                (SELECT COUNT(*) FROM secret_quarantine)",
        [],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        },
    )
    .unwrap()
}

fn fail_quarantine_insert(conn: &rusqlite::Connection) {
    conn.execute_batch(
        "CREATE TRIGGER fail_quarantine_insert
         BEFORE INSERT ON secret_quarantine
         BEGIN
             SELECT RAISE(ABORT, 'synthetic quarantine persistence failure');
         END;",
    )
    .unwrap();
}

fn recovery_state_snapshot(conn: &rusqlite::Connection) -> Vec<(String, Vec<Vec<SqlValue>>)> {
    let tables = conn
        .prepare(
            "SELECT name FROM sqlite_schema
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    tables
        .into_iter()
        .map(|table| {
            let sql = format!("SELECT * FROM \"{table}\" ORDER BY rowid");
            let mut statement = conn.prepare(&sql).unwrap();
            let column_count = statement.column_count();
            let rows = statement
                .query_map([], |row| {
                    (0..column_count)
                        .map(|index| row.get::<_, SqlValue>(index))
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            (table, rows)
        })
        .collect()
}

fn checkpoint_state_snapshot(checkpoint: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn collect_files(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                collect_files(root, &path, files);
            } else if entry.file_type().unwrap().is_file() {
                files.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }

    let mut files = BTreeMap::new();
    collect_files(checkpoint, checkpoint, &mut files);
    files
}

fn set_auto_flush(root: &Path, enabled: bool) {
    let path = root.join(".beads/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["checkpoint"]["auto_flush"] = Value::Bool(enabled);
    fs::write(&path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
}

fn retained_secret_reconcile_pair() -> (tempfile::TempDir, tempfile::TempDir, String) {
    let source_root = isolated_workspace_root("bead-secret-reconcile-retained-");
    bead(source_root.path())
        .args(["init", "--prefix", "gap"])
        .assert()
        .success();
    bead(source_root.path())
        .args(["create", "--title", "clean clone base"])
        .assert()
        .success();
    bead(source_root.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let generation = serde_json::from_slice::<Value>(
        &fs::read(source_root.path().join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap()["generation_id"]
        .as_str()
        .unwrap()
        .to_string();

    let target_root = isolated_workspace_root("bead-secret-reconcile-retained-target-");
    fs::create_dir_all(target_root.path().join(".beads")).unwrap();
    fs::copy(
        source_root.path().join(".beads/config.json"),
        target_root.path().join(".beads/config.json"),
    )
    .unwrap();
    copy_tree(
        &source_root.path().join(".beads/checkpoint"),
        &target_root.path().join(".beads/checkpoint"),
    );
    bead(target_root.path()).args(["init"]).assert().success();
    bead(target_root.path())
        .args([
            "restore",
            "--source",
            ".beads/checkpoint",
            "--generation",
            &generation,
            "--actor",
            "clone-operator",
        ])
        .assert()
        .success();

    let value = provider();
    let mut target_store = SqliteStore::from_conn(
        open_configured_connection(&target_root.path().join(".beads/beads.db")).unwrap(),
    );
    let issue_id: String = target_store
        .conn()
        .query_row("SELECT id FROM issues ORDER BY id LIMIT 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    target_store
        .conn()
        .execute(
            "UPDATE issues SET description = ?1, revision = revision + 1",
            [&value],
        )
        .unwrap();
    target_store
        .conn()
        .execute(
            "INSERT INTO events (issue_id, kind, actor, time, detail)
             VALUES (?1, 'fixture_secret_revision', 'fixture',
                     '2026-10-03T00:01:00Z', '{}')",
            [&issue_id],
        )
        .unwrap();
    bead(target_root.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();

    target_store
        .conn()
        .execute(
            "UPDATE issues SET description = 'clean current description', revision = revision + 1",
            [],
        )
        .unwrap();
    target_store
        .conn()
        .execute(
            "INSERT INTO events (issue_id, kind, actor, time, detail)
             VALUES (?1, 'fixture_clean_revision', 'fixture',
                     '2026-10-03T00:02:00Z', '{}')",
            [&issue_id],
        )
        .unwrap();
    bead(target_root.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();
    drop(target_store);

    let source_checkpoint = source_root.path().join(".beads/checkpoint");
    fs::remove_dir_all(&source_checkpoint).unwrap();
    copy_tree(
        &target_root.path().join(".beads/checkpoint"),
        &source_checkpoint,
    );
    (source_root, target_root, value)
}

fn published_secret_generation(prefix: &str) -> (tempfile::TempDir, String) {
    let source_root = isolated_workspace_root(prefix);
    bead(source_root.path())
        .args(["init", "--prefix", "gap"])
        .assert()
        .success();
    bead(source_root.path())
        .args(["create", "--title", "clean recovery base"])
        .assert()
        .success();
    bead(source_root.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();

    let mut source_store = SqliteStore::from_conn(
        open_configured_connection(&source_root.path().join(".beads/beads.db")).unwrap(),
    );
    let value = provider();
    insert(source_store.conn(), &value);
    source_store
        .conn()
        .execute(
            "INSERT INTO events (issue_id, kind, actor, time, detail)
             VALUES ('gap-1', 'legacy_fixture', 'fixture',
                     '2026-10-03T00:02:00Z', '{}')",
            [],
        )
        .unwrap();
    let output = bead(source_root.path())
        .args(["sync", "flush-only"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains(&value));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(&value));
    let generation = serde_json::from_slice::<Value>(
        &fs::read(source_root.path().join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap()["generation_id"]
        .as_str()
        .unwrap()
        .to_string();
    (source_root, generation)
}

#[test]
fn direct_services_reject_without_cli_and_without_audit_side_effects() {
    let (root, mut store) = workspace();
    let config = WorkspaceConfig {
        root: root.path().to_path_buf(),
        uuid: store
            .conn()
            .query_row("SELECT uuid FROM workspace", [], |row| row.get(0))
            .unwrap(),
        prefix: "gap".to_string(),
    };
    let value = provider();
    assert!(issues::create_issue(
        store.conn(),
        &config,
        "safe title".to_string(),
        Some(value.clone()),
        2,
        None,
        None,
        vec![],
        vec![]
    )
    .is_err());
    insert(store.conn(), "clean");
    assert!(issues::add_comment(store.conn(), "gap-1", &value, "tester").is_err());
    assert!(bead_rs::service::dependencies::add_label(&mut store, "gap-1", &value).is_err());
    assert!(bead_rs::service::data::set_data(
        &mut store,
        "gap-1",
        "test",
        "urn:test:data",
        &serde_json::json!({"credential":value})
    )
    .is_err());
    assert!(bead_rs::service::query::save_view(store.conn(), "safe", &value, "{}").is_err());
    let events: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(events, 0);
    assert_eq!(stored(store.conn()), "clean");
}

#[test]
fn redaction_lock_proof_cannot_be_used_with_another_live_workspace() {
    let (root, _store) = workspace();
    let (_other_root, mut other) = workspace();
    insert(other.conn(), &provider());
    let selected = fingerprints(other.conn());
    let locks = redaction::acquire_redaction_locks(root.path()).unwrap();
    assert!(redaction::redact_finding_holding(
        &mut other,
        &locks,
        &selected[0],
        "tester",
        "wrong workspace"
    )
    .is_err());
    assert!(redaction::preview_redaction_holding(
        &mut other,
        &locks,
        &selected[0],
        "tester",
        "wrong workspace"
    )
    .is_err());
    assert!(redaction::redact_findings_holding(
        &mut other,
        &locks,
        &selected,
        "tester",
        "wrong workspace",
        false
    )
    .is_err());
    let receipts: i64 = other
        .conn()
        .query_row("SELECT COUNT(*) FROM redaction_receipts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(receipts, 0);
    assert_eq!(fingerprints(other.conn()), selected);
}

#[test]
fn same_field_batch_advances_once_and_publishes_one_resumable_epoch() {
    let (root, mut store) = workspace();
    let value = provider();
    insert(store.conn(), &format!("left {value} middle {value} right"));
    let fingerprints = fingerprints(store.conn());
    assert_eq!(fingerprints.len(), 2);
    let locks = redaction::acquire_redaction_locks(root.path()).unwrap();
    let outcomes = redaction::redact_findings_holding(
        &mut store,
        &locks,
        &fingerprints,
        "tester",
        "synthetic batch cleanup",
        false,
    )
    .unwrap();
    assert_eq!(outcomes.len(), 2);
    assert!(outcomes
        .iter()
        .all(|outcome| outcome.receipt.affected_issue_revision == Some(2)
            && outcome.receipt.epoch_id == outcomes[0].receipt.epoch_id));
    assert_eq!(
        stored(store.conn()),
        "left [REDACTED:bead-rs] middle [REDACTED:bead-rs] right"
    );
    let replay = redaction::redact_findings_holding(
        &mut store,
        &locks,
        &fingerprints,
        "tester",
        "synthetic batch cleanup",
        false,
    )
    .unwrap();
    assert!(replay.iter().all(|outcome| outcome.is_replay));
    assert!(redaction::redact_findings_holding(
        &mut store,
        &locks,
        &fingerprints[..1],
        "tester",
        "synthetic batch cleanup",
        false
    )
    .is_err());
    drop(locks);
    drop(store);
    bead(root.path())
        .args([
            "redact",
            "--resume",
            &outcomes[1].receipt.receipt_id,
            "--json",
        ])
        .assert()
        .success();
    let partial = bead(root.path())
        .args([
            "redact",
            "--finding",
            &fingerprints[0],
            "--actor",
            "tester",
            "--reason",
            "synthetic batch cleanup",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(!partial.status.success());
    let conn = open_configured_connection(&root.path().join(".beads/beads.db")).unwrap();
    let epochs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM redaction_epochs WHERE publication_state='published'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let receipts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM redaction_receipts WHERE publication_state='published'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!((epochs, receipts), (1, 2));
}

#[test]
fn stale_duplicate_and_overlapping_batches_leave_every_record_unchanged() {
    let (root, mut store) = workspace();
    let value = provider();
    insert(store.conn(), &format!("token={value}"));
    let findings = secret_diagnostics::scan_live_findings(store.conn()).unwrap();
    let fingerprint = fingerprints(store.conn()).remove(0);
    let before = stored(store.conn());
    let locks = redaction::acquire_redaction_locks(root.path()).unwrap();
    for selected in [
        vec![fingerprint.clone(), "0".repeat(64)],
        vec![fingerprint.clone(), fingerprint.clone()],
    ] {
        assert!(redaction::redact_findings_holding(
            &mut store,
            &locks,
            &selected,
            "tester",
            "rollback fixture",
            false
        )
        .is_err());
        assert_eq!(stored(store.conn()), before);
    }
    let overlapping: Vec<_> = findings
        .iter()
        .filter(|finding| finding.is_blocking_match())
        .map(|finding| finding.fingerprint.clone())
        .collect();
    assert!(overlapping.len() >= 2);
    assert!(redaction::redact_findings_holding(
        &mut store,
        &locks,
        &overlapping,
        "tester",
        "overlap fixture",
        false
    )
    .is_err());
    let count: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM redaction_receipts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn repeated_cli_fingerprints_support_atomic_dry_run_and_commit() {
    let (root, mut store) = workspace();
    let value = provider();
    insert(store.conn(), &format!("{value} and {value}"));
    let fingerprints = fingerprints(store.conn());
    let before = stored(store.conn());
    let args = [
        "redact",
        "--finding",
        &fingerprints[0],
        "--finding",
        &fingerprints[1],
        "--actor",
        "tester",
        "--reason",
        "CLI batch fixture",
        "--json",
    ];
    bead(root.path())
        .args(args)
        .arg("--dry-run")
        .assert()
        .success();
    assert_eq!(stored(store.conn()), before);
    let output = bead(root.path()).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["receipts"].as_array().unwrap().len(), 2);
    assert!(report["receipts"]
        .as_array()
        .unwrap()
        .iter()
        .all(|receipt| receipt["publication_state"] == "published"));
}

#[test]
fn normalized_findings_point_to_the_raw_bytes_and_complete_pem_material() {
    let value = provider();
    let escaped = value
        .bytes()
        .map(|byte| format!("\\u{byte:04x}"))
        .collect::<String>();
    let percent = value
        .bytes()
        .map(|byte| format!("%{byte:02x}"))
        .collect::<String>();
    let ansi = format!("{}\u{1b}[31m{}", &value[..8], &value[8..]);
    let wrapped = format!("{}\\\n  {}", &value[..8], &value[8..]);
    let encoded = base64(&format!("header {value} footer"));
    for text in [escaped, percent, ansi, wrapped, encoded] {
        let report = scan::scan(
            &ScanConfig::enforce(),
            "fixture",
            &[Field::new("description", &text)],
        );
        let finding = report
            .blocking
            .iter()
            .find(|finding| finding.rule_id == "aws-access-key-id")
            .expect("normalized provider must block");
        assert!(text.get(finding.start..finding.end).is_some());
        assert_eq!(
            finding.fingerprint,
            scan::fingerprint::compute(
                scan::RULESET_VERSION,
                &finding.rule_id,
                "fixture",
                "description",
                finding.start,
                finding.end,
                &text.as_bytes()[finding.start..finding.end]
            )
        );
    }
    let pem = format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
        mixed(80)
    );
    let report = scan::scan(
        &ScanConfig::enforce(),
        "fixture",
        &[Field::new("description", &pem)],
    );
    let finding = report
        .blocking
        .iter()
        .find(|finding| finding.rule_id == "pem-private-key")
        .unwrap();
    assert_eq!((finding.start, finding.end), (0, pem.len()));
}

#[test]
fn structural_credentials_block_but_references_and_hashes_are_not_noisy() {
    let value = mixed(32);
    let encoded = base64(&value);
    let positive_cases = [
        (
            "credential-assignment",
            format!("clientSecret={value}"),
            value.clone(),
        ),
        (
            "uri-userinfo-credential",
            format!("postgres://worker:{value}@localhost/db"),
            value.clone(),
        ),
        (
            "authorization-header-credential",
            format!("Authorization: Bearer {value}"),
            value.clone(),
        ),
        (
            "authorization-header-credential",
            format!("bearer {value}"),
            value.clone(),
        ),
        (
            "curl-user-credential",
            format!("curl -u worker:{value}"),
            value.clone(),
        ),
        (
            "curl-user-credential",
            format!("curl -uworker:{value}"),
            value.clone(),
        ),
        (
            "kubernetes-secret-data",
            format!("kind: Secret\nstringData:\n  credential: {value}"),
            value.clone(),
        ),
        (
            "kubernetes-secret-data",
            format!("kind: Secret\nstringData: |\n  password: {value}\n"),
            value.clone(),
        ),
        (
            "kubernetes-secret-data",
            format!("data:\n  connection: {encoded}\nkind: Secret"),
            encoded.clone(),
        ),
        (
            "kubernetes-secret-data",
            format!("stringData:\n  connection: {value}\nkind: \"Secret\""),
            value.clone(),
        ),
        (
            "kubernetes-secret-data",
            serde_json::json!({"kind":"Secret","data":{"password":encoded}}).to_string(),
            encoded.clone(),
        ),
    ];
    for (rule, text, expected) in positive_cases {
        let report = scan::scan(
            &ScanConfig::enforce(),
            "fixture",
            &[Field::new("description", &text)],
        );
        let matches: Vec<_> = report
            .blocking
            .iter()
            .filter(|finding| finding.rule_id == rule)
            .collect();
        assert_eq!(matches.len(), 1, "unexpected structural matches for {rule}");
        let start = text.find(&expected).expect("fixture value is present");
        assert_eq!(
            (matches[0].start, matches[0].end),
            (start, start + expected.len()),
            "wrong structural range for {rule}"
        );
    }
    for text in [
        "Authorization: Bearer token".to_string(),
        "Authorization: Bearer ${BEARER_TOKEN}".to_string(),
        "curl -u worker:password".to_string(),
        "curl -uworker:${CREDENTIAL}".to_string(),
        format!("forbearer {value}"),
        "kind: Secret\nstringData: |\n  password: ${PASSWORD}\n".to_string(),
    ] {
        let report = scan::scan(
            &ScanConfig::enforce(),
            "fixture",
            &[Field::new("description", &text)],
        );
        assert!(
            report.blocking.is_empty(),
            "reference or single-word fixture was blocked: {text}"
        );
    }
    for text in [
        "diagnostic output: token precedes name=1234567890",
        "see secret-rotation-guide.md for the procedure",
    ] {
        let report = scan::scan(
            &ScanConfig::enforce(),
            "fixture",
            &[Field::new("description", text)],
        );
        assert!(
            report.blocking.is_empty(),
            "Git-layer false-positive parity fixture was blocked: {text}"
        );
    }
    let text="secret_path=kv/service/token\nSHA256 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nfencing_token=19\napi_key=your_api_key_here\nthis is explanatory prose about token rotation";
    let report = scan::scan(
        &ScanConfig::enforce(),
        "fixture",
        &[Field::new("description", text)],
    );
    assert!(report.blocking.is_empty());
    assert!(report
        .findings
        .iter()
        .all(|finding| finding.disposition != scan::Disposition::Confirmed));
}

#[test]
fn uri_password_percent_decoding_keeps_encoded_at_inside_userinfo() {
    let first = mixed(8);
    let second = mixed(8);
    let third = mixed(8);
    let password = format!("{first}%2F{second}%40{third}");
    let text = format!("postgres://worker:{password}@localhost/db");
    let report = scan::scan(
        &ScanConfig::enforce(),
        "fixture",
        &[Field::new("description", &text)],
    );
    let findings: Vec<_> = report
        .blocking
        .iter()
        .filter(|finding| finding.rule_id == "uri-userinfo-credential")
        .collect();
    assert_eq!(findings.len(), 1);
    assert_eq!(
        (findings[0].start, findings[0].end),
        (
            text.find(&password).unwrap(),
            text.find(&password).unwrap() + password.len()
        )
    );
}

#[test]
fn new_provider_inventory_has_positive_and_boundary_negative_coverage() {
    let hex: String = (0..64)
        .map(|index| b"a1b2c3d4e5f6"[(index * 7 + 3) % 12] as char)
        .collect();
    let header = base64(r#"{"alg":"HS256","typ":"JWT"}"#)
        .trim_end_matches('=')
        .to_string();
    let payload = base64(r#"{"sub":"synthetic-fixture","aud":"offline-test"}"#)
        .trim_end_matches('=')
        .to_string();
    let cases = [
        (
            "docker-hub-token",
            format!("{}{}", ["dckr", "_pat_"].concat(), mixed(32)),
        ),
        (
            "tailscale-key",
            format!("{}{}", ["tskey", "-api-123aBc-"].concat(), mixed(32)),
        ),
        (
            "vault-batch-token",
            format!("{}{}", ["hv", "b."].concat(), mixed(32)),
        ),
        (
            "vault-recovery-token",
            format!("{}{}", ["hv", "r."].concat(), mixed(32)),
        ),
        (
            "backblaze-application-key",
            format!("{}{}", ["K", "00"].concat(), mixed(28)),
        ),
        (
            "openai-api-key",
            format!("{}{}", ["sk", "-svcacct-"].concat(), mixed(40)),
        ),
        (
            "openrouter-api-key",
            format!("{}{}", ["sk", "-or-v1-"].concat(), hex),
        ),
        (
            "age-secret-key",
            format!(
                "{}{}",
                ["AGE", "-SECRET-KEY-1"].concat(),
                mixed(58).to_ascii_uppercase()
            ),
        ),
        (
            "json-web-token",
            format!("{header}.{payload}.{}", mixed(24)),
        ),
    ];
    for (rule, value) in cases {
        let report = scan::scan(
            &ScanConfig::enforce(),
            "fixture",
            &[Field::new("description", &value)],
        );
        assert!(
            report
                .blocking
                .iter()
                .any(|finding| finding.rule_id == rule),
            "missing provider {rule}"
        );
        for text in [format!("X{value}"), value[..value.len().min(5)].to_string()] {
            let report = scan::scan(
                &ScanConfig::enforce(),
                "fixture",
                &[Field::new("description", &text)],
            );
            assert!(
                report
                    .blocking
                    .iter()
                    .all(|finding| finding.rule_id != rule),
                "boundary near miss admitted by {rule}"
            );
        }
    }
    let legacy = format!("{}{}", ["s", "."].concat(), mixed(24));
    for (present, text) in [
        (true, format!("vault token {legacy}")),
        (false, legacy.clone()),
        (false, format!("vault\n{legacy}")),
    ] {
        let report = scan::scan(
            &ScanConfig::enforce(),
            "fixture",
            &[Field::new("description", &text)],
        );
        assert_eq!(
            report
                .blocking
                .iter()
                .any(|finding| finding.rule_id == "vault-legacy-token"),
            present
        );
    }
    let id = format!(
        "00{}",
        (0..23)
            .map(|index| b"a1b2c3d4e5f6"[(index * 7 + 3) % 12] as char)
            .collect::<String>()
    );
    for (present, text) in [
        (true, format!("storage_key_id={id}")),
        (false, id.clone()),
        (false, format!("object_id={id}")),
    ] {
        let report = scan::scan(
            &ScanConfig::enforce(),
            "fixture",
            &[Field::new("description", &text)],
        );
        assert_eq!(
            report
                .blocking
                .iter()
                .any(|finding| finding.rule_id == "backblaze-key-id-assignment"),
            present
        );
    }
}

#[test]
fn provider_tail_boundaries_use_each_formats_body_alphabet() {
    for (rule, candidate, delimiter) in [
        ("huggingface-token", format!("hf_{}", mixed(34)), '-'),
        (
            "github-fine-grained-pat",
            format!("github_pat_{}", mixed(82)),
            '+',
        ),
        (
            "backblaze-application-key",
            format!("K00{}", mixed(28)),
            '=',
        ),
        ("docker-hub-token", format!("dckr_pat_{}", mixed(28)), '+'),
    ] {
        let text = format!("{candidate}{delimiter}");
        let report = scan::scan(
            &ScanConfig::enforce(),
            "fixture",
            &[Field::new("description", &text)],
        );
        assert!(
            report
                .blocking
                .iter()
                .any(|finding| finding.rule_id == rule),
            "format-specific delimiter suppressed {rule}"
        );
    }
}

#[test]
fn recurrence_generated_text_is_scanned_before_materialization() {
    let (root, mut store) = workspace();
    let value = provider();
    store.conn().execute("INSERT INTO recurrence_templates (id,title,base_title_template,base_description,priority,issue_type,created_at)
        VALUES ('gap-series','imported template','Occurrence {n}',?1,2,'task','2026-10-03T00:00:00Z')",[&value]).unwrap();
    let result = bead_rs::service::recurrence::materialize_next_occurrence(
        store.conn_mut(),
        "gap-series",
        Some("tester"),
    );
    assert!(result.is_err());
    let count: i64 = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM recurrence_materializations",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    let output = bead(root.path())
        .args(["recurrence", "next", "gap-series"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains(&value));
}

#[test]
fn advisory_success_emits_one_counted_notice_and_additive_manifest_json() {
    let (root, _store) = workspace();
    let value = mixed(28);
    let input = root.path().join("manifest.json");
    std::fs::write(&input,serde_json::to_vec(&serde_json::json!({"manifest_version":1,"operations":[{"op":"create","title":"advisory fixture","description":value}]})).unwrap()).unwrap();
    let output = bead(root.path())
        .args([
            "manifest",
            "commit",
            "--input",
            "manifest.json",
            "--format",
            "json",
            "--no-auto-flush",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        stderr
            .lines()
            .filter(|line| line.starts_with("secret_scan advisory:"))
            .count(),
        1
    );
    assert!(stderr.contains("advisory-high-entropy-string"));
    assert!(!stderr.contains(&value));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!report["secret_scan"]["advisory_findings"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn unreadable_previous_generation_keeps_live_findings_and_reports_incomplete_coverage() {
    let (root, mut store) = workspace();
    insert(store.conn(), &provider());
    let checkpoint = root.path().join(".beads/checkpoint");
    std::fs::create_dir_all(&checkpoint).unwrap();
    std::fs::write(checkpoint.join("current.jsonl"), b"").unwrap();
    std::fs::write(checkpoint.join("previous.jsonl"), b"").unwrap();
    std::fs::write(
        checkpoint.join("current.json"),
        serde_json::to_vec(&serde_json::json!({
            "mode": "monolithic",
            "active_root": {"path": "current.jsonl"}
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        checkpoint.join("previous.json"),
        serde_json::to_vec(&serde_json::json!({
            "mode": "monolithic",
            "active_root": {"path": "previous.jsonl"}
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::remove_file(checkpoint.join("previous.jsonl")).unwrap();

    let diagnostics = bead_rs::service::doctor::run_diagnostics_with_scopes(
        &store,
        &[bead_rs::service::doctor::DiagnosticScope::Secrets],
    )
    .unwrap();
    assert!(diagnostics.has_errors);
    let check = diagnostics
        .checks
        .iter()
        .find(|check| check.name == "secret_scan")
        .unwrap();
    assert_eq!(
        check.status,
        bead_rs::service::doctor::DiagnosticStatus::Error
    );
    let details = check.details.as_ref().unwrap();
    assert!(details["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| {
            finding["selector"]
                .as_str()
                .is_some_and(|selector| selector.starts_with("live:issues:"))
        }));
    assert!(details["coverage"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| { source["source"] == "live" && source["status"] == "scanned" }));
    assert!(details["coverage"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| { source["source"] == "current" && source["status"] == "scanned" }));
    assert!(details["coverage"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| {
            source["source"] == "previous"
                && source["status"] == "unreadable"
                && source["reason_code"] == "checkpoint_scan_failed"
        }));
}

#[test]
fn first_checkpoint_publication_reports_previous_generation_absent() {
    let (root, store) = workspace();
    bead(root.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();

    let checkpoint = root.path().join(".beads/checkpoint");
    assert!(checkpoint.join("current.json").is_file());
    assert!(!checkpoint.join("previous.json").exists());

    let diagnostics = bead_rs::service::doctor::run_diagnostics_with_scopes(
        &store,
        &[bead_rs::service::doctor::DiagnosticScope::Secrets],
    )
    .unwrap();
    assert!(!diagnostics.has_errors);
    let check = diagnostics
        .checks
        .iter()
        .find(|check| check.name == "secret_scan")
        .unwrap();
    let details = check.details.as_ref().unwrap();
    assert_eq!(
        details["compiled_policy"],
        if cfg!(feature = "managed-secret-policy") {
            "managed-enforce-no-ack"
        } else {
            "workspace-configurable"
        }
    );
    assert_eq!(
        details["exact_fingerprint_acknowledgment"],
        !cfg!(feature = "managed-secret-policy")
    );
    assert!(details["coverage"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| { source["source"] == "current" && source["status"] == "scanned" }));
    assert!(details["coverage"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| {
            source["source"] == "previous"
                && source["status"] == "absent"
                && source["reason_code"] == "no_retained_generation"
        }));
}

#[cfg(unix)]
#[test]
fn dangling_checkpoint_pointer_is_reported_as_unreadable() {
    use std::os::unix::fs::symlink;

    let (root, store) = workspace();
    let checkpoint = root.path().join(".beads/checkpoint");
    std::fs::create_dir_all(&checkpoint).unwrap();
    std::fs::write(checkpoint.join("current.jsonl"), b"").unwrap();
    std::fs::write(
        checkpoint.join("current.json"),
        serde_json::to_vec(&serde_json::json!({
            "mode": "monolithic",
            "active_root": {"path": "current.jsonl"}
        }))
        .unwrap(),
    )
    .unwrap();
    symlink("missing-previous.json", checkpoint.join("previous.json")).unwrap();

    let report = secret_diagnostics::run_secret_diagnostics(&store).unwrap();
    assert!(!report.coverage_complete);
    assert!(report.coverage.iter().any(|source| {
        source.source == "previous"
            && source.status == "unreadable"
            && source.reason_code == Some("checkpoint_scan_failed")
    }));
}

#[test]
fn diagnostic_paths_never_echo_arbitrary_lowercase_property_names() {
    let (root, _store) = workspace();
    let private_name = format!("npm_{}", "a1b2c3d4e5f6".repeat(3));
    let document = serde_json::json!({private_name.clone(): provider()});
    let input = root.path().join("property-name.json");
    std::fs::write(&input, serde_json::to_vec(&document).unwrap()).unwrap();
    let report = secret_diagnostics::scan_recovery_artifact(&input).unwrap();
    assert!(!report.findings.is_empty());
    assert!(!serde_json::to_string(&report.findings)
        .unwrap()
        .contains(&private_name));
}

#[test]
fn explicitly_tombstoned_previous_root_is_absent_but_unexplained_loss_is_unreadable() {
    let (root, mut store) = workspace();
    insert(store.conn(), &provider());
    let checkpoint = root.path().join(".beads/checkpoint");
    std::fs::create_dir_all(&checkpoint).unwrap();
    let previous = serde_json::json!({"mode":"monolithic","active_root":{"path":"removed.jsonl"}});
    std::fs::write(
        checkpoint.join("previous.json"),
        serde_json::to_vec(&previous).unwrap(),
    )
    .unwrap();
    let current = serde_json::json!({"mode":"monolithic","active_root":{"path":"forensic.jsonl"},"deleted_paths":["removed.jsonl"]});
    std::fs::write(
        checkpoint.join("current.json"),
        serde_json::to_vec(&current).unwrap(),
    )
    .unwrap();
    std::fs::write(checkpoint.join("forensic.jsonl"), b"").unwrap();
    let report = secret_diagnostics::run_secret_diagnostics(&store).unwrap();
    assert!(report.coverage_complete);
    assert!(report.blocking_findings > 0);
    assert!(report
        .coverage
        .iter()
        .any(|source| source.source == "previous" && source.status == "absent"));
    assert!(secret_diagnostics::scan_recovery_artifact(&checkpoint).is_ok());
    std::fs::write(
        checkpoint.join("current.json"),
        serde_json::to_vec(
            &serde_json::json!({"mode":"monolithic","active_root":{"path":"forensic.jsonl"}}),
        )
        .unwrap(),
    )
    .unwrap();
    let report = secret_diagnostics::run_secret_diagnostics(&store).unwrap();
    assert!(!report.coverage_complete);
    assert!(report
        .coverage
        .iter()
        .any(|source| source.source == "previous" && source.status == "unreadable"));
}

#[test]
fn historical_resource_identity_is_admitted_to_quarantine_then_rekeyed() {
    let (root, mut store) = workspace();
    let (_source, mut source_store) = workspace();
    let value = provider();
    insert(source_store.conn(), "clean");
    assert!(bead_rs::service::resource_locks::declare_resource_keys(
        source_store.conn(),
        "gap-1",
        std::slice::from_ref(&value)
    )
    .is_err());
    source_store
        .conn()
        .execute(
            "INSERT INTO issue_resource_keys (issue_id,resource_key) VALUES ('gap-1',?1)",
            [&value],
        )
        .unwrap();
    let input = root.path().join("historical-resources.jsonl");
    bead_rs::service::flush_checkpoint(&mut source_store, &input).unwrap();
    let output = bead(root.path())
        .args([
            "sync",
            "import-only",
            "--input",
            "historical-resources.jsonl",
            "--restore-into-empty",
            "--actor",
            "tester",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "historical recovery unexpectedly refused"
    );
    let count: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM secret_quarantine", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 1);
    let finding = secret_diagnostics::scan_live_findings(store.conn())
        .unwrap()
        .into_iter()
        .find(|finding| {
            finding.rule_id == "aws-access-key-id" && finding.field_path == "resource_key"
        })
        .unwrap();
    let blocked = bead(root.path())
        .args([
            "redact",
            "--finding",
            &finding.fingerprint,
            "--actor",
            "tester",
            "--reason",
            "identity test",
        ])
        .output()
        .unwrap();
    assert!(blocked.status.success());
    assert!(!String::from_utf8_lossy(&blocked.stderr).contains(&value));
    let count: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM redaction_receipts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn imported_secret_is_quarantined_across_restart_until_redaction() {
    let (root, mut store) = workspace();
    let value = provider();
    let input = root.path().join("historical.jsonl");
    let (_source, mut source_store) = workspace();
    insert(source_store.conn(), &value);
    bead_rs::service::flush_checkpoint(&mut source_store, &input).unwrap();
    bead(root.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let pointer_before = fs::read(root.path().join(".beads/checkpoint/current.json")).unwrap();
    let output = bead(root.path())
        .args([
            "sync",
            "import-only",
            "--input",
            "historical.jsonl",
            "--restore-into-empty",
            "--actor",
            "tester",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["local_recovery_succeeded"], true);
    assert_eq!(report["secret_quarantined"], true);
    assert_eq!(report["checkpoint_publication_withheld"], true);
    assert!(stderr.contains("secret_quarantined"));
    assert!(!stdout.contains(&value));
    assert!(!stderr.contains(&value));
    assert!(
        fs::read(root.path().join(".beads/checkpoint/current.json")).unwrap() == pointer_before
    );
    assert!(stored(store.conn()) == value);
    let quarantine: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM secret_quarantine", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(quarantine, 1);
    let status = bead(root.path())
        .args(["sync", "status", "--format", "json"])
        .output()
        .unwrap();
    assert!(status.status.success());
    let status_text = String::from_utf8_lossy(&status.stdout);
    assert!(status_text.contains("aws-access-key-id"));
    assert!(status_text.contains("blocking finding"));
    assert!(status_text.contains("bead redact --all-blocking"));
    assert!(!status_text.contains(&value));
    for args in [
        vec!["sync", "flush-only"],
        vec!["create", "--title", "clean"],
        vec!["sync", "commit", "--dry-run"],
    ] {
        let blocked = bead(root.path()).args(args).output().unwrap();
        assert!(!blocked.status.success());
        let blocked_stdout = String::from_utf8_lossy(&blocked.stdout);
        let blocked_stderr = String::from_utf8_lossy(&blocked.stderr);
        assert!(blocked_stderr.contains("secret_quarantined"));
        assert!(!blocked_stdout.contains(&value));
        assert!(!blocked_stderr.contains(&value));
    }
    let fingerprint = fingerprints(store.conn()).remove(0);
    bead(root.path())
        .args([
            "redact",
            "--finding",
            &fingerprint,
            "--actor",
            "tester",
            "--reason",
            "historical fixture cleanup",
        ])
        .assert()
        .success();
    bead(root.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();
}

#[test]
fn checkpoint_publication_stays_withheld_across_restart_until_redaction() {
    let (root, _) = workspace();
    let value = provider();
    let input = root.path().join("historical.jsonl");
    let (_source, mut source_store) = workspace();
    insert(source_store.conn(), &value);
    bead_rs::service::flush_checkpoint(&mut source_store, &input).unwrap();

    set_auto_flush(root.path(), true);
    bead(root.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let checkpoint = root.path().join(".beads/checkpoint");
    let published_before_recovery = checkpoint_state_snapshot(&checkpoint);

    // This explicit recovery is allowed to commit locally. Its enabled
    // automatic publisher must report the hold without changing the fileset.
    let recovery = bead(root.path())
        .args([
            "sync",
            "import-only",
            "--input",
            "historical.jsonl",
            "--merge",
            "--actor",
            "tester",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(recovery.status.success());
    let recovery_stdout = String::from_utf8_lossy(&recovery.stdout);
    let recovery_stderr = String::from_utf8_lossy(&recovery.stderr);
    assert!(recovery_stdout.contains("local_recovery_succeeded"));
    assert!(recovery_stderr.contains("secret_quarantined"));
    assert!(!recovery_stdout.contains(&value));
    assert!(!recovery_stderr.contains(&value));
    assert_eq!(
        checkpoint_state_snapshot(&checkpoint),
        published_before_recovery
    );

    // Exercise the locked public publisher directly as well as the CLI tail.
    // Neither legacy export nor forensic publication may write while held.
    let conn = open_configured_connection(&root.path().join(".beads/beads.db")).unwrap();
    let mut store = SqliteStore::from_conn(conn);
    let checkpoint_config =
        bead_rs::service::load_checkpoint_config(&root.path().join(".beads")).unwrap();
    let forensic = bead_rs::service::publish_forensic_checkpoint(
        &mut store,
        &checkpoint_config,
        &root.path().join(".beads"),
    )
    .unwrap_err();
    assert!(format!("{forensic:#}").contains("secret_quarantined"));
    assert!(!format!("{forensic:#}").contains(&value));
    let legacy_output = root.path().join("blocked-export.jsonl");
    let legacy = bead_rs::service::flush_checkpoint(&mut store, &legacy_output).unwrap_err();
    assert!(format!("{legacy:#}").contains("secret_quarantined"));
    assert!(!format!("{legacy:#}").contains(&value));
    assert!(!legacy_output.exists());
    assert_eq!(
        checkpoint_state_snapshot(&checkpoint),
        published_before_recovery
    );

    // A second recovery under the per-invocation opt-out and later CLI
    // invocations model process restarts. These settings cannot clear the
    // durable quarantine, and repeated manual flushes remain refused.
    let repeated_recovery = bead(root.path())
        .args([
            "--no-auto-flush",
            "sync",
            "import-only",
            "--input",
            "historical.jsonl",
            "--merge",
            "--actor",
            "tester",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(repeated_recovery.status.success());
    assert!(String::from_utf8_lossy(&repeated_recovery.stdout).contains("secret_quarantined"));
    assert!(!String::from_utf8_lossy(&repeated_recovery.stdout).contains(&value));
    set_auto_flush(root.path(), false);
    for args in [
        vec!["sync", "flush-only"],
        vec!["--no-auto-flush", "sync", "flush-only"],
    ] {
        let blocked = bead(root.path()).args(args).output().unwrap();
        assert!(!blocked.status.success());
        let stdout = String::from_utf8_lossy(&blocked.stdout);
        let stderr = String::from_utf8_lossy(&blocked.stderr);
        assert!(stderr.contains("secret_quarantined"));
        assert!(!stdout.contains(&value));
        assert!(!stderr.contains(&value));
        assert_eq!(
            checkpoint_state_snapshot(&checkpoint),
            published_before_recovery
        );
    }

    let fingerprint = fingerprints(store.conn()).remove(0);
    let redaction = bead(root.path())
        .args([
            "redact",
            "--finding",
            &fingerprint,
            "--actor",
            "tester",
            "--reason",
            "remove recovered synthetic finding",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        redaction.status.success(),
        "{}",
        String::from_utf8_lossy(&redaction.stderr)
    );
    assert!(!String::from_utf8_lossy(&redaction.stdout).contains(&value));
    assert!(!String::from_utf8_lossy(&redaction.stderr).contains(&value));
    let diagnostics = bead(root.path())
        .args(["doctor", "--scope", "secrets", "--format", "json"])
        .output()
        .unwrap();
    assert!(diagnostics.status.success());
    let diagnostic_json: Value = serde_json::from_slice(&diagnostics.stdout).unwrap();
    let secret_report = &diagnostic_json["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "secret_scan")
        .unwrap()["details"];
    assert_eq!(secret_report["quarantined"], false);
    assert_eq!(secret_report["blocking_findings"], 0);
    for generation in ["current", "previous"] {
        assert!(secret_report["coverage"]
            .as_array()
            .unwrap()
            .iter()
            .any(|source| source["source"] == generation && source["status"] == "scanned"));
    }
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(checkpoint.join("current.json")).unwrap())
            .unwrap()["generation_id"],
        serde_json::from_slice::<Value>(&fs::read(checkpoint.join("previous.json")).unwrap())
            .unwrap()["generation_id"]
    );
    assert_eq!(
        store
            .conn()
            .query_row("SELECT COUNT(*) FROM secret_quarantine", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    for bytes in checkpoint_state_snapshot(&checkpoint).values() {
        assert!(!bytes
            .windows(value.len())
            .any(|window| window == value.as_bytes()));
    }
}

#[test]
fn import_only_rolls_back_recovery_and_quarantine_together_on_quarantine_failure() {
    let (root, mut store) = workspace();
    let value = provider();
    let input = root.path().join("historical.jsonl");
    let (_source, mut source_store) = workspace();
    insert(source_store.conn(), &value);
    bead_rs::service::flush_checkpoint(&mut source_store, &input).unwrap();

    bead(root.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let pointer_before = fs::read(root.path().join(".beads/checkpoint/current.json")).unwrap();
    let before = recovery_snapshot(store.conn());
    fail_quarantine_insert(store.conn());
    let output = bead(root.path())
        .args([
            "sync",
            "import-only",
            "--input",
            "historical.jsonl",
            "--restore-into-empty",
            "--actor",
            "tester",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stdout.contains(&value));
    assert!(!stderr.contains(&value));
    assert!(recovery_snapshot(store.conn()) == before);
    assert!(
        fs::read(root.path().join(".beads/checkpoint/current.json")).unwrap() == pointer_before
    );
}

#[test]
fn verified_restore_admits_secret_locally_but_withholds_new_checkpoint() {
    let (source_root, generation) = published_secret_generation("bead-secret-restore-");
    let value = provider();

    let target_root = isolated_workspace_root("bead-secret-restore-target-");
    let output = bead(target_root.path())
        .args([
            "restore",
            "--source",
            source_root
                .path()
                .join(".beads/checkpoint")
                .to_str()
                .unwrap(),
            "--generation",
            &generation,
            "--actor",
            "restore-operator",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["local_recovery_succeeded"].as_bool() == Some(true));
    assert!(report["secret_quarantined"].as_bool() == Some(true));
    assert!(report["checkpoint_publication_withheld"].as_bool() == Some(true));
    assert!(stderr.contains("secret_quarantined"));
    assert!(!stdout.contains(&value));
    assert!(!stderr.contains(&value));

    let conn = open_configured_connection(&target_root.path().join(".beads/beads.db")).unwrap();
    assert!(stored(&conn) == value);
    let quarantine: i64 = conn
        .query_row("SELECT COUNT(*) FROM secret_quarantine", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(quarantine, 1);
    assert!(!target_root
        .path()
        .join(".beads/checkpoint/current.json")
        .exists());
    for args in [
        vec!["sync", "flush-only"],
        vec!["sync", "commit", "--dry-run"],
    ] {
        let blocked = bead(target_root.path()).args(args).output().unwrap();
        assert!(!blocked.status.success());
        let message = String::from_utf8_lossy(&blocked.stderr);
        assert!(message.contains("secret_quarantined"));
        assert!(!message.contains(&value));
    }
}

#[test]
fn verified_restore_rolls_back_recovery_and_quarantine_together_on_quarantine_failure() {
    let (source_root, generation) = published_secret_generation("bead-secret-restore-fail-");
    let value = provider();
    let target_root = isolated_workspace_root("bead-secret-restore-fail-target-");
    bead(target_root.path())
        .args(["init", "--prefix", "gap", "--no-auto-flush"])
        .assert()
        .success();
    bead(target_root.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let pointer_before =
        fs::read(target_root.path().join(".beads/checkpoint/current.json")).unwrap();
    let mut target_store = SqliteStore::from_conn(
        open_configured_connection(&target_root.path().join(".beads/beads.db")).unwrap(),
    );
    let before = recovery_snapshot(target_store.conn());
    fail_quarantine_insert(target_store.conn());

    let output = bead(target_root.path())
        .args([
            "restore",
            "--source",
            source_root
                .path()
                .join(".beads/checkpoint")
                .to_str()
                .unwrap(),
            "--generation",
            &generation,
            "--actor",
            "restore-operator",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stdout.contains(&value));
    assert!(!stderr.contains(&value));
    assert!(recovery_snapshot(target_store.conn()) == before);
    assert!(
        fs::read(target_root.path().join(".beads/checkpoint/current.json")).unwrap()
            == pointer_before
    );
}

#[test]
fn clean_restore_cannot_clear_an_existing_recovery_quarantine() {
    let (target_root, mut target_store) = workspace();
    let value = provider();
    let import_input = target_root.path().join("historical.jsonl");
    let (_secret_source, mut secret_source_store) = workspace();
    insert(secret_source_store.conn(), &value);
    bead_rs::service::flush_checkpoint(&mut secret_source_store, &import_input).unwrap();

    let imported = bead(target_root.path())
        .args([
            "sync",
            "import-only",
            "--input",
            "historical.jsonl",
            "--restore-into-empty",
            "--actor",
            "recovery-operator",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(imported.status.success());
    let imported_report: Value = serde_json::from_slice(&imported.stdout).unwrap();
    assert_eq!(imported_report["secret_quarantined"], true);
    assert!(!String::from_utf8_lossy(&imported.stdout).contains(&value));
    assert!(!String::from_utf8_lossy(&imported.stderr).contains(&value));

    let (_clean_source, mut clean_source_store) = workspace();
    insert(clean_source_store.conn(), "clean recovery content");
    bead(_clean_source.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let clean_checkpoint = _clean_source.path().join(".beads/checkpoint");
    let clean_generation =
        serde_json::from_slice::<Value>(&fs::read(clean_checkpoint.join("current.json")).unwrap())
            .unwrap()["generation_id"]
            .as_str()
            .unwrap()
            .to_string();
    let restore = bead(target_root.path())
        .args([
            "restore",
            "--source",
            clean_checkpoint.to_str().unwrap(),
            "--generation",
            &clean_generation,
            "--actor",
            "recovery-operator",
            "--allow-non-empty",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        restore.status.success(),
        "{}",
        String::from_utf8_lossy(&restore.stderr)
    );
    let report: Value = serde_json::from_slice(&restore.stdout).unwrap();
    assert_eq!(report["local_recovery_succeeded"], true);
    assert_eq!(report["secret_quarantined"], true);
    assert_eq!(report["checkpoint_publication_withheld"], true);
    assert!(String::from_utf8_lossy(&restore.stderr).contains("secret_quarantined"));
    assert!(!String::from_utf8_lossy(&restore.stdout).contains(&value));
    assert!(!String::from_utf8_lossy(&restore.stderr).contains(&value));

    let held: i64 = target_store
        .conn()
        .query_row("SELECT COUNT(*) FROM secret_quarantine", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(held, 1);
}

#[test]
fn reconcile_secret_recovery_succeeds_locally_without_republishing() {
    let source_root = isolated_workspace_root("bead-secret-reconcile-");
    bead(source_root.path())
        .args(["init", "--prefix", "gap"])
        .assert()
        .success();
    bead(source_root.path())
        .args(["create", "--title", "clean clone base"])
        .assert()
        .success();
    bead(source_root.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let generation = serde_json::from_slice::<Value>(
        &fs::read(source_root.path().join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap()["generation_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Build a clean clone from the source's committed identity and checkpoint,
    // then activate it locally before the source advances. This mirrors the
    // transport shape without putting a candidate value in a fixture file.
    let target_root = isolated_workspace_root("bead-secret-reconcile-target-");
    fs::create_dir_all(target_root.path().join(".beads")).unwrap();
    fs::copy(
        source_root.path().join(".beads/config.json"),
        target_root.path().join(".beads/config.json"),
    )
    .unwrap();
    copy_tree(
        &source_root.path().join(".beads/checkpoint"),
        &target_root.path().join(".beads/checkpoint"),
    );
    bead(target_root.path()).args(["init"]).assert().success();
    bead(target_root.path())
        .args([
            "restore",
            "--source",
            ".beads/checkpoint",
            "--generation",
            &generation,
            "--actor",
            "clone-operator",
        ])
        .assert()
        .success();
    // The clone advances through a direct fixture write so the clone itself
    // is not quarantined. Its explicit flush creates the remote-advanced
    // checkpoint that the original workspace must admit locally without
    // republishing.
    let value = provider();
    let mut target_store = SqliteStore::from_conn(
        open_configured_connection(&target_root.path().join(".beads/beads.db")).unwrap(),
    );
    insert(target_store.conn(), &value);
    target_store
        .conn()
        .execute(
            "INSERT INTO events (issue_id, kind, actor, time, detail)
             VALUES ('gap-1', 'fixture_advance', 'fixture',
                     '2026-10-03T00:01:00Z', '{}')",
            [],
        )
        .unwrap();
    bead(target_root.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();

    let source_checkpoint = source_root.path().join(".beads/checkpoint");
    fs::remove_dir_all(&source_checkpoint).unwrap();
    copy_tree(
        &target_root.path().join(".beads/checkpoint"),
        &source_checkpoint,
    );
    let published_before = checkpoint_state_snapshot(&source_checkpoint);

    let output = bead(source_root.path())
        .args(["sync", "reconcile", "--actor", "reconcile-operator"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("Secret quarantined: true"));
    assert!(stdout.contains("Checkpoint publication withheld: true"));
    assert!(stderr.contains("secret_quarantined"));
    assert!(!stdout.contains(&value));
    assert!(!stderr.contains(&value));
    assert!(
        checkpoint_state_snapshot(&source_checkpoint) == published_before,
        "quarantined reconcile changed the published checkpoint set"
    );

    let conn = open_configured_connection(&source_root.path().join(".beads/beads.db")).unwrap();
    let quarantine: i64 = conn
        .query_row("SELECT COUNT(*) FROM secret_quarantine", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(quarantine, 1);
    assert!(
        stored(&conn) == value,
        "incoming issue was not retained locally"
    );

    for args in [
        vec!["sync", "flush-only"],
        vec!["sync", "commit", "--dry-run"],
    ] {
        let blocked = bead(source_root.path()).args(args).output().unwrap();
        assert!(!blocked.status.success());
        let message = String::from_utf8_lossy(&blocked.stderr);
        assert!(message.contains("secret_quarantined"));
        assert!(!message.contains(&value));
    }
}

#[test]
fn reconcile_quarantines_a_finding_only_in_the_retained_generation() {
    let (source_root, _target_root, value) = retained_secret_reconcile_pair();
    let source_checkpoint = source_root.path().join(".beads/checkpoint");
    let published_before = checkpoint_state_snapshot(&source_checkpoint);

    let output = bead(source_root.path())
        .args(["sync", "reconcile", "--actor", "reconcile-operator"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "reconcile failed without exposing its candidate"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("Secret quarantined: true"));
    assert!(stdout.contains("Checkpoint publication withheld: true"));
    assert!(stderr.contains("secret_quarantined"));
    assert!(!stdout.contains(&value));
    assert!(!stderr.contains(&value));
    assert!(
        checkpoint_state_snapshot(&source_checkpoint) == published_before,
        "retained-generation quarantine changed the published checkpoint set"
    );

    let conn = open_configured_connection(&source_root.path().join(".beads/beads.db")).unwrap();
    assert!(
        secret_diagnostics::scan_live_findings(&conn)
            .unwrap()
            .is_empty(),
        "the reconciled live semantic state should be clean"
    );
    let quarantine_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM secret_quarantine", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(quarantine_count, 1);

    let diagnostics = bead(source_root.path())
        .args(["doctor", "--scope", "secrets", "--format", "json"])
        .output()
        .unwrap();
    assert!(diagnostics.status.success());
    let diagnostic_stdout = String::from_utf8_lossy(&diagnostics.stdout);
    let diagnostic_stderr = String::from_utf8_lossy(&diagnostics.stderr);
    assert!(!diagnostic_stdout.contains(&value));
    assert!(!diagnostic_stderr.contains(&value));
    let report: Value = serde_json::from_slice(&diagnostics.stdout).unwrap();
    let findings = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "secret_scan")
        .unwrap()["details"]["findings"]
        .as_array()
        .unwrap();
    assert!(!findings.is_empty());
    assert!(findings.iter().all(|finding| finding["selector"]
        .as_str()
        .is_some_and(|selector| selector.starts_with("checkpoint:previous:record:"))));
}

#[test]
fn failed_reconcile_rolls_back_quarantine_and_local_state() {
    let (source_root, _target_root, value) = retained_secret_reconcile_pair();
    let source_checkpoint = source_root.path().join(".beads/checkpoint");
    let published_before = checkpoint_state_snapshot(&source_checkpoint);
    let source_db = source_root.path().join(".beads/beads.db");
    let conn = open_configured_connection(&source_db).unwrap();
    let state_before = recovery_state_snapshot(&conn);
    fail_quarantine_insert(&conn);
    drop(conn);

    let output = bead(source_root.path())
        .args(["sync", "reconcile", "--actor", "reconcile-operator"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stdout.contains(&value));
    assert!(!stderr.contains(&value));

    let conn = open_configured_connection(&source_db).unwrap();
    assert!(
        recovery_state_snapshot(&conn) == state_before,
        "failed reconcile changed local semantic or quarantine state"
    );
    assert!(
        checkpoint_state_snapshot(&source_checkpoint) == published_before,
        "failed reconcile changed the published checkpoint set"
    );
    let quarantine_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM secret_quarantine", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(quarantine_count, 0);
}

#[cfg(feature = "managed-secret-policy")]
#[test]
fn managed_policy_refuses_workspace_downgrades_and_acknowledgments() {
    for mode in ["off", "advisory"] {
        assert!(ScanConfig::from_config_values(Some(&serde_json::json!(mode)), None).is_err());
    }
    assert!(
        ScanConfig::from_config_values(None, Some(&serde_json::json!(["0".repeat(64)]))).is_err()
    );
    let mut config = ScanConfig::enforce();
    assert!(config
        .add_invocation_acknowledgments(["0".repeat(64)].iter().map(String::as_str))
        .is_err());
    let (root, mut store) = workspace();
    let path = root.path().join(".beads/config.json");
    let original: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let config = WorkspaceConfig {
        root: root.path().to_path_buf(),
        uuid: store
            .conn()
            .query_row("SELECT uuid FROM workspace", [], |row| row.get(0))
            .unwrap(),
        prefix: "gap".to_string(),
    };
    let candidate = provider();
    for mode in ["off", "advisory"] {
        let mut altered = original.clone();
        altered["secret_scan"]["mode"] = Value::String(mode.to_string());
        std::fs::write(&path, serde_json::to_vec(&altered).unwrap()).unwrap();
        let direct_error = issues::create_issue(
            store.conn(),
            &config,
            "clean title".to_string(),
            Some(candidate.clone()),
            2,
            None,
            None,
            vec![],
            vec![],
        )
        .unwrap_err()
        .to_string();
        assert!(!direct_error.contains(&candidate));
        let output = bead(root.path())
            .args(["create", "--title", "clean"])
            .output()
            .unwrap();
        assert!(!output.status.success());
    }

    // A malformed policy value that itself resembles a credential is never
    // echoed by service failures, doctor details, or capability diagnostics.
    let mut altered = original.clone();
    altered["secret_scan"]["mode"] = Value::String(candidate.clone());
    std::fs::write(&path, serde_json::to_vec(&altered).unwrap()).unwrap();
    let direct_error = issues::create_issue(
        store.conn(),
        &config,
        "clean title".to_string(),
        Some(candidate.clone()),
        2,
        None,
        None,
        vec![],
        vec![],
    )
    .unwrap_err()
    .to_string();
    assert!(!direct_error.contains(&candidate));
    let diagnostics = bead_rs::service::doctor::run_diagnostics_with_scopes(
        &store,
        &[bead_rs::service::doctor::DiagnosticScope::Secrets],
    )
    .unwrap();
    assert!(diagnostics.has_errors);
    assert!(!serde_json::to_string(&diagnostics)
        .unwrap()
        .contains(&candidate));
    let output = bead(root.path()).args(["capabilities"]).output().unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains(&candidate));

    std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
    let output = bead(root.path())
        .args([
            "create",
            "--title",
            "clean",
            "--acknowledge-secret",
            &"0".repeat(64),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let count: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
    let output = bead(root.path()).args(["capabilities"]).output().unwrap();
    assert!(output.status.success());
    let capabilities: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        capabilities["secret_scan"]["compiled_policy"],
        "managed-enforce-no-ack"
    );
    assert_eq!(
        capabilities["secret_scan"]["exact_fingerprint_acknowledgment"],
        false
    );
    let diagnostics = bead_rs::service::doctor::run_diagnostics_with_scopes(
        &store,
        &[bead_rs::service::doctor::DiagnosticScope::Secrets],
    )
    .unwrap();
    let secret_check = diagnostics
        .checks
        .iter()
        .find(|check| check.name == "secret_scan")
        .unwrap();
    assert_eq!(
        secret_check.details.as_ref().unwrap()["compiled_policy"],
        "managed-enforce-no-ack"
    );
    assert_eq!(
        secret_check.details.as_ref().unwrap()["exact_fingerprint_acknowledgment"],
        false
    );
}
