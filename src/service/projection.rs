//! Output projection for the `show` and `list` documents (beadrs-0f854018).
//!
//! Two orthogonal knobs bound the size of what a read returns:
//!
//! * `--fields` selects named `cli_issue` members from each emitted document.
//!   The valid names are the field guide's `cli_issue` member list
//!   ([`crate::service::schema::CLI_ISSUE_FIELDS`]), so the CLI can never
//!   accept a name the guide does not publish.
//! * `--notes` bounds the notes member: JSON keeps the full member by default
//!   (the NEEDLE contract stays byte-stable), text views print a bounded tail
//!   preceded by an omission line, and `none`/`tail:N`/`all` apply explicitly.

use crate::error::{Error, Result};
use serde_json::Value;

/// Human-readable default bound: text views print at most the last 2000
/// characters of notes unless asked otherwise. JSON keeps the full member
/// under the default; appended history belongs in comments (R010), not in
/// notes grown past this bound.
pub const DEFAULT_NOTES_TAIL_CHARS: usize = 2000;

/// How much of the notes member a read returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotesMode {
    /// Per-medium default: JSON documents keep the full member unchanged;
    /// text views bound to [`DEFAULT_NOTES_TAIL_CHARS`] with an omission
    /// line naming `--notes all`.
    Auto,
    /// Omit notes entirely.
    None,
    /// The last N characters.
    Tail(usize),
    /// The full member, whatever its size.
    All,
}

/// Parse a `--notes` specification. Invalid values are a CLI usage error
/// (exit 2), mirroring the `--comments` projection.
pub fn parse_notes_mode(spec: &str) -> Result<NotesMode> {
    match spec {
        "auto" => Ok(NotesMode::Auto),
        "none" => Ok(NotesMode::None),
        "all" => Ok(NotesMode::All),
        other => {
            if let Some(count) = other.strip_prefix("tail:") {
                let count = count.parse::<usize>().map_err(|_| {
                    Error::cli_usage(format!(
                        "Invalid notes tail '{other}': tail:N requires N >= 0"
                    ))
                })?;
                return Ok(NotesMode::Tail(count));
            }
            Err(Error::cli_usage(
                "Notes must be one of: auto, none, all, tail:N (N is a character count >= 0)",
            ))
        }
    }
}

/// The last `count` characters of `notes`, counted in characters, not bytes.
pub fn notes_tail(notes: &str, count: usize) -> String {
    let total = notes.chars().count();
    if total <= count {
        return notes.to_string();
    }
    notes.chars().skip(total - count).collect()
}

/// Apply a parsed `--notes` mode to one emitted JSON document. `Auto` and
/// `All` leave the document untouched, so default JSON output remains
/// byte-identical to the pre-projection NEEDLE contract.
pub fn apply_notes_mode_to_document(document: &mut Value, mode: &NotesMode) {
    match mode {
        NotesMode::Auto | NotesMode::All => {}
        NotesMode::None => {
            if let Some(object) = document.as_object_mut() {
                object.remove("notes");
            }
        }
        NotesMode::Tail(count) => {
            if let Some(notes) = document.get("notes").and_then(Value::as_str) {
                let bounded = notes_tail(notes, *count);
                if let Some(object) = document.as_object_mut() {
                    object.insert("notes".to_string(), Value::String(bounded));
                }
            }
        }
    }
}

/// Select named members from one emitted document, in the requested order,
/// keeping only members the document actually carries: conditional members
/// such as `claim_epoch` stay absent until they exist, never materialize as
/// null.
pub fn project_document(document: &Value, fields: &[&'static str]) -> Value {
    let mut projected = serde_json::Map::new();
    if let Some(object) = document.as_object() {
        for field in fields {
            if let Some(value) = object.get(*field) {
                projected.insert((*field).to_string(), value.clone());
            }
        }
    }
    Value::Object(projected)
}

/// Render the human-readable Notes section: bounded per the mode, with an
/// omission line naming `--notes all` whenever characters were cut.
/// [`NotesMode::None`] renders no section at all.
pub fn notes_text_section(notes: &str, mode: &NotesMode) -> Option<String> {
    let limit = match mode {
        NotesMode::None => return None,
        NotesMode::All => usize::MAX,
        NotesMode::Auto => DEFAULT_NOTES_TAIL_CHARS,
        NotesMode::Tail(count) => *count,
    };
    let total = notes.chars().count();
    if total <= limit {
        return Some(format!("Notes: {notes}"));
    }
    let omitted = total - limit;
    let tail = notes_tail(notes, limit);
    Some(format!(
        "Notes: [{omitted} earlier characters omitted; use --notes all for the full text]\n{tail}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_notes_mode_accepts_documented_forms() {
        assert_eq!(parse_notes_mode("auto").unwrap(), NotesMode::Auto);
        assert_eq!(parse_notes_mode("none").unwrap(), NotesMode::None);
        assert_eq!(parse_notes_mode("all").unwrap(), NotesMode::All);
        assert_eq!(parse_notes_mode("tail:0").unwrap(), NotesMode::Tail(0));
        assert_eq!(parse_notes_mode("tail:42").unwrap(), NotesMode::Tail(42));
    }

    #[test]
    fn parse_notes_mode_rejects_garbage_as_usage_error() {
        for bad in ["", "tail", "tail:", "tail:-1", "tail:x", "everything"] {
            let error = parse_notes_mode(bad).unwrap_err();
            assert_eq!(error.exit_code(), 2, "mode {bad:?} must be a usage error");
        }
    }

    #[test]
    fn notes_tail_counts_characters_not_bytes() {
        // 'é' is two bytes but one character
        let notes = "aé3456789";
        assert_eq!(notes_tail(notes, 3), "789");
        assert_eq!(notes_tail(notes, 9), notes);
        assert_eq!(notes_tail(notes, 100), notes);
        assert_eq!(notes_tail(notes, 0), "");
    }

    #[test]
    fn apply_notes_mode_leaves_default_json_untouched() {
        let mut document = json!({"id": "x", "notes": "kept"});
        apply_notes_mode_to_document(&mut document, &NotesMode::Auto);
        assert_eq!(document["notes"], "kept");
        apply_notes_mode_to_document(&mut document, &NotesMode::All);
        assert_eq!(document["notes"], "kept");
    }

    #[test]
    fn apply_notes_mode_none_omits_and_tail_trims() {
        let mut document = json!({"id": "x", "notes": "abcdef"});
        apply_notes_mode_to_document(&mut document, &NotesMode::None);
        assert!(document.get("notes").is_none());

        let mut document = json!({"id": "x", "notes": "abcdef"});
        apply_notes_mode_to_document(&mut document, &NotesMode::Tail(2));
        assert_eq!(document["notes"], "ef");
    }

    #[test]
    fn project_document_keeps_requested_order_and_conditional_absence() {
        let document = json!({"id": "x", "status": "open", "notes": ""});
        let projected = project_document(&document, &["status", "assignee", "id"]);
        let members = projected.as_object().unwrap();
        let names: Vec<&String> = members.keys().collect();
        assert_eq!(names.len(), 2);
        assert!(members.contains_key("status"));
        assert!(members.contains_key("id"));
        assert!(!members.contains_key("assignee"));
    }

    #[test]
    fn notes_text_section_bounds_default_and_names_the_escape_hatch() {
        let notes = "x".repeat(70_000);
        let section = notes_text_section(&notes, &NotesMode::Auto).unwrap();
        let (header, tail) = section.split_once('\n').unwrap();
        assert!(
            header.contains("68000 earlier characters omitted"),
            "{header}"
        );
        assert!(header.contains("--notes all"), "{header}");
        assert_eq!(tail.chars().count(), DEFAULT_NOTES_TAIL_CHARS);

        let short = notes_text_section("short", &NotesMode::Auto).unwrap();
        assert_eq!(short, "Notes: short");
    }

    #[test]
    fn notes_text_section_none_and_all() {
        assert!(notes_text_section("anything", &NotesMode::None).is_none());
        let section = notes_text_section("full", &NotesMode::All).unwrap();
        assert_eq!(section, "Notes: full");
    }

    #[test]
    fn notes_text_section_explicit_tail_states_the_omission() {
        let notes = "y".repeat(30);
        let section = notes_text_section(&notes, &NotesMode::Tail(5)).unwrap();
        let (header, tail) = section.split_once('\n').unwrap();
        assert!(header.contains("25 earlier characters omitted"), "{header}");
        assert_eq!(tail, "yyyyy");
    }
}
