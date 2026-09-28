//! Unknown-field interchange conformance suite
//!
//! AGENTS.md makes unknown-field preservation a hard compatibility rule:
//! "Unknown JSON fields must survive import/export round trips", with
//! `.beads/issues.jsonl` named as the interchange checkpoint. This suite
//! walks one committed payload — the `tests/fixtures/extensions/` records,
//! whose 15 unknown fields cover the shapes a preservation contract can get
//! wrong (deep nesting, empty containers, null, the empty-string key,
//! non-ASCII keys and values, exact scalar types) — through every leg of
//! that rule and asserts the fields come back recursively identical:
//!
//! * `sync flush-only` publishes a workspace generation carrying the full
//!   payload, in both the monolithic and the sharded checkpoint mode
//! * the two recovery commands rebuild a store from that generation with
//!   the payload intact — the R036 verified `bead restore` and the
//!   lower-level `sync import-only --restore-into-empty`
//! * the legs chain: publish → restore → republish (other mode) → import →
//!   republish → restore, with the payload diffed after every hop
//! * the bare `.beads/issues.jsonl` interchange shape (`sync flush-only
//!   --output`, one `Issue` per line, unknown fields flattened at the top
//!   level) round-trips back through `sync import-only` in both the
//!   restore-into-empty and merge modes, and from the restore-into-empty
//!   store back into a verified checkpoint generation
//! * a preservation failure is loud and publishes nothing: when stored
//!   extension state cannot be re-projected, every publication path exits
//!   nonzero naming the field, and the checkpoint pointer, objects, and
//!   manifests are left untouched — never a silently shrunken generation
//!
//! Companion suites pin narrower slices of the same contract:
//! `checkpoint_unknown_field_round_trip.rs` (the forensic generation chain
//! and merge semantics), `checkpoint_restore_unknown_fields.rs` (restore of
//! the committed fixtures), `unknown_field_side_export.rs` (the side export
//! pair), and `checkpoint_unknown_field_record_corpus.rs` (a hand-written
//! corpus). This file owns the full publish × recover × mode matrix as one
//! conformance walk, plus the loud-failure guarantee.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Every key an issue object may carry that is NOT an unknown extension:
/// the serialized fields of `Issue` (src/model.rs) plus the projected
/// collections the checkpoint layer embeds and routes to their own tables.
const KNOWN_ISSUE_KEYS: [&str; 24] = [
    "id",
    "title",
    "revision",
    "description",
    "notes",
    "priority",
    "base_status",
    "manual_blocked",
    "assignee",
    "claim_epoch",
    "issue_type",
    "created_at",
    "updated_at",
    "closed_at",
    "close_reason",
    "source_repo",
    "profile",
    "schema_ref",
    "data",
    "dependencies",
    "labels",
    "comments",
    "external_references",
    "resource_keys",
];

/// The fixture issues whose unknown fields this suite walks. Three issues,
/// fifteen unknown fields.
const FIXTURE_ISSUE_COUNT: usize = 3;
const EXPECTED_UNKNOWN_FIELD_COUNT: usize = 15;

/// The open fixture issue: the only one a probing mutation can touch
/// without a claim-epoch credential.
const OPEN_ISSUE_ID: &str = "bead-4e2505fa";

/// The extension value the loud-failure leg corrupts.
const CORRUPTED_KEY: &str = "weighting";

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extensions")
}

fn bead(dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.args(args).current_dir(dir);
    cmd
}

fn init_workspace(workspace: &Path) {
    bead(
        workspace,
        &["init", "--prefix", "bead", "--skip-foreign-workspace"],
    )
    .assert()
    .success();
}

/// Replace the `checkpoint` section of the workspace config. Publication is
/// pinned to the explicit `sync flush-only` (auto_flush off) so the test
/// observes exactly one publisher, with an optional forced mode.
fn set_checkpoint_config(workspace: &Path, checkpoint: Value) {
    let config_path = workspace.join(".beads/config.json");
    let mut config: Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config
        .as_object_mut()
        .unwrap()
        .insert("checkpoint".to_string(), checkpoint);
    fs::write(&config_path, serde_json::to_string(&config).unwrap()).unwrap();
}

fn monolithic_config() -> Value {
    json!({ "auto_flush": false })
}

fn sharded_config() -> Value {
    json!({ "auto_flush": false, "mode": "sharded" })
}

fn read_jsonl(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn read_pointer(path: impl AsRef<Path>) -> Value {
    serde_json::from_str(&fs::read_to_string(path.as_ref()).unwrap()).unwrap()
}

/// The issue records of the committed monolithic fixture.
fn fixture_issue_records() -> Vec<Value> {
    read_jsonl(&fixture_dir().join("checkpoint.jsonl"))
        .into_iter()
        .filter(|record| record["record_type"] == "issue")
        .collect()
}

/// The unknown fields of each fixture issue, keyed by issue ID: every key
/// the issue object carries beyond the known native fields and projections.
fn expected_extensions(records: &[Value]) -> HashMap<String, serde_json::Map<String, Value>> {
    let mut extensions = HashMap::new();
    for record in records {
        let issue = &record["issue"];
        let mut fields = serde_json::Map::new();
        for (key, value) in issue.as_object().unwrap() {
            if !KNOWN_ISSUE_KEYS.contains(&key.as_str()) {
                fields.insert(key.clone(), value.clone());
            }
        }
        extensions.insert(issue["id"].as_str().unwrap().to_string(), fields);
    }
    extensions
}

/// The unknown fields of one bare issue record (the issues.jsonl shape).
fn unknown_fields_of(issue: &Value) -> serde_json::Map<String, Value> {
    issue
        .as_object()
        .unwrap()
        .iter()
        .filter(|(key, _)| !KNOWN_ISSUE_KEYS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// Assert a payload's unknown fields are exactly the fixture's, nesting
/// included. The map-wide equality is backed by pointed sentinels per
/// issue, so a preservation regression fails naming the exact shape it
/// broke rather than only through an opaque whole-map diff.
fn assert_payload_exact(
    fields_by_issue: &HashMap<String, serde_json::Map<String, Value>>,
    label: &str,
) {
    let expected = expected_extensions(&fixture_issue_records());
    assert_eq!(
        fields_by_issue.len(),
        FIXTURE_ISSUE_COUNT,
        "{label}: expected the payload on all {FIXTURE_ISSUE_COUNT} fixture issues"
    );
    let total: usize = fields_by_issue.values().map(|fields| fields.len()).sum();
    assert_eq!(
        total, EXPECTED_UNKNOWN_FIELD_COUNT,
        "{label}: the payload must arrive complete, never partially dropped"
    );
    for (issue_id, expected_fields) in &expected {
        let actual = fields_by_issue.get(issue_id).unwrap_or_else(|| {
            panic!("{label}: issue {issue_id} carries no unknown fields at all")
        });
        assert_eq!(
            actual, expected_fields,
            "{label}: unknown fields of {issue_id} not preserved recursively"
        );
    }
    // Spot-check the shape sentinels of each issue, so a preservation
    // regression fails with a pointed message rather than only through the
    // map-wide comparison above.
    let open = &fields_by_issue["bead-4e2505fa"];
    assert_eq!(
        open["ratio"],
        json!(0.125),
        "{label}: exactly representable float not preserved"
    );
    assert_eq!(
        open["vendor_metadata"]["tags"],
        json!(["alpha", "beta"]),
        "{label}: nested object array not preserved"
    );
    assert_eq!(
        open["annotation"],
        json!("multi\nline — ünknown ✓"),
        "{label}: newline, em-dash, and non-ASCII text not preserved"
    );
    let awkward = &fields_by_issue["bead-c06ed23d"];
    assert_eq!(
        awkward["trace_context"]["spans"],
        json!([{ "depth": 2, "id": "s1" }]),
        "{label}: two-level nested object array not preserved"
    );
    assert_eq!(
        awkward["cluster"],
        json!([1, "two", [3], { "four": 4 }]),
        "{label}: heterogeneous nested array not preserved"
    );
    assert_eq!(
        awkward[""],
        json!("empty-string key survives round trips"),
        "{label}: empty-string key not preserved"
    );
    assert_eq!(
        awkward["ghost"],
        Value::Null,
        "{label}: JSON null not preserved"
    );
    assert_eq!(
        awkward["hollow"],
        json!({}),
        "{label}: empty object not preserved"
    );
    assert_eq!(
        awkward["void"],
        json!([]),
        "{label}: empty array not preserved"
    );
    let closed = &fields_by_issue["bead-6ccd8451"];
    assert_eq!(
        closed["deeply"]["nested"]["structure"]["leaf"],
        json!([true, false, null]),
        "{label}: three-level nested leaf array not preserved"
    );
    assert_eq!(
        closed["ünkoded_key"],
        json!("vàlue-é"),
        "{label}: non-ASCII key and value not preserved"
    );
}

/// A workspace holding the fixture payload, recovered with the
/// restore-into-empty import.
fn seeded_workspace() -> TempDir {
    let workspace = TempDir::new().unwrap();
    init_workspace(workspace.path());
    bead(
        workspace.path(),
        &[
            "sync",
            "import-only",
            "--input",
            fixture_dir().join("checkpoint.jsonl").to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "unknown-field-conformance",
        ],
    )
    .assert()
    .success()
    .stderr(predicate::str::contains("Restored 3 issues"));
    workspace
}

/// Make the live store genuinely newer than its published generation so the
/// next `sync flush-only` is a real export, not an idempotent no-op.
fn make_dirty(workspace: &Path, marker: &str) {
    bead(
        workspace,
        &[
            "update",
            OPEN_ISSUE_ID,
            "--notes",
            &format!("dirty for conformance walk: {marker}"),
        ],
    )
    .assert()
    .success();
}

/// Publish the workspace with `sync flush-only` under `config` and return
/// the new pointer.
fn publish(workspace: &Path, config: Value, marker: &str) -> Value {
    set_checkpoint_config(workspace, config);
    make_dirty(workspace, marker);
    bead(workspace, &["sync", "flush-only"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Flushed forensic checkpoint:"));
    read_pointer(workspace.join(".beads/checkpoint/current.json"))
}

/// The `issue_extensions` rows of a workspace: issue ID -> key -> parsed
/// JSON value.
fn read_extensions(workspace: &Path) -> HashMap<String, serde_json::Map<String, Value>> {
    let db = workspace.join(".beads/beads.db");
    let conn = rusqlite::Connection::open(&db).unwrap();
    let mut stmt = conn
        .prepare("SELECT issue_id, key, value FROM issue_extensions")
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap();
    let mut extensions: HashMap<String, serde_json::Map<String, Value>> = HashMap::new();
    for row in rows {
        let (issue_id, key, value) = row.unwrap();
        let parsed: Value = serde_json::from_str(&value).unwrap();
        extensions.entry(issue_id).or_default().insert(key, parsed);
    }
    extensions
}

/// Recover a published generation with `bead restore` into a fresh
/// workspace and return the workspace and the parsed report.
fn verified_restore(source: &Path, actor: &str) -> (TempDir, Value) {
    let pointer = read_pointer(source.join("current.json"));
    let workspace = TempDir::new().unwrap();
    init_workspace(workspace.path());
    let output = bead(workspace.path(), &[])
        .args([
            "restore",
            "--source",
            source.to_str().unwrap(),
            "--generation",
            pointer["generation_id"].as_str().unwrap(),
            "--actor",
            actor,
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["generation_id"], pointer["generation_id"],
        "restore must land exactly the named generation"
    );
    assert_eq!(
        report["issues_restored"], pointer["issue_count"],
        "restore must land every issue of the generation"
    );
    assert_eq!(
        report["mode"], pointer["mode"],
        "the report must name the on-disk layout it verified"
    );
    (workspace, report)
}

/// Recover a checkpoint source with `sync import-only --restore-into-empty`
/// into a fresh workspace.
fn import_restore(source: &Path, actor: &str) -> TempDir {
    let workspace = TempDir::new().unwrap();
    init_workspace(workspace.path());
    bead(
        workspace.path(),
        &[
            "sync",
            "import-only",
            "--input",
            source.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            actor,
        ],
    )
    .assert()
    .success()
    .stderr(predicate::str::contains("Restored 3 issues"));
    workspace
}

/// Both recovery commands rebuild a published generation losslessly.
fn assert_both_recoveries(workspace: &Path, label: &str) {
    let checkpoint = workspace.join(".beads/checkpoint");

    let (restored, _report) = verified_restore(&checkpoint, "unknown-field-conformance");
    assert_payload_exact(
        &read_extensions(restored.path()),
        &format!("{label}: bead restore"),
    );

    let imported = import_restore(&checkpoint, "unknown-field-conformance");
    assert_payload_exact(
        &read_extensions(imported.path()),
        &format!("{label}: sync import-only --restore-into-empty"),
    );
}

// ---- the publish x recover x mode matrix -----------------------------------

/// Each checkpoint mode publishes a generation that both recovery commands
/// rebuild with the unknown fields recursively identical — the top-level
/// keys and every nested object, array, and exact scalar inside them.
#[test]
fn published_generations_recover_losslessly_in_both_modes() {
    for (mode, config) in [
        ("monolithic", monolithic_config()),
        ("sharded", sharded_config()),
    ] {
        let workspace = seeded_workspace();
        let pointer = publish(workspace.path(), config, mode);
        assert_eq!(
            pointer["mode"], mode,
            "the flush must publish in the configured mode"
        );
        assert_both_recoveries(workspace.path(), &format!("{mode} publication"));
    }
}

/// The full walk: publish monolithically, recover with `bead restore`,
/// republish shardedly, recover with the restore-into-empty import,
/// republish monolithically again, recover with `bead restore` — the
/// payload intact after every hop, and every hop a different
/// (command × mode) cell of the matrix.
#[test]
fn alternating_chain_crosses_every_command_and_both_modes() {
    let expected = expected_extensions(&fixture_issue_records());

    // Generation 1: monolithic publication of the fixture payload.
    let first = seeded_workspace();
    publish(first.path(), monolithic_config(), "chain gen 1");
    let (second, report) = verified_restore(
        first.path().join(".beads/checkpoint").as_path(),
        "unknown-field-conformance",
    );
    assert_eq!(report["mode"], "monolithic");
    assert_eq!(
        read_extensions(second.path()),
        expected,
        "hop 1 (monolithic flush -> bead restore): payload not preserved"
    );

    // Generation 2: the restored store republishes shardedly.
    let pointer = publish(second.path(), sharded_config(), "chain gen 2");
    assert_eq!(pointer["mode"], "sharded");
    let third = import_restore(
        second.path().join(".beads/checkpoint").as_path(),
        "unknown-field-conformance",
    );
    assert_eq!(
        read_extensions(third.path()),
        expected,
        "hop 2 (sharded flush -> sync import-only --restore-into-empty): payload not preserved"
    );

    // Generation 3: republish monolithically, recover with bead restore.
    publish(third.path(), monolithic_config(), "chain gen 3");
    let (fourth, report) = verified_restore(
        third.path().join(".beads/checkpoint").as_path(),
        "unknown-field-conformance",
    );
    assert_eq!(report["mode"], "monolithic");
    assert_eq!(
        report["generation_id"],
        read_pointer(third.path().join(".beads/checkpoint/current.json"))["generation_id"],
        "the final restore must land the final publication"
    );
    assert_eq!(
        read_extensions(fourth.path()),
        expected,
        "hop 3 (monolithic flush -> bead restore): payload not preserved"
    );
}

// ---- the .beads/issues.jsonl interchange ------------------------------------

/// The bare issue-only interchange shape carries the unknown fields
/// flattened at the top level of each record — no wrapper, no projections
/// leaking into the unknown set — and re-imports through both import modes.
#[test]
fn issues_jsonl_interchange_carries_unknown_fields_both_ways() {
    let expected = expected_extensions(&fixture_issue_records());

    // Publish the payload as a checkpoint, then write the bare interchange
    // file at the workspace root (the --output path is resolved against the
    // workspace root and must stay outside .beads/).
    let workspace = seeded_workspace();
    publish(workspace.path(), monolithic_config(), "interchange source");
    let interchange = workspace.path().join("issues.jsonl");
    bead(
        workspace.path(),
        &["sync", "flush-only", "--output", "issues.jsonl"],
    )
    .assert()
    .success()
    .stderr(predicate::str::contains("Exported issue-only checkpoint:"));

    // The interchange file is the bare shape: one Issue per line, unknown
    // fields flattened at the top level, nesting intact.
    let records = read_jsonl(&interchange);
    assert_eq!(
        records.len(),
        FIXTURE_ISSUE_COUNT,
        "the interchange file carries one bare record per issue"
    );
    assert!(
        records
            .iter()
            .all(|record| record.get("record_type").is_none()),
        "bare interchange records must not carry the forensic record envelope"
    );
    let mut exported = HashMap::new();
    for record in &records {
        exported.insert(
            record["id"].as_str().unwrap().to_string(),
            unknown_fields_of(record),
        );
    }
    assert_eq!(
        exported, expected,
        "the interchange export must flatten the unknown fields verbatim"
    );

    // Interchange back in: both import modes rebuild the payload exactly.
    let source = interchange.to_str().unwrap().to_string();
    let mut return_leg_store: Option<TempDir> = None;
    for (mode, extra) in [
        ("restore-into-empty", "--restore-into-empty"),
        ("merge", "--merge"),
    ] {
        let workspace = TempDir::new().unwrap();
        init_workspace(workspace.path());
        bead(
            workspace.path(),
            &[
                "sync",
                "import-only",
                "--input",
                &source,
                extra,
                "--actor",
                "unknown-field-conformance",
            ],
        )
        .assert()
        .success();
        assert_payload_exact(
            &read_extensions(workspace.path()),
            &format!("issues.jsonl interchange import ({mode})"),
        );
        // The bead-named recovery import seeds the return leg below. The
        // merge-mode store cannot: its merge receipt is stamped with an
        // empty source_store_uuid (the bare interchange shape carries no
        // store identity to copy), and every generation it publishes
        // afterwards fails receipt validation on restore with "Integrity
        // error: Provenance receipt 'merge-...' has invalid required
        // fields" — a recoverability defect of the merge-receipt writer,
        // not an unknown-field loss, tracked on beadrs-68e50219. The
        // merged store's payload itself is asserted complete above.
        if mode == "restore-into-empty" {
            return_leg_store = Some(workspace);
        }
    }
    let interchange_store = return_leg_store.expect("the restore-into-empty leg ran");

    // The return leg: a store rebuilt from the bare interchange file
    // publishes a verified checkpoint generation that `bead restore`
    // rebuilds with the payload intact — checkpoint to issues.jsonl and
    // back to checkpoint without loss.
    let pointer = publish(
        interchange_store.path(),
        sharded_config(),
        "interchange return leg",
    );
    assert_eq!(pointer["mode"], "sharded");
    let (restored, _report) = verified_restore(
        interchange_store.path().join(".beads/checkpoint").as_path(),
        "unknown-field-conformance",
    );
    assert_eq!(
        read_extensions(restored.path()),
        expected,
        "checkpoint -> issues.jsonl -> checkpoint: payload not preserved"
    );
}

// ---- preservation failure is loud ------------------------------------------

/// Count entries of a directory; a missing directory counts as zero.
fn entries_of(dir: &Path) -> usize {
    fs::read_dir(dir)
        .map(|entries| entries.count())
        .unwrap_or(0)
}

/// When stored unknown-field state cannot be re-projected, every
/// publication path must fail loudly naming the field — and publish
/// nothing: the pointer, objects, and manifests stay byte-for-byte where
/// they were, so no generation can silently ship with the field dropped.
#[test]
fn preservation_failure_is_loud_and_publishes_nothing() {
    let workspace = seeded_workspace();
    let checkpoint = workspace.path().join(".beads/checkpoint");
    let pointer_path = checkpoint.join("current.json");

    // Corrupt one stored unknown field out from under the checkpoint layer,
    // then make the store genuinely newer than its generation so the next
    // flush must re-project the whole store to publish.
    let db = workspace.path().join(".beads/beads.db");
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute(
            "UPDATE issue_extensions SET value = 'NOT-VALID-JSON'
             WHERE issue_id = ?1 AND key = ?2",
            [OPEN_ISSUE_ID, CORRUPTED_KEY],
        )
        .unwrap();
    }
    set_checkpoint_config(workspace.path(), monolithic_config());
    make_dirty(workspace.path(), "corrupted extension storage");
    let pointer_before = fs::read_to_string(&pointer_path).unwrap();
    let objects_before = entries_of(&checkpoint.join("objects"));
    let manifests_before = entries_of(&checkpoint.join("manifests"));

    let corruption = format!("Failed to parse extension '{CORRUPTED_KEY}'");

    // Monolithic flush: loud failure, nothing published.
    bead(workspace.path(), &["sync", "flush-only"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(&corruption));
    assert_eq!(
        fs::read_to_string(&pointer_path).unwrap(),
        pointer_before,
        "a failed flush must not advance the checkpoint pointer"
    );
    assert_eq!(
        entries_of(&checkpoint.join("objects")),
        objects_before,
        "a failed flush must not stage new objects"
    );

    // The bare issues.jsonl side export re-projects the same store: it must
    // refuse loudly too, and leave no half-written interchange file.
    let interchange = workspace.path().join("issues.jsonl");
    bead(
        workspace.path(),
        &["sync", "flush-only", "--output", "issues.jsonl"],
    )
    .assert()
    .failure()
    .stderr(predicate::str::contains(&corruption));
    assert!(
        !interchange.exists(),
        "a failed side export must not leave an interchange file behind"
    );

    // Sharded flush: the same loud refusal, and the sharded layout is just
    // as untouched as the monolithic one was.
    set_checkpoint_config(workspace.path(), sharded_config());
    bead(workspace.path(), &["sync", "flush-only"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(&corruption));
    assert_eq!(
        fs::read_to_string(&pointer_path).unwrap(),
        pointer_before,
        "a failed sharded flush must not advance the checkpoint pointer"
    );
    assert_eq!(
        entries_of(&checkpoint.join("objects")),
        objects_before,
        "a failed sharded flush must not stage new objects"
    );
    assert_eq!(
        entries_of(&checkpoint.join("manifests")),
        manifests_before,
        "a failed sharded flush must not stage new manifests"
    );

    // The corrupt field itself is still in the store and in the last good
    // generation: the failure refused to publish, it did not destroy.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        let value: String = conn
            .query_row(
                "SELECT value FROM issue_extensions WHERE issue_id = ?1 AND key = ?2",
                [OPEN_ISSUE_ID, CORRUPTED_KEY],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            value, "NOT-VALID-JSON",
            "the refused publication must not have mutated stored state"
        );
    }
}
