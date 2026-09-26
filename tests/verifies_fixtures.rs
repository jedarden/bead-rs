//! Data-driven conformance for the declared `verifies` kind over the edge-shape
//! fixtures in `tests/fixtures/verifies/` (R025, ADR-001, beadrs-406348a9).
//!
//! Every case in `cases.json` is replayed through the real CLI in its own
//! workspace: the case's beads are created under the fixture titles, its edges
//! are inserted in the fixture's declared order, and four observables are
//! asserted against the case's expectations:
//!
//! - insertion never rejects (ADR-001: report, never reject, so a deliberate
//!   gate stays expressible);
//! - the dependencies-scope doctor stays advisory -- exit 0, no errors --
//!   and reports exactly the case's inverted `(blocked, blocker)` pairs with
//!   the expected check status, while the `blocks`-only cycle check stays
//!   `ok` (a `verifies` ring is not a cycle);
//! - readiness keys on `blocks` alone: the ready frontier matches the case's
//!   expectation, so a `verifies` edge never removes a bead and a `blocks`
//!   edge always does;
//! - `why` carries the distinct `blocked_by_verifier` code on the case's
//!   subject exactly when a declared `verifies` edge pairs with a `blocks`
//!   gate from the same blocker.
//!
//! The `plain-gate-no-declaration` case is the title-inference guard: its
//! titles are shaped so a title heuristic would flag an inversion, and the
//! fixture asserts the diagnosis stays silent because titles are never
//! consulted.

use std::collections::BTreeSet;
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
        .prefix(prefix)
        .tempdir_in("/var/tmp")
        .unwrap();
    let output = run(workspace.path(), &["init", "--prefix", "fx"]);
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

/// IDs of beads on the ready frontier. `list --json` emits JSONL -- one
/// record per line -- and any banner belongs on stderr, never stdout.
fn list_ready(workspace: &Path) -> Vec<String> {
    let output = run(workspace, &["list", "--ready", "--json"]);
    assert!(output.status.success(), "list --ready failed: {output:?}");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.starts_with('{'))
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .unwrap_or_else(|e| panic!("ready-frontier line must be valid JSON ({e}): {line}"))
        })
        .filter_map(|record| record["id"].as_str().map(str::to_string))
        .collect()
}

/// Run the dependencies doctor scope and return its parsed JSON report.
/// Advisory findings must never fail the run.
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

fn why_json(workspace: &Path, id: &str) -> Value {
    let output = run(workspace, &["why", "--id", id, "--json"]);
    assert!(output.status.success(), "why --json failed: {output:?}");
    serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim())
        .expect("why --json must emit valid JSON")
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/verifies")
}

fn load_cases() -> Vec<Value> {
    let path = fixtures_dir().join("cases.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("fixture cases must be readable at {:?}: {e}", path));
    let parsed: Value = serde_json::from_str(&raw).expect("cases.json must be valid JSON");
    parsed["cases"]
        .as_array()
        .cloned()
        .expect("cases.json carries a cases array")
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

/// The `dependency_graph` (blocks-cycle) check of a dependencies-scope report.
fn dependency_graph_check(report: &Value) -> &Value {
    report["checks"]
        .as_array()
        .expect("report carries a checks array")
        .iter()
        .find(|check| check["name"] == "dependency_graph")
        .expect("dependencies scope reports dependency_graph")
}

/// One replayed fixture case, from fixture data to asserted observables.
#[test]
fn fixture_cases_replay_to_their_expected_observables() {
    let cases = load_cases();
    assert!(
        cases.len() >= 5,
        "the fixture set must cover both gate shapes and their controls"
    );

    for case in &cases {
        let name = case["name"]
            .as_str()
            .expect("case carries a name")
            .to_string();
        replay_case(&name, case);
    }
}

fn replay_case(name: &str, case: &Value) {
    let workspace = setup(&format!("bead-verifies-fx-{name}-"));

    // Create the case's beads and resolve fixture keys to real ids.
    let mut ids = std::collections::BTreeMap::new();
    for bead in case["beads"].as_array().expect("case carries beads") {
        let key = bead["key"].as_str().expect("bead carries a key");
        let title = bead["title"].as_str().expect("bead carries a title");
        ids.insert(key.to_string(), create(workspace.path(), title));
    }

    // Insert every edge in the fixture's declared order. ADR-001: no edge is
    // ever rejected at insert time, whatever the kind or orientation.
    let mut edges: Vec<&Value> = case["edges"]
        .as_array()
        .expect("case carries edges")
        .iter()
        .collect();
    edges.sort_by_key(|edge| edge["order"].as_i64().unwrap_or(0));
    for edge in &edges {
        let blocked = ids[edge["blocked"].as_str().expect("edge names blocked key")].clone();
        let blocker = ids[edge["blocker"].as_str().expect("edge names blocker key")].clone();
        let kind = edge["kind"].as_str().expect("edge carries a kind");
        let output = dep_add(workspace.path(), &blocked, &blocker, kind);
        assert!(
            output.status.success(),
            "[{name}] a {kind} edge is never rejected at insert time: {output:?}"
        );
    }

    // Doctor posture: advisory, with exactly the expected gate pairs.
    let report = doctor_dependencies(workspace.path());
    assert!(
        !report["has_errors"].as_bool().unwrap_or(true),
        "[{name}] an advisory finding must not raise doctor errors"
    );
    let check = inverted_gate_check(&report);
    let expected_status = case["expected"]["doctor_check_status"]
        .as_str()
        .expect("case expects a doctor_check_status");
    assert_eq!(
        check["status"].as_str().unwrap(),
        expected_status,
        "[{name}] inverted_verification_gates status"
    );

    let expected_gates: BTreeSet<(String, String)> = case["expected"]["inverted_gates"]
        .as_array()
        .expect("case expects an inverted_gates array")
        .iter()
        .map(|gate| {
            (
                gate["blocked"]
                    .as_str()
                    .expect("gate names blocked")
                    .to_string(),
                gate["blocker"]
                    .as_str()
                    .expect("gate names blocker")
                    .to_string(),
            )
        })
        .map(|(blocked, blocker)| (ids[&blocked].clone(), ids[&blocker].clone()))
        .collect();
    let reported_gates: BTreeSet<(String, String)> = check["details"]["gates"]
        .as_array()
        .expect("gate check carries details.gates")
        .iter()
        .map(|gate| {
            (
                gate["blocked"]
                    .as_str()
                    .expect("gate names blocked")
                    .to_string(),
                gate["blocker"]
                    .as_str()
                    .expect("gate names blocker")
                    .to_string(),
            )
        })
        .collect();
    assert_eq!(
        reported_gates, expected_gates,
        "[{name}] the doctor reports exactly the case's inverted pairs"
    );
    for gate in check["details"]["gates"].as_array().unwrap() {
        assert_eq!(
            gate["reason_code"].as_str().unwrap(),
            "inverted_verification_gate",
            "[{name}] every reported gate carries the reason code"
        );
    }

    // Only `blocks` edges form cycles, so the cycle check stays ok even for
    // the verifies-ring case.
    assert_eq!(
        dependency_graph_check(&report)["status"].as_str().unwrap(),
        case["expected"]["dependency_graph_status"]
            .as_str()
            .expect("case expects a dependency_graph_status"),
        "[{name}] dependency_graph (blocks-cycle) status"
    );

    // Readiness keys on `blocks` alone.
    let expected_ready: BTreeSet<String> = case["expected"]["ready_beads"]
        .as_array()
        .expect("case expects ready_beads")
        .iter()
        .map(|key| ids[key.as_str().expect("ready bead names a key")].clone())
        .collect();
    let ready: BTreeSet<String> = list_ready(workspace.path()).into_iter().collect();
    assert_eq!(ready, expected_ready, "[{name}] ready-frontier membership");

    // `why` keys the distinct code on the declared edge only.
    let subject = case["expected"]["why_subject"]
        .as_str()
        .expect("case names a why_subject");
    let why = why_json(workspace.path(), &ids[subject]);
    let reasons = why["reasons"].as_array().expect("why carries reasons");
    let has_code = reasons.iter().any(|code| code == "blocked_by_verifier");
    assert_eq!(
        has_code,
        case["expected"]["why_blocked_by_verifier"]
            .as_bool()
            .expect("case expects why_blocked_by_verifier"),
        "[{name}] blocked_by_verifier appears exactly when the declared edge pairs with a gate"
    );
}

/// The fixture file itself stays honest: every documented observable is
/// present in every case, so an author cannot add a half-specified shape.
#[test]
fn every_case_declares_all_observables() {
    for case in load_cases() {
        let name = case["name"].as_str().unwrap_or("<unnamed>");
        assert!(
            case["summary"].as_str().is_some(),
            "[{name}] carries a summary"
        );
        assert!(
            !case["beads"].as_array().unwrap().is_empty(),
            "[{name}] carries beads"
        );
        let expected = &case["expected"];
        for field in [
            "all_edges_insert",
            "inverted_gates",
            "doctor_check_status",
            "dependency_graph_status",
            "doctor_has_errors",
            "ready_beads",
            "why_subject",
            "why_blocked_by_verifier",
        ] {
            assert!(
                expected.get(field).is_some(),
                "[{name}] expected.{field} must be declared"
            );
        }
        // The R025 contract fixes both of these for every shape: insertion
        // never rejects, and the diagnosis never raises a doctor error.
        assert!(
            expected["all_edges_insert"].as_bool().unwrap(),
            "[{name}] no fixture shape may declare insert-time rejection"
        );
        assert!(
            !expected["doctor_has_errors"].as_bool().unwrap(),
            "[{name}] no fixture shape may declare a doctor error"
        );
        let why_subject = expected["why_subject"].as_str().unwrap();
        assert!(
            case["beads"]
                .as_array()
                .unwrap()
                .iter()
                .any(|bead| bead["key"].as_str() == Some(why_subject)),
            "[{name}] why_subject names one of the case's beads"
        );
    }
}
