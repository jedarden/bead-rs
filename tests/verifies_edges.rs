//! Declared `verifies` edges and inverted-gate diagnostics (R025, ADR-001,
//! beadrs-89216784).
//!
//! The `verifies` dependency kind lets an author declare that the blocker
//! checks the work the blocked issue performs. Three properties are pinned
//! here:
//!
//! - the kind is declared through the public CLI, persisted, and round-trips
//!   the checkpoint (export -> restore into an empty workspace);
//! - it never affects eligibility: readiness, like every eligibility query,
//!   keys on `blocks` alone, and cycles among `verifies` edges are permitted;
//! - an inverted verification gate -- a `blocks` edge whose blocker also
//!   `verifies` the blocked bead -- is diagnosed from the declared edge and
//!   only from it. Titles are never inspected, the edges stay legal, and the
//!   doctor report is advisory (warning status, exit 0) and deterministic;
//! - the declared edge feeds the R023 `why` explanation and the published
//!   capabilities document: `why` answers "why is this blocked?" with the
//!   distinct `blocked_by_verifier` code and a `verifies_blocked_issue`
//!   flag on the blocker detail, and `capabilities` advertises the
//!   three-kind vocabulary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn run(workspace: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bead"))
        .current_dir(workspace)
        .arg("--skip-foreign-workspace")
        .args(args)
        .output()
        .expect("bead command should start")
}

/// Disposable workspace under /var/tmp so no ancestor carries a foreign
/// `.beads` directory.
fn setup(prefix: &str) -> tempfile::TempDir {
    let workspace = tempfile::Builder::new()
        .prefix("bead-verifies-")
        .tempdir_in("/var/tmp")
        .unwrap();
    let output = run(workspace.path(), &["init", "--prefix", prefix]);
    assert!(output.status.success(), "init failed: {output:?}");
    workspace
}

fn create(workspace: &Path, title: &str) -> String {
    let output = run(
        workspace,
        &["create", "--title", title, "--issue-type", "task"],
    );
    assert!(output.status.success(), "create failed: {output:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn dep_add(workspace: &Path, blocked: &str, blocker: &str, kind: &str) -> Output {
    run(workspace, &["dep", "add", blocked, blocker, "--kind", kind])
}

/// `show --json` emits a one-element array whose record carries the
/// dependencies of the bead.
fn dependencies_of(workspace: &Path, id: &str) -> Vec<Value> {
    let output = run(workspace, &["show", id, "--json"]);
    assert!(output.status.success(), "show --json failed: {output:?}");
    let parsed: Value = serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim())
        .expect("show --json must emit valid JSON");
    parsed
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|record| record["dependencies"].as_array())
        .cloned()
        .unwrap_or_default()
}

/// Run the dependencies doctor scope and return its parsed JSON report.
fn doctor_dependencies(workspace: &Path) -> Value {
    let output = run(
        workspace,
        &["doctor", "--scope", "dependencies", "--format", "json"],
    );
    assert!(
        output.status.success(),
        "doctor must exit 0 for advisory findings: {output:?}"
    );
    serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim())
        .expect("doctor --format json must emit valid JSON")
}

/// The `inverted_verification_gates` check of a dependencies-scope report.
fn inverted_gate_check(report: &Value) -> &Value {
    report["checks"]
        .as_array()
        .expect("report carries a checks array")
        .iter()
        .find(|check| check["name"] == "inverted_verification_gates")
        .expect("dependencies scope reports inverted_verification_gates")
}

/// One inverted pair: the implementation bead and the bead that both blocks
/// and verifies it.
struct InvertedPair {
    blocked: String,
    blocker: String,
}

fn create_inverted_pair(workspace: &Path, n: usize) -> InvertedPair {
    let blocked = create(workspace, &format!("Add tilde expansion helper number {n}"));
    let blocker = create(
        workspace,
        &format!("Run clippy and fix warnings number {n}"),
    );

    // Both edges land on the same (blocked, blocker) pair: the declared check
    // relationship plus the gate that orders it before the work it checks.
    let output = dep_add(workspace, &blocked, &blocker, "verifies");
    assert!(output.status.success(), "verifies add failed: {output:?}");
    let output = dep_add(workspace, &blocked, &blocker, "blocks");
    assert!(output.status.success(), "blocks add failed: {output:?}");

    InvertedPair { blocked, blocker }
}

/// The `verifies` kind is declarable through the public CLI, reported back in
/// the relationship's own terms, and persisted with the kind intact.
#[test]
fn verifies_edge_declared_via_cli_and_persisted() {
    let workspace = setup("ver");
    let impl_id = create(
        workspace.path(),
        "Implement the reconciliation report format",
    );
    let check_id = create(workspace.path(), "Verify the reconciliation report format");

    let output = dep_add(workspace.path(), &impl_id, &check_id, "verifies");
    assert!(
        output.status.success(),
        "dep add --kind verifies failed: {output:?}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout
            .trim()
            .contains(&format!("{impl_id} verified by {check_id}")),
        "success message names the declared relationship, got: {stdout}"
    );

    // The edge persists with its kind; only `blocks` semantics (blocking
    // display, readiness) are absent because the kind is informational.
    let deps = dependencies_of(workspace.path(), &impl_id);
    let edge = deps
        .iter()
        .find(|d| d["blocker"] == check_id.as_str())
        .expect("edge is visible on the verified bead");
    assert_eq!(edge["kind"], "verifies", "kind survives storage verbatim");
}

/// A `verifies` edge survives export and restore into an empty workspace
/// with its kind unchanged.
#[test]
fn verifies_edge_round_trips_through_checkpoint() {
    let source = setup("rt1");
    let impl_id = create(source.path(), "Implement helper");
    let check_id = create(source.path(), "Verify helper");
    dep_add(source.path(), &impl_id, &check_id, "verifies").assert_ok("verifies add");

    // Explicit idempotent flush: database -> checkpoint. Never invokes git.
    let flush = run(source.path(), &["sync", "flush-only"]);
    assert!(flush.status.success(), "flush-only failed: {flush:?}");

    // The published checkpoint carries the edge under its declared kind.
    // Records serialize compactly, so the bytes searched for are exact.
    let needle = format!("\"blocker\":\"{check_id}\",\"kind\":\"verifies\"");
    let checkpoint_dir = source.path().join(".beads/checkpoint");
    let checkpoint_hit = scan_dir(&checkpoint_dir, &needle);
    assert!(
        checkpoint_hit,
        "checkpoint under {} must carry the verifies edge",
        checkpoint_dir.display()
    );

    // Restore into a fresh empty workspace; the edge and its kind survive.
    let target = setup("rt2");
    let restore = run(
        target.path(),
        &[
            "sync",
            "import-only",
            "--input",
            checkpoint_dir.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "roundtrip-probe",
        ],
    );
    assert!(
        restore.status.success(),
        "restore-into-empty failed: {:?}",
        String::from_utf8_lossy(&restore.stderr)
    );

    let deps = dependencies_of(target.path(), &impl_id);
    let edge = deps
        .iter()
        .find(|d| d["blocker"] == check_id.as_str())
        .expect("restored workspace carries the edge");
    assert_eq!(
        edge["kind"], "verifies",
        "kind survives the checkpoint round trip"
    );
}

/// Recursively search every file under `dir` for `needle`.
fn scan_dir(dir: &Path, needle: &str) -> bool {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        let path: PathBuf = entry.path();
        if path.is_dir() {
            if scan_dir(&path, needle) {
                return true;
            }
        } else if let Ok(contents) = std::fs::read_to_string(&path) {
            if contents.contains(needle) {
                return true;
            }
        }
    }
    false
}

/// The inversion diagnosis keys on the declared edge, never on titles.
///
/// The titles are the ADR-001 example a title heuristic would flag: a blocker
/// titled like a check gating a blocked bead titled like implementation work.
/// With no `verifies` edge declared, the graph is healthy; declaring the
/// relationship is what makes the inversion decidable.
#[test]
fn diagnosis_requires_declared_edge_not_titles() {
    let workspace = setup("tit");
    let impl_id = create(workspace.path(), "Add tilde expansion helper function");
    let check_id = create(workspace.path(), "Run clippy and fix warnings");

    // Gate without declaration: titles scream inversion, but no edge says so.
    dep_add(workspace.path(), &impl_id, &check_id, "blocks").assert_ok("blocks add");
    let report = doctor_dependencies(workspace.path());
    let check = inverted_gate_check(&report);
    assert_eq!(
        check["status"], "ok",
        "undeclared pairs are invisible by design; titles are never read"
    );

    // Declaring the relationship is the only thing that turns the gate into a
    // diagnosis.
    dep_add(workspace.path(), &impl_id, &check_id, "verifies").assert_ok("verifies add");
    let report = doctor_dependencies(workspace.path());
    let check = inverted_gate_check(&report);
    assert_eq!(check["status"], "warning", "declared inversion is reported");
    let gates = check["details"]["gates"].as_array().unwrap();
    assert_eq!(gates.len(), 1, "exactly the declared pair");
    assert_eq!(gates[0]["blocked"], impl_id.as_str());
    assert_eq!(gates[0]["blocker"], check_id.as_str());
    assert_eq!(
        gates[0]["reason_code"], "inverted_verification_gate",
        "the finding carries a distinct reason code"
    );
}

/// `verifies` edges never affect eligibility.
///
/// A bead whose only dependency is an unfinished `verifies` edge stays on the
/// ready frontier; adding a `blocks` edge from the same still-open blocker is
/// what removes it. The contrast pair proves the frontier keys on `blocks`
/// alone, not on edge count or kind presence.
#[test]
fn verifies_edges_never_change_eligibility() {
    let workspace = setup("rdy");
    let impl_id = create(workspace.path(), "Implement unblocking work");
    let check_id = create(workspace.path(), "Verify unblocking work");

    // The verifier is open (unfinished); the verified bead is still ready.
    dep_add(workspace.path(), &impl_id, &check_id, "verifies").assert_ok("verifies add");
    let ready = list_ready(workspace.path());
    assert!(
        ready.contains(&impl_id),
        "an unfinished verifies edge must not remove a bead from the ready frontier"
    );

    // Same pair, same open blocker, one `blocks` edge: now it blocks.
    dep_add(workspace.path(), &impl_id, &check_id, "blocks").assert_ok("blocks add");
    let ready = list_ready(workspace.path());
    assert!(
        !ready.contains(&impl_id),
        "a blocks edge from the same blocker must remove it"
    );
}

/// IDs of beads on the ready frontier. `list --json` emits JSONL -- one
/// record per line -- not a single JSON array.
fn list_ready(workspace: &Path) -> Vec<String> {
    let output = run(workspace, &["list", "--ready", "--json"]);
    assert!(output.status.success(), "list --ready failed: {output:?}");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line).unwrap_or_else(|e| {
                panic!("list --ready --json line must be valid JSON ({line}): {e}")
            })
        })
        .filter_map(|record| record["id"].as_str().map(str::to_string))
        .collect()
}

/// Cycles among `verifies` edges alone are permitted -- the kind never enters
/// cycle detection -- and the doctor's cycle check stays clean.
#[test]
fn verifies_cycles_are_permitted() {
    let workspace = setup("cyc");
    let first = create(workspace.path(), "First ring bead");
    let second = create(workspace.path(), "Second ring bead");

    dep_add(workspace.path(), &first, &second, "verifies").assert_ok("verifies add");
    dep_add(workspace.path(), &second, &first, "verifies").assert_ok("reverse verifies add");

    let report = doctor_dependencies(workspace.path());
    let graph = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "dependency_graph")
        .expect("dependency_graph check present");
    assert_eq!(
        graph["status"], "ok",
        "verifies rings are not cycles; only blocks edges form cycles"
    );
}

/// An inverted gate is never rejected at insert time: both edges commit, and
/// the doctor reports the pair as an advisory warning while exiting 0.
#[test]
fn inverted_gate_is_reported_not_rejected() {
    let workspace = setup("adv");
    let pair = create_inverted_pair(workspace.path(), 1);

    // Both edges are visible on the bead at once.
    let deps = dependencies_of(workspace.path(), &pair.blocked);
    assert_eq!(deps.len(), 2, "both the blocks and verifies edge persist");

    // Advisory posture: doctor exits 0 (warnings never fail the run) and the
    // message names the pair and a remedy.
    let report = doctor_dependencies(workspace.path());
    let check = inverted_gate_check(&report);
    assert_eq!(check["status"], "warning");
    let message = check["message"].as_str().unwrap();
    assert!(
        message.contains(&format!(
            "{} is blocked by {}, which also verifies it",
            pair.blocked, pair.blocker
        )),
        "message names the inverted pair, got: {message}"
    );
    assert!(
        !report["has_errors"].as_bool().unwrap_or(true),
        "an advisory finding must not raise doctor errors"
    );
}

/// Reporting is deterministic: two runs produce byte-identical check output,
/// and the gates appear in canonical (blocked, blocker) order regardless of
/// insertion order.
#[test]
fn inverted_gate_reporting_is_deterministic() {
    let workspace = setup("det");
    for n in 0..3 {
        create_inverted_pair(workspace.path(), n);
    }

    let first = doctor_dependencies(workspace.path());
    let second = doctor_dependencies(workspace.path());

    let check_one = inverted_gate_check(&first);
    let check_two = inverted_gate_check(&second);
    assert_eq!(
        serde_json::to_string(check_one).unwrap(),
        serde_json::to_string(check_two).unwrap(),
        "two runs over the same graph must report byte-identical findings"
    );

    let gates = check_one["details"]["gates"].as_array().unwrap();
    assert_eq!(gates.len(), 3, "every declared inverted pair is reported");
    let pairs: Vec<(String, String)> = gates
        .iter()
        .map(|g| {
            (
                g["blocked"].as_str().unwrap().to_string(),
                g["blocker"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let mut sorted = pairs.clone();
    sorted.sort();
    assert_eq!(
        pairs, sorted,
        "gates are reported in canonical (blocked, blocker) order"
    );
}

/// Native mutation fails closed on unknown kinds: `dep add` rejects anything
/// outside the three declared kinds and commits no row.
#[test]
fn unknown_kind_fails_closed_for_native_mutation() {
    let workspace = setup("fai");
    let first = create(workspace.path(), "First bead");
    let second = create(workspace.path(), "Second bead");

    let output = dep_add(workspace.path(), &first, &second, "parent-of");
    assert_eq!(
        output.status.code(),
        Some(4),
        "unknown kinds are a usage/validation failure, got: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    let deps = dependencies_of(workspace.path(), &first);
    assert!(
        deps.is_empty(),
        "a rejected kind must commit no edge, got: {deps:?}"
    );
}

/// The dry-run surface accepts the declared kind, mirroring the commit path.
#[test]
fn dry_run_accepts_verifies_kind() {
    let workspace = setup("dry");
    let impl_id = create(workspace.path(), "Implement dry-run target");
    let check_id = create(workspace.path(), "Verify dry-run target");

    let output = run(
        workspace.path(),
        &[
            "dep",
            "add",
            &impl_id,
            &check_id,
            "--kind",
            "verifies",
            "--dry-run",
        ],
    );
    assert!(
        output.status.success(),
        "dry-run accepts verifies, got: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"verifies\""),
        "dry-run echoes the kind: {stdout}"
    );
}

/// `why --json` for one bead: a single JSON object.
fn why_json(workspace: &Path, id: &str) -> Value {
    let output = run(workspace, &["why", "--id", id, "--json"]);
    assert!(output.status.success(), "why --json failed: {output:?}");
    serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim())
        .expect("why --json must emit valid JSON")
}

/// The legitimate "prove the baseline is green first" gate (ADR-001).
///
/// A deliberate baseline-first ordering is structurally identical to the
/// accidental inversion -- the same `verifies` edge and the same `blocks`
/// edge on the same pair -- and only the author knows which one they meant.
/// The diagnosis must therefore stay advisory for the deliberate reading
/// too: both edges insert cleanly, the doctor reports the pair as a
/// warning while the run itself stays green, and the gate keeps gating.
#[test]
fn baseline_first_gate_is_reported_but_stays_legal() {
    let workspace = setup("base");
    let work = create(workspace.path(), "Add tilde expansion helper");
    let baseline = create(
        workspace.path(),
        "Prove the baseline is green before any work lands",
    );

    // The deliberate authoring order: declare the check relationship,
    // then order the check ahead of the work it checks.
    dep_add(workspace.path(), &work, &baseline, "verifies")
        .assert_ok("declaring the check relationship");
    dep_add(workspace.path(), &work, &baseline, "blocks")
        .assert_ok("the deliberate gate must insert, not reject");

    // Advisory posture: a warning, never an error (doctor_dependencies
    // already asserts the doctor exits 0).
    let report = doctor_dependencies(workspace.path());
    let check = inverted_gate_check(&report);
    assert_eq!(check["status"], "warning");
    assert!(
        !report["has_errors"].as_bool().unwrap_or(true),
        "a deliberate baseline-first gate must not raise doctor errors"
    );

    // Advisory never means inert: the gate still gates, so the work waits
    // for its baseline.
    let ready = list_ready(workspace.path());
    assert!(
        !ready.contains(&work),
        "the gated work stays off the ready frontier, got: {ready:?}"
    );
}

/// `why` answers "why is this blocked?" with the verifier (R023 + R025).
///
/// With both edges declared, the reason codes carry the distinct
/// `blocked_by_verifier` code and the blocker detail carries
/// `verifies_blocked_issue: true` -- the answer that tells the reader
/// something may be wrong, which an ordinary unfinished-blocker code
/// cannot name. The contrast pair proves both key on the declared edge:
/// the same `blocks` edge without a `verifies` twin reports neither.
#[test]
fn why_names_a_blocking_verifier() {
    let workspace = setup("why");
    let inverted = create_inverted_pair(workspace.path(), 0);

    let why = why_json(workspace.path(), &inverted.blocked);
    let codes = why["reasons"]
        .as_array()
        .expect("why --json carries reason codes");
    assert!(
        codes.iter().any(|code| code == "blocked_by_verifier"),
        "reason codes must name the blocking verifier, got: {codes:?}"
    );
    let blockers = why["blockers"]["active_blockers"]
        .as_array()
        .expect("why --json carries active blockers");
    assert_eq!(blockers.len(), 1, "one declared blocker");
    assert_eq!(
        blockers[0]["verifies_blocked_issue"], true,
        "the declared check relationship must surface on the blocker detail"
    );

    // Contrast: the same gate shape with no declared check relationship.
    let plain_work = create(workspace.path(), "Add second helper");
    let plain_gate = create(workspace.path(), "Run the second lint");
    dep_add(workspace.path(), &plain_work, &plain_gate, "blocks").assert_ok("plain blocks add");

    let plain = why_json(workspace.path(), &plain_work);
    let plain_codes = plain["reasons"]
        .as_array()
        .expect("why --json carries reason codes");
    assert!(
        !plain_codes.iter().any(|code| code == "blocked_by_verifier"),
        "the code requires the declared edge, got: {plain_codes:?}"
    );
    let plain_blockers = plain["blockers"]["active_blockers"]
        .as_array()
        .expect("why --json carries active blockers");
    assert_eq!(
        plain_blockers[0]["verifies_blocked_issue"], false,
        "an ordinary blocker is never flagged; titles are not consulted"
    );
}

/// The capabilities document advertises the declared kind vocabulary.
///
/// The published member is the native mutation set (`dep add --kind`),
/// not the interchange set: interchange keeps foreign kinds preservable
/// outside this enum by design.
#[test]
fn capabilities_advertise_the_declared_kinds() {
    let workspace = setup("cap");
    let output = run(workspace.path(), &["capabilities"]);
    assert!(output.status.success(), "capabilities failed: {output:?}");
    let caps: Value = serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim())
        .expect("capabilities must emit valid JSON");
    let kinds = caps["dependency_kinds"]
        .as_array()
        .expect("capabilities advertises dependency_kinds");
    let names: Vec<&str> = kinds.iter().filter_map(|k| k.as_str()).collect();
    assert_eq!(names, vec!["blocks", "relates_to", "verifies"]);
}

/// Small helper: assert the command succeeded, naming `what` on failure.
trait AssertOkExt {
    fn assert_ok(self, what: &str);
}

impl AssertOkExt for Output {
    fn assert_ok(self, what: &str) {
        assert!(self.status.success(), "{what} failed: {self:?}");
    }
}
