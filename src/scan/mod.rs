//! Offline secret scanning and mutation rejection (ADR-014, BR-T14).
//!
//! The scanner is deterministic and compiled in: a scan verdict is a pure
//! function of binary version and input text. It has exactly two tiers —
//! [`Tier::Blocking`] provider formats that reject a mutation under
//! `enforce` mode, and [`Tier::Advisory`] statistical findings that are only
//! ever reported.
//!
//! Findings never carry matched bytes. The bytes exist only inside
//! [`SecretBytes`], which redacts on `Debug`/`Display`/serialization and is
//! overwritten before release; everything that leaves this module —
//! diagnostics, fingerprints, JSON, errors — identifies a finding without
//! quoting it.

pub mod fingerprint;
pub mod rules;

pub use rules::{rule_ids, Checksum, Rule, Tier, CONTRACT_IDENTITY, RULESET_VERSION};

use rules::{keyword_anchors, ADVISORY_ENTROPY_RULE_ID};
use std::collections::BTreeSet;
use std::fmt;
use std::sync::LazyLock;

use aho_corasick::AhoCorasick;
use regex::Regex;

/// Machine reason code carried by every secret rejection (spec §4).
pub const SECRET_DETECTED: &str = "secret_detected";

/// Enforcement mode configured per workspace via `secret_scan.mode`.
///
/// There is no per-invocation blanket bypass; `off` is a workspace-level,
/// capability-visible decision (ADR-014), never a flag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Blocking findings reject the mutation (the compiled default).
    #[default]
    Enforce,
    /// Nothing rejects; findings are reported by `doctor` and dry-run.
    Advisory,
    /// The scanner does not run at all.
    Off,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Enforce => "enforce",
            Mode::Advisory => "advisory",
            Mode::Off => "off",
        }
    }

    /// Parse a `secret_scan.mode` value. Unrecognized values fail closed:
    /// the caller turns [`ScanConfigError`] into a mutation failure naming
    /// the config key (ADR-014 "fail-closed about its own configuration").
    pub fn parse(value: &str) -> Result<Self, ScanConfigError> {
        match value {
            "enforce" => Ok(Mode::Enforce),
            "advisory" => Ok(Mode::Advisory),
            "off" => Ok(Mode::Off),
            _ => Err(ScanConfigError::UnknownMode),
        }
    }
}

/// Malformed `secret_scan` workspace configuration. Fails closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanConfigError {
    UnknownMode,
    InvalidAcknowledgment { index: usize },
    WrongAcknowledgmentType,
    WrongModeType,
}

impl fmt::Display for ScanConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScanConfigError::UnknownMode => write!(
                f,
                "invalid secret_scan.mode in .beads/config.json: expected \
                 \"enforce\", \"advisory\", or \"off\"; fix the key to continue"
            ),
            ScanConfigError::WrongModeType => write!(
                f,
                "invalid secret_scan.mode in .beads/config.json: expected a string, \
                 found a different JSON type; fix the key to continue"
            ),
            ScanConfigError::WrongAcknowledgmentType => write!(
                f,
                "invalid secret_scan.acknowledged in .beads/config.json: expected an \
                 array of 64-character lowercase hex finding fingerprints; fix the key \
                 to continue"
            ),
            ScanConfigError::InvalidAcknowledgment { index } => write!(
                f,
                "invalid secret_scan.acknowledged entry at index {} in \
                 .beads/config.json: expected a 64-character lowercase hex finding \
                 fingerprint; fix the key to continue",
                index
            ),
        }
    }
}

impl std::error::Error for ScanConfigError {}

/// Workspace scan configuration: the mode plus the exact-fingerprint
/// acknowledgment list. Rules are not configurable and never appear here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanConfig {
    mode: Mode,
    acknowledged: BTreeSet<String>,
}

impl ScanConfig {
    /// The compiled default: `enforce`, no acknowledgments.
    pub fn enforce() -> Self {
        Self::default()
    }

    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            acknowledged: BTreeSet::new(),
        }
    }

    /// Build from parsed `secret_scan` workspace configuration values.
    ///
    /// `mode` is the raw JSON value of `secret_scan.mode` (`None` = the
    /// compiled default `enforce`); `acknowledged` is the raw JSON value of
    /// `secret_scan.acknowledged` (`None` = empty). Any malformed shape
    /// fails closed with an error naming the config key.
    pub fn from_config_values(
        mode: Option<&serde_json::Value>,
        acknowledged: Option<&serde_json::Value>,
    ) -> Result<Self, ScanConfigError> {
        let mode = match mode {
            None => Mode::Enforce,
            Some(serde_json::Value::String(s)) => Mode::parse(s)?,
            Some(_) => return Err(ScanConfigError::WrongModeType),
        };
        let mut list = BTreeSet::new();
        match acknowledged {
            None => {}
            Some(serde_json::Value::Array(items)) => {
                for (index, item) in items.iter().enumerate() {
                    let fingerprint = item
                        .as_str()
                        .ok_or(ScanConfigError::InvalidAcknowledgment { index })?;
                    if !is_fingerprint_shape(fingerprint) {
                        return Err(ScanConfigError::InvalidAcknowledgment { index });
                    }
                    list.insert(fingerprint.to_string());
                }
            }
            Some(_) => return Err(ScanConfigError::WrongAcknowledgmentType),
        }
        Ok(Self {
            mode,
            acknowledged: list,
        })
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The acknowledged fingerprints, in sorted order (diagnostics only —
    /// these are fingerprints, never values).
    pub fn acknowledged(&self) -> impl Iterator<Item = &str> {
        self.acknowledged.iter().map(String::as_str)
    }

    fn is_acknowledged(&self, fingerprint: &str) -> bool {
        self.acknowledged.contains(fingerprint)
    }
}

fn is_fingerprint_shape(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Matched bytes, held only for as long as the fingerprint takes.
///
/// Every rendering is redacted and `Drop` overwrites the buffer. Under the
/// crate-wide `#![forbid(unsafe_code)]` the overwrite cannot use volatile
/// writes, so it is followed by a `black_box` barrier that the optimizer
/// must treat as observable — the buffer is provably written, even if a
/// future optimizer is permitted to elide a provably-dead store.
pub(crate) struct SecretBytes(Box<[u8]>);

impl SecretBytes {
    fn from_str(value: &str) -> Self {
        Self(Box::from(value.as_bytes()))
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.0.len()
    }

    fn with_bytes<R>(&self, use_bytes: impl FnOnce(&[u8]) -> R) -> R {
        use_bytes(&self.0)
    }

    fn overwrite(&mut self) {
        for byte in self.0.iter_mut() {
            *byte = 0;
        }
        std::hint::black_box(&self.0);
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        self.overwrite();
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[redacted: {} bytes]", self.0.len())
    }
}

impl fmt::Display for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[redacted: {} bytes]", self.0.len())
    }
}

/// Why a blocking-tier candidate did not block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Disposition {
    /// The candidate carries no lookalike marker: it blocks.
    #[default]
    Confirmed,
    /// Placeholder-shaped value (spec §3): reported, never blocking.
    Placeholder,
    /// The format's embedded checksum failed: a lookalike, advisory only.
    ChecksumFailed,
}

impl Disposition {
    pub fn as_str(self) -> &'static str {
        match self {
            Disposition::Confirmed => "confirmed",
            Disposition::Placeholder => "placeholder",
            Disposition::ChecksumFailed => "checksum_failed",
        }
    }
}

/// A redacted finding (spec §2): identity and location only, never bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub ruleset_version: u32,
    pub rule_id: String,
    pub provider: String,
    pub tier: Tier,
    pub disposition: Disposition,
    /// Semantic record selector, e.g. `issue:beadrs-abc` or
    /// `checkpoint:current`.
    pub selector: String,
    /// Field path within the record, e.g. `description` or `comment.body`.
    pub field_path: String,
    /// Byte range within the field's UTF-8 bytes.
    pub start: usize,
    pub end: usize,
    /// Lowercase-hex SHA-256 fingerprint over the domain separator, ruleset
    /// version, rule id, selector, field path, byte range, and matched
    /// bytes. Identifies the finding exactly without exposing it.
    pub fingerprint: String,
}

impl Finding {
    /// Whether this finding rejects a mutation under the given mode.
    fn is_rejecting(&self) -> bool {
        self.tier == Tier::Blocking && self.disposition == Disposition::Confirmed
    }

    /// One-line redacted diagnostic naming rule, field, selector, range, and
    /// fingerprint (ADR-007 shape, used by rejections, doctor, and dry-run).
    pub fn diagnostic(&self) -> String {
        let tier = match self.tier {
            Tier::Blocking => "blocking",
            Tier::Advisory => "advisory",
        };
        format!(
            "{} rule {} ({}) in {} field {} bytes {}..{} fingerprint {}",
            tier,
            self.rule_id,
            self.provider,
            self.selector,
            self.field_path,
            self.start,
            self.end,
            self.fingerprint
        )
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.diagnostic())
    }
}

impl serde::Serialize for Finding {
    /// Hand-written so no future field can quietly add matched bytes to the
    /// serialization surface.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Finding", 10)?;
        state.serialize_field("ruleset_version", &self.ruleset_version)?;
        state.serialize_field("rule_id", &self.rule_id)?;
        state.serialize_field("provider", &self.provider)?;
        state.serialize_field(
            "tier",
            match self.tier {
                Tier::Blocking => "blocking",
                Tier::Advisory => "advisory",
            },
        )?;
        state.serialize_field("disposition", self.disposition.as_str())?;
        state.serialize_field("selector", &self.selector)?;
        state.serialize_field("field_path", &self.field_path)?;
        state.serialize_field("start", &self.start)?;
        state.serialize_field("end", &self.end)?;
        state.serialize_field("fingerprint", &self.fingerprint)?;
        state.end()
    }
}

/// One operator-supplied text field of a canonical mutation request.
pub struct Field<'a> {
    /// Field path reported in findings, e.g. `"notes"`.
    pub path: &'a str,
    /// The operator-supplied text.
    pub text: &'a str,
}

impl<'a> Field<'a> {
    pub fn new(path: &'a str, text: &'a str) -> Self {
        Self { path, text }
    }
}

/// Outcome of scanning a complete canonical request.
#[derive(Debug, Default)]
pub struct ScanReport {
    /// Every finding in deterministic (field path, byte start, rule id)
    /// order — including acknowledged and downgraded ones.
    pub findings: Vec<Finding>,
    /// Findings admitted only because their exact fingerprint is
    /// acknowledged. Every mutation that commits while one of these is
    /// present must record the accompanying audit event.
    pub acknowledged: Vec<Finding>,
    /// Unacknowledged blocking findings. Non-empty under `enforce` mode
    /// means the request must be rejected before any transaction opens.
    pub blocking: Vec<Finding>,
}

impl ScanReport {
    /// True when the request is clean of every finding.
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// True when no finding blocks this request in its mode.
    pub fn is_admitted(&self) -> bool {
        self.blocking.is_empty()
    }
}

/// A rejection: the unacknowledged blocking finding plus the full operator
/// message (ADR-007: names rule, field, fingerprint, the acknowledgment
/// mechanism, and the rotate-and-reference remedy — never the value).
#[derive(Clone, Debug)]
pub struct Rejection {
    pub finding: Finding,
    pub message: String,
}

impl Rejection {
    fn build(finding: Finding) -> Self {
        let fingerprint = finding.fingerprint.clone();
        let field = finding.field_path.clone();
        let diagnostic = finding.diagnostic();
        let message = format!(
            "{SECRET_DETECTED}: {diagnostic}. Matched bytes are never shown.\n\
             The value reached this command line, so treat it as exposed: rotate it, \
             then store the reference (a vault path or retrieval command), not the \
             credential.\n\
             To admit this exact finding only, retry with \
             --acknowledge-secret {fingerprint} (field {field}), or add that \
             fingerprint to secret_scan.acknowledged in .beads/config.json. There is \
             no blanket bypass flag."
        );
        Self { finding, message }
    }
}

/// Keyword prefilter shared by every scan: one case-insensitive pass maps
/// keyword anchors to the rules that may match, so a field with no anchor
/// pays no regex cost at all.
static PREFILTER: LazyLock<(AhoCorasick, Vec<Vec<usize>>)> = LazyLock::new(|| {
    let anchors = keyword_anchors();
    let patterns: Vec<&str> = anchors.iter().map(|(a, _)| a.as_str()).collect();
    let automaton = AhoCorasick::builder()
        .ascii_case_insensitive(true)
        .build(&patterns)
        .expect("keyword anchors are static and valid");
    let mapping: Vec<Vec<usize>> = anchors.into_iter().map(|(_, rules)| rules).collect();
    (automaton, mapping)
});

/// The rule table with its patterns compiled once. Compilation is total:
/// every pattern is a build-time fixture, and a pattern that did not compile
/// is a release-blocking bug rather than a runtime failure mode.
static COMPILED_RULES: LazyLock<Vec<(&'static Rule, Regex)>> = LazyLock::new(|| {
    rules::RULES
        .iter()
        .map(|rule| {
            let regex = Regex::new(rule.pattern)
                .unwrap_or_else(|err| panic!("rule {} pattern failed to compile: {err}", rule.id));
            (rule, regex)
        })
        .collect()
});

/// Scan one field's text for every rule the prefilter gates, plus the
/// statistical advisory scanner.
pub(crate) fn scan_field(selector: &str, field: &Field<'_>) -> Vec<Finding> {
    if field.text.is_empty() {
        return Vec::new();
    }
    let (automaton, mapping) = &*PREFILTER;
    let mut candidate_rules = BTreeSet::new();
    for hit in automaton.find_iter(field.text) {
        if let Some(rules) = mapping.get(hit.pattern().as_usize()) {
            candidate_rules.extend(rules.iter().copied());
        }
    }

    let mut findings = Vec::new();
    for rule_index in candidate_rules {
        let (rule, regex) = &COMPILED_RULES[rule_index];
        for capture in regex.captures_iter(field.text) {
            let whole = capture.get(0).expect("pattern has a group 0");
            let body = capture.get(1).unwrap_or(whole);
            let mut matched = SecretBytes::from_str(whole.as_str());
            let disposition = match rule.tier {
                Tier::Advisory => Disposition::Confirmed,
                Tier::Blocking => {
                    if is_placeholder(body.as_str()) {
                        Disposition::Placeholder
                    } else if let Some(checksum) = rule.checksum {
                        if rules::checksum_valid(checksum, body.as_str()) {
                            Disposition::Confirmed
                        } else {
                            Disposition::ChecksumFailed
                        }
                    } else {
                        Disposition::Confirmed
                    }
                }
            };
            let fingerprint = matched.with_bytes(|bytes| {
                fingerprint::compute(
                    RULESET_VERSION,
                    rule.id,
                    selector,
                    field.path,
                    whole.start(),
                    whole.end(),
                    bytes,
                )
            });
            matched.overwrite();
            drop(matched);
            findings.push(Finding {
                ruleset_version: RULESET_VERSION,
                rule_id: rule.id.to_string(),
                provider: rule.provider.to_string(),
                tier: rule.tier,
                disposition,
                selector: selector.to_string(),
                field_path: field.path.to_string(),
                start: whole.start(),
                end: whole.end(),
                fingerprint,
            });
        }
    }

    findings.extend(scan_entropy(selector, field));
    findings
}

/// Placeholder heuristics (spec §3): placeholder-shaped values are not
/// blocking. All-one-character bodies and example/replace/your-here markers
/// pass through as lookalikes.
fn is_placeholder(body: &str) -> bool {
    let mut bytes = body.bytes();
    if let Some(first) = bytes.next() {
        if bytes.all(|b| b == first) {
            return true;
        }
    }
    const MARKERS: [&str; 9] = [
        "example",
        "placeholder",
        "replace",
        "your_",
        "_here",
        "dummy",
        "sample",
        "xxxx",
        "0000000",
    ];
    let lowered = body.to_ascii_lowercase();
    MARKERS.iter().any(|marker| lowered.contains(marker))
}

/// Statistical advisory scan: high-entropy base64/base62 runs that no
/// provider rule covers. Advisory only — this class never rejects, because
/// bead text is dense with hash-shaped strings (ADR-014).
fn scan_entropy(selector: &str, field: &Field<'_>) -> Vec<Finding> {
    const MIN_RUN: usize = 25;
    const MIN_ENTROPY_BITS: f64 = 4.3;
    let text = field.text.as_bytes();
    let mut findings = Vec::new();
    let mut run_start: Option<usize> = None;
    let flush = |start: usize, end: usize, findings: &mut Vec<Finding>| {
        if end - start < MIN_RUN {
            return;
        }
        let mut counts = [0u32; 256];
        for byte in &text[start..end] {
            counts[*byte as usize] += 1;
        }
        let len = (end - start) as f64;
        let entropy: f64 = counts
            .iter()
            .filter(|c| **c > 0)
            .map(|c| {
                let p = *c as f64 / len;
                -p * p.log2()
            })
            .sum();
        if entropy < MIN_ENTROPY_BITS {
            return;
        }
        let mut matched = SecretBytes(Box::from(&text[start..end]));
        let fingerprint = matched.with_bytes(|bytes| {
            fingerprint::compute(
                RULESET_VERSION,
                ADVISORY_ENTROPY_RULE_ID,
                selector,
                field.path,
                start,
                end,
                bytes,
            )
        });
        matched.overwrite();
        drop(matched);
        findings.push(Finding {
            ruleset_version: RULESET_VERSION,
            rule_id: ADVISORY_ENTROPY_RULE_ID.to_string(),
            provider: "heuristic".to_string(),
            tier: Tier::Advisory,
            disposition: Disposition::Confirmed,
            selector: selector.to_string(),
            field_path: field.path.to_string(),
            start,
            end,
            fingerprint,
        });
    };
    for (index, byte) in text.iter().enumerate() {
        let candidate = byte.is_ascii_alphanumeric()
            || matches!(byte, b'+' | b'/' | b'=' | b'_' | b'.' | b'~' | b'-');
        match (candidate, run_start) {
            (true, None) => run_start = Some(index),
            (false, Some(start)) => {
                flush(start, index, &mut findings);
                run_start = None;
            }
            _ => {}
        }
    }
    if let Some(start) = run_start {
        flush(start, text.len(), &mut findings);
    }
    findings
}

/// Scan a complete canonical request (spec §4: the whole request, before the
/// semantic write transaction opens) and classify the findings against the
/// workspace mode and acknowledgment list.
///
/// `off` mode does not scan at all: the report is empty and nothing rejects.
pub fn scan(config: &ScanConfig, selector: &str, fields: &[Field<'_>]) -> ScanReport {
    let mut report = ScanReport::default();
    if config.mode == Mode::Off {
        return report;
    }
    for field in fields {
        for mut finding in scan_field(selector, field) {
            if finding.is_rejecting() && config.is_acknowledged(&finding.fingerprint) {
                finding.tier = Tier::Advisory;
                report.acknowledged.push(finding.clone());
            }
            report.findings.push(finding);
        }
    }
    // Deterministic selector order (spec §5): field path, byte start, rule id.
    report.findings.sort_by(|a, b| {
        (&a.field_path, a.start, &a.rule_id).cmp(&(&b.field_path, b.start, &b.rule_id))
    });
    report.acknowledged.sort_by(|a, b| {
        (&a.field_path, a.start, &a.rule_id).cmp(&(&b.field_path, b.start, &b.rule_id))
    });
    report.blocking = report
        .findings
        .iter()
        .filter(|f| f.is_rejecting())
        .cloned()
        .collect();
    report
}

/// Reject a request before any transaction opens, returning the rejection
/// that the CLI boundary renders at exit 2 (ADR-014).
///
/// `advisory` and `off` modes never reject.
pub fn reject_if_blocked(config: &ScanConfig, report: &ScanReport) -> Option<Rejection> {
    if config.mode != Mode::Enforce {
        return None;
    }
    report.blocking.first().cloned().map(Rejection::build)
}

#[cfg(test)]
mod tests;
