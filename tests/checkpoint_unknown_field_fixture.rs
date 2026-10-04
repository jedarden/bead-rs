//! Restore conformance for the reusable unknown-field checkpoint corpus.
//!
//! The fixture is deliberately data-driven: `expected.json` names each
//! sentinel and its selector, so adding another checkpoint object level does
//! not hide coverage in a hand-written assertion. The test validates the
//! authored checkpoint, restores it through the native empty-store path, and
//! checks the newly published generation. Generated IDs, timestamps, hashes,
//! and native omission behavior are compared through explicit, path-scoped
//! allowances in `expected.json`; no whole-record or whole-document ignore is
//! permitted.

use assert_cmd::Command;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const ISSUE_KEYS: &[&str] = &[
    "$schema",
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
    "labels",
    "comments",
    "dependencies",
    "external_references",
    "data",
    "resource_keys",
];
const EVENT_KEYS: &[&str] = &[
    "$schema",
    "origin_store_uuid",
    "origin_event_sequence",
    "issue_id",
    "kind",
    "actor",
    "time",
    "detail",
];
const DEPENDENCY_KEYS: &[&str] = &["blocker", "kind"];
const REFERENCE_KEYS: &[&str] = &["namespace", "key", "value", "unique_ref"];
const DATA_KEYS: &[&str] = &["schema_ref", "value"];
const RESOURCE_KEY_KEYS: &[&str] = &["resource_key"];
const RECEIPT_KEYS: &[&str] = &[
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
const POINTER_KEYS: &[&str] = &[
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
];

const REQUIRED_SENTINEL_LEVELS: [&str; 8] = [
    "issue",
    "event",
    "dependency",
    "external reference",
    "structured data",
    "resource key",
    "receipt",
    "pointer",
];

const POINTER_REWRITE_PATHS: &[&str] = &[
    "/generation_id",
    "/active_root/path",
    "/active_root/sha256",
    "/snapshot_sequence",
    "/created_at",
    "/added_paths",
    "/replaced_paths",
    "/deleted_paths",
    "/receipt_count",
    "/total_record_count",
];

const GENERATED_RECEIPT_REWRITE_PATHS: &[&str] = &[
    "/receipt_id",
    "/source_root_sha256",
    "/created_at",
    "/receipt_sha256",
];

const ISSUE_REWRITE_PATHS: &[&str] = &[
    "/assignee",
    "/claim_epoch",
    "/labels",
    "/resource_keys",
    "/dependencies",
];

#[derive(Debug)]
struct SentinelExpectation {
    level: String,
    field: String,
    value: Value,
    selector: Value,
}

#[derive(Debug)]
struct NativeRewriteAllowance {
    scope: String,
    path: String,
    kind: String,
    reason: String,
    record_type: Option<String>,
    record_selector: Option<Value>,
}

#[derive(Debug)]
struct FixtureExpectations {
    fixture_format: u64,
    profile: String,
    input_mode: String,
    input_pointer: String,
    input_root: String,
    sentinels: Vec<SentinelExpectation>,
    allowed_native_rewrites: Vec<NativeRewriteAllowance>,
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/checkpoint-unknown-fields-v1")
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(
        &fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn read_records(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn required_object<'a>(value: &'a Value, path: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{path}: expected a JSON object"))
}

fn required_string(value: &Value, path: &str) -> Result<String, String> {
    let string = value
        .as_str()
        .ok_or_else(|| format!("{path}: expected a string"))?;
    if string.trim().is_empty() {
        return Err(format!("{path}: expected a non-empty string"));
    }
    Ok(string.to_owned())
}

fn member<'a>(object: &'a Map<String, Value>, key: &str, path: &str) -> Result<&'a Value, String> {
    object
        .get(key)
        .ok_or_else(|| format!("{path}.{key}: required member is missing"))
}

fn validate_selector(
    selector: &Value,
    level: &str,
    field: &str,
    index: usize,
) -> Result<(), String> {
    let path = format!("sentinels[{index}].selector");
    let selector = required_object(selector, &path)?;
    let record_type = required_string(
        member(selector, "record_type", &path)?,
        &format!("{path}.record_type"),
    )?;

    match level {
        "issue" => {
            if record_type != "issue" {
                return Err(format!(
                    "{path}.record_type: expected \"issue\" for {field}, got {record_type:?}"
                ));
            }
            required_string(member(selector, "id", &path)?, &format!("{path}.id"))?;
        }
        "event" => {
            if record_type != "event" {
                return Err(format!(
                    "{path}.record_type: expected \"event\" for {field}, got {record_type:?}"
                ));
            }
            required_string(
                member(selector, "origin_store_uuid", &path)?,
                &format!("{path}.origin_store_uuid"),
            )?;
            let sequence = member(selector, "origin_event_sequence", &path)?
                .as_u64()
                .ok_or_else(|| {
                    format!("{path}.origin_event_sequence: expected a positive integer")
                })?;
            if sequence == 0 {
                return Err(format!(
                    "{path}.origin_event_sequence: expected a positive integer"
                ));
            }
        }
        "receipt" => {
            if record_type != "provenance_receipt" {
                return Err(format!(
                    "{path}.record_type: expected \"provenance_receipt\" for {field}, got {record_type:?}"
                ));
            }
            required_string(
                member(selector, "receipt_id", &path)?,
                &format!("{path}.receipt_id"),
            )?;
        }
        "dependency" => {
            validate_nested_selector(selector, &path, "dependency_blocker", "issue", field)?;
        }
        "external reference" => {
            validate_nested_selector(selector, &path, "reference_key", "issue", field)?;
        }
        "structured data" => {
            validate_nested_selector(selector, &path, "data_namespace", "issue", field)?;
        }
        "resource key" => {
            validate_nested_selector(selector, &path, "resource_key", "issue", field)?;
        }
        "pointer" => {
            if record_type != "pointer" {
                return Err(format!(
                    "{path}.record_type: expected \"pointer\" for {field}, got {record_type:?}"
                ));
            }
            let pointer_path =
                required_string(member(selector, "path", &path)?, &format!("{path}.path"))?;
            let expected_path = format!("/{field}");
            if pointer_path != expected_path {
                return Err(format!(
                    "{path}.path: expected {expected_path:?} for {field}, got {pointer_path:?}"
                ));
            }
        }
        other => {
            return Err(format!(
                "sentinels[{index}].level: unsupported level {other:?}"
            ))
        }
    }
    Ok(())
}

fn validate_nested_selector(
    selector: &Map<String, Value>,
    path: &str,
    discriminator: &str,
    expected_record_type: &str,
    field: &str,
) -> Result<(), String> {
    let record_type = required_string(
        member(selector, "record_type", path)?,
        &format!("{path}.record_type"),
    )?;
    if record_type != expected_record_type {
        return Err(format!(
            "{path}.record_type: expected {expected_record_type:?} for {field}, got {record_type:?}"
        ));
    }
    required_string(member(selector, "id", path)?, &format!("{path}.id"))?;
    required_string(
        member(selector, discriminator, path)?,
        &format!("{path}.{discriminator}"),
    )?;
    Ok(())
}

fn parse_expectations(expected: &Value) -> Result<FixtureExpectations, String> {
    let root = required_object(expected, "expected.json")?;
    let fixture_format = member(root, "fixture_format", "expected.json")?
        .as_u64()
        .ok_or_else(|| "expected.json.fixture_format: expected a positive integer".to_string())?;
    if fixture_format != 1 {
        return Err(format!(
            "expected.json.fixture_format: expected 1, got {fixture_format}"
        ));
    }
    let profile = required_string(
        member(root, "profile", "expected.json")?,
        "expected.json.profile",
    )?;
    let input = required_object(
        member(root, "input", "expected.json")?,
        "expected.json.input",
    )?;
    let input_mode = required_string(
        member(input, "mode", "expected.json.input")?,
        "expected.json.input.mode",
    )?;
    let input_pointer = required_string(
        member(input, "pointer", "expected.json.input")?,
        "expected.json.input.pointer",
    )?;
    let input_root = required_string(
        member(input, "root", "expected.json.input")?,
        "expected.json.input.root",
    )?;

    let raw_sentinels = member(root, "sentinels", "expected.json")?
        .as_array()
        .ok_or_else(|| "expected.json.sentinels: expected an array".to_string())?;
    if raw_sentinels.len() != REQUIRED_SENTINEL_LEVELS.len() {
        return Err(format!(
            "expected.json.sentinels: expected exactly {} entries (one per required level), found {}",
            REQUIRED_SENTINEL_LEVELS.len(),
            raw_sentinels.len()
        ));
    }

    let mut levels = HashSet::new();
    let mut fields = HashSet::new();
    let mut sentinels = Vec::with_capacity(raw_sentinels.len());
    for (index, raw) in raw_sentinels.iter().enumerate() {
        let path = format!("sentinels[{index}]");
        let sentinel = required_object(raw, &format!("expected.json.{path}"))?;
        let level = required_string(member(sentinel, "level", &path)?, &path)?;
        if !REQUIRED_SENTINEL_LEVELS.contains(&level.as_str()) {
            return Err(format!(
                "expected.json.{path}.level: unsupported level {level:?}; expected one of {REQUIRED_SENTINEL_LEVELS:?}"
            ));
        }
        if !levels.insert(level.clone()) {
            return Err(format!(
                "expected.json.{path}.level: duplicate sentinel level {level:?}; every required level must appear exactly once"
            ));
        }
        let field = required_string(member(sentinel, "field", &path)?, &path)?;
        if !fields.insert(field.clone()) {
            return Err(format!(
                "expected.json.{path}.field: duplicate sentinel field {field:?}"
            ));
        }
        if known_keys(&level).contains(&field.as_str()) {
            return Err(format!(
                "expected.json.{path}.field: {field:?} is a native {level} field, not an unknown sentinel"
            ));
        }
        let selector = member(sentinel, "selector", &path)?.clone();
        validate_selector(&selector, &level, &field, index)?;
        let value = member(sentinel, "value", &path)?.clone();
        sentinels.push(SentinelExpectation {
            level,
            field,
            value,
            selector,
        });
    }
    for required_level in REQUIRED_SENTINEL_LEVELS {
        if !levels.contains(required_level) {
            return Err(format!(
                "expected.json.sentinels: missing required {required_level:?} sentinel"
            ));
        }
    }

    let raw_rewrites = member(root, "allowed_native_rewrites", "expected.json")?
        .as_array()
        .ok_or_else(|| "expected.json.allowed_native_rewrites: expected an array".to_string())?;
    if raw_rewrites.is_empty() {
        return Err(
            "expected.json.allowed_native_rewrites: expected at least one explicit allowance"
                .to_string(),
        );
    }
    let mut rewrite_keys = HashSet::new();
    let mut allowed_native_rewrites = Vec::with_capacity(raw_rewrites.len());
    for (index, raw) in raw_rewrites.iter().enumerate() {
        let path = format!("allowed_native_rewrites[{index}]");
        let rewrite = required_object(raw, &format!("expected.json.{path}"))?;
        let scope = required_string(member(rewrite, "scope", &path)?, &path)?;
        let rewrite_path = required_string(member(rewrite, "path", &path)?, &path)?;
        if !rewrite_path.starts_with('/') || rewrite_path.contains('*') {
            return Err(format!(
                "expected.json.{path}.path: expected an exact JSON path without wildcards, got {rewrite_path:?}"
            ));
        }
        let kind = required_string(member(rewrite, "kind", &path)?, &path)?;
        let reason = required_string(member(rewrite, "reason", &path)?, &path)?;
        let record_type = rewrite
            .get("record_type")
            .map(|value| required_string(value, &format!("{path}.record_type")))
            .transpose()?;
        let record_selector = rewrite.get("record_selector").cloned();
        match scope.as_str() {
            "pointer" => {
                if !POINTER_REWRITE_PATHS.contains(&rewrite_path.as_str()) {
                    return Err(format!(
                        "expected.json.{path}.path: {rewrite_path:?} is not a documented pointer rewrite"
                    ));
                }
                if record_type.is_some() || record_selector.is_some() {
                    return Err(format!(
                        "expected.json.{path}: pointer allowances must not carry record selectors"
                    ));
                }
            }
            "record" => {
                if record_type.as_deref() != Some("issue") {
                    return Err(format!(
                        "expected.json.{path}.record_type: record allowances must target issue"
                    ));
                }
                let selector = record_selector.as_ref().ok_or_else(|| {
                    format!("expected.json.{path}.record_selector: required for record")
                })?;
                let selector =
                    required_object(selector, &format!("expected.json.{path}.record_selector"))?;
                let selector_path = format!("{path}.record_selector");
                let selector_type = required_string(
                    member(selector, "record_type", &selector_path)?,
                    &format!("{selector_path}.record_type"),
                )?;
                if selector_type != "issue" {
                    return Err(format!(
                        "{selector_path}.record_type: expected \"issue\", got {selector_type:?}"
                    ));
                }
                required_string(
                    member(selector, "id", &selector_path)?,
                    &format!("{selector_path}.id"),
                )?;
                if !ISSUE_REWRITE_PATHS.contains(&rewrite_path.as_str()) {
                    return Err(format!(
                        "expected.json.{path}.path: {rewrite_path:?} is not a documented issue rewrite"
                    ));
                }
            }
            "generated_record" => {
                if record_type.as_deref() != Some("provenance_receipt") {
                    return Err(format!(
                        "expected.json.{path}.record_type: generated_record allowances must target provenance_receipt"
                    ));
                }
                let selector = record_selector.as_ref().ok_or_else(|| {
                    format!("expected.json.{path}.record_selector: required for generated_record")
                })?;
                let selector =
                    required_object(selector, &format!("expected.json.{path}.record_selector"))?;
                required_string(
                    member(selector, "kind", &format!("{path}.record_selector"))?,
                    &format!("{path}.record_selector"),
                )?;
                required_string(
                    member(selector, "actor", &format!("{path}.record_selector"))?,
                    &format!("{path}.record_selector"),
                )?;
                if !GENERATED_RECEIPT_REWRITE_PATHS.contains(&rewrite_path.as_str()) {
                    return Err(format!(
                        "expected.json.{path}.path: {rewrite_path:?} is not a documented generated receipt rewrite"
                    ));
                }
            }
            other => {
                return Err(format!(
                    "expected.json.{path}.scope: unsupported scope {other:?}; use pointer, record, or generated_record"
                ));
            }
        }
        let selector_key = record_selector
            .as_ref()
            .map(|selector| serde_json::to_string(selector).expect("JSON values are serializable"));
        let key = format!("{scope}:{record_type:?}:{selector_key:?}:{rewrite_path}");
        if !rewrite_keys.insert(key) {
            return Err(format!(
                "expected.json.{path}: duplicate native rewrite allowance for {scope} {rewrite_path}"
            ));
        }
        allowed_native_rewrites.push(NativeRewriteAllowance {
            scope,
            path: rewrite_path,
            kind,
            reason,
            record_type,
            record_selector,
        });
    }

    Ok(FixtureExpectations {
        fixture_format,
        profile,
        input_mode,
        input_pointer,
        input_root,
        sentinels,
        allowed_native_rewrites,
    })
}

fn fixture_expectations() -> FixtureExpectations {
    let path = fixture_dir().join("expected.json");
    let expected = read_json(&path);
    parse_expectations(&expected).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn fixture_pointer() -> Value {
    read_json(&fixture_dir().join("current.json"))
}

fn fixture_records() -> Vec<Value> {
    read_records(&fixture_dir().join("objects/unknown-fields.jsonl"))
}

fn object<'a>(value: &'a Value, context: &str) -> &'a Map<String, Value> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("{context}: expected a JSON object"))
}

fn issue<'a>(records: &'a [Value], id: &str) -> &'a Value {
    records
        .iter()
        .filter(|record| record["record_type"] == "issue")
        .map(|record| &record["issue"])
        .find(|candidate| candidate["id"] == id)
        .unwrap_or_else(|| panic!("issue {id} is missing"))
}

fn selected_record<'a>(records: &'a [Value], selector: &Value) -> &'a Value {
    let selector = object(selector, "selector");
    match selector["record_type"].as_str().unwrap() {
        "issue" => issue(records, selector["id"].as_str().unwrap()),
        "event" => records
            .iter()
            .filter(|record| record["record_type"] == "event")
            .map(|record| &record["event"])
            .find(|event| {
                event["origin_store_uuid"] == selector["origin_store_uuid"]
                    && event["origin_event_sequence"] == selector["origin_event_sequence"]
            })
            .unwrap_or_else(|| panic!("event selector did not match")),
        "provenance_receipt" => records
            .iter()
            .filter(|record| record["record_type"] == "provenance_receipt")
            .map(|record| &record["provenance_receipt"])
            .find(|receipt| receipt["receipt_id"] == selector["receipt_id"])
            .unwrap_or_else(|| panic!("receipt selector did not match")),
        other => panic!("unsupported record selector {other:?}"),
    }
}

fn nested_selected_record<'a>(records: &'a [Value], selector: &Value, level: &str) -> &'a Value {
    let selector = object(selector, "selector");
    let issue = issue(records, selector["id"].as_str().unwrap());
    let issue = object(issue, "issue");
    match level {
        "dependency" => issue["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["blocker"] == selector["dependency_blocker"])
            .unwrap_or_else(|| panic!("dependency selector did not match")),
        "external reference" => issue["external_references"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["key"] == selector["reference_key"])
            .unwrap_or_else(|| panic!("external-reference selector did not match")),
        "structured data" => issue["data"]
            .get(selector["data_namespace"].as_str().unwrap())
            .unwrap_or_else(|| panic!("structured-data selector did not match")),
        "resource key" => issue["resource_keys"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["resource_key"] == selector["resource_key"])
            .unwrap_or_else(|| panic!("resource-key selector did not match")),
        other => panic!("unsupported nested level {other:?}"),
    }
}

fn known_keys(level: &str) -> &'static [&'static str] {
    match level {
        "issue" => ISSUE_KEYS,
        "event" => EVENT_KEYS,
        "dependency" => DEPENDENCY_KEYS,
        "external reference" => REFERENCE_KEYS,
        "structured data" => DATA_KEYS,
        "resource key" => RESOURCE_KEY_KEYS,
        "receipt" => RECEIPT_KEYS,
        "pointer" => POINTER_KEYS,
        other => panic!("unknown fixture level {other:?}"),
    }
}

fn json_pointer<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(value);
    }
    path.strip_prefix('/')?
        .split('/')
        .try_fold(value, |current, part| {
            let part = part.replace("~1", "/").replace("~0", "~");
            current.as_object()?.get(&part)
        })
}

fn pointer_allowances(expectations: &FixtureExpectations) -> HashSet<String> {
    expectations
        .allowed_native_rewrites
        .iter()
        .filter(|rewrite| rewrite.scope == "pointer")
        .map(|rewrite| rewrite.path.clone())
        .collect()
}

fn json_path(parent: &str, key: &str) -> String {
    format!("{parent}/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn assert_json_equal_except_allowances(
    expected: &Value,
    actual: &Value,
    path: &str,
    allowances: &HashSet<String>,
    phase: &str,
) {
    if allowances.contains(path) {
        return;
    }
    match (expected, actual) {
        (Value::Object(expected), Value::Object(actual)) => {
            let mut keys = HashSet::new();
            keys.extend(expected.keys());
            keys.extend(actual.keys());
            for key in keys {
                let child_path = json_path(path, key);
                if allowances.contains(&child_path) {
                    continue;
                }
                match (expected.get(key), actual.get(key)) {
                    (Some(expected), Some(actual)) => assert_json_equal_except_allowances(
                        expected,
                        actual,
                        &child_path,
                        allowances,
                        phase,
                    ),
                    (Some(_), None) => panic!(
                        "{phase}: native field at {child_path} disappeared without an explicit allowance"
                    ),
                    (None, Some(_)) => panic!(
                        "{phase}: native field at {child_path} appeared without an explicit allowance"
                    ),
                    (None, None) => unreachable!(),
                }
            }
        }
        _ => assert_eq!(
            expected, actual,
            "{phase}: value at {path} changed without an explicit native rewrite allowance"
        ),
    }
}

fn record_key(record: &Value) -> String {
    let record_type = record["record_type"]
        .as_str()
        .unwrap_or_else(|| panic!("record has no string record_type: {record}"));
    let payload_key = match record_type {
        "issue" => format!("id={}", record["issue"]["id"]),
        "event" => format!(
            "store={}#{}",
            record["event"]["origin_store_uuid"], record["event"]["origin_event_sequence"]
        ),
        "provenance_receipt" => {
            format!("id={}", record["provenance_receipt"]["receipt_id"])
        }
        other => panic!("unsupported checkpoint record type {other:?}"),
    };
    format!("{record_type}:{payload_key}")
}

fn record_payload(record: &Value) -> &Value {
    match record["record_type"].as_str() {
        Some("issue") => &record["issue"],
        Some("event") => &record["event"],
        Some("provenance_receipt") => &record["provenance_receipt"],
        Some(other) => panic!("unsupported checkpoint record type {other:?}"),
        None => panic!("record has no string record_type: {record}"),
    }
}

fn selector_matches_payload(selector: &Value, record_type: &str, payload: &Value) -> bool {
    let Some(selector) = selector.as_object() else {
        return false;
    };
    selector.iter().all(|(key, expected)| {
        if key == "record_type" {
            return expected.as_str() == Some(record_type);
        }
        payload.get(key) == Some(expected)
    })
}

fn record_allowances(expectations: &FixtureExpectations, record: &Value) -> HashSet<String> {
    let record_type = record["record_type"].as_str().unwrap_or_default();
    let payload = record_payload(record);
    expectations
        .allowed_native_rewrites
        .iter()
        .filter(|rewrite| {
            rewrite.scope == "record"
                && rewrite.record_type.as_deref() == Some(record_type)
                && rewrite.record_selector.as_ref().is_some_and(|selector| {
                    selector_matches_payload(selector, record_type, payload)
                })
        })
        .map(|rewrite| rewrite.path.clone())
        .collect()
}

fn assert_source_records_preserved(
    expected_records: &[Value],
    actual_records: &[Value],
    expectations: &FixtureExpectations,
    phase: &str,
) {
    let mut actual_by_key = HashMap::new();
    for record in actual_records {
        let key = record_key(record);
        assert!(
            actual_by_key.insert(key.clone(), record).is_none(),
            "{phase}: duplicate checkpoint record {key}"
        );
    }
    for expected in expected_records {
        let key = record_key(expected);
        let actual = actual_by_key
            .get(&key)
            .unwrap_or_else(|| panic!("{phase}: source checkpoint record {key} disappeared"));
        assert_eq!(
            expected["record_type"], actual["record_type"],
            "{phase}: source checkpoint record {key} changed record type"
        );
        assert_json_equal_except_allowances(
            record_payload(expected),
            record_payload(actual),
            "",
            &record_allowances(expectations, expected),
            &format!("{phase} record {key}"),
        );
    }

    let expected_keys: HashSet<_> = expected_records.iter().map(record_key).collect();
    let extra: Vec<_> = actual_by_key
        .iter()
        .filter(|(key, _)| !expected_keys.contains(*key))
        .map(|(_, record)| *record)
        .collect();
    assert_eq!(
        extra.len(),
        1,
        "{phase}: expected exactly one generated restore receipt beyond the fixture records"
    );
    let generated = object(&extra[0]["provenance_receipt"], "generated restore receipt");
    let generated_allowances: Vec<_> = expectations
        .allowed_native_rewrites
        .iter()
        .filter(|rewrite| rewrite.scope == "generated_record")
        .collect();
    assert!(
        !generated_allowances.is_empty(),
        "{phase}: generated restore receipt has no explicit native rewrite allowances"
    );
    let first = generated_allowances[0];
    assert_eq!(
        first.record_type.as_deref(),
        Some("provenance_receipt"),
        "{phase}: generated receipt allowance has the wrong record type"
    );
    for allowance in &generated_allowances {
        assert_eq!(
            allowance.record_type.as_deref(),
            Some("provenance_receipt"),
            "{phase}: generated receipt allowance has the wrong record type"
        );
        let selector = allowance
            .record_selector
            .as_ref()
            .expect("validated generated receipt allowance must have a selector");
        let selector = object(selector, "generated receipt selector");
        for (key, expected) in selector {
            assert_eq!(
                generated.get(key),
                Some(expected),
                "{phase}: generated receipt selector {key} did not match"
            );
        }
        assert!(
            json_pointer(&Value::Object(generated.clone()), &allowance.path).is_some(),
            "{phase}: generated receipt allowance {} points to a missing field",
            allowance.path
        );
        assert!(
            !allowance.kind.trim().is_empty() && !allowance.reason.trim().is_empty(),
            "{phase}: generated receipt allowance {} needs a kind and reason",
            allowance.path
        );
    }
    let allowed_generated_fields: HashSet<_> = generated_allowances
        .iter()
        .map(|allowance| allowance.path.trim_start_matches('/'))
        .collect();
    for field in generated.keys() {
        assert!(
            RECEIPT_KEYS.contains(&field.as_str())
                || allowed_generated_fields.contains(field.as_str()),
            "{phase}: generated receipt field {field} is not documented as native or explicitly allowed"
        );
    }
}

fn assert_sentinels(
    records: &[Value],
    pointer: &Value,
    expectations: &FixtureExpectations,
    phase: &str,
) {
    let mut levels = HashSet::new();
    for sentinel in &expectations.sentinels {
        assert!(
            levels.insert(sentinel.level.as_str()),
            "{phase}: duplicate sentinel level {}",
            sentinel.level
        );
        if sentinel.level == "pointer" {
            let selector = object(&sentinel.selector, "pointer sentinel selector");
            let selected = json_pointer(pointer, selector["path"].as_str().unwrap())
                .unwrap_or_else(|| {
                    panic!(
                        "{phase}: pointer selector {} did not resolve",
                        selector["path"]
                    )
                });
            assert_eq!(
                selected, &sentinel.value,
                "{phase}: pointer sentinel {} changed",
                sentinel.field
            );
        } else {
            let selected = match sentinel.level.as_str() {
                "issue" | "event" | "receipt" => selected_record(records, &sentinel.selector),
                "dependency" | "external reference" | "structured data" | "resource key" => {
                    nested_selected_record(records, &sentinel.selector, &sentinel.level)
                }
                other => panic!("{phase}: unsupported sentinel level {other:?}"),
            };
            assert_eq!(
                object(selected, &sentinel.level).get(&sentinel.field),
                Some(&sentinel.value),
                "{phase}: {} sentinel {} changed",
                sentinel.level,
                sentinel.field
            );
        }
    }
    assert_eq!(
        levels,
        REQUIRED_SENTINEL_LEVELS.into_iter().collect(),
        "{phase}: sentinel coverage must contain exactly the eight required levels"
    );
}

fn bead(workspace: &Path, args: &[&str]) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command.current_dir(workspace).args(args);
    command
}

fn active_records(workspace: &Path, pointer: &Value) -> Vec<Value> {
    let root = workspace
        .join(".beads/checkpoint")
        .join(pointer["active_root"]["path"].as_str().unwrap());
    read_records(&root)
}

#[test]
fn unknown_field_fixture_restores_and_republishes_all_required_levels() {
    let fixture = fixture_dir();
    let expectations = fixture_expectations();
    let pointer = fixture_pointer();
    let root_path = fixture.join(&expectations.input_root);
    let root_bytes = fs::read(&root_path).unwrap();
    let root_hash = format!("{:x}", Sha256::digest(root_bytes));

    assert_eq!(expectations.fixture_format, 1);
    assert_eq!(expectations.profile, "native-v1");
    assert_eq!(pointer["mode"], expectations.input_mode);
    assert_eq!(expectations.input_pointer, "current.json");
    assert_eq!(pointer["active_root"]["path"], expectations.input_root);
    assert_eq!(pointer["active_root"]["sha256"], root_hash);
    let records = fixture_records();
    assert_eq!(records.len(), 4);
    assert_eq!(pointer["total_record_count"], records.len());
    assert_sentinels(&records, &pointer, &expectations, "fixture input");

    let target = TempDir::new().unwrap();
    bead(
        target.path(),
        &[
            "init",
            "--prefix",
            "ufx",
            "--no-auto-flush",
            "--skip-foreign-workspace",
        ],
    )
    .assert()
    .success();
    bead(
        target.path(),
        &[
            "sync",
            "import-only",
            "--input",
            fixture.to_str().unwrap(),
            "--restore-into-empty",
            "--actor",
            "unknown-field-fixture-test",
        ],
    )
    .assert()
    .success();
    bead(target.path(), &["sync", "flush-only"])
        .assert()
        .success();

    let restored_pointer = read_json(&target.path().join(".beads/checkpoint/current.json"));
    let restored_records = active_records(target.path(), &restored_pointer);
    assert_json_equal_except_allowances(
        &pointer,
        &restored_pointer,
        "",
        &pointer_allowances(&expectations),
        "restored publication pointer",
    );
    assert_source_records_preserved(
        &records,
        &restored_records,
        &expectations,
        "restored publication records",
    );
    assert_sentinels(
        &restored_records,
        &restored_pointer,
        &expectations,
        "restored publication",
    );
}

#[test]
fn unknown_field_fixture_declares_only_documented_native_rewrites() {
    let expectations = fixture_expectations();
    assert!(!expectations.allowed_native_rewrites.is_empty());
    for rewrite in &expectations.allowed_native_rewrites {
        assert!(
            rewrite.path.starts_with('/')
                && !rewrite.path.contains('*')
                && !rewrite.kind.trim().is_empty()
                && !rewrite.reason.trim().is_empty(),
            "native rewrite documentation must identify one exact path, kind, and reason"
        );
    }
}

#[test]
fn malformed_sentinel_coverage_reports_the_missing_count() {
    let mut expected = read_json(&fixture_dir().join("expected.json"));
    expected["sentinels"].as_array_mut().unwrap().pop();
    let error = parse_expectations(&expected).expect_err("missing sentinel must be rejected");
    assert!(
        error.contains("expected.json.sentinels: expected exactly 8 entries")
            && error.contains("found 7"),
        "diagnostic should identify the missing sentinel coverage: {error}"
    );
}

#[test]
fn malformed_sentinel_selector_reports_its_exact_location() {
    let mut expected = read_json(&fixture_dir().join("expected.json"));
    expected["sentinels"][0]["selector"]["id"] = Value::Null;
    let error = parse_expectations(&expected).expect_err("malformed selector must be rejected");
    assert!(
        error.contains("sentinels[0].selector.id") && error.contains("expected a string"),
        "diagnostic should identify the malformed selector: {error}"
    );
}
