//! Full opaque authorization values, raw provenance and atomic scrub witness.
//! Inputs are independently assembled in private, identity-fenced stores.
use assert_cmd::Command;
use bead_rs::scan::{self, Field, ScanConfig};
use bead_rs::store::{open_configured_connection, SqliteStore};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

fn command(root: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command
        .current_dir(root)
        .env("BEAD_ORG_SECRET_SCANNER", "off");
    command
}

#[test]
fn authorization_captures_complete_printable_punctuation_and_deduplicates_views() {
    for candidate in [
        format!(
            "{}!{}@{}",
            "aB3".repeat(4),
            "cD4".repeat(4),
            "eF5".repeat(4)
        ),
        format!("{}%2f{}", "aB3".repeat(8), "cD4".repeat(4)),
    ] {
        for header in [
            "Authorization: Bearer",
            "authorization=Basic",
            "AUTHORIZATION: Token",
            "authorization: apikey",
            "Bearer",
        ] {
            let text = format!("{header} {candidate}");
            let report = scan::scan(
                &ScanConfig::enforce(),
                "owned-authorization-fixture",
                &[Field::new("description", &text)],
            );
            let findings: Vec<_> = report
                .blocking
                .iter()
                .filter(|finding| finding.rule_id == "authorization-header-credential")
                .collect();
            assert_eq!(
                findings.len(),
                1,
                "authorization value missing, partial or duplicated"
            );
            let start = header.len() + 1;
            assert_eq!((findings[0].start, findings[0].end), (start, text.len()));
            assert!(
                findings[0].fingerprint
                    == scan::fingerprint::compute(
                        scan::RULESET_VERSION,
                        "authorization-header-credential",
                        "owned-authorization-fixture",
                        "description",
                        start,
                        text.len(),
                        candidate.as_bytes(),
                    )
            );
        }
    }
}

#[test]
fn authorization_header_and_bare_bearer_use_their_distinct_minimums() {
    let alphabet = "aB3cD4eF5gH6iJ7kL8mN9";
    for length in [11, 12, 19, 20] {
        let candidate = &alphabet[..length];
        for header in [
            "Authorization: Bearer",
            "authorization=Basic",
            "AUTHORIZATION: Token",
            "authorization: apikey",
            "Bearer",
        ] {
            let text = format!("{header} {candidate}");
            let report = scan::scan(
                &ScanConfig::enforce(),
                "owned-authorization-fixture",
                &[Field::new("description", &text)],
            );
            let findings: Vec<_> = report
                .blocking
                .iter()
                .filter(|finding| finding.rule_id == "authorization-header-credential")
                .collect();
            let expected = length >= if header == "Bearer" { 20 } else { 12 };
            assert_eq!(findings.len(), usize::from(expected));
            if expected {
                assert_eq!(
                    (findings[0].start, findings[0].end),
                    (header.len() + 1, text.len())
                );
            }
        }
    }
}

#[test]
fn full_authorization_finding_is_selectable_by_all_blocking_atomic_redaction() {
    let root = tempfile::Builder::new()
        .prefix("authorization-redaction-")
        .tempdir_in("/var/tmp")
        .unwrap();
    fs::create_dir(root.path().join(".beads")).unwrap();
    fs::write(
        root.path().join(".beads/config.json"),
        serde_json::to_vec(
            &json!({"version":1,"uuid":uuid::Uuid::new_v4().to_string(),"prefix":"auth"}),
        )
        .unwrap(),
    )
    .unwrap();
    command(root.path())
        .args(["init", "--no-auto-flush"])
        .assert()
        .success();
    let candidate = format!("{}%2f{}", "aB3".repeat(8), "cD4".repeat(4));
    let text = format!("Authorization: Bearer {candidate}");
    let mut store = SqliteStore::from_conn(
        open_configured_connection(&root.path().join(".beads/beads.db")).unwrap(),
    );
    store.conn().execute(
        "INSERT INTO issues (id,title,description,priority,issue_type,base_status,created_at,updated_at,revision)
         VALUES ('auth-1','stable',?1,2,'task','open','2026-10-06T00:00:00Z','2026-10-06T00:00:00Z',1)", [&text],
    ).unwrap();
    let before: (String, i64) = store
        .conn()
        .query_row(
            "SELECT description,revision FROM issues WHERE id='auth-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let dry_run = command(root.path())
        .args([
            "redact",
            "--all-blocking",
            "--actor",
            "fixture-operator",
            "--reason",
            "synthetic conformance",
            "--dry-run",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        dry_run.status.success(),
        "all-blocking dry-run refused the complete value"
    );
    let after_preview: (String, i64) = store
        .conn()
        .query_row(
            "SELECT description,revision FROM issues WHERE id='auth-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(before == after_preview);
    let output = command(root.path())
        .args([
            "redact",
            "--all-blocking",
            "--actor",
            "fixture-operator",
            "--reason",
            "synthetic conformance",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "all-blocking redaction refused the complete value"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains(&candidate));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(&candidate));
    let _: Value = serde_json::from_slice(&output.stdout).unwrap();
    let after: (String, i64) = store
        .conn()
        .query_row(
            "SELECT description,revision FROM issues WHERE id='auth-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(after.0, "Authorization: Bearer [REDACTED:bead-rs]");
    assert_eq!(after.1, before.1 + 1);
}
