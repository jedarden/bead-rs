//! Conformance tests for the committed checkpoint fixtures under
//! `tests/fixtures/attempts/`.
//!
//! These fixtures are the shared baseline every compatibility test builds on, so
//! they are validated here against the *contract* rather than against whatever
//! binary happens to be in the build directory. A binary can lag the source: the
//! pointer writer gained `attempt_outcome_count` and the restore verifier began
//! summing it at different times, so a stale binary rejects a well-formed fixture
//! with a record-count error that is not the fixture's fault. Asserting the
//! invariants directly keeps that from being misread as fixture breakage.
//!
//! Covered here:
//!
//! * pointer/manifest structure for all four fixtures (old/new x
//!   monolithic/sharded)
//! * record counts reconciling against the pointer, including attempt outcomes
//! * the full outcome vocabulary (5 values) and action vocabulary (5 values)
//!   from `src/model/attempt.rs`
//! * the full event-kind vocabulary (6 values) from the
//!   `urn:bead-rs:schema:event:native-v1` enum in `src/service/schema.rs`, plus
//!   the `created` event that issue creation writes outside that enum
//! * the old/new feature boundary: pre-feature fixtures carry no
//!   `attempt_outcome` records and no `attempt_outcome_shards` manifest key
//! * a live `sync import-only` restore of every fixture into an empty workspace

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::assert::{Assert, OutputAssertExt};
use assert_cmd::cargo::CommandCargoExt;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

/// The outcome vocabulary of `attempt-outcome-v1`, from `src/model/attempt.rs`.
const ALL_OUTCOMES: [&str; 5] = [
    "verified_success",
    "work_failure",
    "infrastructure_failure",
    "cancelled",
    "indeterminate",
];

/// The action vocabulary of `attempt-outcome-v1`, from `src/model/attempt.rs`.
const ALL_ACTIONS: [&str; 5] = ["close", "release", "quarantine", "block", "none"];

/// Every combination accepted by `validate_outcome_action_combo`.
const VALID_OUTCOME_ACTION_PAIRS: [(&str, &str); 15] = [
    ("verified_success", "close"),
    ("verified_success", "none"),
    ("verified_success", "release"),
    ("work_failure", "close"),
    ("work_failure", "quarantine"),
    ("work_failure", "release"),
    ("work_failure", "none"),
    ("infrastructure_failure", "none"),
    ("infrastructure_failure", "release"),
    ("cancelled", "close"),
    ("cancelled", "release"),
    ("cancelled", "none"),
    ("indeterminate", "block"),
    ("indeterminate", "release"),
    ("indeterminate", "none"),
];

/// The `audit_event.kind` enum, from `src/service/schema.rs`.
const ENUM_EVENT_KINDS: [&str; 6] = [
    "updated",
    "claimed",
    "released",
    "reopened",
    "closed",
    "assignment_cleared",
];

/// `created` is written by issue creation but deliberately sits outside the
/// `audit_event.kind` enum, so it is asserted separately.
const CREATED_EVENT_KIND: &str = "created";

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/attempts")
}

/// One fixture: a pointer plus either a monolithic JSONL or a sharded object set.
struct Fixture {
    /// e.g. "new/sharded"
    label: &'static str,
    /// Whether this fixture predates the attempt-resolution feature.
    pre_feature: bool,
    /// Whether this fixture is expected to carry attempt_outcome records.
    with_outcomes: bool,
}

const FIXTURES: [Fixture; 4] = [
    Fixture {
        label: "old",
        pre_feature: true,
        with_outcomes: false,
    },
    Fixture {
        label: "old/sharded",
        pre_feature: true,
        with_outcomes: false,
    },
    Fixture {
        label: "new",
        pre_feature: false,
        with_outcomes: true,
    },
    Fixture {
        label: "new/sharded",
        pre_feature: false,
        with_outcomes: true,
    },
];

impl Fixture {
    fn dir(&self) -> PathBuf {
        fixtures_dir().join(self.label)
    }

    fn is_sharded(&self) -> bool {
        self.label.ends_with("sharded")
    }

    fn expected_mode(&self) -> &'static str {
        if self.is_sharded() {
            "sharded"
        } else {
            "monolithic"
        }
    }

    fn pointer_path(&self) -> PathBuf {
        self.dir().join("current.json")
    }

    /// Every record in the fixture, regardless of layout.
    fn records(&self) -> Vec<Value> {
        if self.is_sharded() {
            let objects = self.dir().join("objects");
            let mut paths: Vec<PathBuf> = fs::read_dir(&objects)
                .unwrap_or_else(|e| panic!("{}: read objects dir: {e}", self.label))
                .map(|e| e.unwrap().path())
                .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
                .collect();
            paths.sort();
            paths.iter().flat_map(|p| read_jsonl(p)).collect()
        } else {
            read_jsonl(&self.dir().join("checkpoint.jsonl"))
        }
    }

    /// The inner manifest for sharded fixtures (absent for monolithic).
    fn inner_manifest(&self) -> Option<Value> {
        if !self.is_sharded() {
            return None;
        }
        let manifests = self.dir().join("manifests");
        let mut paths: Vec<PathBuf> = fs::read_dir(&manifests)
            .unwrap_or_else(|e| panic!("{}: read manifests dir: {e}", self.label))
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        assert_eq!(
            paths.len(),
            1,
            "{}: expected exactly one inner manifest",
            self.label
        );
        let path = paths.remove(0);
        Some(serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap())
    }
}

fn read_jsonl(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{}: {}: {e}", path.display(), "read"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .enumerate()
        .map(|(i, line)| {
            serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("{} line {}: {e}", path.display(), i + 1))
        })
        .collect()
}

fn sha256_hex(path: &Path) -> String {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("{}: read: {e}", path.display()));
    format!("{:x}", Sha256::digest(bytes))
}

fn pointer(fixture: &Fixture) -> Value {
    let raw = fs::read_to_string(fixture.pointer_path())
        .unwrap_or_else(|e| panic!("{}: read pointer: {e}", fixture.label));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{}: parse pointer: {e}", fixture.label))
}

fn count_records(records: &[Value], record_type: &str) -> usize {
    records
        .iter()
        .filter(|r| r["record_type"].as_str() == Some(record_type))
        .count()
}

/// Distinct values of a field across every attempt_outcome record.
fn distinct_outcome_field(records: &[Value], field: &str) -> BTreeSet<String> {
    records
        .iter()
        .filter(|r| r["record_type"] == "attempt_outcome")
        .filter_map(|r| r["attempt_outcome"][field].as_str())
        .map(str::to_string)
        .collect()
}

fn distinct_event_kinds(records: &[Value]) -> BTreeSet<String> {
    records
        .iter()
        .filter(|r| r["record_type"] == "event")
        .filter_map(|r| r["event"]["kind"].as_str())
        .map(str::to_string)
        .collect()
}

// ---- pointer structure ------------------------------------------------------

#[test]
fn pointer_matches_declared_mode_and_schema() {
    for fixture in &FIXTURES {
        let pointer = pointer(fixture);
        assert_eq!(
            pointer["schema_version"].as_u64(),
            Some(1),
            "{}: schema_version",
            fixture.label
        );
        assert_eq!(
            pointer["mode"].as_str(),
            Some(fixture.expected_mode()),
            "{}: mode must match its directory layout",
            fixture.label
        );
        for field in [
            "generation_id",
            "store_uuid",
            "total_record_count",
            "issue_count",
            "event_count",
            "receipt_count",
        ] {
            assert!(
                !pointer[field].is_null(),
                "{}: pointer field {field} must be present",
                fixture.label
            );
        }
    }
}

#[test]
fn attempt_outcome_count_marks_the_feature_boundary_in_pointers() {
    for fixture in &FIXTURES {
        let pointer = pointer(fixture);
        assert_eq!(
            pointer["attempt_outcome_count"].is_null(),
            fixture.pre_feature,
            "{}: attempt_outcome_count presence must match the feature boundary \
             (present post-feature, absent pre-feature)",
            fixture.label
        );
    }
}

#[test]
fn pointer_record_counts_reconcile_with_contents() {
    for fixture in &FIXTURES {
        let pointer = pointer(fixture);
        let records = fixture.records();

        let issues = count_records(&records, "issue");
        let events = count_records(&records, "event");
        let outcomes = count_records(&records, "attempt_outcome");

        assert_eq!(
            pointer["issue_count"].as_u64(),
            Some(issues as u64),
            "{}: issue_count",
            fixture.label
        );
        assert_eq!(
            pointer["event_count"].as_u64(),
            Some(events as u64),
            "{}: event_count",
            fixture.label
        );
        assert_eq!(
            pointer["total_record_count"].as_u64(),
            Some((issues + events + outcomes) as u64),
            "{}: total_record_count must equal issues + events + attempt outcomes \
             (receipt_count is asserted separately and is 0 in every fixture)",
            fixture.label
        );

        let declared_outcomes = pointer["attempt_outcome_count"].as_u64();
        if fixture.with_outcomes {
            assert_eq!(
                declared_outcomes,
                Some(outcomes as u64),
                "{}: attempt_outcome_count",
                fixture.label
            );
        } else {
            assert_eq!(
                outcomes, 0,
                "{}: pre-feature must carry no outcomes",
                fixture.label
            );
        }
    }
}

#[test]
fn active_roots_and_sharded_objects_are_content_addressed() {
    for fixture in &FIXTURES {
        let pointer = pointer(fixture);
        let root = &pointer["active_root"];
        let root_path = fixture.dir().join(root["path"].as_str().unwrap());
        let declared_root_hash = root["sha256"].as_str().unwrap();
        assert_eq!(
            sha256_hex(&root_path),
            declared_root_hash,
            "{}: active root hash",
            fixture.label
        );

        if !fixture.is_sharded() {
            continue;
        }

        assert_eq!(
            root_path.file_stem().and_then(|value| value.to_str()),
            Some(declared_root_hash),
            "{}: manifest filename must equal its SHA-256",
            fixture.label
        );

        let manifest = fixture.inner_manifest().unwrap();
        for shard_field in [
            "issue_shards",
            "event_shards",
            "receipt_shards",
            "attempt_outcome_shards",
            "redaction_shards",
        ] {
            let Some(shards) = manifest.get(shard_field).and_then(Value::as_array) else {
                let allowed_legacy_omission = match shard_field {
                    "attempt_outcome_shards" => fixture.pre_feature,
                    "redaction_shards" => manifest["redaction_record_count"].is_null(),
                    _ => false,
                };
                assert!(
                    allowed_legacy_omission,
                    "{}: unexpected missing shard role {shard_field}",
                    fixture.label
                );
                continue;
            };
            for shard in shards {
                let relative = shard["path"].as_str().unwrap();
                let path = fixture.dir().join(relative);
                let declared_hash = shard["sha256"].as_str().unwrap();
                assert_eq!(
                    sha256_hex(&path),
                    declared_hash,
                    "{}: hash for {relative}",
                    fixture.label
                );
                assert_eq!(
                    path.file_stem().and_then(|value| value.to_str()),
                    Some(declared_hash),
                    "{}: object filename for {relative}",
                    fixture.label
                );
                assert_eq!(
                    fs::metadata(&path).unwrap().len(),
                    shard["byte_length"].as_u64().unwrap(),
                    "{}: byte length for {relative}",
                    fixture.label
                );
                assert_eq!(
                    read_jsonl(&path).len() as u64,
                    shard["record_count"].as_u64().unwrap(),
                    "{}: record count for {relative}",
                    fixture.label
                );
            }
        }
    }
}

// ---- old/new feature boundary ----------------------------------------------

#[test]
fn sharded_inner_manifest_marks_the_feature_boundary() {
    for fixture in &FIXTURES {
        let Some(manifest) = fixture.inner_manifest() else {
            continue;
        };
        assert_eq!(
            manifest
                .as_object()
                .unwrap()
                .contains_key("attempt_outcome_shards"),
            !fixture.pre_feature,
            "{}: attempt_outcome_shards key must exist post-feature and be absent \
             pre-feature -- a post-feature binary writing the old fixture would \
             silently turn it into a new-format manifest",
            fixture.label
        );
    }
}

#[test]
fn pre_feature_fixtures_carry_no_attempt_outcomes() {
    for fixture in FIXTURES.iter().filter(|f| f.pre_feature) {
        let records = fixture.records();
        assert_eq!(
            count_records(&records, "attempt_outcome"),
            0,
            "{}: pre-feature fixtures must not carry attempt_outcome records",
            fixture.label
        );
    }
}

// ---- vocabulary coverage ----------------------------------------------------

#[test]
fn outcome_fixtures_cover_every_outcome_value() {
    for fixture in FIXTURES.iter().filter(|f| f.with_outcomes) {
        let seen = distinct_outcome_field(&fixture.records(), "outcome");
        let missing: Vec<_> = ALL_OUTCOMES
            .iter()
            .filter(|o| !seen.contains(**o))
            .collect();
        assert!(
            missing.is_empty(),
            "{}: outcome values not covered: {missing:?} (seen: {:?})",
            fixture.label,
            seen
        );
    }
}

#[test]
fn outcome_fixtures_cover_every_action_value() {
    for fixture in FIXTURES.iter().filter(|f| f.with_outcomes) {
        let seen = distinct_outcome_field(&fixture.records(), "action");
        let missing: Vec<_> = ALL_ACTIONS.iter().filter(|a| !seen.contains(**a)).collect();
        assert!(
            missing.is_empty(),
            "{}: action values not covered: {missing:?} (seen: {:?})",
            fixture.label,
            seen
        );
    }
}

#[test]
fn sharded_fixtures_cover_create_update_close_reopen_claim_and_release() {
    for fixture in FIXTURES.iter().filter(|f| f.is_sharded()) {
        let seen = distinct_event_kinds(&fixture.records());
        for kind in ENUM_EVENT_KINDS {
            assert!(
                seen.contains(kind),
                "{}: event kind '{kind}' missing from the schema enum coverage \
                 (seen: {:?})",
                fixture.label,
                seen
            );
        }
        assert!(
            seen.contains(CREATED_EVENT_KIND),
            "{}: 'created' event missing -- issue creation writes it outside the \
             audit_event.kind enum, so it is asserted separately",
            fixture.label
        );
    }
}

#[test]
fn event_kinds_stay_inside_the_schema_vocabulary() {
    // Every kind a fixture carries must be either in the enum or the standalone
    // `created`. Anything else means either the enum in src/service/schema.rs
    // moved or a fixture was produced by a foreign writer.
    for fixture in &FIXTURES {
        for kind in distinct_event_kinds(&fixture.records()) {
            assert!(
                ENUM_EVENT_KINDS.contains(&kind.as_str()) || kind == CREATED_EVENT_KIND,
                "{}: event kind '{kind}' is not in the audit_event.kind enum",
                fixture.label
            );
        }
    }
}

#[test]
fn outcome_action_pairs_are_valid_combinations() {
    for fixture in FIXTURES.iter().filter(|f| f.with_outcomes) {
        let records = fixture.records();
        for record in records
            .iter()
            .filter(|r| r["record_type"] == "attempt_outcome")
        {
            let outcome = record["attempt_outcome"]["outcome"].as_str().unwrap();
            let action = record["attempt_outcome"]["action"].as_str().unwrap();
            assert!(
                VALID_OUTCOME_ACTION_PAIRS.contains(&(outcome, action)),
                "{}: pair ({outcome}, {action}) is not a valid outcome/action \
                 combination",
                fixture.label
            );
        }
    }
}

#[test]
fn monolithic_outcomes_cover_every_valid_outcome_action_pair() {
    let fixture = FIXTURES.iter().find(|f| f.label == "new").unwrap();
    let seen: BTreeSet<(String, String)> = fixture
        .records()
        .into_iter()
        .filter(|record| record["record_type"] == "attempt_outcome")
        .map(|record| {
            (
                record["attempt_outcome"]["outcome"]
                    .as_str()
                    .unwrap()
                    .to_string(),
                record["attempt_outcome"]["action"]
                    .as_str()
                    .unwrap()
                    .to_string(),
            )
        })
        .collect();
    let expected: BTreeSet<(String, String)> = VALID_OUTCOME_ACTION_PAIRS
        .iter()
        .map(|(outcome, action)| (outcome.to_string(), action.to_string()))
        .collect();
    assert_eq!(seen, expected);
}

#[test]
fn attempt_outcome_records_carry_the_required_fields() {
    const REQUIRED: [&str; 12] = [
        "attempt_id",
        "issue_id",
        "outcome",
        "action",
        "reason",
        "canonical_request_hash",
        "resulting_issue_revision",
        "resulting_state",
        "resulting_attempt_tier",
        "receipt_id",
        "actor",
        "created_at",
    ];
    for fixture in FIXTURES.iter().filter(|f| f.with_outcomes) {
        for record in fixture.records() {
            if record["record_type"] != "attempt_outcome" {
                continue;
            }
            let outcome = &record["attempt_outcome"];
            for field in REQUIRED {
                assert!(
                    !outcome[field].is_null(),
                    "{}: attempt_outcome missing required field '{field}'",
                    fixture.label
                );
            }
            let schema_ref = outcome
                .get("$schema")
                .or_else(|| outcome.get("schema_ref"))
                .and_then(Value::as_str);
            assert!(
                schema_ref.is_some_and(|s| { s == "urn:bead-rs:schema:attempt-outcome:native-v1" }),
                "{}: unexpected schema_ref",
                fixture.label
            );
            assert!(
                outcome["canonical_request_hash"]
                    .as_str()
                    .is_some_and(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())),
                "{}: canonical_request_hash must be 64 hex chars",
                fixture.label
            );
        }
    }
}

// ---- live restore -----------------------------------------------------------

fn run_bead(dir: &Path, args: &[&str]) -> Assert {
    Command::cargo_bin("bead")
        .unwrap()
        .args(args)
        .current_dir(dir)
        .assert()
}

#[test]
fn every_fixture_restores_into_an_empty_workspace() {
    for fixture in &FIXTURES {
        let workspace = TempDir::new().unwrap();

        run_bead(
            workspace.path(),
            &["init", "--prefix", "bead", "--skip-foreign-workspace"],
        )
        .success();

        // Monolithic fixtures were captured with auto_flush publishing only the
        // JSONL+pointer pair, so the restore source is the fixture directory.
        let source = if fixture.is_sharded() {
            fixture.dir().join("current.json")
        } else {
            fixture.dir().join("checkpoint.jsonl")
        };
        let source_str = source.to_str().unwrap();

        run_bead(
            workspace.path(),
            &[
                "sync",
                "import-only",
                "--input",
                source_str,
                "--restore-into-empty",
                "--actor",
                "fixture-conformance",
            ],
        )
        .success();

        let db = workspace.path().join(".beads/beads.db");
        let conn = rusqlite::Connection::open(&db).unwrap();

        let pointer = pointer(fixture);
        let issues: i64 = conn
            .query_row("SELECT count(*) FROM issues", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            issues as u64,
            pointer["issue_count"].as_u64().unwrap(),
            "{}: restored issue count",
            fixture.label
        );

        let events: i64 = conn
            .query_row("SELECT count(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            events as u64,
            pointer["event_count"].as_u64().unwrap(),
            "{}: restored event count",
            fixture.label
        );

        let outcomes: i64 = conn
            .query_row("SELECT count(*) FROM attempt_outcomes", [], |r| r.get(0))
            .unwrap();
        let expected_outcomes = count_records(&fixture.records(), "attempt_outcome") as i64;
        assert_eq!(
            outcomes, expected_outcomes,
            "{}: restored attempt_outcome count",
            fixture.label
        );
    }
}
