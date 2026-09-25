//! Agent-guided rehydration reconciliation reports (ADR-002).
//!
//! When work moves from another tracker into `bead-rs`, an agent rehydrates
//! it by creating native beads exclusively through public `bead` commands.
//! The agent then emits a reconciliation report: a reviewable JSONL document
//! that accounts for every source identifier with exactly one disposition
//! (`native`, `omitted`, `merged`, or `unresolved`). The report is a review
//! artifact only — it is never accepted as native checkpoint input.
//! The normative format lives in `research/specs/reconciliation-report-v1.md`.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

/// Immutable schema identity carried by every record of a reconciliation
/// report. This identifier is deliberately outside the native store schema
/// family: no native store input path accepts it.
pub const SCHEMA_REF: &str = "urn:bead-rs:schema:reconciliation-report:v1";

/// One reviewable outcome for one source identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Disposition {
    /// A native bead was created for this source identifier.
    Native,
    /// The source identifier was deliberately not migrated.
    Omitted,
    /// The source identifier was absorbed into another entry's native bead.
    Merged,
    /// The agent could not decide; the entry awaits human review.
    Unresolved,
}

impl Disposition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Disposition::Native => "native",
            Disposition::Omitted => "omitted",
            Disposition::Merged => "merged",
            Disposition::Unresolved => "unresolved",
        }
    }
}

/// Per-disposition totals declared by the header record. The declared
/// counts must equal the tallies of the entry records exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispositionCounts {
    pub total: u64,
    pub native: u64,
    pub omitted: u64,
    pub merged: u64,
    pub unresolved: u64,
}

/// The required first record of a report. It identifies the source
/// repository and commit the export was taken from, plus the agent and
/// destination workspace, so a reviewer can reproduce and audit the run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportHeader {
    pub record_type: String,
    pub schema_ref: String,
    pub source_repository: String,
    pub source_commit: String,
    pub source_tracker: String,
    pub generated_at: String,
    pub generator: String,
    pub destination_workspace: String,
    pub counts: DispositionCounts,
    /// Unknown fields are preserved so source context survives round trips.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

/// One entry record: one source identifier and its single disposition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportEntry {
    pub record_type: String,
    pub schema_ref: String,
    /// The identifier in the source tracker, unique within the report.
    pub source_id: String,
    pub disposition: Disposition,
    /// The native bead minted for this source identifier. Required exactly
    /// when the disposition is `native`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_bead: Option<String>,
    /// The `source_id` of the `native` entry that absorbed this item.
    /// Required exactly when the disposition is `merged`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_into: Option<String>,
    /// Why the disposition was chosen. Required for every disposition
    /// except `native`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    /// Optional human context copied from the source for review.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_title: Option<String>,
    /// Unknown fields are preserved so source context survives round trips.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

/// A parsed, fully validated reconciliation report.
#[derive(Debug, Clone, PartialEq)]
pub struct ReconciliationReport {
    pub header: ReportHeader,
    pub entries: Vec<ReportEntry>,
}

impl ReconciliationReport {
    /// True when no entry awaits human review. A runbook must not treat an
    /// unclean report as finished.
    pub fn is_clean(&self) -> bool {
        !self
            .entries
            .iter()
            .any(|entry| entry.disposition == Disposition::Unresolved)
    }

    /// Tally the entries by disposition.
    pub fn disposition_counts(&self) -> DispositionCounts {
        let mut counts = DispositionCounts {
            total: self.entries.len() as u64,
            native: 0,
            omitted: 0,
            merged: 0,
            unresolved: 0,
        };
        for entry in &self.entries {
            match entry.disposition {
                Disposition::Native => counts.native += 1,
                Disposition::Omitted => counts.omitted += 1,
                Disposition::Merged => counts.merged += 1,
                Disposition::Unresolved => counts.unresolved += 1,
            }
        }
        counts
    }

    /// Enforce every format invariant: header completeness, disposition
    /// field rules, merge references, uniqueness, and count agreement.
    pub fn validate(&self) -> Result<()> {
        self.validate_header()?;
        let tallies = self.validate_entries()?;
        self.validate_counts(tallies)
    }

    fn validate_header(&self) -> Result<()> {
        let header = &self.header;
        if header.record_type != "header" {
            return Err(Error::validation(format!(
                "reconciliation report header: record_type must be \"header\", got {:?}",
                header.record_type
            )));
        }
        if header.schema_ref != SCHEMA_REF {
            return Err(Error::validation(format!(
                "reconciliation report header: schema_ref must be {SCHEMA_REF}, got {:?}",
                header.schema_ref
            )));
        }
        for field in [
            "source_repository",
            "source_commit",
            "source_tracker",
            "destination_workspace",
            "generator",
        ] {
            let value = match field {
                "source_repository" => &header.source_repository,
                "source_commit" => &header.source_commit,
                "source_tracker" => &header.source_tracker,
                "destination_workspace" => &header.destination_workspace,
                _ => &header.generator,
            };
            if value.trim().is_empty() {
                return Err(Error::validation(format!(
                    "reconciliation report header: {field} must be a nonempty string"
                )));
            }
        }
        if time::OffsetDateTime::parse(
            &header.generated_at,
            &time::format_description::well_known::Rfc3339,
        )
        .is_err()
        {
            return Err(Error::validation(
                "reconciliation report header: generated_at must be an RFC 3339 timestamp",
            ));
        }
        Ok(())
    }

    /// Validate every entry and return the observed disposition tallies.
    fn validate_entries(&self) -> Result<DispositionCounts> {
        let mut seen_source_ids: HashSet<&str> = HashSet::new();
        let mut seen_target_beads: HashSet<&str> = HashSet::new();
        let mut tallies = DispositionCounts {
            total: self.entries.len() as u64,
            native: 0,
            omitted: 0,
            merged: 0,
            unresolved: 0,
        };

        for entry in &self.entries {
            let label = format!(
                "reconciliation report entry {:?}",
                if entry.source_id.is_empty() {
                    "<unnamed>"
                } else {
                    &entry.source_id
                }
            );
            if entry.record_type != "entry" {
                return Err(Error::validation(format!(
                    "{label}: record_type must be \"entry\", got {:?}",
                    entry.record_type
                )));
            }
            if entry.schema_ref != SCHEMA_REF {
                return Err(Error::validation(format!(
                    "{label}: schema_ref must be {SCHEMA_REF}, got {:?}",
                    entry.schema_ref
                )));
            }
            if entry.source_id.trim().is_empty() || entry.source_id.trim() != entry.source_id {
                return Err(Error::validation(format!(
                    "{label}: source_id must be a nonempty string without surrounding whitespace"
                )));
            }
            if !seen_source_ids.insert(entry.source_id.as_str()) {
                return Err(Error::validation(format!(
                    "{label}: duplicate source_id; every source identifier appears exactly once"
                )));
            }

            match entry.disposition {
                Disposition::Native => {
                    let target = entry.target_bead.as_deref().ok_or_else(|| {
                        Error::validation(format!(
                            "{label}: disposition \"native\" requires target_bead"
                        ))
                    })?;
                    crate::model::validate_issue_id(target).map_err(|error| {
                        Error::validation(format!(
                            "{label}: target_bead is not a valid native issue ID: {error}"
                        ))
                    })?;
                    if !seen_target_beads.insert(target) {
                        return Err(Error::validation(format!(
                            "{label}: target_bead {target} is already claimed by another \
                             native entry; one native bead accounts for one source identifier"
                        )));
                    }
                    if entry.merged_into.is_some() {
                        return Err(Error::validation(format!(
                            "{label}: disposition \"native\" must not carry merged_into"
                        )));
                    }
                    tallies.native += 1;
                }
                Disposition::Merged => {
                    let merged_into = entry.merged_into.as_deref().ok_or_else(|| {
                        Error::validation(format!(
                            "{label}: disposition \"merged\" requires merged_into"
                        ))
                    })?;
                    if entry.target_bead.is_some() {
                        return Err(Error::validation(format!(
                            "{label}: disposition \"merged\" must not carry target_bead"
                        )));
                    }
                    let absorbing = self
                        .entries
                        .iter()
                        .find(|candidate| candidate.source_id == merged_into)
                        .ok_or_else(|| {
                            Error::validation(format!(
                                "{label}: merged_into references unknown source_id \
                                 {merged_into:?}; the reference must name an entry in this report"
                            ))
                        })?;
                    if absorbing.disposition != Disposition::Native {
                        return Err(Error::validation(format!(
                            "{label}: merged_into references {merged_into:?} with disposition \
                             {:?}; merges must resolve to a \"native\" entry in one hop",
                            absorbing.disposition.as_str()
                        )));
                    }
                    require_rationale(&label, entry)?;
                    tallies.merged += 1;
                }
                Disposition::Omitted => {
                    require_no_merge_fields(&label, entry)?;
                    require_rationale(&label, entry)?;
                    tallies.omitted += 1;
                }
                Disposition::Unresolved => {
                    require_no_merge_fields(&label, entry)?;
                    require_rationale(&label, entry)?;
                    tallies.unresolved += 1;
                }
            }
        }

        Ok(tallies)
    }

    fn validate_counts(&self, tallies: DispositionCounts) -> Result<()> {
        let declared = self.header.counts;
        let observed = [
            ("total", tallies.total, declared.total),
            ("native", tallies.native, declared.native),
            ("omitted", tallies.omitted, declared.omitted),
            ("merged", tallies.merged, declared.merged),
            ("unresolved", tallies.unresolved, declared.unresolved),
        ];
        for (field, expected, actual) in observed {
            if expected != actual {
                return Err(Error::validation(format!(
                    "reconciliation report header: counts.{field} declares {actual} but the \
                     entries tally {expected}"
                )));
            }
        }
        Ok(())
    }
}

fn require_rationale(label: &str, entry: &ReportEntry) -> Result<()> {
    match entry.rationale.as_deref() {
        Some(rationale) if !rationale.trim().is_empty() => Ok(()),
        _ => Err(Error::validation(format!(
            "{label}: disposition {:?} requires a nonempty rationale",
            entry.disposition.as_str()
        ))),
    }
}

fn require_no_merge_fields(label: &str, entry: &ReportEntry) -> Result<()> {
    if entry.target_bead.is_some() || entry.merged_into.is_some() {
        return Err(Error::validation(format!(
            "{label}: disposition {:?} must not carry target_bead or merged_into",
            entry.disposition.as_str()
        )));
    }
    Ok(())
}

/// Parse a reconciliation report from its JSONL text and fully validate it.
///
/// The first non-blank line must be the header record; every following
/// non-blank line must be an entry record. Blank lines are ignored. Every
/// format invariant is enforced before the report is returned, so a
/// successfully returned report is review-ready.
pub fn parse_report(input: &str) -> Result<ReconciliationReport> {
    let mut header: Option<ReportHeader> = None;
    let mut entries: Vec<ReportEntry> = Vec::new();

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        if line.trim().is_empty() {
            continue;
        }
        let record: serde_json::Value = serde_json::from_str(line).map_err(|error| {
            Error::validation(format!(
                "reconciliation report line {line_number}: malformed JSON: {error}"
            ))
        })?;
        let record_type = record
            .get("record_type")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                Error::validation(format!(
                    "reconciliation report line {line_number}: missing record_type"
                ))
            })?;
        match record_type {
            "header" => {
                if header.is_some() {
                    return Err(Error::validation(format!(
                        "reconciliation report line {line_number}: duplicate header record; \
                         a report carries exactly one header, as its first record"
                    )));
                }
                let parsed: ReportHeader = serde_json::from_value(record).map_err(|error| {
                    Error::validation(format!(
                        "reconciliation report line {line_number}: invalid header: {error}"
                    ))
                })?;
                header = Some(parsed);
            }
            "entry" => {
                if header.is_none() {
                    return Err(Error::validation(format!(
                        "reconciliation report line {line_number}: entry record appears \
                         before the header record"
                    )));
                }
                let parsed: ReportEntry = serde_json::from_value(record).map_err(|error| {
                    Error::validation(format!(
                        "reconciliation report line {line_number}: invalid entry: {error}"
                    ))
                })?;
                entries.push(parsed);
            }
            other => {
                return Err(Error::validation(format!(
                    "reconciliation report line {line_number}: unknown record_type {other:?}; \
                     records are \"header\" or \"entry\""
                )));
            }
        }
    }

    let header = header.ok_or_else(|| {
        Error::validation(
            "reconciliation report is empty: the first record must be the header record",
        )
    })?;

    let report = ReconciliationReport { header, entries };
    report.validate()?;
    Ok(report)
}
