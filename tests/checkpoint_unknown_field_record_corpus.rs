//! Unknown-field conformance over a corpus that contains every representative
//! checkpoint record kind: an issue record carrying unknown JSON fields
//! (nested values included), event records, the relationship graph (a
//! dependency edge plus labels, projected onto issue records), and a
//! provenance receipt.
//!
//! The contract under test: unknown JSON fields survive native checkpoint
//! export and import without loss or silent interpretation -- and their
//! preservation must hold in a realistic stream, not a single-record toy, so
//! the corpus is built entirely through public commands and round-tripped
//! export -> import -> export with every record kind diffed at each hop.
//!
//! Where unknown fields live is part of the contract, not an omission:
//! `Issue` carries the `#[serde(flatten)]` extensions catch-all persisted in
//! `issue_extensions` (src/model.rs, src/service/checkpoint.rs), and every
//! other checkpoint-carried record kind keeps its own additive-field map
//! (`RecordExtensions`).
//!
//! The corpus therefore has two halves. The three-generation corpus below is
//! built entirely from public commands, so its event and provenance-receipt
//! records carry no unknown fields -- for those records the obligation
//! pinned there is exact, lossless preservation of their known fields, never
//! a re-projected or reinterpreted stray key.
//! `published_checkpoint_carries_unknown_fields_at_every_object_level` is
//! the other half: it seeds a distinct, clearly-opaque payload at every
//! object level a checkpoint can carry -- issue, event, dependency edge,
//! external reference, structured-data envelope, resource key, attempt
//! outcome, provenance receipt, and the `current.json` generation pointer --
//! and requires an explicitly flushed publication to physically contain
//! every one of them, across the restore hop that contributes the receipt
//! level. Walking the extension-bearing forms across successive generations
//! is checkpoint_unknown_field_generation_conformance.rs.
//!
//! Corpus construction (all public commands, no fixture files):
//!
//! 1. source workspace: three issues driven through a real lifecycle
//!    (notes, labels, a `blocks` dependency edge, close), unknown fields
//!    direct-inserted the way every suite here does (no CLI produces one),
//!    then `sync flush-only` -> generation 1
//! 2. corpus workspace: `sync import-only --restore-into-empty` writes a
//!    provenance receipt into the restored store; a probe issue makes it
//!    genuinely dirty; `sync flush-only` -> generation 2, the one
//!    generation that carries all four kinds
//! 3. round-trip workspace: restore of generation 2, a second probe issue,
//!    `sync flush-only` -> generation 3, a genuine re-export of a store
//!    that never held the source lifecycle
//!
//! Assertions diff generation 1 -> 2 -> 3 per record kind: unknown fields
//! parse-equal at every hop (shapes intact, flat, never wrapped), events a
//! growing subset with exact identities, relationships equal across all
//! three generations and landed in the restored store's own tables, receipts
//! carried forward exactly, and non-issue records confined to their known
//! key sets.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
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

/// Every key a serialized event record may carry (src/service/checkpoint.rs
/// `EventRecord`), additive extensions included. These corpus records carry
/// none, so an export that re-projects anything else onto them is
/// reinterpreting the stream.
const KNOWN_EVENT_KEYS: [&str; 8] = [
    "$schema",
    "origin_store_uuid",
    "origin_event_sequence",
    "issue_id",
    "kind",
    "actor",
    "time",
    "detail",
];

/// Every key a serialized provenance receipt may carry
/// (`ProvenanceReceipt`), additive extensions included; these corpus
/// receipts carry none.
const KNOWN_RECEIPT_KEYS: [&str; 12] = [
    "$schema",
    "receipt_id",
    "kind",
    "source_store_uuid",
    "target_store_uuid",
    "source_root_sha256",
    "actor",
    "created_at",
    "counts",
    "result",
    "summary_event_identity",
    "receipt_sha256",
];

/// Every key an exported attempt-outcome record may carry beside its
/// additive extensions (src/service/checkpoint.rs `AttemptOutcomeRecord`).
const KNOWN_ATTEMPT_OUTCOME_KEYS: [&str; 17] = [
    "$schema",
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
    "evidence_refs",
    "model",
    "harness",
    "harness_version",
];

/// Every key a projected dependency-edge entry may carry beside its
/// additive extensions.
const KNOWN_DEPENDENCY_ENTRY_KEYS: [&str; 2] = ["blocker", "kind"];

/// Every key a projected external-reference entry may carry beside its
/// additive extensions.
const KNOWN_REFERENCE_ENTRY_KEYS: [&str; 4] = ["namespace", "key", "value", "unique_ref"];

/// Every key a structured-data envelope may carry beside its additive
/// extensions.
const KNOWN_DATA_ENVELOPE_KEYS: [&str; 2] = ["schema_ref", "value"];

/// Every key a projected resource-key entry may carry beside its additive
/// extensions.
const KNOWN_RESOURCE_KEY_ENTRY_KEYS: [&str; 1] = ["resource_key"];

/// Every top-level key a generation pointer may carry beside its additive
/// extensions (src/service/checkpoint.rs `POINTER_KNOWN_KEYS`).
const KNOWN_POINTER_KEYS: [&str; 19] = [
    "schema_version",
    "generation_id",
    "mode",
    "store_uuid",
    "snapshot_sequence",
    "active_root",
    "added_paths",
    "replaced_paths",
    "deleted_paths",
    "issue_count",
    "event_count",
    "receipt_count",
    "attempt_outcome_count",
    "redaction_record_count",
    "total_record_count",
    "created_at",
    "redaction_epoch_id",
    "previous_generation_reset",
    "superseded_generations",
];

/// The event-level extension key the all-levels corpus seeds; the exported
/// event is located by carrying it.
const EVENT_EXTENSION_KEY: &str = "future_event_trace";

/// The pointer-level extension key the all-levels corpus seeds into a
/// published `current.json` and requires the next flush to re-project.
const POINTER_EXTENSION_KEY: &str = "future_pointer_lineage";

/// The unknown-field payload carried by the corpus issue marked
/// "payload A": the shapes a preservation contract can get wrong, two of
/// them nested. Parsed-JSON equality at every hop is the assertion, so an
/// int that arrives back as a float, a null that arrives back as a string,
/// or a reordered nested object all fail.
fn payload_a() -> Value {
    serde_json::json!({
        "trace_context": { "spans": [{ "id": "s1", "depth": 2 }] },
        "deeply": { "level": [1, "two", { "three": [true, false, null] }] },
        "": "empty-string key",
        "ghost": null,
        "hollow": {},
        "void": [],
        "ratio": 0.125,
        "negative": -42,
        "ünkoded": "ünïcode — em-dash\nnewline",
    })
}

/// A second issue carries one scalar, so per-issue routing is proven rather
/// than one row's survival.
fn payload_b() -> Value {
    serde_json::json!({ "carried": "over" })
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

/// Pin publication to the explicit `sync flush-only` so each generation is
/// observed exactly once, by the test, with no auto publisher interleaving.
fn suppress_auto_flush(workspace: &Path) {
    let config_path = workspace.join(".beads/config.json");
    let mut config: Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config.as_object_mut().unwrap().insert(
        "checkpoint".to_string(),
        serde_json::json!({ "auto_flush": false }),
    );
    fs::write(&config_path, serde_json::to_string(&config).unwrap()).unwrap();
}

fn extract_bead_id(output: &str) -> String {
    output
        .split_whitespace()
        .find(|token| token.starts_with("bead-"))
        .unwrap_or_else(|| panic!("no bead id in output: {output}"))
        .to_string()
}

fn create_issue(workspace: &Path, title: &str) -> String {
    let output = bead(workspace, &["create", "--title", title])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    extract_bead_id(&String::from_utf8_lossy(&output))
}

/// Direct-insert an issue's unknown fields the way every unknown-field suite
/// here does: no CLI command produces one, and the side table is the
/// storage the importer uses, so a row inserted there is indistinguishable
/// from one an import landed.
fn insert_unknown_fields(workspace: &Path, issue_id: &str, payload: &Value) {
    let db = workspace.join(".beads/beads.db");
    let conn = rusqlite::Connection::open(&db).unwrap();
    for (key, value) in payload.as_object().unwrap() {
        conn.execute(
            "INSERT INTO issue_extensions (issue_id, key, value, profile)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                issue_id,
                key,
                serde_json::to_string(value).unwrap(),
                "native-v1",
            ],
        )
        .unwrap();
    }
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

/// Every record of a workspace's active published generation, any layout.
fn active_generation_records(workspace: &Path) -> Vec<Value> {
    let checkpoint_dir = workspace.join(".beads/checkpoint");
    let pointer = read_pointer(checkpoint_dir.join("current.json"));
    let root = checkpoint_dir.join(pointer["active_root"]["path"].as_str().unwrap());

    if pointer["mode"] == "sharded" {
        let manifest = read_pointer(&root);
        let mut records = Vec::new();
        for shard in manifest["issue_shards"].as_array().unwrap() {
            records.extend(read_jsonl(
                &checkpoint_dir.join(shard["path"].as_str().unwrap()),
            ));
        }
        for shard in manifest["event_shards"].as_array().unwrap() {
            records.extend(read_jsonl(
                &checkpoint_dir.join(shard["path"].as_str().unwrap()),
            ));
        }
        for shard in manifest["receipt_shards"].as_array().unwrap() {
            records.extend(read_jsonl(
                &checkpoint_dir.join(shard["path"].as_str().unwrap()),
            ));
        }
        for shard in manifest["attempt_outcome_shards"].as_array().unwrap() {
            records.extend(read_jsonl(
                &checkpoint_dir.join(shard["path"].as_str().unwrap()),
            ));
        }
        records
    } else {
        read_jsonl(&root)
    }
}

fn records_of_type<'a>(records: &'a [Value], record_type: &str) -> Vec<&'a Value> {
    records
        .iter()
        .filter(|record| record["record_type"] == record_type)
        .collect()
}

/// The unknown fields of one exported issue object: every key beyond the
/// known native fields and projected collections.
fn unknown_fields_of(issue: &Value) -> serde_json::Map<String, Value> {
    issue
        .as_object()
        .unwrap()
        .iter()
        .filter(|(key, _)| !KNOWN_ISSUE_KEYS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
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

/// Events of a generation keyed by their identity, payload intact: the map
/// form makes the subset assertion across hops exact and tells the failure
/// apart (missing event vs. reinterpreted event).
fn events_of(records: &[Value]) -> HashMap<String, Value> {
    records_of_type(records, "event")
        .into_iter()
        .map(|record| {
            let event = &record["event"];
            let identity = format!(
                "{}:{}",
                event["origin_store_uuid"].as_str().unwrap(),
                event["origin_event_sequence"]
            );
            (identity, event.clone())
        })
        .collect()
}

/// Receipts of a generation keyed by receipt ID, payload intact.
fn receipts_of(records: &[Value]) -> HashMap<String, Value> {
    records_of_type(records, "provenance_receipt")
        .into_iter()
        .map(|record| {
            let receipt = &record["provenance_receipt"];
            (
                receipt["receipt_id"].as_str().unwrap().to_string(),
                receipt.clone(),
            )
        })
        .collect()
}

/// Dependency edges of a generation as (blocked, blocker, kind) triples
/// extracted from the issue records' projected collections.
fn dependencies_of(records: &[Value]) -> HashSet<(String, String, String)> {
    records_of_type(records, "issue")
        .into_iter()
        .flat_map(|record| {
            let issue = &record["issue"];
            let id = issue["id"].as_str().unwrap().to_string();
            issue["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |dep| {
                    (
                        id.clone(),
                        dep["blocker"].as_str().unwrap().to_string(),
                        dep["kind"].as_str().unwrap().to_string(),
                    )
                })
        })
        .collect()
}

/// Label assignments of a generation as (issue, label) pairs.
fn labels_of(records: &[Value]) -> HashSet<(String, String)> {
    records_of_type(records, "issue")
        .into_iter()
        .flat_map(|record| {
            let issue = &record["issue"];
            let id = issue["id"].as_str().unwrap().to_string();
            issue["labels"]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |label| (id.clone(), label.as_str().unwrap().to_string()))
        })
        .collect()
}

/// A fresh workspace root under `/var/tmp`, never the TMPDIR default.
/// Workspace discovery stops at the first ancestor `.beads`, and the shared
/// lab box keeps one at `/tmp/.beads`: a bare `TempDir::new()` there is
/// silently adopted by `init --skip-foreign-workspace` (it exits 0 against
/// the ancestor store) instead of receiving a fresh workspace, and every
/// corpus test then fails on the config it never got. `/var/tmp` is the
/// base the definition-of-done script vets clean for the same reason.
fn fresh_workspace() -> TempDir {
    tempfile::Builder::new()
        .prefix("ufk-corpus-")
        .tempdir_in("/var/tmp")
        .unwrap()
}

/// Restore a generation file into a fresh workspace. Returns the workspace
/// so the caller keeps the temp dir alive.
fn restore_into_empty(generation: &Path, actor: &str, expected_issue_count: usize) -> TempDir {
    let workspace = fresh_workspace();
    init_workspace(workspace.path());
    suppress_auto_flush(workspace.path());
    bead(
        workspace.path(),
        &[
            "sync",
            "import-only",
            "--input",
            generation.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            actor,
        ],
    )
    .assert()
    .success()
    .stderr(predicate::str::contains(format!(
        "Restored {expected_issue_count} issues"
    )));
    workspace
}

/// The published generation file of a workspace whose auto flush is
/// suppressed: flush explicitly, then resolve the active root.
fn flush_and_read_generation(workspace: &Path) -> Vec<Value> {
    bead(workspace, &["sync", "flush-only"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Flushed forensic checkpoint:"));
    active_generation_records(workspace)
}

/// The full three-workspace corpus. Each generation is read exactly once,
/// at flush time, into memory.
struct Corpus {
    _source: TempDir,
    _corpus: TempDir,
    _round_tripped: TempDir,
    generation_1: Vec<Value>,
    generation_2: Vec<Value>,
    generation_3: Vec<Value>,
    payload_issue_a: String,
    payload_issue_b: String,
}

fn build_corpus() -> Corpus {
    // -- source workspace: real lifecycle, unknown fields, relationships ----
    let source = fresh_workspace();
    init_workspace(source.path());
    suppress_auto_flush(source.path());

    let issue_a = create_issue(source.path(), "corpus payload A — projections coexist");
    let issue_b = create_issue(source.path(), "corpus payload B — one scalar");
    let issue_c = create_issue(source.path(), "corpus payload C — closed");

    bead(
        source.path(),
        &["update", &issue_a, "--notes", "lifecycle notes"],
    )
    .assert()
    .success();
    bead(
        source.path(),
        &["label", "add", "--label", "corpus", &issue_a],
    )
    .assert()
    .success();
    bead(
        source.path(),
        &["label", "add", "--label", "unknown-fields", &issue_a],
    )
    .assert()
    .success();
    bead(
        source.path(),
        &["dep", "add", &issue_a, &issue_b, "--kind", "blocks"],
    )
    .assert()
    .success();
    bead(
        source.path(),
        &["close", &issue_c, "--reason", "corpus-complete"],
    )
    .assert()
    .success();

    insert_unknown_fields(source.path(), &issue_a, &payload_a());
    insert_unknown_fields(source.path(), &issue_b, &payload_b());

    let generation_1 = flush_and_read_generation(source.path());

    // -- corpus workspace: restore writes the provenance receipt -----------
    let generation_1_path = {
        let checkpoint_dir = source.path().join(".beads/checkpoint");
        let pointer = read_pointer(checkpoint_dir.join("current.json"));
        checkpoint_dir.join(pointer["active_root"]["path"].as_str().unwrap())
    };
    let corpus = restore_into_empty(&generation_1_path, "corpus-restore", 3);

    // The restore receipt must be in the store before the corpus flush: a
    // real mutation makes the live store genuinely dirty so the next flush
    // is a genuine export rather than the materialized restore no-op.
    let probe_b = create_issue(corpus.path(), "corpus probe B — forces the export");
    assert_ne!(probe_b, issue_a, "probe must be a distinct issue");

    let generation_2 = flush_and_read_generation(corpus.path());

    // -- round-trip workspace: a genuine re-export one hop further ---------
    let generation_2_path = {
        let checkpoint_dir = corpus.path().join(".beads/checkpoint");
        let pointer = read_pointer(checkpoint_dir.join("current.json"));
        checkpoint_dir.join(pointer["active_root"]["path"].as_str().unwrap())
    };
    let round_tripped = restore_into_empty(&generation_2_path, "corpus-round-trip", 4);
    let _probe_c = create_issue(
        round_tripped.path(),
        "corpus probe C — forces the re-export",
    );

    let generation_3 = flush_and_read_generation(round_tripped.path());

    Corpus {
        _source: source,
        _corpus: corpus,
        _round_tripped: round_tripped,
        generation_1,
        generation_2,
        generation_3,
        payload_issue_a: issue_a,
        payload_issue_b: issue_b,
    }
}

/// Every generation must contain the four representative kinds: issue
/// records (with the relationship projections embedded), event records, and
/// at least one provenance receipt from generation 2 onward.
#[test]
fn corpus_generations_carry_all_four_record_kinds() {
    let corpus = build_corpus();

    assert!(
        !records_of_type(&corpus.generation_1, "issue").is_empty(),
        "generation 1 must carry issue records"
    );
    assert!(
        !records_of_type(&corpus.generation_1, "event").is_empty(),
        "generation 1 must carry event records from the real lifecycle"
    );
    assert!(
        records_of_type(&corpus.generation_1, "provenance_receipt").is_empty(),
        "the source store never restored anything, so generation 1 must not \
         carry receipts"
    );

    assert!(
        !records_of_type(&corpus.generation_2, "issue").is_empty(),
        "generation 2 must carry issue records"
    );
    assert!(
        !records_of_type(&corpus.generation_2, "event").is_empty(),
        "generation 2 must carry event records"
    );
    assert!(
        !records_of_type(&corpus.generation_2, "provenance_receipt").is_empty(),
        "generation 2 must carry the receipt its restore wrote: without it \
         the corpus would not contain all four kinds"
    );
    assert!(
        !records_of_type(&corpus.generation_3, "provenance_receipt").is_empty(),
        "generation 3 must carry the receipts its import restored"
    );
}

/// The relationship graph and the unknown fields ride the same round trip:
/// both must land in the restored store's own tables exactly, the
/// projections never masquerading as unknown fields and the unknown fields
/// never collapsing into a projection.
#[test]
fn unknown_fields_and_relationships_reach_the_restored_tables() {
    let corpus = build_corpus();

    let restored = corpus._round_tripped.path();

    let extensions = read_extensions(restored);
    let expected_a: serde_json::Map<String, Value> = payload_a().as_object().unwrap().clone();
    let expected_b: serde_json::Map<String, Value> = payload_b().as_object().unwrap().clone();
    assert_eq!(
        extensions.get(&corpus.payload_issue_a),
        Some(&expected_a),
        "payload A must reach issue_extensions with exact JSON shapes after \
         two restore hops"
    );
    assert_eq!(
        extensions.get(&corpus.payload_issue_b),
        Some(&expected_b),
        "payload B must reach issue_extensions on its own issue"
    );
    assert_eq!(
        extensions.len(),
        2,
        "only the two payload issues may carry unknown fields"
    );

    let db = restored.join(".beads/beads.db");
    let conn = rusqlite::Connection::open(&db).unwrap();

    let edges: HashSet<(String, String, String)> = {
        let mut stmt = conn
            .prepare("SELECT blocked_issue_id, blocker_issue_id, kind FROM dependencies")
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
        rows.map(|row| row.unwrap()).collect()
    };
    assert_eq!(
        edges,
        HashSet::from([(
            corpus.payload_issue_a.clone(),
            corpus.payload_issue_b.clone(),
            "blocks".to_string(),
        )]),
        "the dependency edge must land in the restored store's own table"
    );

    let labels: HashSet<(String, String)> = {
        let mut stmt = conn.prepare("SELECT issue_id, label FROM labels").unwrap();
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap();
        rows.map(|row| row.unwrap()).collect()
    };
    assert_eq!(
        labels,
        HashSet::from([
            (corpus.payload_issue_a.clone(), "corpus".to_string()),
            (corpus.payload_issue_a.clone(), "unknown-fields".to_string()),
        ]),
        "the labels must land in the restored store's own table"
    );
}

/// The unknown fields must be re-projected flat on the issue object at
/// every export, parse-equal to the inserted payload: no wrapper key, no
/// stringified values, no int-to-float drift, nested structures intact.
#[test]
fn unknown_fields_reproject_flat_and_parse_equal_at_every_hop() {
    let corpus = build_corpus();

    let expected_a: serde_json::Map<String, Value> = payload_a().as_object().unwrap().clone();
    let expected_b: serde_json::Map<String, Value> = payload_b().as_object().unwrap().clone();

    for (label, generation) in [
        ("generation 1", &corpus.generation_1),
        ("generation 2", &corpus.generation_2),
        ("generation 3", &corpus.generation_3),
    ] {
        let mut found = 0;
        for record in records_of_type(generation, "issue") {
            let issue = &record["issue"];
            let id = issue["id"].as_str().unwrap();
            let expected = if id == corpus.payload_issue_a {
                Some(&expected_a)
            } else if id == corpus.payload_issue_b {
                Some(&expected_b)
            } else {
                None
            };
            let Some(expected) = expected else {
                continue;
            };
            found += 1;

            let fields = unknown_fields_of(issue);
            assert!(
                !issue.as_object().unwrap().contains_key("extensions"),
                "{label}: unknown fields must be re-projected flat, not nested \
                 under an extensions wrapper"
            );
            assert_eq!(
                &fields, expected,
                "{label}: unknown fields of {id} did not survive this export \
                 unchanged"
            );
        }
        assert_eq!(
            found, 2,
            "{label}: both payload issues must appear in the export"
        );
    }
}

/// Events must survive as a growing subset with exact identities and exact
/// payloads: the restore adopts the source store UUID, so generation 1's
/// events are literally present in generations 2 and 3, alongside each
/// restore's own summary events -- never merged away, renumbered, or
/// reinterpreted.
#[test]
fn events_round_trip_as_an_exact_growing_subset() {
    let corpus = build_corpus();

    let events_1 = events_of(&corpus.generation_1);
    let events_2 = events_of(&corpus.generation_2);
    let events_3 = events_of(&corpus.generation_3);

    assert!(!events_1.is_empty(), "the lifecycle must produce events");
    for (identity, event) in &events_1 {
        assert_eq!(
            events_2.get(identity),
            Some(event),
            "event {identity} was not carried into generation 2 exactly"
        );
        assert_eq!(
            events_3.get(identity),
            Some(event),
            "event {identity} was not carried into generation 3 exactly"
        );
    }
    assert!(
        events_2.len() > events_1.len(),
        "the corpus restore must add its own summary events"
    );
    for (identity, event) in &events_2 {
        assert_eq!(
            events_3.get(identity),
            Some(event),
            "event {identity} was not carried into generation 3 exactly"
        );
    }
}

/// Receipts must be carried forward exactly: generation 2's restore receipt
/// reappears in generation 3's export byte-shape-identical, proving the
/// import restored it rather than synthesizing a replacement.
#[test]
fn receipts_round_trip_exactly() {
    let corpus = build_corpus();

    let receipts_2 = receipts_of(&corpus.generation_2);
    let receipts_3 = receipts_of(&corpus.generation_3);

    assert_eq!(
        receipts_2.len(),
        1,
        "exactly one receipt (the corpus restore's) is expected in \
         generation 2"
    );
    for (receipt_id, receipt) in &receipts_2 {
        assert_eq!(
            receipts_3.get(receipt_id),
            Some(receipt),
            "receipt {receipt_id} was not carried into generation 3 exactly; \
             the re-export must reproduce the restored receipt, not a \
             synthesized one"
        );
    }

    // The receipt must name the source generation's bytes: it is the
    // provenance anchor that makes the whole corpus chain auditable.
    let receipt = receipts_2.values().next().unwrap();
    assert_eq!(
        receipt["kind"], "restore",
        "the corpus receipt records a restore"
    );
    assert_eq!(
        receipt["result"], "success",
        "the corpus receipt records a successful restore"
    );
}

/// Dependencies and labels must be equal across all three generations: the
/// projections ride the issue records through the export, and neither the
/// restores nor the re-export may reverse, drop, or re-kind an edge.
#[test]
fn relationships_survive_all_three_generations() {
    let corpus = build_corpus();

    let expected_edge = HashSet::from([(
        corpus.payload_issue_a.clone(),
        corpus.payload_issue_b.clone(),
        "blocks".to_string(),
    )]);
    let expected_labels = HashSet::from([
        (corpus.payload_issue_a.clone(), "corpus".to_string()),
        (corpus.payload_issue_a.clone(), "unknown-fields".to_string()),
    ]);

    for (label, generation) in [
        ("generation 1", &corpus.generation_1),
        ("generation 2", &corpus.generation_2),
        ("generation 3", &corpus.generation_3),
    ] {
        assert_eq!(
            dependencies_of(generation),
            expected_edge,
            "{label}: the dependency edge did not survive"
        );
        assert_eq!(
            labels_of(generation),
            expected_labels,
            "{label}: the labels did not survive"
        );
    }
}

/// The boundary of the three-generation corpus: its event and receipt
/// records were built entirely from public commands, so they carry no
/// unknown fields and their exports must confine themselves to the known
/// key set. A key appearing there is a re-projected or reinterpreted
/// stray, which is exactly the silent interpretation this suite exists to
/// rule out. (Record kinds that do have extension capacity are exercised
/// with seeded payloads by the all-levels corpus below.)
#[test]
fn non_issue_records_carry_no_unknown_fields() {
    let corpus = build_corpus();

    for (label, generation) in [
        ("generation 1", &corpus.generation_1),
        ("generation 2", &corpus.generation_2),
        ("generation 3", &corpus.generation_3),
    ] {
        for record in records_of_type(generation, "event") {
            for key in record["event"].as_object().unwrap().keys() {
                assert!(
                    KNOWN_EVENT_KEYS.contains(&key.as_str()),
                    "{label}: event record carries unknown key {key:?}; these \
                     corpus events carry no extensions, so this is a \
                     re-projection"
                );
            }
        }
        for record in records_of_type(generation, "provenance_receipt") {
            for key in record["provenance_receipt"].as_object().unwrap().keys() {
                assert!(
                    KNOWN_RECEIPT_KEYS.contains(&key.as_str()),
                    "{label}: receipt record carries unknown key {key:?}; \
                     these corpus receipts carry no extensions, so this is a \
                     re-projection"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The all-levels corpus
//
// The three-generation corpus above is built entirely from public commands,
// so its only unknown fields are issue-level ones: no CLI produces an
// unknown field on any record kind. The corpus below seeds every remaining
// level the way a newer producer would have left it -- direct store writes
// and a hand-edited published pointer -- then requires explicitly flushed
// publications to carry all of them at once.
// ---------------------------------------------------------------------------

/// Distinct, clearly-opaque unknown-field payloads, one per object level a
/// checkpoint can carry. Every payload nests an object and an array -- the
/// shapes a projection can flatten, stringify, or reorder -- and no two
/// levels share a key, so a payload surfacing at the wrong level fails the
/// exact-map assertion loudly.
fn issue_level_payload() -> Value {
    serde_json::json!({
        "future_issue_signal": { "routes": ["iad", "ord"], "gate": { "open": false } }
    })
}

fn event_level_payload() -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert(
        EVENT_EXTENSION_KEY.to_string(),
        serde_json::json!({ "spans": ["s1", "s2"], "gate": { "sampled": true } }),
    );
    Value::Object(payload)
}

fn dependency_level_payload() -> Value {
    serde_json::json!({
        "future_edge_weight": { "lanes": [1, 2], "gate": { "soft": true } }
    })
}

fn reference_level_payload() -> Value {
    serde_json::json!({
        "future_ref_visibility": { "audiences": ["audit", "ops"], "gate": { "internal": true } }
    })
}

fn data_level_payload() -> Value {
    serde_json::json!({
        "future_envelope_etag": { "digests": ["a", "b"], "gate": { "weak": false } }
    })
}

fn resource_key_level_payload() -> Value {
    serde_json::json!({
        "future_key_exclusivity": { "windows": ["2026-12-01"], "gate": { "hard": true } }
    })
}

fn attempt_outcome_level_payload() -> Value {
    serde_json::json!({
        "future_telemetry": { "gpus": ["0", "1"], "gate": { "sampled": false } }
    })
}

fn receipt_level_payload() -> Value {
    serde_json::json!({
        "future_receipt_signature": { "keys": ["pk-1"], "gate": { "sealed": true } }
    })
}

/// The pointer-level unknown value a newer producer would leave in a
/// published `current.json`.
fn pointer_extension_value() -> Value {
    serde_json::json!({ "parents": ["gen-1"], "gate": { "sealed": false } })
}

fn pointer_level_payload() -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert(POINTER_EXTENSION_KEY.to_string(), pointer_extension_value());
    Value::Object(payload)
}

/// Every extension key the corpus seeds, taken from the payloads themselves
/// so the physical-containment sweep can never drift from what is seeded.
/// The receipt key only exists once the restore has contributed its level,
/// and the pointer key only once a flush has re-projected it.
fn all_level_extension_keys(receipt_level: bool, pointer_level: bool) -> Vec<String> {
    let mut payloads = vec![
        issue_level_payload(),
        event_level_payload(),
        dependency_level_payload(),
        reference_level_payload(),
        data_level_payload(),
        resource_key_level_payload(),
        attempt_outcome_level_payload(),
    ];
    if receipt_level {
        payloads.push(receipt_level_payload());
    }
    let mut keys: Vec<String> = payloads
        .iter()
        .flat_map(|payload| payload.as_object().unwrap().keys().cloned())
        .collect();
    if pointer_level {
        keys.extend(pointer_level_payload().as_object().unwrap().keys().cloned());
    }
    keys
}

/// Seed the event-level unknown fields on the store's earliest event, the
/// way a newer producer's row would carry them.
fn set_first_event_extension(workspace: &Path, payload: &Value) {
    let conn = rusqlite::Connection::open(workspace.join(".beads/beads.db")).unwrap();
    let sequence: i64 = conn
        .query_row(
            "SELECT sequence FROM events ORDER BY sequence ASC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    conn.execute(
        "UPDATE events SET extensions_json = ?1 WHERE sequence = ?2",
        rusqlite::params![serde_json::to_string(payload).unwrap(), sequence],
    )
    .unwrap();
}

/// Seed the unknown fields of one dependency edge.
fn set_dependency_extension(workspace: &Path, blocked: &str, blocker: &str, payload: &Value) {
    let conn = rusqlite::Connection::open(workspace.join(".beads/beads.db")).unwrap();
    let changed = conn
        .execute(
            "UPDATE dependencies SET extensions_json = ?1
             WHERE blocked_issue_id = ?2 AND blocker_issue_id = ?3",
            rusqlite::params![serde_json::to_string(payload).unwrap(), blocked, blocker],
        )
        .unwrap();
    assert_eq!(changed, 1, "the dependency edge must exist to be seeded");
}

/// Insert an external reference carrying unknown fields.
fn insert_extended_external_reference(workspace: &Path, issue_id: &str, payload: &Value) {
    let conn = rusqlite::Connection::open(workspace.join(".beads/beads.db")).unwrap();
    conn.execute(
        "INSERT INTO external_references (issue_id, namespace, key, value, extensions_json)
         VALUES (?1, 'future-tracker', 'src-id', 'src-value-1', ?2)",
        rusqlite::params![issue_id, serde_json::to_string(payload).unwrap()],
    )
    .unwrap();
}

/// Insert a structured-data envelope carrying unknown fields.
fn insert_extended_issue_data(workspace: &Path, issue_id: &str, payload: &Value) {
    let conn = rusqlite::Connection::open(workspace.join(".beads/beads.db")).unwrap();
    conn.execute(
        "INSERT INTO issue_data (issue_id, namespace, schema_ref, value, extensions_json)
         VALUES (?1, 'future-config', 'urn:test:future', '{\"region\":\"iad\"}', ?2)",
        rusqlite::params![issue_id, serde_json::to_string(payload).unwrap()],
    )
    .unwrap();
}

/// Seed the unknown fields of the issue's declared resource key, turning it
/// into the extension-capable object form a newer producer writes.
fn set_resource_key_extension(workspace: &Path, issue_id: &str, payload: &Value) {
    let conn = rusqlite::Connection::open(workspace.join(".beads/beads.db")).unwrap();
    let changed = conn
        .execute(
            "UPDATE issue_resource_keys SET extensions_json = ?1
             WHERE issue_id = ?2 AND resource_key = 'gpu:0'",
            rusqlite::params![serde_json::to_string(payload).unwrap(), issue_id],
        )
        .unwrap();
    assert_eq!(changed, 1, "the resource key must exist to be seeded");
}

/// Insert an attempt outcome carrying unknown fields; returns its attempt
/// identity, which locates the exported record.
fn insert_extended_attempt_outcome(workspace: &Path, issue_id: &str, payload: &Value) -> String {
    let conn = rusqlite::Connection::open(workspace.join(".beads/beads.db")).unwrap();
    let attempt_id = String::from("urn:needle:attempt:all-levels-001");
    conn.execute(
        "INSERT INTO attempt_outcomes (
            receipt_id, attempt_id, issue_id, outcome, action, reason,
            canonical_request_hash, prior_attempt_tier, resulting_attempt_tier,
            resulting_issue_revision, actor, created_at, evidence_refs_json,
            model, harness, harness_version, resulting_state, extensions_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        rusqlite::params![
            "ao-all-levels-001",
            &attempt_id,
            issue_id,
            "verified_success",
            "none",
            "all-levels corpus attempt",
            "all-levels-canonical-request-hash",
            0i64,
            0i64,
            0i64,
            "corpus-worker",
            "2026-09-29T00:00:00Z",
            r#"["s3:logs/all-levels.tar.gz"]"#,
            "glm-5.3-flash",
            "needle",
            "1.0.0",
            "open",
            serde_json::to_string(payload).unwrap(),
        ],
    )
    .unwrap();
    attempt_id
}

/// Seed the provenance receipt's unknown fields and return the receipt ID
/// whose exported record must carry them.
fn extend_single_receipt(workspace: &Path, payload: &Value) -> String {
    let conn = rusqlite::Connection::open(workspace.join(".beads/beads.db")).unwrap();
    let receipt_id: String = conn
        .query_row("SELECT receipt_id FROM provenance_receipts", [], |row| {
            row.get(0)
        })
        .unwrap();
    conn.execute(
        "UPDATE provenance_receipts SET extensions_json = ?1 WHERE receipt_id = ?2",
        rusqlite::params![serde_json::to_string(payload).unwrap(), &receipt_id],
    )
    .unwrap();
    receipt_id
}

/// Add the pointer-level unknown key to the published `current.json`, the
/// way a newer producer's republish would have left it behind.
fn seed_pointer_extension(workspace: &Path) {
    let path = workspace.join(".beads/checkpoint/current.json");
    let mut pointer: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    pointer
        .as_object_mut()
        .unwrap()
        .insert(POINTER_EXTENSION_KEY.to_string(), pointer_extension_value());
    fs::write(&path, serde_json::to_string_pretty(&pointer).unwrap()).unwrap();
}

/// The unknown members of one exported object: every key beyond the known
/// native ones. Parse-equality of this map against its seeded payload is
/// the preservation assertion -- loss, rewrapping, and reinterpretation all
/// fail it.
fn unknown_members(value: &Value, known: &[&str]) -> serde_json::Map<String, Value> {
    value
        .as_object()
        .unwrap()
        .iter()
        .filter(|(key, _)| !known.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn object_of(payload: &Value) -> serde_json::Map<String, Value> {
    payload.as_object().unwrap().clone()
}

/// The exported event carrying the seeded event extension: exactly one.
fn extended_event(records: &[Value]) -> &Value {
    let carriers: Vec<&Value> = records_of_type(records, "event")
        .into_iter()
        .map(|record| &record["event"])
        .filter(|event| event.get(EVENT_EXTENSION_KEY).is_some())
        .collect();
    assert_eq!(
        carriers.len(),
        1,
        "exactly one event must carry the seeded event extension"
    );
    carriers[0]
}

/// The exported attempt-outcome record carrying the seeded payload,
/// located by its attempt identity.
fn extended_attempt_outcome<'a>(records: &'a [Value], attempt_id: &str) -> &'a Value {
    records_of_type(records, "attempt_outcome")
        .into_iter()
        .map(|record| &record["attempt_outcome"])
        .find(|outcome| outcome["attempt_id"].as_str() == Some(attempt_id))
        .unwrap_or_else(|| panic!("attempt outcome {attempt_id} missing from generation"))
}

/// Every record-carried level must surface parse-equal on exactly its own
/// exported object: the issue's own unknown fields, each embedded entry's
/// unknown fields, the event's, and the attempt outcome's -- with a clean
/// control issue proving routing is per record, never per store.
fn assert_record_levels(
    records: &[Value],
    issue_a: &str,
    issue_b: &str,
    attempt_id: &str,
    label: &str,
) {
    let issues: Vec<&Value> = records_of_type(records, "issue")
        .into_iter()
        .map(|record| &record["issue"])
        .collect();

    let issue = issues
        .iter()
        .find(|issue| issue["id"].as_str() == Some(issue_a))
        .unwrap_or_else(|| panic!("{label}: issue {issue_a} missing from generation"));
    let control = issues
        .iter()
        .find(|issue| issue["id"].as_str() == Some(issue_b))
        .unwrap_or_else(|| panic!("{label}: issue {issue_b} missing from generation"));

    assert_eq!(
        unknown_members(issue, &KNOWN_ISSUE_KEYS),
        object_of(&issue_level_payload()),
        "{label}: issue-level unknown fields must survive parse-equal"
    );
    assert!(
        unknown_members(control, &KNOWN_ISSUE_KEYS).is_empty(),
        "{label}: the control issue must carry no unknown fields"
    );

    let edge = issue["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["blocker"].as_str() == Some(issue_b))
        .unwrap_or_else(|| panic!("{label}: dependency edge missing from generation"));
    assert_eq!(
        unknown_members(edge, &KNOWN_DEPENDENCY_ENTRY_KEYS),
        object_of(&dependency_level_payload()),
        "{label}: dependency-edge unknown fields must survive parse-equal"
    );

    let reference = issue["external_references"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["key"] == "src-id")
        .unwrap_or_else(|| panic!("{label}: external reference missing from generation"));
    assert_eq!(
        unknown_members(reference, &KNOWN_REFERENCE_ENTRY_KEYS),
        object_of(&reference_level_payload()),
        "{label}: external-reference unknown fields must survive parse-equal"
    );

    let envelope = &issue["data"]["future-config"];
    assert_eq!(
        unknown_members(envelope, &KNOWN_DATA_ENVELOPE_KEYS),
        object_of(&data_level_payload()),
        "{label}: structured-data envelope unknown fields must survive \
         parse-equal"
    );

    let resource_key = issue["resource_keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["resource_key"] == "gpu:0")
        .unwrap_or_else(|| panic!("{label}: resource-key entry missing from generation"));
    assert_eq!(
        unknown_members(resource_key, &KNOWN_RESOURCE_KEY_ENTRY_KEYS),
        object_of(&resource_key_level_payload()),
        "{label}: resource-key unknown fields must survive parse-equal"
    );

    assert_eq!(
        unknown_members(extended_event(records), &KNOWN_EVENT_KEYS),
        object_of(&event_level_payload()),
        "{label}: event-level unknown fields must survive parse-equal"
    );
    assert_eq!(
        unknown_members(
            extended_attempt_outcome(records, attempt_id),
            &KNOWN_ATTEMPT_OUTCOME_KEYS
        ),
        object_of(&attempt_outcome_level_payload()),
        "{label}: attempt-outcome unknown fields must survive parse-equal"
    );
}

/// The provenance receipt carrying seeded unknown fields must keep them
/// parse-equal and alone at its level.
fn assert_receipt_level(generation: &[Value], receipt_id: &str, label: &str) {
    let receipt = records_of_type(generation, "provenance_receipt")
        .into_iter()
        .map(|record| &record["provenance_receipt"])
        .find(|receipt| receipt["receipt_id"].as_str() == Some(receipt_id))
        .unwrap_or_else(|| panic!("{label}: receipt {receipt_id} missing from generation"));
    assert_eq!(
        unknown_members(receipt, &KNOWN_RECEIPT_KEYS),
        object_of(&receipt_level_payload()),
        "{label}: receipt-level unknown fields must survive parse-equal"
    );
}

/// The published pointer must re-project its seeded unknown key, and only
/// that one.
fn assert_pointer_level(workspace: &Path, label: &str) {
    let pointer = read_pointer(workspace.join(".beads/checkpoint/current.json"));
    assert_eq!(
        unknown_members(&pointer, &KNOWN_POINTER_KEYS),
        object_of(&pointer_level_payload()),
        "{label}: pointer must re-project its seeded unknown key verbatim"
    );
}

/// Every file the active published generation physically occupies: the
/// monolithic root, or the manifest-referenced shards.
fn published_generation_files(workspace: &Path) -> Vec<std::path::PathBuf> {
    let checkpoint_dir = workspace.join(".beads/checkpoint");
    let pointer = read_pointer(checkpoint_dir.join("current.json"));
    let root = checkpoint_dir.join(pointer["active_root"]["path"].as_str().unwrap());

    if pointer["mode"] == "sharded" {
        let manifest = read_pointer(&root);
        let mut paths = vec![root];
        for family in [
            "issue_shards",
            "event_shards",
            "receipt_shards",
            "attempt_outcome_shards",
        ] {
            for shard in manifest[family].as_array().unwrap() {
                paths.push(checkpoint_dir.join(shard["path"].as_str().unwrap()));
            }
        }
        paths
    } else {
        vec![root]
    }
}

/// The published checkpoint's bytes must physically contain every seeded
/// extension key -- present in the JSON on disk, not merely derivable from
/// the store. The pointer file counts: it is part of the published
/// checkpoint.
fn assert_extensions_physically_published(workspace: &Path, keys: &[String], label: &str) {
    let mut raw = published_generation_files(workspace)
        .iter()
        .map(|path| fs::read_to_string(path).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    raw.push_str(&fs::read_to_string(workspace.join(".beads/checkpoint/current.json")).unwrap());
    for key in keys {
        assert!(
            raw.contains(&format!("\"{key}\"")),
            "{label}: published checkpoint bytes contain no {key:?}"
        );
    }
}

/// Publish a store carrying unknown fields at every object level a
/// checkpoint defines -- issue, event, dependency edge, external reference,
/// structured-data envelope, resource key, attempt outcome, provenance
/// receipt, and the `current.json` generation pointer -- via explicit
/// `sync flush-only` publications, and require the published checkpoint to
/// physically contain every one of them.
///
/// The restore hop contributes the receipt level: restoring the source
/// generation writes a provenance receipt into the restored store, the seed
/// lands on that receipt, and the next explicit flush publishes it. The
/// pointer level is a newer producer's key left in the published
/// `current.json`; the next explicit flush must re-project it.
#[test]
fn published_checkpoint_carries_unknown_fields_at_every_object_level() {
    // -- source workspace: every level a store carries before a restore ----
    let source = fresh_workspace();
    init_workspace(source.path());
    suppress_auto_flush(source.path());

    let issue_a = create_issue(source.path(), "all-levels payload A — record-level seeds");
    let issue_b = create_issue(source.path(), "all-levels payload B — edge target");

    bead(
        source.path(),
        &["dep", "add", &issue_a, &issue_b, "--kind", "blocks"],
    )
    .assert()
    .success();
    bead(
        source.path(),
        &["resource", "add", &issue_a, "--key", "gpu:0"],
    )
    .assert()
    .success();

    insert_unknown_fields(source.path(), &issue_a, &issue_level_payload());
    set_first_event_extension(source.path(), &event_level_payload());
    set_dependency_extension(
        source.path(),
        &issue_a,
        &issue_b,
        &dependency_level_payload(),
    );
    insert_extended_external_reference(source.path(), &issue_a, &reference_level_payload());
    insert_extended_issue_data(source.path(), &issue_a, &data_level_payload());
    set_resource_key_extension(source.path(), &issue_a, &resource_key_level_payload());
    let attempt_id =
        insert_extended_attempt_outcome(source.path(), &issue_a, &attempt_outcome_level_payload());

    let generation_1 = flush_and_read_generation(source.path());

    assert_record_levels(
        &generation_1,
        &issue_a,
        &issue_b,
        &attempt_id,
        "generation 1",
    );
    assert_extensions_physically_published(
        source.path(),
        &all_level_extension_keys(false, false),
        "generation 1",
    );

    // -- corpus workspace: the restore contributes the receipt level -------
    let generation_1_path = {
        let checkpoint_dir = source.path().join(".beads/checkpoint");
        let pointer = read_pointer(checkpoint_dir.join("current.json"));
        checkpoint_dir.join(pointer["active_root"]["path"].as_str().unwrap())
    };
    let corpus = restore_into_empty(&generation_1_path, "all-levels-restore", 2);

    // The restore receipt must be in the store before the corpus flush, and
    // the seeded payload must ride the receipt the next publication exports.
    let receipt_id = extend_single_receipt(corpus.path(), &receipt_level_payload());
    create_issue(
        corpus.path(),
        "all-levels probe 1 — dirties the restored store",
    );
    let generation_2 = flush_and_read_generation(corpus.path());

    assert_record_levels(
        &generation_2,
        &issue_a,
        &issue_b,
        &attempt_id,
        "generation 2",
    );
    assert_receipt_level(&generation_2, &receipt_id, "generation 2");

    // The pointer level: a newer producer's key left in the published
    // pointer, which the next explicit flush must re-project.
    seed_pointer_extension(corpus.path());
    create_issue(
        corpus.path(),
        "all-levels probe 2 — forces the pointer republish",
    );
    let generation_3 = flush_and_read_generation(corpus.path());

    assert_record_levels(
        &generation_3,
        &issue_a,
        &issue_b,
        &attempt_id,
        "generation 3",
    );
    assert_receipt_level(&generation_3, &receipt_id, "generation 3");
    assert_pointer_level(corpus.path(), "generation 3");
    assert_extensions_physically_published(
        corpus.path(),
        &all_level_extension_keys(true, true),
        "generation 3",
    );
}
