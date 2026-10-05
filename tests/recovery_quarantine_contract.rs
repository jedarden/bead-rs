//! Value-free contract checks for recovery-finding-quarantine-v1.

use serde_json::Value;
use std::collections::BTreeSet;

const SPEC: &str = include_str!("../research/specs/recovery-finding-quarantine-v1.md");
const FIXTURE: &str = include_str!("../research/fixtures/recovery-finding-quarantine-v1.json");

#[test]
fn recovery_quarantine_manifest_covers_the_shared_state_machine() {
    let manifest: Value = serde_json::from_str(FIXTURE).expect("manifest JSON");
    assert_eq!(
        manifest["contract"],
        "urn:bead-rs:spec:recovery-finding-quarantine:v1"
    );
    assert_eq!(manifest["status"], "accepted");
    assert_eq!(manifest["provenance"]["contains_candidate_values"], false);

    let states: BTreeSet<_> = manifest["states"]
        .as_array()
        .expect("states array")
        .iter()
        .map(|state| state.as_str().expect("state name"))
        .collect();
    assert_eq!(
        states,
        BTreeSet::from([
            "clean",
            "quarantined",
            "redaction_pending",
            "review_pending"
        ])
    );

    let transitions = manifest["transitions"]
        .as_array()
        .expect("transitions array");
    for required in [
        ("clean", "blocking_or_incomplete_recovery", "quarantined"),
        (
            "quarantined",
            "audited_redaction_commit",
            "redaction_pending",
        ),
        ("redaction_pending", "sanitized_publication", "clean"),
        (
            "quarantined",
            "reviewed_resolution_commit",
            "review_pending",
        ),
        ("review_pending", "reviewed_publication", "clean"),
        ("quarantined", "precommit_failure", "quarantined"),
    ] {
        assert!(transitions.iter().any(|transition| {
            transition["from"] == required.0
                && transition["event"] == required.1
                && transition["to"] == required.2
        }));
    }

    let scenarios: BTreeSet<_> = manifest["scenarios"]
        .as_array()
        .expect("scenarios array")
        .iter()
        .map(|scenario| scenario["id"].as_str().expect("scenario id"))
        .collect();
    assert_eq!(scenarios.len(), 12);
    for required in [
        "restore-blocking-split",
        "import-blocking-rollback",
        "reconcile-retained-blocking-split",
        "scanner-incomplete-holds",
        "quarantined-publication-refuses",
        "quarantined-commit-refuses",
        "live-redaction-clears",
        "retained-only-sanitized-reset",
        "stale-redaction-rolls-back",
        "reviewed-resolution-capability-gated",
    ] {
        assert!(scenarios.contains(required), "missing scenario {required}");
    }
}

#[test]
fn recovery_quarantine_contract_is_explicitly_value_free() {
    let manifest: Value = serde_json::from_str(FIXTURE).expect("manifest JSON");
    let forbidden: BTreeSet<_> = manifest["forbidden_fixture_fields"]
        .as_array()
        .expect("forbidden field list")
        .iter()
        .map(|field| field.as_str().expect("forbidden field name"))
        .collect();

    fn walk(value: &Value, forbidden: &BTreeSet<&str>) {
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    assert!(!forbidden.contains(key.as_str()), "forbidden field {key}");
                    walk(child, forbidden);
                }
            }
            Value::Array(values) => values.iter().for_each(|value| walk(value, forbidden)),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
    }

    walk(&manifest, &forbidden);
    for required in [
        "Local recovery",
        "Git publication",
        "local_success_quarantined",
        "Audited redaction",
        "Explicitly reviewed resolution",
        "rolled_back",
        "matched bytes",
    ] {
        assert!(SPEC.contains(required), "specification omits {required}");
    }
}
