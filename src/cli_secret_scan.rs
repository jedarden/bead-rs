//! CLI-to-scanner canonicalization for ADR-014.
//!
//! This module is intentionally an exhaustive match over the public command
//! enum. A new mutation cannot silently inherit an unscanned string field:
//! the command-inventory test below must classify it as a mutation or as an
//! explicitly read-only/recovery operation.

use crate::cli::{
    Command, DataCommand, DepCommand, LabelCommand, RecurrenceCommand, RefCommand, ResourceCommand,
    SyncCommand,
};
use crate::error::{Error, Result};
use crate::scan::{self, Field, ScanConfig, ScanReport};
use crate::store::WorkspaceConfig;

pub(crate) struct PreparedScan {
    report: ScanReport,
    actor: String,
}

impl PreparedScan {
    pub(crate) fn arm_audit(&self) -> scan::AcknowledgmentAuditGuard {
        scan::arm_acknowledgment_audit(&self.report, &self.actor)
    }
}

struct CanonicalRequest<'a> {
    selector: String,
    actor: &'a str,
    fields: Vec<Field<'a>>,
}

fn record_selector(kind: &str, candidate: &str) -> String {
    let safe = !candidate.is_empty()
        && candidate.len() <= 255
        && candidate
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if safe {
        format!("{kind}:{candidate}")
    } else {
        format!("{kind}:input")
    }
}

impl<'a> CanonicalRequest<'a> {
    fn new(selector: impl Into<String>, actor: &'a str) -> Self {
        Self {
            selector: selector.into(),
            actor,
            fields: Vec::new(),
        }
    }

    fn field(mut self, path: &'a str, value: &'a str) -> Self {
        self.fields.push(Field::new(path, value));
        self
    }

    fn optional(mut self, path: &'a str, value: Option<&'a str>) -> Self {
        if let Some(value) = value {
            self.fields.push(Field::new(path, value));
        }
        self
    }

    fn repeated(mut self, path: &'a str, values: &'a [String]) -> Self {
        self.fields
            .extend(values.iter().map(|value| Field::new(path, value)));
        self
    }
}

/// Scan every operator-supplied text field that a CLI mutation can persist.
/// Recovery inputs are deliberately excluded: the bytes already exist and
/// the contract requires reporting rather than refusal for recovery paths.
pub(crate) fn prepare(cli: &crate::cli::Cli) -> Result<Option<PreparedScan>> {
    if let Command::Manifest(command) = &cli.command {
        let opts = match command {
            crate::cli::ManifestCommand::DryRun(opts)
            | crate::cli::ManifestCommand::Commit(opts) => opts,
        };
        let manifest = crate::service::load_manifest(std::path::Path::new(&opts.input))?;
        let Some(workspace) = WorkspaceConfig::discover()? else {
            return Ok(None);
        };
        let config = configured_policy(cli, &workspace)?;
        let report = crate::service::manifest::scan_manifest(&config, &manifest);
        return finalize(config, report, "cli");
    }

    let Some(request) = canonical_request(&cli.command) else {
        if !cli.acknowledge_secret.is_empty() {
            return Err(Error::cli_usage(
                "--acknowledge-secret is valid only for a mutating command",
            ));
        }
        return Ok(None);
    };

    let Some(workspace) = WorkspaceConfig::discover()? else {
        // Preserve the command's existing no-workspace diagnostic. Init and
        // recovery never reach this branch, and there is no state to mutate.
        return Ok(None);
    };
    let config = configured_policy(cli, &workspace)?;
    let report = scan::scan(&config, &request.selector, &request.fields);
    finalize(config, report, request.actor)
}

fn configured_policy(cli: &crate::cli::Cli, workspace: &WorkspaceConfig) -> Result<ScanConfig> {
    let mut config = ScanConfig::load_from_workspace_root(&workspace.root)
        .map_err(|error| Error::cli_usage(error.to_string()))?;
    config
        .add_invocation_acknowledgments(cli.acknowledge_secret.iter().map(String::as_str))
        .map_err(|error| Error::cli_usage(error.to_string()))?;
    Ok(config)
}

fn finalize(config: ScanConfig, report: ScanReport, actor: &str) -> Result<Option<PreparedScan>> {
    if let Some(rejection) = scan::reject_if_blocked(&config, &report) {
        return Err(Error::cli_usage(rejection.message));
    }
    Ok(Some(PreparedScan {
        report,
        actor: actor.to_string(),
    }))
}

fn canonical_request(command: &Command) -> Option<CanonicalRequest<'_>> {
    match command {
        Command::Create(opts) => Some(
            CanonicalRequest::new("issue:new", "cli")
                .field("title", &opts.title)
                .optional("description", opts.description.as_deref())
                .optional("issue_type", opts.issue_type.as_deref())
                .optional("assignee", opts.assignee.as_deref())
                .repeated("labels[]", &opts.label)
                .repeated("resource_keys[]", &opts.resource_keys)
                .optional("unique_ref", opts.unique_ref.as_deref())
                .optional("unsupported.status", opts.status.as_deref())
                .optional("unsupported.notes", opts.notes.as_deref()),
        ),
        Command::Claim(opts) => Some(
            CanonicalRequest::new("claim:ready-frontier", "cli")
                .field("assignee", &opts.assignee)
                .field("policy", &opts.policy),
        ),
        Command::Update(opts) => Some(
            CanonicalRequest::new(record_selector("issue", &opts.id), "cli")
                .field("id", &opts.id)
                .optional("status", opts.status.as_deref())
                .optional("assignee", opts.assignee.as_deref())
                .optional("notes", opts.notes.as_deref())
                .optional("unsupported.title", opts.title.as_deref())
                .optional("unsupported.description", opts.description.as_deref())
                .optional("unsupported.issue_type", opts.issue_type_hidden.as_deref())
                .optional("unsupported.label", opts.label.as_deref()),
        ),
        Command::Release(opts) => Some(
            CanonicalRequest::new(record_selector("issue", &opts.id), "cli").field("id", &opts.id),
        ),
        Command::Close(opts) => Some(
            CanonicalRequest::new(record_selector("issue", &opts.id), "cli")
                .field("id", &opts.id)
                .optional("close_reason", opts.reason.as_deref())
                .optional("unsupported.body", opts.body.as_deref()),
        ),
        Command::Reopen(opts) => Some(
            CanonicalRequest::new(record_selector("issue", &opts.id), "cli").field("id", &opts.id),
        ),
        Command::Resolve(opts) => Some(
            CanonicalRequest::new(record_selector("issue", &opts.id), "cli")
                .field("id", &opts.id)
                .field("attempt_id", &opts.attempt_id)
                .field("outcome", &opts.outcome)
                .optional("action", opts.action.as_deref())
                .optional("reason", opts.reason.as_deref())
                .repeated("evidence_refs[]", &opts.evidence_ref)
                .field("actor", &opts.actor)
                .optional("model", opts.model.as_deref())
                .optional("harness", opts.harness.as_deref())
                .optional("harness_version", opts.harness_version.as_deref()),
        ),
        Command::Label(command) => match command {
            LabelCommand::Add(opts) => Some(
                CanonicalRequest::new(record_selector("issue", &opts.id), "cli")
                    .field("id", &opts.id)
                    .field("label", &opts.label),
            ),
            LabelCommand::Remove(opts) => Some(
                CanonicalRequest::new(record_selector("issue", &opts.id), "cli")
                    .field("id", &opts.id)
                    .field("label", &opts.label),
            ),
        },
        Command::Resource(command) => match command {
            ResourceCommand::Add(opts) => Some(
                CanonicalRequest::new(record_selector("issue", &opts.id), "cli")
                    .field("id", &opts.id)
                    .repeated("resource_keys[]", &opts.keys),
            ),
            ResourceCommand::Remove(opts) => Some(
                CanonicalRequest::new(record_selector("issue", &opts.id), "cli")
                    .field("id", &opts.id)
                    .repeated("resource_keys[]", &opts.keys),
            ),
            ResourceCommand::List(_) => None,
        },
        Command::Dep(command) => match command {
            DepCommand::Add(opts) => Some(
                CanonicalRequest::new("dependency:input", "cli")
                    .field("blocked_issue_id", &opts.blocked)
                    .field("blocker_issue_id", &opts.blocker)
                    .field("kind", &opts.kind)
                    .optional("condition", opts.condition.as_deref()),
            ),
            DepCommand::Remove(opts) => Some(
                CanonicalRequest::new("dependency:input", "cli")
                    .field("blocked_issue_id", &opts.blocked)
                    .field("blocker_issue_id", &opts.blocker)
                    .optional("kind", opts.kind.as_deref()),
            ),
        },
        Command::Ref(command) => match command {
            RefCommand::Add(opts) => Some(
                CanonicalRequest::new(record_selector("issue", &opts.id), "cli")
                    .field("id", &opts.id)
                    .field("external_ref.namespace", &opts.namespace)
                    .field("external_ref.key", &opts.key)
                    .field("external_ref.value", &opts.value),
            ),
            RefCommand::Remove(opts) => Some(
                CanonicalRequest::new(record_selector("issue", &opts.id), "cli")
                    .field("id", &opts.id)
                    .field("external_ref.namespace", &opts.namespace)
                    .field("external_ref.key", &opts.key),
            ),
            RefCommand::List(_) | RefCommand::Find(_) => None,
        },
        Command::Data(command) => match command {
            DataCommand::Set(opts) => Some(
                CanonicalRequest::new(record_selector("issue", &opts.id), "cli")
                    .field("id", &opts.id)
                    .field("data.namespace", &opts.namespace)
                    .field("data.schema_ref", &opts.schema_ref)
                    .field("data.value", &opts.value),
            ),
            DataCommand::Remove(opts) => Some(
                CanonicalRequest::new(record_selector("issue", &opts.id), "cli")
                    .field("id", &opts.id)
                    .field("data.namespace", &opts.namespace),
            ),
            DataCommand::Get(_) | DataCommand::List(_) => None,
        },
        Command::Recurrence(command) => match command {
            RecurrenceCommand::Create(opts) => Some(
                CanonicalRequest::new(record_selector("recurrence", &opts.id), "cli")
                    .field("id", &opts.id)
                    .field("title", &opts.title)
                    .optional("description", opts.description.as_deref())
                    .field("base_title_template", &opts.base_title_template)
                    .optional("base_description", opts.base_description.as_deref())
                    .optional("issue_type", opts.issue_type.as_deref())
                    .optional("labels", opts.labels.as_deref()),
            ),
            RecurrenceCommand::Delete(opts) => Some(
                CanonicalRequest::new(record_selector("recurrence", &opts.id), "cli")
                    .field("id", &opts.id),
            ),
            RecurrenceCommand::Materialize(opts) => Some(
                CanonicalRequest::new(record_selector("recurrence", &opts.id), "cli")
                    .field("id", &opts.id)
                    .optional("actor", opts.actor.as_deref()),
            ),
            RecurrenceCommand::Show(_)
            | RecurrenceCommand::List(_)
            | RecurrenceCommand::History(_) => None,
        },
        Command::Sync(SyncCommand::Fork(opts)) => Some(
            CanonicalRequest::new("workspace:fork", "cli")
                .field("actor", &opts.actor)
                .optional("reason", opts.reason.as_deref()),
        ),
        // Manifest content is parsed and scanned by the manifest service so
        // its complete canonical operation list is covered before its single
        // transaction opens. Import, restore, reconcile, and checkpoint
        // archaeology are recovery/reporting paths and must not reject bytes
        // that already exist.
        Command::Manifest(_)
        | Command::Init(_)
        | Command::Restore(_)
        | Command::Sync(_)
        | Command::List(_)
        | Command::Show(_)
        | Command::Doctor(_)
        | Command::Capabilities(_)
        | Command::Schema(_)
        | Command::Query(_)
        | Command::Changes(_)
        | Command::Why(_)
        | Command::AnalyzeExclusion(_)
        | Command::Compare(_)
        | Command::Policy(_)
        | Command::Watchdog(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parsed(args: &[&str]) -> crate::cli::Cli {
        crate::cli::Cli::try_parse_from(args).unwrap()
    }

    #[test]
    fn command_inventory_classifies_representative_mutations() {
        let cases: &[&[&str]] = &[
            &["bead", "create", "--title", "x"],
            &["bead", "claim", "--assignee", "x"],
            &["bead", "update", "x", "--notes", "x"],
            &["bead", "release", "x"],
            &["bead", "close", "x", "--reason", "x"],
            &["bead", "reopen", "x"],
            &[
                "bead",
                "resolve",
                "x",
                "--attempt-id",
                "attempt:x",
                "--outcome",
                "cancelled",
            ],
            &["bead", "label", "add", "x", "--label", "x"],
            &["bead", "resource", "add", "x", "--key", "x"],
            &["bead", "dep", "add", "x", "y"],
            &[
                "bead",
                "ref",
                "add",
                "--id",
                "x",
                "--namespace",
                "n",
                "--key",
                "k",
                "--value",
                "v",
            ],
            &[
                "bead",
                "data",
                "set",
                "--id",
                "x",
                "--namespace",
                "n",
                "--schema-ref",
                "s",
                "--value",
                "{}",
            ],
            &[
                "bead",
                "recurrence",
                "create",
                "--id",
                "x",
                "--title",
                "x",
                "--base-title-template",
                "x {n}",
            ],
            &["bead", "sync", "fork", "--actor", "x"],
        ];
        for args in cases {
            let cli = parsed(args);
            assert!(canonical_request(&cli.command).is_some(), "{args:?}");
        }
    }

    #[test]
    fn read_and_recovery_commands_do_not_create_mutation_requests() {
        for args in [
            &["bead", "list"][..],
            &["bead", "show", "x"][..],
            &["bead", "capabilities"][..],
            &["bead", "sync", "status"][..],
        ] {
            let cli = parsed(args);
            assert!(canonical_request(&cli.command).is_none(), "{args:?}");
        }
    }
}
