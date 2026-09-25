//! Integration tests for the agent-guided rehydration reconciliation report
//! (ADR-002, `research/specs/reconciliation-report-v1.md`).
//!
//! Format fixtures validate the parser against every required disposition
//! and every rejection rule. Binary-level tests prove the explicit
//! prevention rule: a reconciliation report handed to a native checkpoint
//! import path is refused by name and never treated as checkpoint input.

use assert_cmd::Command;
use bead_rs::reconciliation::{parse_report, Disposition, ReconciliationReport, SCHEMA_REF};
use predicates::prelude::*;
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

/// A valid report covering all four dispositions: two native beads, one
/// merge into the first native entry, one omission, one unresolved.
const VALID_REPORT: &str = concat!(
    r#"{"record_type":"header","schema_ref":"urn:bead-rs:schema:reconciliation-report:v1","#,
    r#""source_repository":"https://git.ardenone.com/jedarden/legacy-tracker.git","#,
    r#""source_commit":"9f2c1ab4d77e0c5b8a1f0e3d2c4b5a6978fedcba","#,
    r#""source_tracker":"bf-v1","generated_at":"2026-09-16T12:00:00Z","#,
    r#""generator":"rehydration-agent","destination_workspace":"/work/legacy-rehydration","#,
    r#""counts":{"total":5,"native":2,"omitted":1,"merged":1,"unresolved":1}}"#,
    "\n",
    r#"{"record_type":"entry","schema_ref":"urn:bead-rs:schema:reconciliation-report:v1","#,
    r#""source_id":"LEGACY-101","disposition":"native","target_bead":"bead-3f9a2c11","#,
    r#""source_title":"Fix login race","source_labels":["backend"]}"#,
    "\n",
    r#"{"record_type":"entry","schema_ref":"urn:bead-rs:schema:reconciliation-report:v1","#,
    r#""source_id":"LEGACY-102","disposition":"native","target_bead":"bead-77b0e4d2"}"#,
    "\n",
    r#"{"record_type":"entry","schema_ref":"urn:bead-rs:schema:reconciliation-report:v1","#,
    r#""source_id":"LEGACY-103","disposition":"merged","merged_into":"LEGACY-101","#,
    r#""rationale":"Duplicate of LEGACY-101; both describe the login race fix."}"#,
    "\n",
    r#"{"record_type":"entry","schema_ref":"urn:bead-rs:schema:reconciliation-report:v1","#,
    r#""source_id":"LEGACY-104","disposition":"omitted","#,
    r#""rationale":"Already completed and archived upstream; no open work remains."}"#,
    "\n",
    r#"{"record_type":"entry","schema_ref":"urn:bead-rs:schema:reconciliation-report:v1","#,
    r#""source_id":"LEGACY-105","disposition":"unresolved","#,
    r#""rationale":"Title mentions billing but the body is a deploy runbook."}"#,
);

/// Parse the valid fixture into mutable JSON values so rejection tests can
/// mutate exactly one aspect and re-serialize.
fn fixture_lines() -> Vec<serde_json::Value> {
    VALID_REPORT
        .lines()
        .map(|line| serde_json::from_str(line).expect("fixture line is valid JSON"))
        .collect()
}

fn render(lines: &[serde_json::Value]) -> String {
    lines
        .iter()
        .map(|value| serde_json::to_string(value).expect("value serializes"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn entry_mut(lines: &mut [serde_json::Value], index: usize) -> &mut serde_json::Value {
    &mut lines[index + 1]
}

fn assert_rejected(input: &str, expected_fragment: &str) {
    let error = parse_report(input)
        .expect_err("mutated fixture must be rejected")
        .to_string();
    assert!(
        error.contains(expected_fragment),
        "expected error to mention {expected_fragment:?}, got: {error}"
    );
}

#[test]
fn valid_report_parses_with_all_dispositions() {
    let report = parse_report(VALID_REPORT).expect("valid fixture must parse");

    assert_eq!(report.header.schema_ref, SCHEMA_REF);
    assert_eq!(
        report.header.source_repository,
        "https://git.ardenone.com/jedarden/legacy-tracker.git"
    );
    assert_eq!(
        report.header.source_commit,
        "9f2c1ab4d77e0c5b8a1f0e3d2c4b5a6978fedcba"
    );
    assert_eq!(report.entries.len(), 5);

    let dispositions: Vec<_> = report
        .entries
        .iter()
        .map(|entry| entry.disposition)
        .collect();
    assert_eq!(
        dispositions,
        vec![
            Disposition::Native,
            Disposition::Native,
            Disposition::Merged,
            Disposition::Omitted,
            Disposition::Unresolved,
        ]
    );

    let counts = report.disposition_counts();
    assert_eq!(counts.total, 5);
    assert_eq!(counts.native, 2);
    assert_eq!(counts.omitted, 1);
    assert_eq!(counts.merged, 1);
    assert_eq!(counts.unresolved, 1);
    assert!(!report.is_clean(), "an unresolved entry is not clean");
}

#[test]
fn extensions_are_preserved() {
    let report = parse_report(VALID_REPORT).expect("valid fixture must parse");
    let first = &report.entries[0];
    let labels = first
        .extensions
        .get("source_labels")
        .and_then(|value| value.as_array())
        .and_then(|labels| labels.first())
        .and_then(|value| value.as_str());
    assert_eq!(labels, Some("backend"));
    assert!(report.header.extensions.is_empty());
}

#[test]
fn clean_report_round_trips_through_serialization() {
    let report = parse_report(VALID_REPORT).expect("valid fixture must parse");

    // Drop the unresolved entry, retally the header, and re-serialize from
    // the typed records to prove the Serialize side carries the format.
    let entries: Vec<_> = report
        .entries
        .iter()
        .filter(|entry| entry.disposition != Disposition::Unresolved)
        .cloned()
        .collect();
    let mut header = report.header.clone();
    header.counts.total = 4;
    header.counts.unresolved = 0;

    let mut lines: Vec<String> = vec![serde_json::to_string(&header).expect("header")];
    lines.extend(
        entries
            .iter()
            .map(|entry| serde_json::to_string(entry).expect("entry")),
    );
    let text = lines.join("\n");

    let clean: ReconciliationReport = parse_report(&text).expect("retallied report parses");
    assert!(clean.is_clean());
    assert_eq!(clean.disposition_counts().total, 4);
}

#[test]
fn entry_before_header_is_rejected() {
    let mut lines = fixture_lines();
    lines.rotate_left(1);
    assert_rejected(
        &render(&lines),
        "entry record appears before the header record",
    );
}

#[test]
fn missing_header_is_rejected() {
    let lines = fixture_lines();
    let entries_only = render(&lines[1..]);
    assert_rejected(
        &entries_only,
        "entry record appears before the header record",
    );
}

#[test]
fn duplicate_header_is_rejected() {
    let mut lines = fixture_lines();
    let header = lines[0].clone();
    lines.insert(2, header);
    assert_rejected(&render(&lines), "duplicate header record");
}

#[test]
fn missing_source_commit_is_rejected() {
    let mut lines = fixture_lines();
    lines[0]
        .as_object_mut()
        .expect("header is an object")
        .remove("source_commit");
    assert_rejected(&render(&lines), "missing field `source_commit`");
}

#[test]
fn blank_source_repository_is_rejected() {
    let mut lines = fixture_lines();
    lines[0]["source_repository"] = serde_json::Value::String("  ".to_string());
    assert_rejected(
        &render(&lines),
        "source_repository must be a nonempty string",
    );
}

#[test]
fn non_rfc3339_generated_at_is_rejected() {
    let mut lines = fixture_lines();
    lines[0]["generated_at"] = serde_json::Value::String("yesterday".to_string());
    assert_rejected(
        &render(&lines),
        "generated_at must be an RFC 3339 timestamp",
    );
}

#[test]
fn wrong_schema_ref_is_rejected() {
    let mut lines = fixture_lines();
    entry_mut(&mut lines, 0)["schema_ref"] =
        serde_json::Value::String("urn:bead-rs:schema:issue:native-v1".to_string());
    assert_rejected(&render(&lines), "schema_ref must be");
}

#[test]
fn duplicate_source_id_is_rejected() {
    let mut lines = fixture_lines();
    entry_mut(&mut lines, 1)["source_id"] = serde_json::Value::String("LEGACY-101".to_string());
    assert_rejected(
        &render(&lines),
        "duplicate source_id; every source identifier appears exactly once",
    );
}

#[test]
fn native_without_target_bead_is_rejected() {
    let mut lines = fixture_lines();
    entry_mut(&mut lines, 0)
        .as_object_mut()
        .expect("entry is an object")
        .remove("target_bead");
    assert_rejected(&render(&lines), "requires target_bead");
}

#[test]
fn native_with_invalid_target_bead_is_rejected() {
    let mut lines = fixture_lines();
    entry_mut(&mut lines, 0)["target_bead"] =
        serde_json::Value::String("bead/../../escape".to_string());
    assert_rejected(
        &render(&lines),
        "target_bead is not a valid native issue ID",
    );
}

#[test]
fn native_with_merged_into_is_rejected() {
    let mut lines = fixture_lines();
    entry_mut(&mut lines, 0)["merged_into"] = serde_json::Value::String("LEGACY-103".to_string());
    assert_rejected(&render(&lines), "must not carry merged_into");
}

#[test]
fn shared_target_bead_is_rejected() {
    let mut lines = fixture_lines();
    entry_mut(&mut lines, 1)["target_bead"] =
        serde_json::Value::String("bead-3f9a2c11".to_string());
    assert_rejected(&render(&lines), "already claimed by another native entry");
}

#[test]
fn omitted_with_target_bead_is_rejected() {
    let mut lines = fixture_lines();
    entry_mut(&mut lines, 3)["target_bead"] =
        serde_json::Value::String("bead-0000ffff".to_string());
    assert_rejected(&render(&lines), "must not carry target_bead or merged_into");
}

#[test]
fn merged_without_merged_into_is_rejected() {
    let mut lines = fixture_lines();
    entry_mut(&mut lines, 2)
        .as_object_mut()
        .expect("entry is an object")
        .remove("merged_into");
    assert_rejected(&render(&lines), "requires merged_into");
}

#[test]
fn merged_chain_is_rejected() {
    // LEGACY-103 merges into LEGACY-101; pointing LEGACY-104 at LEGACY-103
    // would form a two-hop chain, which the format forbids.
    let mut lines = fixture_lines();
    {
        let entry = entry_mut(&mut lines, 3)
            .as_object_mut()
            .expect("entry is an object");
        entry.insert(
            "disposition".to_string(),
            serde_json::Value::String("merged".to_string()),
        );
        entry.insert(
            "merged_into".to_string(),
            serde_json::Value::String("LEGACY-103".to_string()),
        );
        entry.remove("rationale");
        lines[0]["counts"]["omitted"] = serde_json::json!(0);
        lines[0]["counts"]["merged"] = serde_json::json!(2);
    }
    assert_rejected(
        &render(&lines),
        "merges must resolve to a \"native\" entry in one hop",
    );
}

#[test]
fn merged_into_unknown_source_is_rejected() {
    let mut lines = fixture_lines();
    entry_mut(&mut lines, 2)["merged_into"] = serde_json::Value::String("LEGACY-999".to_string());
    assert_rejected(&render(&lines), "merged_into references unknown source_id");
}

#[test]
fn unresolved_without_rationale_is_rejected() {
    let mut lines = fixture_lines();
    entry_mut(&mut lines, 4)
        .as_object_mut()
        .expect("entry is an object")
        .remove("rationale");
    assert_rejected(&render(&lines), "requires a nonempty rationale");
}

#[test]
fn count_mismatch_is_rejected() {
    let mut lines = fixture_lines();
    lines[0]["counts"]["native"] = serde_json::json!(3);
    assert_rejected(
        &render(&lines),
        "counts.native declares 3 but the entries tally 2",
    );
}

#[test]
fn malformed_json_line_is_rejected() {
    let lines = fixture_lines();
    let mut text = render(&lines[..lines.len() - 1]);
    text.push_str("\n{\"record_type\":");
    assert_rejected(&text, "malformed JSON");
}

#[test]
fn unknown_record_type_is_rejected() {
    let mut lines = fixture_lines();
    let mut trailer = lines[1].clone();
    trailer["record_type"] = serde_json::Value::String("footer".to_string());
    lines.push(trailer);
    assert_rejected(&render(&lines), "unknown record_type \"footer\"");
}

#[test]
fn empty_input_is_rejected() {
    assert_rejected("", "the first record must be the header record");
    assert_rejected("\n\n", "the first record must be the header record");
}

// --- Explicit prevention: reports are never native checkpoint input ---

fn write_report_file(temp_dir: &TempDir, name: &str, contents: &str) -> String {
    let path = temp_dir.path().join(name);
    fs::write(&path, contents).expect("report file writes");
    path.to_str().expect("utf-8 path").to_string()
}

#[test]
#[serial]
fn import_rejects_jsonl_reconciliation_report() {
    let temp_dir = TempDir::new().unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    let report_path = write_report_file(&temp_dir, "report.jsonl", VALID_REPORT);
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            &report_path,
            "--profile",
            "native-v1",
            "--restore-into-empty",
            "--actor",
            "testuser",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("reconciliation report"))
        .stderr(predicate::str::contains("never native checkpoint input"));

    // The refusal happens at staging: no source identifier can have leaked
    // into the store as a native bead.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["list", "--json"])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("LEGACY-").not());
}

#[test]
#[serial]
fn import_rejects_single_document_reconciliation_report() {
    let temp_dir = TempDir::new().unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    // A report saved as one JSON document (header only) must get the same
    // named refusal from the whole-document pre-classifier.
    let header_only: String = VALID_REPORT.lines().next().unwrap().to_string();
    let report_path = write_report_file(&temp_dir, "report.json", &header_only);
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            &report_path,
            "--profile",
            "native-v1",
            "--restore-into-empty",
            "--actor",
            "testuser",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("never native checkpoint input"));
}

#[test]
#[serial]
fn import_merge_mode_also_rejects_reconciliation_report() {
    let temp_dir = TempDir::new().unwrap();
    Command::cargo_bin("bead")
        .unwrap()
        .args(["init", "--prefix", "test"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    let report_path = write_report_file(&temp_dir, "report.jsonl", VALID_REPORT);
    Command::cargo_bin("bead")
        .unwrap()
        .args([
            "sync",
            "import-only",
            "--input",
            &report_path,
            "--profile",
            "native-v1",
            "--merge",
            "--actor",
            "testuser",
        ])
        .current_dir(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("never native checkpoint input"));
}
