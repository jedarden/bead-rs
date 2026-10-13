//! Conformance suite for the public comment mutation commands (R010,
//! comment-mutation-v1).
//!
//! Covers the command contract end to end: help, creation, replies,
//! resolution transitions (including the import-compatibility NULL), body
//! transports, selector/refusal rules, exit codes, projection defaults,
//! checkpoint round-trip, capability advertisement, revision independence,
//! dry-runs, the actor ladder, and the secret-scan interaction. Any test here
//! fails if the `comment` command tree is removed.

use assert_cmd::Command;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::Path;

fn command(root: &Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command
        .current_dir(root)
        .env("BEAD_ORG_SECRET_SCANNER", "off")
        .args(["--skip-foreign-workspace", "--no-auto-flush"]);
    command
}

fn workspace() -> (tempfile::TempDir, String) {
    let dir = tempfile::Builder::new()
        .prefix("bead-comment-mutation-")
        .tempdir_in("/var/tmp")
        .unwrap();
    command(dir.path()).arg("init").assert().success();
    let issue = create_issue(dir.path(), "comment mutation probe");
    (dir, issue)
}

fn create_issue(root: &Path, title: &str) -> String {
    let output = command(root)
        .args(["create", "--title", title])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn comment_command(root: &Path, args: &[&str]) -> Command {
    let mut command = command(root);
    command.arg("comment").args(args);
    command
}

/// The issue's projected comments through `show --comments <LEVEL> --json`.
fn projected_comments(root: &Path, issue: &str, level: &str) -> Vec<Value> {
    let output = command(root)
        .args(["show", issue, "--comments", level, "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: Value = serde_json::from_str(stdout.trim()).unwrap();
    let issue_json = if parsed.is_array() {
        parsed.as_array().unwrap()[0].clone()
    } else {
        parsed
    };
    issue_json["comments"]
        .as_array()
        .expect("comments array")
        .clone()
}

fn comments(root: &Path, issue: &str) -> Vec<Value> {
    projected_comments(root, issue, "all")
}

fn comment_by_id(root: &Path, issue: &str, comment_id: &str) -> Value {
    comments(root, issue)
        .into_iter()
        .find(|comment| comment["id"] == *comment_id)
        .unwrap_or_else(|| panic!("comment {comment_id} projected on {issue}"))
}

/// A blocking body: the same AWS key shape the redaction fixtures use.
fn blocking_body() -> String {
    ["AK", "IA", "7Q9W2E4R6T8Y1U3I"].concat()
}

fn db(root: &Path) -> Connection {
    Connection::open(root.join(".beads/beads.db")).unwrap()
}

fn event_count(root: &Path, kind: &str) -> i64 {
    db(root)
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind = ?1",
            [kind],
            |row| row.get(0),
        )
        .unwrap()
}

fn comment_count(root: &Path) -> i64 {
    db(root)
        .query_row("SELECT COUNT(*) FROM comments", [], |row| row.get(0))
        .unwrap()
}

fn event_details(root: &Path, kind: &str) -> Vec<String> {
    let conn = db(root);
    let mut statement = conn
        .prepare("SELECT detail FROM events WHERE kind = ?1 ORDER BY sequence")
        .unwrap();
    let rows = statement
        .query_map([kind], |row| row.get::<_, String>(0))
        .unwrap();
    rows.map(|row| row.unwrap()).collect()
}

fn workspace_sequence(root: &Path) -> i64 {
    db(root)
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get(0)
        })
        .unwrap()
}

#[test]
fn comment_help_exits_zero_and_lists_the_tree() {
    let (root, _issue) = workspace();
    let output = comment_command(root.path(), &["--help"]).output().unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    for subcommand in ["create", "reply", "resolve", "unresolve"] {
        assert!(
            help.contains(subcommand),
            "`bead comment --help` must list `{subcommand}`"
        );
    }
}

#[test]
fn create_reply_and_resolution_transitions_conform() {
    let (root, issue) = workspace();

    // Scenario 1: create commits the row and one comment_added event; stdout
    // is the ID and never echoes the body.
    let output = comment_command(root.path(), &["create", &issue, "--body", "root"])
        .env_remove("BEAD_ACTOR")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let first = stdout.trim().to_owned();
    assert!(
        !stdout.contains("root"),
        "no output echoes the body: {stdout:?}"
    );
    assert_eq!(first, comment_by_id(root.path(), &issue, &first)["id"]);
    let created = comment_by_id(root.path(), &issue, &first);
    assert_eq!(created["author"], "system");
    assert_eq!(created["resolution_state"], "unresolved");
    assert!(created["reply_to_id"].is_null());
    let created_at = created["created_at"].as_str().unwrap();
    assert!(
        time::OffsetDateTime::parse(created_at, &time::format_description::well_known::Rfc3339)
            .is_ok(),
        "native created_at must be RFC 3339 UTC: {created_at}"
    );
    assert_eq!(event_count(root.path(), "comment_added"), 1);
    let detail: Value =
        serde_json::from_str(&event_details(root.path(), "comment_added")[0]).unwrap();
    assert_eq!(detail["comment_id"], *first);
    assert!(detail.get("body").is_none(), "bodies never enter events");

    // Scenario 2: reply commits reply_to_id and one event carrying both IDs.
    let reply_output = comment_command(
        root.path(),
        &[
            "reply",
            &issue,
            "--reply-to",
            &first,
            "--body",
            "point",
            "--actor",
            "worker-2",
        ],
    )
    .output()
    .unwrap();
    assert!(reply_output.status.success());
    let second = String::from_utf8(reply_output.stdout)
        .unwrap()
        .trim()
        .to_owned();
    assert_ne!(second, first);
    let reply = comment_by_id(root.path(), &issue, &second);
    assert_eq!(reply["reply_to_id"], *first);
    assert_eq!(reply["author"], "worker-2");
    assert_eq!(event_count(root.path(), "comment_added"), 2);
    let detail: Value =
        serde_json::from_str(&event_details(root.path(), "comment_added")[1]).unwrap();
    assert_eq!(detail["comment_id"], second);
    assert_eq!(detail["reply_to_id"], *first);

    // Scenario 3: resolve commits one comment_resolved event; the repeat is
    // a no-op that emits nothing.
    let resolve = comment_command(
        root.path(),
        &["resolve", &issue, &first, "--actor", "reviewer"],
    )
    .output()
    .unwrap();
    assert!(resolve.status.success());
    assert_eq!(
        String::from_utf8(resolve.stdout).unwrap().trim(),
        first,
        "resolution prints the target ID"
    );
    assert_eq!(
        comment_by_id(root.path(), &issue, &first)["resolution_state"],
        "resolved"
    );
    assert_eq!(event_count(root.path(), "comment_resolved"), 1);
    let sequence_before = workspace_sequence(root.path());
    let repeat = comment_command(root.path(), &["resolve", &issue, &first, "--json"])
        .output()
        .unwrap();
    assert!(repeat.status.success());
    let envelope: Value = serde_json::from_slice(&repeat.stdout).unwrap();
    assert_eq!(envelope["operation"], "resolved");
    assert_eq!(envelope["semantic_change"], false);
    assert_eq!(envelope["previous_resolution_state"], "resolved");
    assert_eq!(envelope["resolution_state"], "resolved");
    assert!(envelope.get("reply_to_id").is_none());
    assert!(envelope.get("body").is_none());
    assert_eq!(
        workspace_sequence(root.path()),
        sequence_before,
        "a no-op transition emits no event"
    );

    // unresolve emits comment_unresolved once.
    let unresolve = comment_command(root.path(), &["unresolve", &issue, &first])
        .output()
        .unwrap();
    assert!(unresolve.status.success());
    assert_eq!(event_count(root.path(), "comment_unresolved"), 1);
    assert_eq!(
        comment_by_id(root.path(), &issue, &first)["resolution_state"],
        "unresolved"
    );

    // Scenario 9 needs a resolved parent again: the sequence above left
    // `first` unresolved, so re-resolve before the omission claims below.
    comment_command(root.path(), &["resolve", &issue, &first])
        .assert()
        .success();

    // Scenario 9: the reply and the resolved state are visible through the
    // unchanged projection defaults.
    let all = comments(root.path(), &issue);
    assert_eq!(all.len(), 2);
    assert!(all[0].get("body").is_some());
    let unresolved = projected_comments(root.path(), &issue, "unresolved");
    assert_eq!(unresolved.len(), 1, "the resolved parent is omitted");
    assert_eq!(unresolved[0]["id"], second);
    let default_show = command(root.path())
        .args(["show", &issue, "--json"])
        .output()
        .unwrap();
    let default_json: Value = serde_json::from_slice(&default_show.stdout).unwrap();
    let default_issue = if default_json.is_array() {
        default_json.as_array().unwrap()[0].clone()
    } else {
        default_json
    };
    assert!(
        default_issue["comments"]
            .as_array()
            .unwrap()
            .iter()
            .all(|comment| comment.get("body").is_none()),
        "the default projection carries no bodies"
    );

    // Scenario 11: comment traffic never moves the parent revision.
    let before = {
        let output = command(root.path())
            .args(["show", &issue, "--json"])
            .output()
            .unwrap();
        let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
        let issue_json = if parsed.is_array() {
            parsed.as_array().unwrap()[0].clone()
        } else {
            parsed
        };
        (
            issue_json["revision"].clone(),
            issue_json["updated_at"].clone(),
        )
    };
    comment_command(root.path(), &["create", &issue, "--body", "later"])
        .assert()
        .success();
    let after = {
        let output = command(root.path())
            .args(["show", &issue, "--json"])
            .output()
            .unwrap();
        let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
        let issue_json = if parsed.is_array() {
            parsed.as_array().unwrap()[0].clone()
        } else {
            parsed
        };
        (
            issue_json["revision"].clone(),
            issue_json["updated_at"].clone(),
        )
    };
    assert_eq!(before, after, "comment mutations never bump the issue row");

    // A concurrent `update --if-revision N` still succeeds after comments.
    let revision = before.0.as_i64().unwrap();
    command(root.path())
        .args([
            "update",
            &issue,
            "--notes",
            "after comments",
            "--if-revision",
            &revision.to_string(),
        ])
        .assert()
        .success();
}

#[test]
fn imported_null_resolution_state_reads_as_unresolved() {
    // Scenario 4: a comment whose stored resolution state is NULL (the
    // import-compatibility spelling) unresolves as a no-op and resolves as a
    // semantic transition whose envelope reports a null previous state.
    let (root, issue) = workspace();
    let output = comment_command(root.path(), &["create", &issue, "--body", "legacy"])
        .output()
        .unwrap();
    let comment = String::from_utf8(output.stdout).unwrap().trim().to_owned();
    db(root.path())
        .execute(
            "UPDATE comments SET resolution_state = NULL WHERE id = ?1",
            [&comment],
        )
        .unwrap();

    let sequence_before = workspace_sequence(root.path());
    let noop = comment_command(root.path(), &["unresolve", &issue, &comment, "--json"])
        .output()
        .unwrap();
    assert!(noop.status.success());
    let envelope: Value = serde_json::from_slice(&noop.stdout).unwrap();
    assert_eq!(envelope["operation"], "unresolved");
    assert_eq!(envelope["semantic_change"], false);
    assert_eq!(envelope["previous_resolution_state"], Value::Null);
    assert_eq!(envelope["resolution_state"], "unresolved");
    assert_eq!(workspace_sequence(root.path()), sequence_before);

    let resolve = comment_command(root.path(), &["resolve", &issue, &comment, "--json"])
        .output()
        .unwrap();
    assert!(resolve.status.success());
    let envelope: Value = serde_json::from_slice(&resolve.stdout).unwrap();
    assert_eq!(envelope["semantic_change"], true);
    assert_eq!(envelope["previous_resolution_state"], Value::Null);
    assert_eq!(envelope["resolution_state"], "resolved");
    assert_eq!(workspace_sequence(root.path()), sequence_before + 1);
}

#[test]
fn selector_and_target_refusals_map_to_exit_codes() {
    // Scenarios 5 and 6: missing selectors exit 3, bad targets exit 4, and
    // nothing is touched.
    let (root, issue) = workspace();
    let comments_before = comment_count(root.path());
    let events_before = workspace_sequence(root.path());

    let missing_issue = comment_command(root.path(), &["create", "bead-missing0", "--body", "x"])
        .output()
        .unwrap();
    assert_eq!(missing_issue.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&missing_issue.stderr).contains("Issue bead-missing0 not found")
    );

    let missing_reply = comment_command(
        root.path(),
        &[
            "reply",
            &issue,
            "--reply-to",
            "comment-00000000",
            "--body",
            "x",
        ],
    )
    .output()
    .unwrap();
    assert_eq!(missing_reply.status.code(), Some(4));

    let missing_resolve = comment_command(root.path(), &["resolve", &issue, "comment-00000000"])
        .output()
        .unwrap();
    assert_eq!(missing_resolve.status.code(), Some(4));

    // A cross-issue reply target exists but belongs to another issue.
    let other = create_issue(root.path(), "other issue");
    let target = comment_command(root.path(), &["create", &other, "--body", "theirs"])
        .output()
        .unwrap();
    let theirs = String::from_utf8(target.stdout).unwrap().trim().to_owned();
    let cross = comment_command(
        root.path(),
        &["reply", &issue, "--reply-to", &theirs, "--body", "x"],
    )
    .output()
    .unwrap();
    assert_eq!(cross.status.code(), Some(4));
    let cross_resolve = comment_command(root.path(), &["resolve", &issue, &theirs])
        .output()
        .unwrap();
    assert_eq!(cross_resolve.status.code(), Some(4));

    assert_eq!(comment_count(root.path()), comments_before + 1);
    assert_eq!(
        workspace_sequence(root.path()),
        events_before + 2,
        "only the other issue's creation event and its own comment creation committed"
    );
}

#[test]
fn body_transport_rules() {
    // Scenario 7: transports are exclusive, emptiness is refused, stdin is
    // stored and scanned like the literal form, and failures exit per the
    // ladder without echoing the body.
    let (root, issue) = workspace();

    let neither = comment_command(root.path(), &["create", &issue])
        .output()
        .unwrap();
    assert_eq!(neither.status.code(), Some(2));

    let file = root.path().join("body.txt");
    std::fs::write(&file, b"from file").unwrap();
    let both = comment_command(root.path(), &["create", &issue])
        .arg("--body")
        .arg("x")
        .arg("--body-file")
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(both.status.code(), Some(2));

    let empty = comment_command(root.path(), &["create", &issue, "--body", "   "])
        .output()
        .unwrap();
    assert_eq!(empty.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&empty.stderr).contains("Comment body cannot be empty"));

    // The 4 MiB cap is exercised through --body-file: a body that size
    // cannot travel as one argv element (ArgumentListTooLong), and the cap
    // applies to the transported bytes identically (V5).
    let oversize_path = root.path().join("oversize.txt");
    std::fs::write(&oversize_path, vec![b'x'; 4_194_305]).unwrap();
    let oversize = comment_command(root.path(), &["create", &issue])
        .arg("--body-file")
        .arg(&oversize_path)
        .output()
        .unwrap();
    assert_eq!(oversize.status.code(), Some(2));

    let stdin_body = comment_command(root.path(), &["create", &issue])
        .arg("--body-file")
        .arg("-")
        .write_stdin("piped body\n")
        .output()
        .unwrap();
    assert!(stdin_body.status.success());
    let piped = String::from_utf8(stdin_body.stdout)
        .unwrap()
        .trim()
        .to_owned();
    assert_eq!(
        comment_by_id(root.path(), &issue, &piped)["body"],
        "piped body\n"
    );

    let missing_file = comment_command(root.path(), &["create", &issue])
        .arg("--body-file")
        .arg(root.path().join("absent.txt"))
        .output()
        .unwrap();
    assert_eq!(missing_file.status.code(), Some(1));

    let binary = root.path().join("binary.bin");
    std::fs::write(&binary, [0xff, 0xfe, 0x00, 0x01]).unwrap();
    let invalid = comment_command(root.path(), &["create", &issue])
        .arg("--body-file")
        .arg(&binary)
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("not valid UTF-8"));
}

#[test]
fn comment_add_near_miss_names_the_remedy() {
    let (root, issue) = workspace();
    let output = comment_command(root.path(), &["add", &issue, "--body", "x"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("comment create"),
        "remedy must name create: {stderr}"
    );
    assert!(stderr.contains("unrecognized subcommand"));
}

#[test]
fn checkpoint_round_trip_preserves_comments() {
    // Scenario 10: flush publishes the rows and events; import/restore into a
    // fresh workspace restores them verbatim.
    let (root, issue) = workspace();
    let first = comment_command(
        root.path(),
        &["create", &issue, "--body", "root", "--actor", "w1"],
    )
    .output()
    .unwrap();
    let first = String::from_utf8(first.stdout).unwrap().trim().to_owned();
    let second = comment_command(
        root.path(),
        &[
            "reply",
            &issue,
            "--reply-to",
            &first,
            "--body",
            "reply",
            "--actor",
            "w2",
        ],
    )
    .output()
    .unwrap();
    let second = String::from_utf8(second.stdout).unwrap().trim().to_owned();
    comment_command(root.path(), &["resolve", &issue, &first])
        .assert()
        .success();
    command(root.path())
        .args(["sync", "flush-only"])
        .assert()
        .success();

    let pointer: Value = serde_json::from_slice(
        &std::fs::read(root.path().join(".beads/checkpoint/current.json")).unwrap(),
    )
    .unwrap();
    let generation = pointer["generation_id"].as_str().unwrap().to_owned();

    let target = tempfile::Builder::new()
        .prefix("bead-comment-restore-")
        .tempdir_in("/var/tmp")
        .unwrap();
    command(target.path())
        .args([
            "restore",
            "--source",
            root.path().join(".beads/checkpoint").to_str().unwrap(),
            "--generation",
            &generation,
            "--actor",
            "tester",
        ])
        .assert()
        .success();

    let restored = comments(target.path(), &issue);
    assert_eq!(restored.len(), 2);
    let restored_first = comment_by_id(target.path(), &issue, &first);
    assert_eq!(restored_first["body"], "root");
    assert_eq!(restored_first["author"], "w1");
    assert_eq!(restored_first["resolution_state"], "resolved");
    let restored_second = comment_by_id(target.path(), &issue, &second);
    assert_eq!(restored_second["reply_to_id"], first);
    assert_eq!(restored_second["author"], "w2");
    assert_eq!(restored_second["resolution_state"], "unresolved");
    assert_eq!(event_count(target.path(), "comment_added"), 2);
    assert_eq!(event_count(target.path(), "comment_resolved"), 1);

    // The restored comments stay mutable in the restored workspace.
    let resolve = comment_command(target.path(), &["resolve", &issue, &second])
        .output()
        .unwrap();
    assert!(resolve.status.success());
}

#[test]
fn capabilities_advertise_comment() {
    let (root, _issue) = workspace();
    let output = command(root.path())
        .args(["capabilities", "--format", "json", "--profile", "needle-v1"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let capabilities: Value = serde_json::from_slice(&output.stdout).unwrap();
    let commands = capabilities["commands"].as_array().unwrap();
    let position = commands
        .iter()
        .position(|command| command == "comment")
        .expect("capabilities.commands must advertise `comment`");
    // Alphabetical position: between `close` and `compare`.
    assert_eq!(commands[position - 1], "close");
    assert_eq!(commands[position + 1], "compare");
}

#[test]
fn dry_run_projects_the_envelope_without_committing() {
    let (root, issue) = workspace();
    let before_comments = comment_count(root.path());
    let before_sequence = workspace_sequence(root.path());

    let dry = comment_command(root.path(), &["create", &issue, "--body", "would commit"])
        .arg("--dry-run")
        .output()
        .unwrap();
    assert!(dry.status.success());
    let envelope: Value = serde_json::from_slice(&dry.stdout).unwrap();
    assert_eq!(envelope["operation"], "created");
    assert_eq!(envelope["semantic_change"], true);
    assert_eq!(
        envelope["workspace_sequence"],
        Value::from(before_sequence),
        "a dry-run reports the current maximum"
    );
    assert!(
        !String::from_utf8_lossy(&dry.stdout).contains("would commit"),
        "the envelope never carries the body"
    );

    // A dry-run of a no-op reports semantic_change false.
    let target = comment_command(root.path(), &["create", &issue, "--body", "real"])
        .output()
        .unwrap();
    let target = String::from_utf8(target.stdout).unwrap().trim().to_owned();
    comment_command(root.path(), &["resolve", &issue, &target])
        .assert()
        .success();
    let dry_noop = comment_command(root.path(), &["resolve", &issue, &target])
        .arg("--dry-run")
        .output()
        .unwrap();
    let envelope: Value = serde_json::from_slice(&dry_noop.stdout).unwrap();
    assert_eq!(envelope["semantic_change"], false);

    assert_eq!(comment_count(root.path()), before_comments + 1);
    // The two dry-runs appended nothing; the real create and the real
    // resolve each appended one event.
    assert_eq!(workspace_sequence(root.path()), before_sequence + 2);
}

#[test]
fn actor_ladder_flag_env_then_system() {
    let (root, issue) = workspace();

    // --actor wins over BEAD_ACTOR and is stored verbatim.
    let flagged = comment_command(
        root.path(),
        &["create", &issue, "--body", "b", "--actor", "flag-actor"],
    )
    .env("BEAD_ACTOR", "env-actor")
    .output()
    .unwrap();
    let flagged = String::from_utf8(flagged.stdout).unwrap().trim().to_owned();
    assert_eq!(
        comment_by_id(root.path(), &issue, &flagged)["author"],
        "flag-actor"
    );

    // BEAD_ACTOR applies when no --actor is given.
    let from_env = comment_command(root.path(), &["create", &issue, "--body", "b"])
        .env("BEAD_ACTOR", "env-actor")
        .output()
        .unwrap();
    let from_env = String::from_utf8(from_env.stdout)
        .unwrap()
        .trim()
        .to_owned();
    assert_eq!(
        comment_by_id(root.path(), &issue, &from_env)["author"],
        "env-actor"
    );

    // A whitespace-only --actor falls through the ladder; the default is
    // `system`; the audit event carries the same actor.
    let default = comment_command(
        root.path(),
        &["create", &issue, "--body", "b", "--actor", "   "],
    )
    .output()
    .unwrap();
    let default = String::from_utf8(default.stdout).unwrap().trim().to_owned();
    assert_eq!(
        comment_by_id(root.path(), &issue, &default)["author"],
        "system"
    );

    // An oversize actor is refused with a usage error.
    let oversize = comment_command(root.path(), &["create", &issue])
        .arg("--body")
        .arg("b")
        .arg("--actor")
        .arg("a".repeat(256))
        .output()
        .unwrap();
    assert_eq!(oversize.status.code(), Some(2));
}

#[test]
fn secret_scan_modes_apply_to_comment_bodies() {
    // Scenario 8: enforce rejects with secret_detected and leaves nothing;
    // off does not scan. ADR-014 pins content-validation rejections at
    // exit 2 — the exit-2 `secret_detected` family every other scanned
    // field rejects through — and comment-mutation-v1's own "reuse the
    // existing error ladder without adding a code" rule adopts it here.
    let (root, issue) = workspace();
    let body = blocking_body();
    let sequence_before = workspace_sequence(root.path());

    let blocked = comment_command(root.path(), &["create", &issue])
        .arg("--body")
        .arg(&body)
        .output()
        .unwrap();
    assert_eq!(blocked.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&blocked.stderr);
    assert!(
        stderr.contains("secret_detected"),
        "stable machine reason: {stderr}"
    );
    assert_eq!(comment_count(root.path()), 0);
    assert_eq!(workspace_sequence(root.path()), sequence_before);

    let path = root.path().join(".beads/config.json");
    let mut config: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    config["secret_scan"] = json!({"mode": "off"});
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();

    let allowed = comment_command(root.path(), &["create", &issue])
        .arg("--body")
        .arg(&body)
        .env("BEAD_ORG_SECRET_SCANNER", "off")
        .output()
        .unwrap();
    assert!(
        allowed.status.success(),
        "off mode does not scan: {}",
        String::from_utf8_lossy(&allowed.stderr)
    );
    assert_eq!(comment_count(root.path()), 1);
}

#[test]
fn envelope_and_events_stay_free_of_bodies() {
    let (root, issue) = workspace();
    let body = "sensitive-body-content-marker";
    let created = comment_command(root.path(), &["create", &issue, "--body", body, "--json"])
        .output()
        .unwrap();
    assert!(created.status.success());
    let stdout = String::from_utf8(created.stdout).unwrap();
    assert!(!stdout.contains(body), "envelope carries no body: {stdout}");
    let resolved = comment_command(root.path(), &["resolve", &issue, stdout.trim()])
        .arg("--json")
        .output()
        .unwrap();
    let resolved_stdout = String::from_utf8(resolved.stdout).unwrap();
    assert!(!resolved_stdout.contains(body));
    for kind in ["comment_added", "comment_resolved"] {
        for detail in event_details(root.path(), kind) {
            assert!(!detail.contains(body), "{kind} detail carries no body");
        }
    }
}

/// The spec's reply-depth and per-comment-resolution rules.
#[test]
fn unbounded_reply_depth_and_per_comment_resolution() {
    let (root, issue) = workspace();
    let mut chain: Vec<String> = Vec::new();
    for depth in 0..5 {
        let output = if let Some(parent) = chain.last() {
            comment_command(
                root.path(),
                &[
                    "reply",
                    &issue,
                    "--reply-to",
                    parent,
                    "--body",
                    &format!("d{depth}"),
                ],
            )
            .output()
            .unwrap()
        } else {
            comment_command(root.path(), &["create", &issue, "--body", "d0"])
                .output()
                .unwrap()
        };
        assert!(output.status.success());
        chain.push(String::from_utf8(output.stdout).unwrap().trim().to_owned());
    }
    let projected = comments(root.path(), &issue);
    assert_eq!(projected.len(), 5);
    // Resolve the root only: the deepest reply stays visible under the
    // unresolved projection while its resolved parent is omitted.
    comment_command(root.path(), &["resolve", &issue, &chain[0]])
        .assert()
        .success();
    let unresolved = projected_comments(root.path(), &issue, "unresolved");
    assert_eq!(unresolved.len(), 4);
    assert!(!unresolved.iter().any(|comment| comment["id"] == chain[0]));
}
