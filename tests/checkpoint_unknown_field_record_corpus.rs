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
//! `issue_extensions` (src/model.rs, src/service/checkpoint.rs), while event
//! and provenance-receipt records have no extension capacity. For those kinds
//! the conformance obligation is exact, lossless preservation of their known
//! fields -- never a re-projected or reinterpreted stray key -- which these
//! tests pin as well.
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
/// `EventRecord`). The record has no extension catch-all, so an export that
/// re-projects anything else onto it is reinterpreting the stream.
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
/// (`ProvenanceReceipt`), likewise with no extension capacity.
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

/// Restore a generation file into a fresh workspace. Returns the workspace
/// so the caller keeps the temp dir alive.
fn restore_into_empty(generation: &Path, actor: &str, expected_issue_count: usize) -> TempDir {
    let workspace = TempDir::new().unwrap();
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
    let source = TempDir::new().unwrap();
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

/// The boundary of the contract: event and receipt records have no
/// extension capacity, so their exports must confine themselves to the
/// known key set. A key appearing there is a re-projected or reinterpreted
/// stray, which is exactly the silent interpretation this suite exists to
/// rule out.
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
                    "{label}: event record carries unknown key {key:?}; events \
                     have no extension capacity, so this is a re-projection"
                );
            }
        }
        for record in records_of_type(generation, "provenance_receipt") {
            for key in record["provenance_receipt"].as_object().unwrap().keys() {
                assert!(
                    KNOWN_RECEIPT_KEYS.contains(&key.as_str()),
                    "{label}: receipt record carries unknown key {key:?}; \
                     receipts have no extension capacity, so this is a \
                     re-projection"
                );
            }
        }
    }
}
