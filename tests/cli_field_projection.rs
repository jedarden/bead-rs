//! Integration tests for `--fields` projection and `--notes` bounding on
//! `show` and `list` (beadrs-0f854018).
//!
//! Contract under test:
//!
//! * `--fields` selects named `cli_issue` members per record in text, --json,
//!   and --jsonl output; unknown names exit 2 naming the valid set
//! * `--notes tail:N` keeps the last N characters, `none` omits the member,
//!   `all` matches today, and the default leaves JSON byte-identical
//! * default text `show` prints the last 2000 characters of notes preceded by
//!   an omission line naming `--notes all`
//! * the text `Notes:` section recommends `--notes all` as the escape hatch
//!   because appended history belongs in comments (R010)

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn bead(workspace: &std::path::Path) -> Command {
    let mut command = Command::cargo_bin("bead").unwrap();
    command.current_dir(workspace);
    command
}

fn setup_workspace() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    bead(temp_dir.path())
        .args(["init", "--prefix", "proj", "--skip-foreign-workspace"])
        .assert()
        .success();
    temp_dir
}

fn create_issue(workspace: &std::path::Path, title: &str) -> String {
    let output = bead(workspace)
        .args(["create", "--title", title])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(output).unwrap().trim().to_string()
}

fn show_json(workspace: &std::path::Path, args: &[&str]) -> Value {
    let output = bead(workspace)
        .args(["show"])
        .args(args)
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).unwrap()
}

#[test]
fn show_json_projection_returns_exactly_the_requested_members() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Projected");

    // claim_epoch is a conditional member: claim so it exists, as the
    // acceptance criterion reads it back from a claimed issue. Only this one
    // issue exists, so the frontier selection is deterministic.
    let claim_output = bead(workspace.path())
        .args(["claim", "--assignee", "worker-a", "--json"])
        .output()
        .unwrap();
    let claim: Value = serde_json::from_slice(&claim_output.stdout).unwrap();
    assert_eq!(claim["bead_id"], issue_id, "claim selected another issue");

    let document = show_json(
        workspace.path(),
        &[&issue_id, "--fields", "status,assignee,claim_epoch"],
    );
    let members = document.as_object().unwrap();
    let mut names: Vec<&str> = members.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(names, vec!["assignee", "claim_epoch", "status"]);
    assert_eq!(members["assignee"], "worker-a");
    assert!(members["claim_epoch"].as_i64().unwrap() >= 1);
    assert_eq!(members["status"], "in_progress");

    // Without a claim the conditional member stays absent, never null
    let unclaimed_id = create_issue(workspace.path(), "Unclaimed");
    let document = show_json(
        workspace.path(),
        &[&unclaimed_id, "--fields", "status,assignee,claim_epoch"],
    );
    let members = document.as_object().unwrap();
    assert!(members.contains_key("status"));
    assert!(!members.contains_key("claim_epoch"));
}

#[test]
fn unknown_field_exits_2_naming_the_valid_fields() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Unknown field");

    bead(workspace.path())
        .args(["show", &issue_id, "--json", "--fields", "nope"])
        .assert()
        .code(2)
        .stderr(predicates::str::contains(
            "Unknown field 'nope' in --fields",
        ))
        .stderr(predicates::str::contains("Valid fields are:"))
        // the guide-published set, and where it is published
        .stderr(predicates::str::contains("effective_status"))
        .stderr(predicates::str::contains("claim_epoch"))
        .stderr(predicates::str::contains(
            "bead schema explain urn:bead-rs:schema:field-guide:native-v1",
        ));

    // list shares the parser
    bead(workspace.path())
        .args(["list", "--json", "--fields", "also_nope"])
        .assert()
        .code(2)
        .stderr(predicates::str::contains(
            "Unknown field 'also_nope' in --fields",
        ));
}

#[test]
fn notes_projection_modes_on_show_json() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Notes modes");
    let notes = format!("{}TAILMARK", "a".repeat(3000));
    bead(workspace.path())
        .args(["update", &issue_id, "--notes", &notes])
        .assert()
        .success();

    let tail = show_json(workspace.path(), &[&issue_id, "--notes", "tail:8"]);
    assert_eq!(tail["notes"], "TAILMARK");

    let none = show_json(workspace.path(), &[&issue_id, "--notes", "none"]);
    assert!(none.get("notes").is_none(), "none must omit the member");

    let all = show_json(workspace.path(), &[&issue_id, "--notes", "all"]);
    assert_eq!(all["notes"], notes);

    // all matches today's full member; auto is the unchanged default
    let default = show_json(workspace.path(), &[&issue_id]);
    assert_eq!(default["notes"], notes);
    assert_eq!(default["notes"], all["notes"]);
}

#[test]
fn default_json_stays_byte_identical_to_notes_all() {
    let workspace = setup_workspace();
    let first = create_issue(workspace.path(), "Byte identity one");
    let second = create_issue(workspace.path(), "Byte identity two");
    let notes = format!("{}END", "n".repeat(2500));
    for issue_id in [&first, &second] {
        bead(workspace.path())
            .args(["update", issue_id, "--notes", &notes])
            .assert()
            .success();
    }

    let default_show = bead(workspace.path())
        .args(["show", &first, "--json"])
        .output()
        .unwrap();
    let all_show = bead(workspace.path())
        .args(["show", &first, "--json", "--notes", "all"])
        .output()
        .unwrap();
    assert_eq!(default_show.stdout, all_show.stdout);

    let default_list = bead(workspace.path())
        .args(["list", "--jsonl"])
        .output()
        .unwrap();
    let all_list = bead(workspace.path())
        .args(["list", "--jsonl", "--notes", "all"])
        .output()
        .unwrap();
    assert_eq!(default_list.stdout, all_list.stdout);
}

#[test]
fn default_text_show_bounds_long_notes_with_omission_line() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Bounded text");
    let notes = format!("HEADSENTINEL{}TAILMARK", "a".repeat(70_000 - 20));
    assert_eq!(notes.chars().count(), 70_000);
    bead(workspace.path())
        .args(["update", &issue_id, "--notes", &notes])
        .assert()
        .success();

    let output = bead(workspace.path())
        .args(["show", &issue_id])
        .output()
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();

    // The omission line precedes the bounded tail and names --notes all
    assert!(
        text.contains("68000 earlier characters omitted; use --notes all"),
        "missing omission line in: {}",
        &text[..600.min(text.len())]
    );

    // At most the last 2000 characters of the note, so the head is gone
    assert!(!text.contains("HEADSENTINEL"));
    assert!(text.contains("TAILMARK"));
    let section = text
        .split_once(
            "Notes: [68000 earlier characters omitted; use --notes all for the full text]\n",
        )
        .map(|(_, rest)| rest)
        .unwrap();
    let body = section.split("\nType:").next().unwrap();
    assert_eq!(body.chars().count(), 2000);

    // --notes all restores the full member in text
    let full = bead(workspace.path())
        .args(["show", &issue_id, "--notes", "all"])
        .output()
        .unwrap();
    let full_text = String::from_utf8(full.stdout).unwrap();
    assert!(full_text.contains("HEADSENTINEL"));
    assert!(full_text.contains("TAILMARK"));
    assert!(full_text.len() > text.len());

    // --notes none drops the section entirely
    let none = bead(workspace.path())
        .args(["show", &issue_id, "--notes", "none"])
        .output()
        .unwrap();
    let none_text = String::from_utf8(none.stdout).unwrap();
    assert!(!none_text.contains("Notes:"));
    assert!(!none_text.contains("TAILMARK"));
}

#[test]
fn list_applies_field_projection_per_record() {
    let workspace = setup_workspace();
    let first = create_issue(workspace.path(), "List projection one");
    let second = create_issue(workspace.path(), "List projection two");

    let output = bead(workspace.path())
        .args(["list", "--all", "--json", "--fields", "id,status"])
        .output()
        .unwrap();
    let documents: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(documents.len() >= 2);
    for document in &documents {
        let members = document.as_object().unwrap();
        let mut names: Vec<&str> = members.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, vec!["id", "status"], "per-record projection");
    }
    let ids: Vec<&str> = documents
        .iter()
        .map(|document| document["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&first.as_str()));
    assert!(ids.contains(&second.as_str()));

    // --jsonl applies the same projection per line
    let output = bead(workspace.path())
        .args(["list", "--all", "--jsonl", "--fields", "id,priority"])
        .output()
        .unwrap();
    for line in String::from_utf8(output.stdout).unwrap().lines() {
        let document: Value = serde_json::from_str(line).unwrap();
        let members = document.as_object().unwrap();
        let mut names: Vec<&str> = members.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, vec!["id", "priority"]);
    }
}

#[test]
fn text_views_with_fields_render_only_requested_members() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Text projection");

    let output = bead(workspace.path())
        .args(["show", &issue_id, "--fields", "id,status,priority"])
        .output()
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("ID: "), "{text}");
    assert!(text.contains("Status: open"), "{text}");
    assert!(text.contains("Priority: P"), "{text}");
    assert!(!text.contains("Title:"), "{text}");
    assert!(!text.contains("Created:"), "{text}");

    // list projects per record: the id stays unindented, the rest indented
    let output = bead(workspace.path())
        .args(["list", "--all", "--fields", "id,title"])
        .output()
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains(&format!("ID: {issue_id}")), "{text}");
    assert!(text.contains("  Title: Text projection"), "{text}");
}

#[test]
fn invalid_notes_mode_is_a_usage_error() {
    let workspace = setup_workspace();
    let issue_id = create_issue(workspace.path(), "Bad notes mode");

    bead(workspace.path())
        .args(["show", &issue_id, "--notes", "banana"])
        .assert()
        .code(2)
        .stderr(predicates::str::contains(
            "Notes must be one of: auto, none, all, tail:N",
        ));

    bead(workspace.path())
        .args(["show", &issue_id, "--notes", "tail:-3"])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("tail:N requires N >= 0"));
}
