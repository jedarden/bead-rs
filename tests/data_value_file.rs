//! File/stdin transport keeps structured-data validation and scanning intact.

use assert_cmd::Command;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::Path;

fn command(root: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command
        .current_dir(root)
        .env("BEAD_ORG_SECRET_SCANNER", "off")
        .args(["--skip-foreign-workspace", "--no-auto-flush"]);
    command
}

fn workspace() -> (tempfile::TempDir, String) {
    let dir = tempfile::Builder::new()
        .prefix("bead-data-input-")
        .tempdir_in("/var/tmp")
        .unwrap();
    command(dir.path()).arg("init").assert().success();
    let output = command(dir.path())
        .args(["create", "--title", "Structured data input"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let id = String::from_utf8(output.stdout).unwrap().trim().to_owned();
    (dir, id)
}

fn set(root: &Path, id: &str, namespace: &str, schema: &str) -> Command {
    let mut command = command(root);
    command.args([
        "data",
        "set",
        "--id",
        id,
        "--namespace",
        namespace,
        "--schema-ref",
        schema,
    ]);
    command
}

fn counts(root: &Path) -> (i64, i64) {
    Connection::open(root.join(".beads/beads.db"))
        .unwrap()
        .query_row(
            "SELECT (SELECT COUNT(*) FROM issue_data), (SELECT COUNT(*) FROM events)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

fn attach_input(command: &mut Command, root: &Path, stdin: bool, input: &[u8]) {
    if stdin {
        command.args(["--value-file", "-"]).write_stdin(input);
    } else {
        std::fs::write(root.join("value.json"), input).unwrap();
        command.args(["--value-file", "value.json"]);
    }
}

#[test]
fn file_and_stdin_round_trip_payload_larger_than_argument_limit() {
    let (workspace, id) = workspace();
    let root = workspace.path();
    let value = json!({"history": "checkpoint complete\n".repeat(12_000), "unicode": "✓"});
    let input = format!(" \n{}\n ", serde_json::to_string_pretty(&value).unwrap());
    assert!(input.len() > 128 * 1024);

    for stdin in [false, true] {
        let namespace = if stdin { "stdin" } else { "file" };
        let mut command = set(root, &id, namespace, "schema:test");
        attach_input(&mut command, root, stdin, input.as_bytes());
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("checkpoint complete"));

        let output = self::command(root)
            .args([
                "data",
                "get",
                "--id",
                &id,
                "--namespace",
                namespace,
                "--json",
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(actual["value"], value);
    }
}

#[test]
fn value_source_is_required_and_mutually_exclusive() {
    let (workspace, id) = workspace();
    let root = workspace.path();
    let before = counts(root);
    set(root, &id, "test", "schema:test").assert().code(2);
    for args in [
        ["--value", "{}", "--value-file", "missing.json"],
        ["--value-file", "missing.json", "--value", "{}"],
    ] {
        let output = set(root, &id, "test", "schema:test")
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be used with"));
    }
    assert_eq!(counts(root), before);
}

#[test]
fn invalid_json_from_file_or_stdin_is_rejected_without_mutation_or_echo() {
    let (workspace, id) = workspace();
    let root = workspace.path();
    let before = counts(root);
    let input = b"{\"private_marker\": \"do not echo this input\", invalid}";
    for stdin in [false, true] {
        let mut command = set(root, &id, "test", "schema:test");
        attach_input(&mut command, root, stdin, input);
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("Invalid JSON value"));
        assert!(!stderr.contains("private_marker"));
        assert!(output.stdout.is_empty());
        assert_eq!(counts(root), before);
    }
}

#[test]
fn missing_file_and_non_utf8_input_fail_without_mutation() {
    let (workspace, id) = workspace();
    let root = workspace.path();
    let before = counts(root);
    let output = set(root, &id, "test", "schema:test")
        .args(["--value-file", "missing.json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("I/O error"));
    for stdin in [false, true] {
        let mut command = set(root, &id, "test", "schema:test");
        attach_input(&mut command, root, stdin, &[0xff, 0xfe]);
        let output = command.output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("I/O error"));
    }
    assert_eq!(counts(root), before);
}

#[test]
fn file_and_stdin_preserve_service_schema_and_namespace_validation() {
    let (workspace, id) = workspace();
    let root = workspace.path();
    let before = counts(root);
    for stdin in [false, true] {
        for (namespace, schema, diagnostic) in [
            ("test", "", "Schema reference cannot be empty"),
            ("INVALID", "schema:test", "Namespace must start"),
        ] {
            let mut command = set(root, &id, namespace, schema);
            attach_input(&mut command, root, stdin, b"{}");
            let output = command.output().unwrap();
            assert_eq!(output.status.code(), Some(2));
            assert!(String::from_utf8_lossy(&output.stderr).contains(diagnostic));
            assert_eq!(counts(root), before);
        }
    }
}

#[test]
fn large_file_and_stdin_keep_secret_rejection_atomic_and_redacted() {
    let (workspace, id) = workspace();
    let root = workspace.path();
    let before = counts(root);
    // Synthetic fixture assembled here; no credential is supplied in argv.
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let synthetic: String = (0..40)
        .map(|index| alphabet[(index * 11 + 7) % alphabet.len()] as char)
        .collect();
    let assignment = format!("BEDROCK_AWS_SECRET_ACCESS_KEY={synthetic}");
    let input =
        json!({"history": "checkpoint complete\n".repeat(12_000), "tail": assignment}).to_string();
    assert!(input.len() > 128 * 1024);
    for stdin in [false, true] {
        let mut command = set(root, &id, "test", "schema:test");
        attach_input(&mut command, root, stdin, input.as_bytes());
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("secret_detected"));
        assert!(stderr.contains("data.value"));
        assert!(!stderr.contains(&synthetic));
        assert!(output.stdout.is_empty());
        assert_eq!(counts(root), before);
    }
}
