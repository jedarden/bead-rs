//! R033: atomic bulk transaction manifests.
//!
//! A manifest is a thin composition of existing command primitives applied
//! in array order inside one SQLite transaction. The contract under test is
//! `research/specs/bulk-manifests-v1.md`: document validation fails before
//! anything runs, a dry-run reports the exact semantic delta because it
//! executes the same transaction and rolls it back, a commit is all-or-none
//! and publishes exactly one checkpoint generation for the whole manifest
//! (where the equivalent individual commands publish one each), and the
//! result map links every manifest entry to the real IDs it created or
//! touched.

use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run(workspace: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bead"))
        .current_dir(workspace)
        .arg("--skip-foreign-workspace")
        .args(args)
        .output()
        .expect("bead command should start")
}

fn setup(prefix: &str) -> tempfile::TempDir {
    let workspace = tempfile::tempdir().unwrap();
    let output = run(workspace.path(), &["init", "--prefix", prefix]);
    assert!(output.status.success(), "init failed: {output:?}");
    workspace
}

/// Write a manifest document into the workspace and return its path.
fn write_manifest(workspace: &Path, name: &str, body: &str) -> PathBuf {
    let path = workspace.join(name);
    fs::write(&path, body).unwrap();
    path
}

/// Run `bead manifest <SUB> --input <PATH> --format json` and parse stdout.
fn manifest_json(workspace: &Path, sub: &str, path: &Path) -> (Output, Value) {
    let output = run(
        workspace,
        &[
            "manifest",
            sub,
            "--input",
            &path.display().to_string(),
            "--format",
            "json",
        ],
    );
    let report = if output.status.success() {
        serde_json::from_slice(&output.stdout).unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    (output, report)
}

fn manifest(workspace: &Path, sub: &str, path: &Path) -> Output {
    run(
        workspace,
        &["manifest", sub, "--input", &path.display().to_string()],
    )
}

fn open_db(workspace: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(workspace.join(".beads/beads.db")).unwrap()
}

fn count(workspace: &Path, sql: &str) -> i64 {
    let conn = open_db(workspace);
    conn.query_row(sql, [], |row| row.get(0)).unwrap()
}

fn issue_count(workspace: &Path) -> i64 {
    count(workspace, "SELECT COUNT(*) FROM issues")
}

fn event_count(workspace: &Path) -> i64 {
    count(workspace, "SELECT COUNT(*) FROM events")
}

fn create_issue(workspace: &Path, title: &str) -> String {
    let output = run(workspace, &["create", "--title", title]);
    assert!(output.status.success(), "create failed: {output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

/// `sync status --format json` parsed from stdout.
fn status(workspace: &Path) -> Value {
    let output = run(workspace, &["sync", "status", "--format", "json"]);
    assert!(output.status.success(), "status failed: {output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}

/// Every file under `.beads/checkpoint/` with its bytes, keyed by
/// checkpoint-relative path. Publication rewrites pointers and mints
/// content-addressed objects, so any publication at all changes this map --
/// and only a publication does.
fn snapshot_checkpoint(workspace: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, prefix: String, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_str().unwrap().to_string();
            if path.is_dir() {
                walk(&path, format!("{prefix}{name}/"), out);
            } else {
                out.insert(format!("{prefix}{name}"), fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(
        &workspace.join(".beads/checkpoint"),
        String::new(),
        &mut out,
    );
    out
}

fn assert_exit(output: &Output, code: i32, context: &str) {
    let actual = output.status.code().unwrap_or(-1);
    assert_eq!(
        actual,
        code,
        "{context}: expected exit {code}, got {actual}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn assert_stderr_contains(output: &Output, needle: &str, context: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(needle),
        "{context}: stderr did not contain {needle:?}\nstderr: {stderr}",
    );
}

/// Document validation failures are malformed input: exit 5 from both
/// subcommands, reported before the workspace is touched.
#[test]
fn malformed_documents_are_refused_by_both_subcommands() {
    let workspace = setup("r033mal");
    let bad_manifests: &[(&str, &str)] = &[
        (
            "unsupported version",
            r#"{"manifest_version": 2, "operations": []}"#,
        ),
        (
            "non-integer version",
            r#"{"manifest_version": "1", "operations": []}"#,
        ),
        ("missing operations", r#"{"manifest_version": 1}"#),
        (
            "extra root key",
            r#"{"manifest_version": 1, "operations": [], "actor": "x"}"#,
        ),
        (
            "operations not an array",
            r#"{"manifest_version": 1, "operations": {}}"#,
        ),
        (
            "operation not an object",
            r#"{"manifest_version": 1, "operations": ["create"]}"#,
        ),
        (
            "unknown op kind",
            r#"{"manifest_version": 1, "operations": [
                {"op": "reopen", "id": "r033mal-00000000"}]}"#,
        ),
        (
            "unknown field",
            r#"{"manifest_version": 1, "operations": [
                {"op": "create", "title": "t", "mystery": true}]}"#,
        ),
        (
            "fieldless update",
            r#"{"manifest_version": 1, "operations": [
                {"op": "update", "id": "r033mal-00000000"}]}"#,
        ),
        (
            "wrong field type",
            r#"{"manifest_version": 1, "operations": [
                {"op": "create", "title": "t", "priority": "high"}]}"#,
        ),
        (
            "local_id containing $",
            r#"{"manifest_version": 1, "operations": [
                {"op": "create", "local_id": "$a", "title": "t"}]}"#,
        ),
        (
            "duplicate local_id",
            r#"{"manifest_version": 1, "operations": [
                {"op": "create", "local_id": "a", "title": "t"},
                {"op": "create", "local_id": "a", "title": "u"}]}"#,
        ),
        (
            "forward reference",
            r#"{"manifest_version": 1, "operations": [
                {"op": "close", "id": "$a", "reason": "r"},
                {"op": "create", "local_id": "a", "title": "t"}]}"#,
        ),
        (
            "undefined reference",
            r#"{"manifest_version": 1, "operations": [
                {"op": "label_add", "id": "$nope", "label": "x"}]}"#,
        ),
    ];

    let events_before_refusals = event_count(workspace.path());

    for (name, body) in bad_manifests {
        let path = write_manifest(workspace.path(), "bad.json", body);
        let dry = manifest(workspace.path(), "dry-run", &path);
        assert_exit(&dry, 5, &format!("dry-run refused {name}"));
        let commit = manifest(workspace.path(), "commit", &path);
        assert_exit(&commit, 5, &format!("commit refused {name}"));
    }

    // Nothing was executed: no issue appeared, and the event stream is
    // exactly what it was before the refused documents ran (init itself
    // appends no events, so this is zero for a fresh workspace).
    assert_eq!(issue_count(workspace.path()), 0);
    assert_eq!(
        event_count(workspace.path()),
        events_before_refusals,
        "a refused document executed something"
    );
}

/// A dry-run reports the full semantic delta of every operation, including
/// before/after field changes, and leaves the database and checkpoint
/// byte-identical.
#[test]
fn dry_run_reports_the_full_semantic_delta_without_mutation() {
    let workspace = setup("r033dry");
    let existing = create_issue(workspace.path(), "existing work");

    // The close targets the issue this manifest creates, not the one it
    // claims: `existing` is assigned by the second operation, which mints a
    // claim epoch, and a credential-less manifest op cannot mutate a claimed
    // issue any more than `bead close` can (see tests/claim_epoch.rs). A
    // static manifest could not carry that epoch anyway -- it is minted later
    // in the same transaction -- so claim-then-close is not expressible here
    // by design.
    let body = format!(
        r#"{{"manifest_version": 1, "operations": [
            {{"op": "create", "local_id": "a", "title": "manifest work",
             "priority": 1, "labels": ["docs"]}},
            {{"op": "update", "id": "{existing}", "status": "in_progress",
             "assignee": "worker-1"}},
            {{"op": "label_add", "id": "{existing}", "label": "ops"}},
            {{"op": "dep_add", "blocked": "$a", "blocker": "{existing}"}},
            {{"op": "close", "id": "$a", "reason": "done"}}
        ]}}"#
    );
    let path = write_manifest(workspace.path(), "plan.json", &body);

    let events_before = event_count(workspace.path());
    let checkpoint_before = snapshot_checkpoint(workspace.path());
    let live_before = status(workspace.path())["live_sequence"].clone();

    let (output, report) = manifest_json(workspace.path(), "dry-run", &path);
    assert_exit(&output, 0, "dry-run of a valid manifest");
    assert_eq!(report["dry_run"], Value::Bool(true));
    assert_eq!(report["committed"], Value::Bool(false));
    assert_eq!(report["operations"], Value::from(5));
    assert_eq!(report["semantic_changes"], Value::from(5));
    assert_eq!(report["workspace_sequence"], live_before);

    let results = report["results"].as_array().unwrap();
    assert_eq!(results.len(), 5);

    // create: local_id linked to the provisional ID it minted.
    assert_eq!(results[0]["op"], Value::String("create".into()));
    assert_eq!(results[0]["local_id"], Value::String("a".into()));
    assert_eq!(results[0]["outcome"], Value::String("created".into()));
    let provisional = results[0]["issue_id"].as_str().unwrap();
    assert!(provisional.starts_with("r033dry-"), "id was {provisional}");
    assert_eq!(
        results[0]["issue"]["title"],
        Value::String("manifest work".into())
    );
    assert_eq!(results[0]["issue"]["labels"], serde_json::json!(["docs"]));

    // update: the delta reports before/after for every field it moved.
    assert_eq!(results[1]["op"], Value::String("update".into()));
    assert_eq!(results[1]["id"], Value::String(existing.clone()));
    assert_eq!(results[1]["outcome"], Value::String("updated".into()));
    let changes = &results[1]["changes"];
    assert_eq!(
        changes["base_status"]["before"],
        Value::String("open".into())
    );
    assert_eq!(
        changes["base_status"]["after"],
        Value::String("in_progress".into())
    );
    assert_eq!(changes["assignee"]["before"], Value::Null);
    assert_eq!(
        changes["assignee"]["after"],
        Value::String("worker-1".into())
    );

    assert_eq!(results[2]["outcome"], Value::String("added".into()));
    assert_eq!(results[2]["label"], Value::String("ops".into()));

    // dep_add: the local reference resolved to the create's provisional ID.
    assert_eq!(results[3]["op"], Value::String("dep_add".into()));
    assert_eq!(results[3]["blocked"], Value::String(provisional.into()));
    assert_eq!(results[3]["blocker"], Value::String(existing.clone()));
    assert_eq!(results[3]["outcome"], Value::String("added".into()));

    assert_eq!(results[4]["outcome"], Value::String("closed".into()));
    assert_eq!(
        results[4]["changes"]["close_reason"]["after"],
        Value::String("done".into())
    );

    // Nothing mutated: same single issue, same events, byte-identical
    // checkpoint (no publication, not even a rewritten pointer).
    assert_eq!(issue_count(workspace.path()), 1);
    assert_eq!(event_count(workspace.path()), events_before);
    assert_eq!(
        snapshot_checkpoint(workspace.path()),
        checkpoint_before,
        "dry-run changed the durable checkpoint"
    );
}

/// A dry-run executes the real transaction, so it fails exactly where a
/// commit would fail -- against the same guards, with the failing
/// operation's position prepended.
#[test]
fn dry_run_fails_exactly_where_commit_would_fail() {
    let workspace = setup("r033fail");

    // A real ID that does not exist fails at its operation's position with
    // the command's not-found error, not malformed input.
    let path = write_manifest(
        workspace.path(),
        "missing.json",
        r#"{"manifest_version": 1, "operations": [
            {"op": "create", "local_id": "a", "title": "valid create"},
            {"op": "close", "id": "r033fail-deadbeef", "reason": "r"}
        ]}"#,
    );
    let (output, _) = manifest_json(workspace.path(), "dry-run", &path);
    assert_exit(&output, 3, "not-found target");
    assert_stderr_contains(&output, "operation 1 (close)", "not-found context");
    assert_stderr_contains(&output, "r033fail-deadbeef", "not-found names the id");
    assert_eq!(issue_count(workspace.path()), 0, "nothing survived");

    // A blocks cycle formed inside one manifest is caught at the second
    // edge: cycle detection sees edges earlier operations added.
    let path = write_manifest(
        workspace.path(),
        "cycle.json",
        r#"{"manifest_version": 1, "operations": [
            {"op": "create", "local_id": "a", "title": "one"},
            {"op": "create", "local_id": "b", "title": "two"},
            {"op": "dep_add", "blocked": "$a", "blocker": "$b"},
            {"op": "dep_add", "blocked": "$b", "blocker": "$a"}
        ]}"#,
    );
    let (output, _) = manifest_json(workspace.path(), "dry-run", &path);
    assert_exit(&output, 4, "in-manifest cycle");
    assert_stderr_contains(&output, "cycle", "cycle refusal");
    assert_stderr_contains(&output, "operation 3 (dep_add)", "cycle position");
    assert_eq!(issue_count(workspace.path()), 0);
}

/// A commit is all-or-none: an operation that fails at its position rolls
/// back every earlier operation's issue, label, edge, and event.
#[test]
fn commit_is_all_or_none() {
    let workspace = setup("r033atom");
    let existing = create_issue(workspace.path(), "survivor");

    let path = write_manifest(
        workspace.path(),
        "partial.json",
        &format!(
            r#"{{"manifest_version": 1, "operations": [
                {{"op": "create", "local_id": "a", "title": "would be created"}},
                {{"op": "label_add", "id": "$a", "label": "docs"}},
                {{"op": "dep_add", "blocked": "$a", "blocker": "{existing}"}},
                {{"op": "close", "id": "r033atom-deadbeef", "reason": "boom"}}
            ]}}"#
        ),
    );

    let events_before = event_count(workspace.path());
    let checkpoint_before = snapshot_checkpoint(workspace.path());

    let (output, _) = manifest_json(workspace.path(), "commit", &path);
    assert_exit(&output, 3, "mid-manifest failure");
    assert_stderr_contains(&output, "operation 3 (close)", "failure position");

    // The three earlier operations left nothing behind.
    assert_eq!(issue_count(workspace.path()), 1, "only the survivor exists");
    assert_eq!(
        count(workspace.path(), "SELECT COUNT(*) FROM labels"),
        0,
        "no label survived"
    );
    assert_eq!(
        count(workspace.path(), "SELECT COUNT(*) FROM dependencies"),
        0,
        "no edge survived"
    );
    assert_eq!(
        event_count(workspace.path()),
        events_before,
        "no event survived the rollback"
    );
    // The command failed, so the chokepoint never ran either.
    assert_eq!(
        snapshot_checkpoint(workspace.path()),
        checkpoint_before,
        "a failed commit published a generation"
    );
}

/// One manifest commit publishes exactly one checkpoint generation for the
/// whole manifest, where the equivalent individual commands publish one
/// each, and the event stream it leaves is the union of theirs.
#[test]
fn commit_publishes_exactly_one_generation_for_the_whole_manifest() {
    let via_manifest = setup("r033one");
    let via_commands = setup("r033onecmd");

    // Two setup mutations per workspace put each checkpoint in its steady
    // state (current + previous + one object per retained generation)
    // before the snapshot, so only the operation under test can move the
    // file set.
    for workspace in [via_manifest.path(), via_commands.path()] {
        create_issue(workspace, "steady state setup 1");
        create_issue(workspace, "steady state setup 2");
    }

    let body = r#"{"manifest_version": 1, "operations": [
        {"op": "create", "local_id": "a", "title": "blocked work", "labels": ["docs"]},
        {"op": "create", "local_id": "b", "title": "blocker work"},
        {"op": "dep_add", "blocked": "$a", "blocker": "$b"},
        {"op": "update", "id": "$b", "status": "in_progress"},
        {"op": "label_add", "id": "$b", "label": "sprint"}
    ]}"#;
    let path = write_manifest(via_manifest.path(), "plan.json", body);

    let manifest_events_before = event_count(via_manifest.path());
    let manifest_checkpoint_before = snapshot_checkpoint(via_manifest.path());
    let manifest_generation_before = status(via_manifest.path())["generation_id"]
        .as_str()
        .unwrap()
        .to_string();

    let (output, report) = manifest_json(via_manifest.path(), "commit", &path);
    assert_exit(&output, 0, "manifest commit");
    assert_eq!(report["committed"], Value::Bool(true));
    assert_eq!(report["dry_run"], Value::Bool(false));
    assert_eq!(report["operations"], Value::from(5));
    assert_eq!(report["semantic_changes"], Value::from(5));

    // The result map links every entry to real IDs: the creates name the
    // issue IDs they minted, and later entries carry those same real IDs
    // where they referenced them as $a / $b.
    let results = report["results"].as_array().unwrap();
    let created_a = results[0]["issue_id"].as_str().unwrap().to_string();
    let created_b = results[1]["issue_id"].as_str().unwrap().to_string();
    assert_eq!(results[0]["local_id"], Value::String("a".into()));
    assert_eq!(results[0]["outcome"], Value::String("created".into()));
    assert_eq!(results[1]["local_id"], Value::String("b".into()));
    assert_eq!(results[2]["blocked"], Value::String(created_a.clone()));
    assert_eq!(results[2]["blocker"], Value::String(created_b.clone()));
    assert_eq!(results[3]["id"], Value::String(created_b.clone()));
    assert_eq!(
        results[3]["changes"]["base_status"]["after"],
        Value::String("in_progress".into())
    );
    assert_eq!(results[4]["id"], Value::String(created_b.clone()));

    // Everything landed in the live store.
    {
        let conn = open_db(via_manifest.path());
        let (status_b, reason_label): (String, i64) = conn
            .query_row(
                "SELECT base_status,
                        (SELECT COUNT(*) FROM labels
                         WHERE issue_id = ?1 AND label = 'sprint')
                 FROM issues WHERE id = ?1",
                [&created_b],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status_b, "in_progress");
        assert_eq!(reason_label, 1);
        let labels_a: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM labels WHERE issue_id = ?1 AND label = 'docs'",
                [&created_a],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(labels_a, 1);
        let edge: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM dependencies
                 WHERE blocked_issue_id = ?1 AND blocker_issue_id = ?2 AND kind = 'blocks'",
                [&created_a, &created_b],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(edge, 1);
    }

    // The chokepoint published: the checkpoint covers the live sequence,
    // the pointer moved to a new generation, and exactly one object (the
    // new root) was minted for the whole five-operation manifest.
    let manifest_status = status(via_manifest.path());
    assert_eq!(
        manifest_status["covered_sequence"], manifest_status["live_sequence"],
        "the manifest's generation does not cover its whole event span"
    );
    assert_eq!(
        manifest_status["ready_to_commit"],
        Value::Bool(true),
        "the published generation left the checkpoint dirty"
    );
    let manifest_generation_after = manifest_status["generation_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(
        manifest_generation_after, manifest_generation_before,
        "five semantic operations published no generation"
    );
    let manifest_checkpoint_after = snapshot_checkpoint(via_manifest.path());
    let manifest_added: Vec<&String> = manifest_checkpoint_after
        .keys()
        .filter(|key| !manifest_checkpoint_before.contains_key(*key))
        .collect();
    let root_path = manifest_status["root_path"].as_str().unwrap().to_string();
    assert_eq!(
        manifest_added,
        vec![&root_path],
        "one manifest must mint exactly one object; instead it added {manifest_added:?}"
    );

    // The equivalent individual commands on a twin workspace leave the
    // same event stream -- the union, no manifest-level event -- but
    // publish one generation per command: six generations where the
    // manifest published one. Publication rewrites the content-addressed
    // root and retention deletes the superseded one, so object files on
    // disk do not accumulate; the observable is the `generation_id` the
    // pointer carries after each command.
    let commands_events_before = event_count(via_commands.path());
    let generation = || {
        status(via_commands.path())["generation_id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let generation_before_commands = generation();
    let mut generations = Vec::new();

    let id_a = create_issue(via_commands.path(), "blocked work");
    generations.push(generation());
    assert!(
        run(
            via_commands.path(),
            &["label", "add", &id_a, "--label", "docs"],
        )
        .status
        .success(),
        "twin label add a"
    );
    generations.push(generation());
    let id_b = create_issue(via_commands.path(), "blocker work");
    generations.push(generation());
    assert!(
        run(via_commands.path(), &["dep", "add", &id_a, &id_b])
            .status
            .success(),
        "twin dep add"
    );
    generations.push(generation());
    assert!(
        run(
            via_commands.path(),
            &["update", &id_b, "--status", "in_progress"],
        )
        .status
        .success(),
        "twin update"
    );
    generations.push(generation());
    assert!(
        run(
            via_commands.path(),
            &["label", "add", &id_b, "--label", "sprint"],
        )
        .status
        .success(),
        "twin label add b"
    );
    generations.push(generation());

    // The twin's creates did not carry labels at create time, so the twin
    // ran one extra label_add; align on the create-with-label path by
    // comparing against the manifest's count minus that one event.
    assert_eq!(
        event_count(via_manifest.path()) - manifest_events_before,
        event_count(via_commands.path()) - commands_events_before - 1,
        "the manifest's event stream must be exactly the union of the \
         equivalent commands' events"
    );

    // One generation per command: every command left a new generation_id,
    // none repeated, none equal to the pre-run pointer. Six generations
    // for six commands -- the per-command publication this bead exists to
    // collapse -- where the five-operation manifest published one.
    assert_eq!(generations.len(), 6);
    let mut distinct = std::collections::BTreeSet::from_iter(generations.iter().cloned());
    distinct.insert(generation_before_commands);
    assert_eq!(
        distinct.len(),
        7,
        "the twin's six commands did not each publish their own generation"
    );
}

/// A manifest whose operations all land on the commands' own idempotent
/// no-op paths commits successfully, changes nothing semantic, and
/// publishes no generation at all.
#[test]
fn all_no_op_manifest_commits_without_publishing() {
    let workspace = setup("r033noop");

    let first = write_manifest(
        workspace.path(),
        "first.json",
        r#"{"manifest_version": 1, "operations": [
            {"op": "create", "local_id": "a", "title": "once", "labels": ["docs"],
             "unique_ref": "tracker:issue-1"}
        ]}"#,
    );
    let (output, report) = manifest_json(workspace.path(), "commit", &first);
    assert_exit(&output, 0, "first commit");
    let issue_id = report["results"][0]["issue_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(report["semantic_changes"], Value::from(1));

    // Same unique_ref (existing bead), same reason close on an already
    // closed issue, label add of a label the create already attached.
    let close_output = run(workspace.path(), &["close", &issue_id, "--reason", "done"]);
    assert_exit(&close_output, 0, "setup close");
    // The close is a real mutation and publishes its own generation; only
    // what happens after it is attributable to the manifest under test.
    let checkpoint_after_first = snapshot_checkpoint(workspace.path());
    let second = write_manifest(
        workspace.path(),
        "second.json",
        r#"{"manifest_version": 1, "operations": [
            {"op": "create", "local_id": "b", "title": "again",
             "unique_ref": "tracker:issue-1"},
            {"op": "label_add", "id": "$b", "label": "docs"},
            {"op": "close", "id": "$b", "reason": "done"}
        ]}"#,
    );
    let (output, report) = manifest_json(workspace.path(), "commit", &second);
    assert_exit(&output, 0, "no-op commit succeeds");
    assert_eq!(report["semantic_changes"], Value::from(0));

    let results = report["results"].as_array().unwrap();
    assert_eq!(
        results[0]["outcome"],
        Value::String("existing_closed".into())
    );
    // The local reference resolved to the existing bead's real ID, so the
    // follow-up operations named that same bead.
    assert_eq!(results[0]["issue_id"], Value::String(issue_id.clone()));
    assert_eq!(results[1]["outcome"], Value::String("no-op".into()));
    assert_eq!(results[1]["id"], Value::String(issue_id.clone()));
    assert_eq!(results[2]["outcome"], Value::String("no-op".into()));

    assert_eq!(issue_count(workspace.path()), 1, "no duplicate was created");
    assert_eq!(
        snapshot_checkpoint(workspace.path()),
        checkpoint_after_first,
        "a no-op manifest published a generation"
    );
}

/// An empty manifest is valid: both subcommands succeed and nothing
/// happens -- no event, no generation.
#[test]
fn empty_manifest_is_a_valid_no_op() {
    let workspace = setup("r033empty");
    create_issue(workspace.path(), "steady state");

    let events_before = event_count(workspace.path());
    let checkpoint_before = snapshot_checkpoint(workspace.path());

    let empty = write_manifest(
        workspace.path(),
        "empty.json",
        r#"{"manifest_version": 1, "operations": []}"#,
    );
    let (output, report) = manifest_json(workspace.path(), "dry-run", &empty);
    assert_exit(&output, 0, "dry-run of an empty manifest");
    assert_eq!(report["operations"], Value::from(0));

    let (output, report) = manifest_json(workspace.path(), "commit", &empty);
    assert_exit(&output, 0, "commit of an empty manifest");
    assert_eq!(report["operations"], Value::from(0));
    assert_eq!(report["semantic_changes"], Value::from(0));

    assert_eq!(event_count(workspace.path()), events_before);
    assert_eq!(snapshot_checkpoint(workspace.path()), checkpoint_before);
}

/// Version 1 refuses any semantics a single existing command does not
/// already have: unspellable operations are malformed input, and a
/// spellable one that the command itself refuses fails with the command's
/// own error and rolls the whole manifest back.
#[test]
fn v1_refuses_semantics_no_command_has() {
    let workspace = setup("r033refuse");

    // Operations no v1 primitive provides, and fields no command takes.
    for body in [
        r#"{"manifest_version": 1, "operations": [{"op": "claim", "id": "r033refuse-0"}]}"#,
        r#"{"manifest_version": 1, "operations": [{"op": "release", "id": "r033refuse-0"}]}"#,
        r#"{"manifest_version": 1, "operations": [{"op": "reopen", "id": "r033refuse-0"}]}"#,
        r#"{"manifest_version": 1, "operations": [
            {"op": "update", "id": "r033refuse-0", "title": "retitled"}]}"#,
        r#"{"manifest_version": 1, "operations": [
            {"op": "create", "title": "t", "when": "* * * * *"}]}"#,
    ] {
        let path = write_manifest(workspace.path(), "refused.json", body);
        let (output, _) = manifest_json(workspace.path(), "commit", &path);
        assert_exit(&output, 5, "refused semantics must be malformed input");
    }

    // update --status closed is a real field with the command's own
    // refusal naming the remedy, so it is execution failure (exit 4), not
    // malformed input -- and the earlier create rolls back with it.
    let path = write_manifest(
        workspace.path(),
        "closed.json",
        r#"{"manifest_version": 1, "operations": [
            {"op": "create", "local_id": "a", "title": "created then rolled back"},
            {"op": "update", "id": "$a", "status": "closed"}
        ]}"#,
    );
    let (output, _) = manifest_json(workspace.path(), "commit", &path);
    assert_exit(&output, 4, "update refuses closed");
    assert_stderr_contains(&output, "Use 'close' command", "remedy is named");
    assert_stderr_contains(&output, "operation 1 (update)", "failure position");
    assert_eq!(issue_count(workspace.path()), 0, "rollback left nothing");

    // The revision guard is enforced at the operation's position against
    // the revision current there: a stale if_revision fails the manifest.
    let existing = create_issue(workspace.path(), "guarded");
    let path = write_manifest(
        workspace.path(),
        "stale.json",
        &format!(
            r#"{{"manifest_version": 1, "operations": [
                {{"op": "create", "local_id": "a", "title": "also rolled back"}},
                {{"op": "update", "id": "{existing}", "notes": "n", "if_revision": 99}}
            ]}}"#
        ),
    );
    let (output, _) = manifest_json(workspace.path(), "commit", &path);
    assert_exit(&output, 4, "stale if_revision");
    assert_eq!(
        issue_count(workspace.path()),
        1,
        "only the pre-existing issue"
    );
}

/// The text format prints one line per operation with the same information
/// the JSON result map carries.
#[test]
fn text_format_lists_every_operation() {
    let workspace = setup("r033text");
    let existing = create_issue(workspace.path(), "text target");

    let path = write_manifest(
        workspace.path(),
        "plan.json",
        &format!(
            r#"{{"manifest_version": 1, "operations": [
                {{"op": "create", "local_id": "a", "title": "text create"}},
                {{"op": "label_add", "id": "$a", "label": "docs"}},
                {{"op": "close", "id": "{existing}", "reason": "done"}}
            ]}}"#
        ),
    );

    let output = manifest(workspace.path(), "dry-run", &path);
    assert_exit(&output, 0, "text dry-run");
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("dry-run (nothing mutated)"),
        "header missing: {text}"
    );
    assert!(
        text.contains("[0] create local 'a' ->"),
        "create line: {text}"
    );
    assert!(text.contains("(created)"), "create outcome: {text}");
    assert!(text.contains("[1] label_add"), "label line: {text}");
    assert!(text.contains("[2] close"), "close line: {text}");

    // An unknown --format is a usage error, and a missing file is not found.
    let missing = run(
        workspace.path(),
        &["manifest", "dry-run", "--input", "no-such-manifest.json"],
    );
    assert_exit(&missing, 3, "missing manifest file");
    let bogus = run(
        workspace.path(),
        &[
            "manifest",
            "dry-run",
            "--input",
            &path.display().to_string(),
            "--format",
            "yaml",
        ],
    );
    assert_exit(&bogus, 2, "unknown --format");
}
