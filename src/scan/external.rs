//! Organization secret-scanner parity (beadrs-1c110ec3,
//! `research/specs/org-scanner-parity-v1.md`).
//!
//! The fleet's Git hooks and Forgejo gate run an organization scanner
//! (`secret-scanner`) over checkpoint files. When it blocks a commit on bead
//! text that bead-rs's own rules do not flag, `bead redact` had nothing to
//! target. Instead of a second copy of those rules, bead-rs asks the scanner
//! itself where each finding sits: the scanner's `--serve` mode answers a
//! length-framed text with value-free byte spans, and those spans become
//! ordinary findings. They flow through the same write gate, doctor
//! inventory, acknowledgment, tombstone and atomic redaction paths as native
//! findings.
//!
//! Opt-in: set `BEAD_ORG_SECRET_SCANNER` to the scanner's path. Unset, empty
//! or `off` disables it, so hermetic builds and tests never depend on a
//! host binary. Findings use their own ruleset version and an
//! `org-scanner:` rule-ID prefix, so their fingerprints never collide with
//! native ones and stored fingerprints keep resolving after either ruleset
//! changes.
//!
//! Each field is scanned in two views: its raw text, and the JSON-escaped
//! text exactly as a checkpoint line carries it (which is what the Git-side
//! scanner reads), with every escaped byte mapped back to the raw range it
//! came from. A scanner failure disables the scanner for the rest of the
//! process with one warning; it never makes a mutation fail, and the
//! Git-side gate still scans what is committed.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

use super::{fingerprint, Disposition, Field, Finding, Tier};

/// Ruleset version stamped on organization-scanner findings. Disjoint from
/// the native ruleset's numbering, so the two can never alias.
pub const ORG_SCANNER_RULESET_VERSION: u32 = 1000;
/// Prefix of every organization-scanner rule ID.
pub const ORG_SCANNER_RULE_PREFIX: &str = "org-scanner:";
/// Environment variable naming the scanner executable.
pub const ORG_SCANNER_ENV: &str = "BEAD_ORG_SECRET_SCANNER";
/// The scanner's own `--serve` document cap.
const MAX_DOCUMENT: usize = 64 * 1024 * 1024;

struct Service {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

enum State {
    Unstarted,
    Running(Service),
    Disabled,
}

static SERVICE: Mutex<State> = Mutex::new(State::Unstarted);

/// The configured scanner path, or `None` when organization parity is off.
pub fn configured_scanner() -> Option<PathBuf> {
    let value = std::env::var_os(ORG_SCANNER_ENV)?;
    if value.is_empty() || value == "off" {
        return None;
    }
    Some(PathBuf::from(value))
}

/// Whether organization-scanner parity is active for this process.
pub fn enabled() -> bool {
    configured_scanner().is_some()
        && !matches!(
            *SERVICE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            State::Disabled
        )
}

/// One value-free span reported by the scanner.
#[derive(Debug, PartialEq, Eq)]
struct Span {
    rule: String,
    start: usize,
    end: usize,
}

/// Findings the organization scanner reports for one field, in raw-text
/// coordinates. Empty when parity is off or the scanner is unavailable.
pub(crate) fn scan(selector: &str, field: &Field<'_>) -> Vec<Finding> {
    if field.text.is_empty() || configured_scanner().is_none() {
        return Vec::new();
    }
    let mut ranges: Vec<(String, usize, usize)> = Vec::new();

    if !field.text.as_bytes().contains(&0) {
        match request(field.text.as_bytes()) {
            Some(spans) => ranges.extend(
                spans
                    .into_iter()
                    .map(|span| (span.rule, span.start, span.end)),
            ),
            None => return Vec::new(),
        }
    }

    let (escaped, map) = json_escaped_view(field.text);
    match request(escaped.as_bytes()) {
        Some(spans) => {
            for span in spans {
                if span.start >= span.end || span.end > map.len() {
                    continue;
                }
                let start = map[span.start].0;
                let end = map[span.end - 1].1;
                ranges.push((span.rule, start, end));
            }
        }
        None => return Vec::new(),
    }

    ranges.sort();
    ranges.dedup();
    ranges
        .into_iter()
        .filter(|(_, start, end)| start < end && *end <= field.text.len())
        .map(|(rule, start, end)| {
            let rule_id = format!("{ORG_SCANNER_RULE_PREFIX}{rule}");
            let fingerprint = fingerprint::compute(
                ORG_SCANNER_RULESET_VERSION,
                &rule_id,
                selector,
                field.path,
                start,
                end,
                &field.text.as_bytes()[start..end],
            );
            Finding {
                ruleset_version: ORG_SCANNER_RULESET_VERSION,
                rule_id,
                provider: "org-secret-scanner".to_string(),
                tier: Tier::Blocking,
                disposition: Disposition::Confirmed,
                selector: selector.to_string(),
                field_path: field.path.to_string(),
                start,
                end,
                fingerprint,
            }
        })
        .collect()
}

/// The text as a JSON string body (no surrounding quotes), escaped the way
/// `serde_json` writes checkpoint records, with each output byte mapped to
/// the raw byte range of the character it encodes.
fn json_escaped_view(text: &str) -> (String, Vec<(usize, usize)>) {
    let mut escaped = String::with_capacity(text.len() + 16);
    let mut map = Vec::with_capacity(text.len() + 16);
    for (offset, character) in text.char_indices() {
        let range = (offset, offset + character.len_utf8());
        let before = escaped.len();
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0c}' => escaped.push_str("\\f"),
            control if (control as u32) < 0x20 => {
                escaped.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => escaped.push(other),
        }
        map.extend(std::iter::repeat_n(range, escaped.len() - before));
    }
    (escaped, map)
}

/// Send one document to the long-lived scanner and parse its span line.
/// `None` means the scanner is unavailable; it is then disabled for the rest
/// of the process.
fn request(document: &[u8]) -> Option<Vec<Span>> {
    if document.len() > MAX_DOCUMENT {
        return Some(Vec::new());
    }
    let mut state = SERVICE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if matches!(*state, State::Unstarted) {
        *state = match start() {
            Ok(service) => State::Running(service),
            Err(reason) => disable(&reason),
        };
    }
    let State::Running(service) = &mut *state else {
        return None;
    };
    match exchange(service, document) {
        Ok(spans) => Some(spans),
        Err(reason) => {
            *state = disable(&reason);
            None
        }
    }
}

fn disable(reason: &str) -> State {
    eprintln!(
        "bead: organization secret scanner disabled for this command ({reason}); \
         native rules still apply and the Git-side gate still scans commits"
    );
    State::Disabled
}

fn start() -> Result<Service, String> {
    let path = configured_scanner().ok_or_else(|| "not configured".to_string())?;
    let mut child = Command::new(&path)
        .arg("--serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("could not start {}: {}", path.display(), error.kind()))?;
    let stdin = child.stdin.take().ok_or("scanner stdin unavailable")?;
    let stdout = BufReader::new(child.stdout.take().ok_or("scanner stdout unavailable")?);
    Ok(Service {
        child,
        stdin,
        stdout,
    })
}

fn exchange(service: &mut Service, document: &[u8]) -> Result<Vec<Span>, String> {
    let length = u32::try_from(document.len()).map_err(|_| "document too large".to_string())?;
    service
        .stdin
        .write_all(&length.to_be_bytes())
        .and_then(|()| service.stdin.write_all(document))
        .and_then(|()| service.stdin.flush())
        .map_err(|error| format!("write failed: {}", error.kind()))?;
    let mut line = String::new();
    let read = (&mut service.stdout)
        .take(16 * 1024 * 1024)
        .read_line(&mut line)
        .map_err(|error| format!("read failed: {}", error.kind()))?;
    if read == 0 {
        return Err("scanner exited".to_string());
    }
    parse_response(line.trim())
}

/// Parse a `--serve` response: a JSON array of `{line, rule, start, end}` or
/// a `{"skipped": …}` object (binary or oversized input carries no spans).
fn parse_response(line: &str) -> Result<Vec<Span>, String> {
    let value: serde_json::Value =
        serde_json::from_str(line).map_err(|_| "unreadable scanner response".to_string())?;
    if value.get("skipped").is_some() {
        return Ok(Vec::new());
    }
    let entries = value
        .as_array()
        .ok_or_else(|| "unexpected scanner response".to_string())?;
    entries
        .iter()
        .map(|entry| {
            let rule = entry
                .get("rule")
                .and_then(serde_json::Value::as_str)
                .filter(|rule| {
                    !rule.is_empty()
                        && rule.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
                        })
                })
                .ok_or_else(|| "scanner span without a valid rule".to_string())?;
            let number = |name: &str| {
                entry
                    .get(name)
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| format!("scanner span without {name}"))
            };
            Ok(Span {
                rule: rule.to_string(),
                start: number("start")?,
                end: number("end")?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaped_view_maps_every_byte_back_to_its_raw_character() {
        let text = "a\"b\\c\nd\u{1}é";
        let (escaped, map) = json_escaped_view(text);
        assert_eq!(
            escaped,
            serde_json::to_string(text).unwrap().trim_matches('"')
        );
        assert_eq!(map.len(), escaped.len());
        // "\n" occupies two escaped bytes, both mapping to the one raw byte.
        let newline = text.find('\n').unwrap();
        let escaped_newline = escaped.find("\\n").unwrap();
        assert_eq!(map[escaped_newline], (newline, newline + 1));
        assert_eq!(map[escaped_newline + 1], (newline, newline + 1));
        // A multi-byte character maps to its whole raw range.
        let accent = text.find('é').unwrap();
        assert_eq!(*map.last().unwrap(), (accent, accent + 2));
    }

    #[test]
    fn responses_parse_spans_and_skips_and_reject_malformed_rules() {
        assert_eq!(
            parse_response(r#"[{"line":1,"rule":"generic-api-key","start":3,"end":9}]"#).unwrap(),
            vec![Span {
                rule: "generic-api-key".to_string(),
                start: 3,
                end: 9
            }]
        );
        assert!(parse_response(r#"{"skipped":"binary"}"#)
            .unwrap()
            .is_empty());
        assert!(parse_response("[]").unwrap().is_empty());
        assert!(parse_response(r#"[{"rule":"bad rule","start":1,"end":2}]"#).is_err());
        assert!(parse_response("not json").is_err());
    }
}
