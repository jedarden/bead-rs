//! Independently assembled ruleset-v4 section 3.2 bound/coverage witnesses.
//! Only owned temporary stores contain synthetic candidates. Assertions never
//! print candidate bytes; derived matches retain raw fingerprint coordinates.
use assert_cmd::Command;
use bead_rs::scan::{self, Field, Mode, ScanConfig, RULESET_VERSION};
use bead_rs::service::secret_diagnostics;
use bead_rs::store::{open_configured_connection, SqliteStore};
use serde_json::json;
use std::fs;

#[test]
fn diagnostics_report_each_limited_source_without_locations_or_content() {
    let root = tempfile::Builder::new()
        .prefix("decoded-coverage-")
        .tempdir_in("/var/tmp")
        .unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(root.path())
        .args(["init", "--prefix", "cv", "--no-auto-flush"])
        .assert()
        .success();
    let mut store = SqliteStore::from_conn(
        open_configured_connection(&root.path().join(".beads/beads.db")).unwrap(),
    );
    let limited = format!("{}!{}", "A".repeat(65_537), "=".repeat(40));
    let limited = format!("{limited}!{}", format!("{}!", "=".repeat(40)).repeat(64));
    // Fixture-only SQL creates pre-existing bytes for a read-only diagnostic.
    store.conn().execute(
        "INSERT INTO issues (id,title,description,priority,issue_type,base_status,created_at,updated_at,revision)
         VALUES ('cv-1','stable',?1,2,'task','open','2026-10-06T00:00:00Z','2026-10-06T00:00:00Z',1)",
        [&limited],
    ).unwrap();
    let checkpoint = root.path().join(".beads/checkpoint");
    fs::create_dir_all(&checkpoint).unwrap();
    for source in ["current", "previous"] {
        fs::write(
            checkpoint.join(format!("{source}.jsonl")),
            serde_json::to_vec(&json!({
                "record_type": "issue", "issue": {"id": "cv-1", "description": limited}
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            checkpoint.join(format!("{source}.json")),
            serde_json::to_vec(&json!({
                "mode": "monolithic", "active_root": {"path": format!("{source}.jsonl")}
            }))
            .unwrap(),
        )
        .unwrap();
    }
    let report = secret_diagnostics::run_secret_diagnostics(&store).unwrap();
    assert!(report.coverage_complete); // All sources were read, not all decoding unlimited.
    assert_eq!(report.blocking_findings, 0);
    assert_eq!(
        serde_json::to_value(&report.view_coverage).unwrap(),
        json!([
            {"source":"live","view":"decoded","status":"limited","reason_codes":["run_count_limit","run_size_limit"]},
            {"source":"current","view":"decoded","status":"limited","reason_codes":["run_count_limit","run_size_limit"]},
            {"source":"previous","view":"decoded","status":"limited","reason_codes":["run_count_limit","run_size_limit"]}
        ])
    );
}

fn base64(text: &[u8]) -> String {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in text.chunks(3) {
        let packed = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | chunk.get(2).copied().unwrap_or(0) as u32;
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            result.push(if index > chunk.len() {
                '='
            } else {
                alphabet[((packed >> shift) & 63) as usize] as char
            });
        }
    }
    result
}

#[test]
fn decoded_noncanonical_padding_and_non_utf8_remainder_preserve_raw_fingerprint() {
    let provider = ["AK", "IA", "7Q9W2E4R6T8Y1U3I"].concat();
    let mut decoded = format!("header {provider} tail!").into_bytes();
    decoded.push(0xff);
    // Ensure the final base64 digit has unused low bits available to change.
    while decoded.len() % 3 != 1 {
        decoded.push(b'!');
    }
    let mut encoded = base64(&decoded).into_bytes();
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let final_digit = encoded.len() - 3;
    let position = alphabet
        .iter()
        .position(|byte| *byte == encoded[final_digit])
        .unwrap();
    encoded[final_digit] = alphabet[position + 1];
    let encoded = String::from_utf8(encoded).unwrap();
    let text = format!("!{encoded}!");
    let report = scan::scan(
        &ScanConfig::new(Mode::Enforce),
        "owned-fixture",
        &[Field::new("description", &text)],
    );
    let finding = report
        .blocking
        .iter()
        .find(|finding| finding.rule_id == "aws-access-key-id")
        .unwrap();
    assert_eq!((finding.start, finding.end), (1, encoded.len() + 1));
    assert!(
        finding.fingerprint
            == scan::fingerprint::compute(
                RULESET_VERSION,
                "aws-access-key-id",
                "owned-fixture",
                "description",
                1,
                encoded.len() + 1,
                encoded.as_bytes()
            )
    );
    assert!(report.decoded_view_limits.is_empty());

    // The fingerprint must remain selectable by the real scrubber, not just
    // reproducible by the scanner. Seed only this owned disposable database.
    let root = tempfile::Builder::new()
        .prefix("decoded-redaction-")
        .tempdir_in("/var/tmp")
        .unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(root.path())
        .args(["init", "--prefix", "cv", "--no-auto-flush"])
        .assert()
        .success();
    let mut store = SqliteStore::from_conn(
        open_configured_connection(&root.path().join(".beads/beads.db")).unwrap(),
    );
    store.conn().execute(
        "INSERT INTO issues (id,title,description,priority,issue_type,base_status,created_at,updated_at,revision)
         VALUES ('cv-1','stable',?1,2,'task','open','2026-10-06T00:00:00Z','2026-10-06T00:00:00Z',1)",
        [&text],
    ).unwrap();
    let diagnostics = secret_diagnostics::run_secret_diagnostics(&store).unwrap();
    let selected = diagnostics
        .findings
        .iter()
        .find(|finding| finding.rule_id == "aws-access-key-id")
        .unwrap();
    let arguments = [
        "redact",
        "--finding",
        &selected.fingerprint,
        "--actor",
        "decoded-fixture",
        "--reason",
        "invented fixture cleanup",
        "--json",
    ];
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(root.path())
        .args(arguments)
        .arg("--dry-run")
        .assert()
        .success();
    let description = |store: &mut SqliteStore| -> String {
        store
            .conn()
            .query_row(
                "SELECT description FROM issues WHERE id='cv-1'",
                [],
                |row| row.get(0),
            )
            .unwrap()
    };
    assert!(description(&mut store) == text);
    Command::cargo_bin("bead")
        .unwrap()
        .current_dir(root.path())
        .args(arguments)
        .assert()
        .success();
    assert_eq!(
        description(&mut store),
        format!("!{}!", bead_rs::model::redaction::REDACTION_MARKER)
    );
}

#[test]
fn exhausted_decoding_keeps_raw_blocking_and_does_not_reject_limits_alone() {
    let exhausted = format!("{}!", "=".repeat(40)).repeat(65);
    let config = ScanConfig::new(Mode::Enforce);
    let limited = scan::scan(
        &config,
        "owned-fixture",
        &[Field::new("description", &exhausted)],
    );
    assert!(limited.is_admitted());
    assert!(limited.decoded_view_limits.contains("run_count_limit"));
    let text = format!("{exhausted}{}", ["AK", "IA", "7Q9W2E4R6T8Y1U3I"].concat());
    let report = scan::scan(
        &config,
        "owned-fixture",
        &[Field::new("description", &text)],
    );
    assert!(report
        .blocking
        .iter()
        .any(|finding| finding.rule_id == "aws-access-key-id"));
    let off = scan::scan(
        &ScanConfig::new(Mode::Off),
        "owned-fixture",
        &[Field::new("description", &text)],
    );
    assert!(off.findings.is_empty());
    assert!(off.decoded_view_limits.is_empty());
}
