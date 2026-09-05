//! Class-closing contract test: every mutating command advances the live event
//! sequence.
//!
//! The defect class: a mutating command ships without appending an audit event,
//! so the live event sequence stops being a sound dirtiness signal. A checkpoint
//! flush then reads the workspace as clean even though semantic state moved
//! (plan 6.2.1 P3; ADR-003). Four instances of this class were fixed one at a
//! time -- the public create path (beadrs-23fea176), dependency mutations
//! (beadrs-633ff22e), external references (beadrs-4ba1412e), structured data
//! (beadrs-857f897d), and label mutations (beadrs-2dbb8037) -- and each fix
//! protected only its own command. This suite closes the class instead.
//!
//! It walks the live `clap` command tree (the structural source of truth per
//! plan 5.3) and requires every public leaf command to carry an explicit
//! classification:
//!
//! - `Mutating`: the command performs a semantic mutation -- a change to state
//!   the forensic checkpoint carries (issues, lifecycle, claims, labels,
//!   dependencies, external references, structured data). Invoked once against
//!   a fixture workspace with a real effect, it MUST advance the live event
//!   sequence by at least one.
//! - `NonMutating`: the command must not advance the sequence, and must not
//!   visibly alter the issue set.
//!
//! A newly added command fails this suite until it is classified, and a newly
//! added mutating command fails it until it appends an event. Misclassifying a
//! mutating command as `NonMutating` to silence the first failure does not
//! silence the second: a command that changes issues without appending an
//! event is caught by the issue-set fingerprint asserted alongside the
//! sequence check.

use assert_cmd::Command;
use clap::CommandFactory;
use std::path::{Path, PathBuf};

/// How a leaf command relates to the event-sequence contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandClass {
    /// Performs a semantic mutation; a successful invocation with a real
    /// effect must advance the live event sequence.
    Mutating,
    /// Performs no semantic mutation; must never advance the live event
    /// sequence. `reason` documents why, because several of these still write
    /// (schema, checkpoint files, or store rows the checkpoint does not
    /// serialize).
    NonMutating,
}

/// One classified leaf command.
struct RegisteredCommand {
    /// Full invocation path, e.g. `"bead label add"`.
    path: &'static str,
    class: CommandClass,
    /// Why the command sits in its class; mandatory for `NonMutating`
    /// commands that write anything.
    reason: &'static str,
    /// Builds the argv (after the leading `bead`) used to invoke the command
    /// against the fixture workspace.
    invoke: fn(&Fixture) -> Vec<String>,
}

/// The mutating commands of the plan section 5 command table. Every entry
/// must be classified `Mutating` in the registry, so the plan's own contract
/// cannot silently drift from the enforced one.
const SECTION_5_MUTATING: &[&str] = &[
    "bead create",
    "bead update",
    "bead claim",
    "bead release",
    "bead close",
    "bead reopen",
    "bead redact",
    "bead label add",
    "bead label remove",
    "bead dep add",
    "bead dep remove",
    "bead sync import-only",
];

/// The read-only commands of the plan section 5 command table. Every entry
/// must be classified `NonMutating` in the registry.
const SECTION_5_READ_ONLY: &[&str] = &[
    "bead init",
    "bead list",
    "bead show",
    "bead sync flush-only",
    "bead sync status",
    "bead doctor",
    "bead capabilities",
    "bead schema list",
    "bead schema show",
    "bead schema explain",
];

fn registry() -> Vec<RegisteredCommand> {
    use CommandClass::{Mutating, NonMutating};

    let mut cmds: Vec<RegisteredCommand> = vec![
        // ---- lifecycle and claims ----
        RegisteredCommand {
            path: "bead create",
            class: Mutating,
            reason: "issue creation appends a `created` event",
            invoke: |_| {
                vec![
                    "create".into(),
                    "--title".into(),
                    "contract probe create".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead update",
            class: Mutating,
            reason: "a committed field change appends an `updated` event",
            invoke: |f| {
                vec![
                    "update".into(),
                    f.update_target.clone(),
                    "--notes".into(),
                    "contract probe note".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead claim",
            class: Mutating,
            reason: "an atomic claim appends a `claimed` event",
            invoke: |_| {
                vec![
                    "claim".into(),
                    "--assignee".into(),
                    "contract-probe-worker".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead release",
            class: Mutating,
            reason: "a semantic release appends a `released` event",
            invoke: |f| vec!["release".into(), f.in_progress.clone()],
        },
        RegisteredCommand {
            path: "bead close",
            class: Mutating,
            reason: "a semantic close appends a `closed` event",
            invoke: |f| {
                vec![
                    "close".into(),
                    f.to_close.clone(),
                    "--reason".into(),
                    "contract probe complete".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead reopen",
            class: Mutating,
            reason: "a semantic reopen appends a `reopened` event",
            invoke: |f| vec!["reopen".into(), f.closed.clone()],
        },
        RegisteredCommand {
            path: "bead redact",
            class: Mutating,
            reason: "a committed historical redaction appends one `historical_redaction` event",
            invoke: |f| {
                vec![
                    "redact".into(),
                    "--finding".into(),
                    f.redaction_fingerprint.clone(),
                    "--actor".into(),
                    "contract-probe".into(),
                    "--reason".into(),
                    "exercise event contract".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead resolve",
            class: Mutating,
            reason: "attempt resolution records its receipt and lifecycle event in one transaction",
            invoke: |f| {
                vec![
                    "resolve".into(),
                    f.resolve_target.clone(),
                    "--attempt-id".into(),
                    "urn:needle:attempt:event-contract".into(),
                    "--outcome".into(),
                    "verified_success".into(),
                    "--action".into(),
                    "close".into(),
                    "--reason".into(),
                    "contract probe complete".into(),
                    "--actor".into(),
                    "contract-probe".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead watchdog",
            class: Mutating,
            reason: "a forced stale-claim recovery appends audited override and release events",
            invoke: |_| {
                vec![
                    "watchdog".into(),
                    "--threshold".into(),
                    "1m".into(),
                    "--force".into(),
                    "--json".into(),
                ]
            },
        },
        // ---- label mutations ----
        RegisteredCommand {
            path: "bead label add",
            class: Mutating,
            reason: "a committed add appends a `label_added` event",
            invoke: |f| {
                vec![
                    "label".into(),
                    "add".into(),
                    f.label_target.clone(),
                    "--label".into(),
                    "contract-probe-label".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead label remove",
            class: Mutating,
            reason: "a committed remove appends a `label_removed` event",
            invoke: |f| {
                vec![
                    "label".into(),
                    "remove".into(),
                    f.labeled.clone(),
                    "--label".into(),
                    "contract-probe-label".into(),
                ]
            },
        },
        // ---- dependency mutations ----
        RegisteredCommand {
            path: "bead dep add",
            class: Mutating,
            reason: "a committed add appends a `dependency_added` event",
            invoke: |f| {
                vec![
                    "dep".into(),
                    "add".into(),
                    f.blocked.clone(),
                    f.blocker.clone(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead dep remove",
            class: Mutating,
            reason: "a committed remove appends a `dependency_removed` event",
            invoke: |f| {
                vec![
                    "dep".into(),
                    "remove".into(),
                    f.blocked.clone(),
                    f.blocker.clone(),
                ]
            },
        },
        // ---- external-reference mutations ----
        RegisteredCommand {
            path: "bead ref add",
            class: Mutating,
            reason: "a committed add appends an `external_ref_added` event",
            invoke: |f| {
                vec![
                    "ref".into(),
                    "add".into(),
                    "--id".into(),
                    f.ref_target.clone(),
                    "--namespace".into(),
                    "github".into(),
                    "--key".into(),
                    "probe".into(),
                    "--value".into(),
                    "contract-probe-2".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead ref remove",
            class: Mutating,
            reason: "a committed remove appends an `external_ref_removed` event",
            invoke: |f| {
                vec![
                    "ref".into(),
                    "remove".into(),
                    "--id".into(),
                    f.with_ref.clone(),
                    "--namespace".into(),
                    "github".into(),
                    "--key".into(),
                    "probe".into(),
                ]
            },
        },
        // ---- structured-data mutations ----
        RegisteredCommand {
            path: "bead data set",
            class: Mutating,
            reason: "a semantic set appends a `data_set` event",
            invoke: |f| {
                vec![
                    "data".into(),
                    "set".into(),
                    "--id".into(),
                    f.data_target.clone(),
                    "--namespace".into(),
                    "cfg".into(),
                    "--schema-ref".into(),
                    "probe:v1".into(),
                    "--value".into(),
                    "{\"probe\": true}".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead data remove",
            class: Mutating,
            reason: "a committed remove appends a `data_removed` event",
            invoke: |f| {
                vec![
                    "data".into(),
                    "remove".into(),
                    "--id".into(),
                    f.with_data.clone(),
                    "--namespace".into(),
                    "cfg".into(),
                ]
            },
        },
        // ---- recurrence ----
        RegisteredCommand {
            path: "bead recurrence materialize",
            class: Mutating,
            reason: "materialization creates an occurrence issue, which appends a `created` event",
            invoke: |f| {
                vec![
                    "recurrence".into(),
                    "materialize".into(),
                    "--id".into(),
                    f.template.clone(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead recurrence create",
            class: NonMutating,
            reason: "template rows are not serialized into the forensic checkpoint, so no \
                     audit event is defined for them; if templates ever become \
                     checkpoint-durable this MUST become Mutating",
            invoke: |f| {
                vec![
                    "recurrence".into(),
                    "create".into(),
                    "--id".into(),
                    f.spare_template.clone(),
                    "--title".into(),
                    "Contract Probe Spare".into(),
                    "--base-title-template".into(),
                    "Contract Probe Spare {n}".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead recurrence delete",
            class: NonMutating,
            reason: "same as `bead recurrence create`: template rows are outside the \
                     checkpoint the event sequence dirties",
            invoke: |f| {
                vec![
                    "recurrence".into(),
                    "delete".into(),
                    "--id".into(),
                    f.spare_template.clone(),
                ]
            },
        },
        // ---- checkpoint ingest ----
        RegisteredCommand {
            path: "bead sync import-only",
            class: Mutating,
            reason: "restore adopts the checkpoint's events and merge inserts the foreign \
                     events plus a `checkpoint_*` summary event",
            invoke: |f| {
                vec![
                    "sync".into(),
                    "import-only".into(),
                    "--input".into(),
                    f.foreign_checkpoint.display().to_string(),
                    "--merge".into(),
                    "--actor".into(),
                    "contract-probe".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead restore",
            class: Mutating,
            reason: "verified restore replaces the issue/event corpus and appends a local \
                     `checkpoint_restored` audit event plus a provenance receipt",
            invoke: |f| {
                vec![
                    "restore".into(),
                    "--source".into(),
                    f.restore_checkpoint.display().to_string(),
                    "--generation".into(),
                    f.restore_generation.clone(),
                    "--actor".into(),
                    "contract-probe".into(),
                    "--allow-non-empty".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead sync reconcile",
            class: Mutating,
            reason: "reconciling a remote-advanced checkpoint merges the pulled events into \
                     the live store and appends a merge summary event",
            invoke: |f| {
                // Reproduce the Git-transported pull using workspace
                // artifacts only (ADR-009: bead-rs never runs Git): clone
                // the workspace wholesale -- the clone shares this store's
                // UUID (R028) -- advance the clone by one issue (its
                // automatic flush publishes the covering generation), then
                // deliver that checkpoint back over this workspace's own.
                // The live store is left the strict subset that the
                // reconcile command exists to absorb.
                let clone_home = tempfile::tempdir().expect("clone tempdir");
                let clone = clone_home.path().join("clone");
                copy_tree(&f.workspace, &clone);
                create_issue(&clone, "pulled advancement");
                std::fs::remove_dir_all(f.workspace.join(".beads/checkpoint")).unwrap();
                copy_tree(
                    &clone.join(".beads/checkpoint"),
                    &f.workspace.join(".beads/checkpoint"),
                );
                vec![
                    "sync".into(),
                    "reconcile".into(),
                    "--actor".into(),
                    "contract-probe".into(),
                ]
            },
        },
        // ---- local resource declarations (R031) ----
        RegisteredCommand {
            path: "bead resource add",
            class: Mutating,
            reason: "declaring keys appends a `resource_keys_added` audit event in the \
                     declaring transaction",
            invoke: |f| {
                let mut args = vec![
                    "resource".into(),
                    "add".into(),
                    f.update_target.clone(),
                    "--key".into(),
                    "probe:gpu0".into(),
                ];
                // The sweep's earlier `bead claim` may hold this issue; a
                // claimed issue only mutates for its own credential.
                if let Some(credential) = held_credential(&f.workspace, &f.update_target) {
                    args.extend(["--fencing-token".into(), credential]);
                }
                args
            },
        },
        RegisteredCommand {
            path: "bead resource list",
            class: NonMutating,
            reason: "read-only listing of an issue's declared keys",
            invoke: |f| vec!["resource".into(), "list".into(), f.update_target.clone()],
        },
        RegisteredCommand {
            path: "bead resource remove",
            class: Mutating,
            reason: "removing keys appends a `resource_keys_removed` audit event in the \
                     removing transaction",
            invoke: |f| {
                let mut args = vec![
                    "resource".into(),
                    "remove".into(),
                    f.update_target.clone(),
                    "--key".into(),
                    "probe:gpu0".into(),
                ];
                if let Some(credential) = held_credential(&f.workspace, &f.update_target) {
                    args.extend(["--fencing-token".into(), credential]);
                }
                args
            },
        },
        // ---- workspace identity and remote reconciliation ----
        RegisteredCommand {
            path: "bead sync fork",
            class: Mutating,
            reason: "forking re-origins the workspace under a new store UUID and appends a \
                     fork summary event plus a provenance receipt",
            invoke: |f| {
                // Fork refuses a dirty checkpoint. The sweep's earlier
                // mutations publish under the automatic default, but an
                // explicit flush keeps that cleanliness hold regardless of
                // publication suppression, so the classification does not
                // depend on where the entry sorts.
                bead(&f.workspace)
                    .args(["sync", "flush-only"])
                    .assert()
                    .success();
                vec![
                    "sync".into(),
                    "fork".into(),
                    "--actor".into(),
                    "contract-probe".into(),
                ]
            },
        },
        // ---- inspection ----
        RegisteredCommand {
            path: "bead list",
            class: NonMutating,
            reason: "read-only inspection of the ready frontier",
            invoke: |_| {
                vec![
                    "list".into(),
                    "--json".into(),
                    "--limit".into(),
                    "10".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead analyze-exclusion",
            class: NonMutating,
            reason: "read-only explanation of why open issues are absent from the ready frontier",
            invoke: |_| vec!["analyze-exclusion".into(), "--json".into()],
        },
        RegisteredCommand {
            path: "bead show",
            class: NonMutating,
            reason: "read-only inspection of one issue",
            invoke: |f| vec!["show".into(), f.update_target.clone(), "--json".into()],
        },
        RegisteredCommand {
            path: "bead why",
            class: NonMutating,
            reason: "read-only explanation of issue state",
            invoke: |f| vec!["why".into(), "--id".into(), f.update_target.clone()],
        },
        RegisteredCommand {
            path: "bead compare",
            class: NonMutating,
            reason: "read-only cross-profile comparison",
            invoke: |f| {
                vec![
                    "compare".into(),
                    "--id".into(),
                    f.update_target.clone(),
                    "--source".into(),
                    "native-v1".into(),
                    "--target".into(),
                    "needle-v1".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead changes",
            class: NonMutating,
            reason: "read-only cursor feed over the event log",
            invoke: |_| vec!["changes".into(), "--latest".into()],
        },
        RegisteredCommand {
            path: "bead query",
            class: NonMutating,
            reason: "read-only query execution; saved views (--save-as/--delete-view) \
                     persist convenience state the checkpoint does not serialize",
            invoke: |_| {
                vec![
                "query".into(),
                "--json".into(),
                "{\"version\":\"v1\",\"predicates\":[{\"field\":\"base_status\",\"operator\":\"equals\",\"value\":\"open\"}],\"sort\":[]}".into(),
            ]
            },
        },
        RegisteredCommand {
            path: "bead schema list",
            class: NonMutating,
            reason: "workspace-independent catalog inspection",
            invoke: |_| vec!["schema".into(), "list".into()],
        },
        RegisteredCommand {
            path: "bead schema show",
            class: NonMutating,
            reason: "workspace-independent catalog inspection",
            invoke: |_| {
                vec![
                    "schema".into(),
                    "show".into(),
                    "urn:bead-rs:schema:issue:native-v1".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead schema explain",
            class: NonMutating,
            reason: "workspace-independent catalog inspection",
            invoke: |_| {
                vec![
                    "schema".into(),
                    "explain".into(),
                    "urn:bead-rs:schema:issue:native-v1".into(),
                    "--format".into(),
                    "markdown".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead capabilities",
            class: NonMutating,
            reason: "versioned capability inspection",
            invoke: |_| vec!["capabilities".into()],
        },
        RegisteredCommand {
            path: "bead ref list",
            class: NonMutating,
            reason: "read-only inspection of an issue's references",
            invoke: |f| {
                vec![
                    "ref".into(),
                    "list".into(),
                    "--id".into(),
                    f.with_ref.clone(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead ref find",
            class: NonMutating,
            reason: "read-only reverse lookup by reference value",
            invoke: |_| {
                vec![
                    "ref".into(),
                    "find".into(),
                    "--namespace".into(),
                    "github".into(),
                    "--value".into(),
                    "contract-probe-1".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead data get",
            class: NonMutating,
            reason: "read-only inspection of one structured-data namespace",
            invoke: |f| {
                vec![
                    "data".into(),
                    "get".into(),
                    "--id".into(),
                    f.with_data.clone(),
                    "--namespace".into(),
                    "cfg".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead data list",
            class: NonMutating,
            reason: "read-only inspection of an issue's structured-data namespaces",
            invoke: |f| {
                vec![
                    "data".into(),
                    "list".into(),
                    "--id".into(),
                    f.with_data.clone(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead recurrence list",
            class: NonMutating,
            reason: "read-only inspection of templates",
            invoke: |_| vec!["recurrence".into(), "list".into()],
        },
        RegisteredCommand {
            path: "bead recurrence show",
            class: NonMutating,
            reason: "read-only inspection of one template",
            invoke: |f| {
                vec![
                    "recurrence".into(),
                    "show".into(),
                    "--id".into(),
                    f.template.clone(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead recurrence history",
            class: NonMutating,
            reason: "read-only inspection of a template's materializations",
            invoke: |f| {
                vec![
                    "recurrence".into(),
                    "history".into(),
                    "--id".into(),
                    f.template.clone(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead policy check",
            class: NonMutating,
            reason: "advisory lint; never changes claimability or state",
            invoke: |_| vec!["policy".into(), "check".into()],
        },
        // ---- workspace infrastructure: writes, but never a semantic mutation ----
        RegisteredCommand {
            path: "bead init",
            class: NonMutating,
            reason: "creates or verifies schema only; `sync import-only \
                     --restore-into-empty` requires a pristine init to have appended no \
                     events",
            invoke: |_| vec!["init".into(), "--prefix".into(), "probe".into()],
        },
        RegisteredCommand {
            path: "bead doctor",
            class: NonMutating,
            reason: "read-only diagnostics by default; --repair performs safe \
                     housekeeping on files and checkpoint views, never semantic mutations",
            invoke: |_| vec!["doctor".into()],
        },
        RegisteredCommand {
            path: "bead sync flush-only",
            class: NonMutating,
            reason: "publishes the checkpoint and advances the covered sequence; a flush \
                     that appended an event would re-dirty the checkpoint it just cleaned",
            invoke: |_| vec!["sync".into(), "flush-only".into()],
        },
        RegisteredCommand {
            path: "bead sync status",
            class: NonMutating,
            reason: "read-only checkpoint freshness report",
            invoke: |_| vec!["sync".into(), "status".into()],
        },
        RegisteredCommand {
            path: "bead sync diff",
            class: NonMutating,
            reason: "verifies and compares retained generations in an ephemeral view",
            invoke: |f| {
                let checkpoint = f.restore_checkpoint.join("current.json");
                vec![
                    "sync".into(),
                    "diff".into(),
                    checkpoint.display().to_string(),
                    checkpoint.display().to_string(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead sync bisect",
            class: NonMutating,
            reason: "queries retained generations in an ephemeral read-only view",
            invoke: |f| {
                vec![
                    "sync".into(),
                    "bisect".into(),
                    "--checkpoint".into(),
                    f.restore_checkpoint.join("current.json").display().to_string(),
                    "--query".into(),
                    "{\"version\":\"v1\",\"predicates\":[{\"field\":\"base_status\",\"operator\":\"equals\",\"value\":\"open\"}],\"sort\":[]}".into(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead manifest commit",
            class: Mutating,
            reason: "a committed manifest appends the union of its operations' audit \
                     events in the manifest's single transaction (R033)",
            invoke: |f| {
                let manifest_path = f.workspace.join("contract-probe-manifest.json");
                std::fs::write(
                    &manifest_path,
                    concat!(
                        r#"{"manifest_version": 1, "operations": ["#,
                        r#"{"op": "create", "local_id": "probe", "title": "contract probe manifest"}]}"#
                    ),
                )
                .expect("write contract-probe manifest");
                vec![
                    "manifest".into(),
                    "commit".into(),
                    "--input".into(),
                    manifest_path.display().to_string(),
                ]
            },
        },
        RegisteredCommand {
            path: "bead manifest dry-run",
            class: NonMutating,
            reason: "executes the whole manifest inside one transaction that is always \
                     rolled back; no event survives (R033)",
            invoke: |f| {
                let manifest_path = f.workspace.join("contract-probe-manifest.json");
                std::fs::write(
                    &manifest_path,
                    concat!(
                        r#"{"manifest_version": 1, "operations": ["#,
                        r#"{"op": "create", "local_id": "probe", "title": "contract probe manifest"}]}"#
                    ),
                )
                .expect("write contract-probe manifest");
                vec![
                    "manifest".into(),
                    "dry-run".into(),
                    "--input".into(),
                    manifest_path.display().to_string(),
                ]
            },
        },
    ];

    // Redaction is intentionally last in the mutating sweep. The restore
    // probe uses the frozen setup generation, which predates this command's
    // receipt and therefore must not be allowed to erase it.
    cmds.sort_by(|a, b| (a.path == "bead redact", a.path).cmp(&(b.path == "bead redact", b.path)));
    cmds
}

/// Fixture workspace state the registered invocations refer to.
struct Fixture {
    /// Keeps the workspace directories alive for the whole test.
    _dirs: Vec<tempfile::TempDir>,
    workspace: PathBuf,
    /// Open, unassigned: target for `update --notes`, `show`, `why`, `compare`.
    update_target: String,
    /// Carries `contract-probe-label` from setup: target for `label remove`.
    labeled: String,
    /// Any open issue: target for `label add`.
    label_target: String,
    /// Pair used for `dep add` / `dep remove`.
    blocked: String,
    blocker: String,
    /// Carries the `github/probe` reference from setup: target for `ref remove`.
    with_ref: String,
    /// Any issue without a reference: target for `ref add`.
    ref_target: String,
    /// Carries the `cfg` structured data from setup: target for `data remove`.
    with_data: String,
    /// Any issue without structured data: target for `data set`.
    data_target: String,
    /// Held `in_progress` from setup: target for `release`.
    in_progress: String,
    /// Open: target for `close`.
    to_close: String,
    /// Closed in setup: target for `reopen`.
    closed: String,
    /// Open issue resolved through the attempt-outcome transaction.
    resolve_target: String,
    /// Opaque scanner identity for the historical-redaction probe.
    redaction_fingerprint: String,
    /// Recurrence template created in setup: target for `materialize`.
    template: String,
    /// Created and deleted only by the NonMutating phase.
    spare_template: String,
    /// Checkpoint of a second workspace, merged in by `sync import-only`.
    foreign_checkpoint: PathBuf,
    /// Stable copy of this workspace's setup generation for `bead restore`.
    restore_checkpoint: PathBuf,
    restore_generation: String,
}

fn bead(workspace: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bead").unwrap();
    cmd.current_dir(workspace);
    cmd
}

fn create_issue(workspace: &Path, title: &str) -> String {
    let output = bead(workspace)
        .args(["create", "--title", title])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(output).unwrap().trim().to_string()
}

/// The claim-epoch credential `issue` currently holds, if any. The sweep runs
/// every mutating command against one shared fixture in registry order, so a
/// command can be handed an issue an earlier command claimed -- and a claimed
/// issue only mutates for the credential its claim issued (see
/// tests/claim_epoch.rs). `None` for an unclaimed issue, where no credential
/// exists to present.
fn held_credential(workspace: &Path, issue: &str) -> Option<String> {
    let output = bead(workspace)
        .args(["show", issue, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let shown: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let epoch = shown[0]["claim_epoch"].as_i64()?;
    (epoch > 0).then(|| epoch.to_string())
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let source = entry.path();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&source, &target);
        } else {
            std::fs::copy(source, target).unwrap();
        }
    }
}

fn build_fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("fixture tempdir");
    let workspace = dir.path().to_path_buf();

    bead(&workspace)
        .args(["init", "--prefix", "probe"])
        .assert()
        .success();

    let update_target = create_issue(&workspace, "update target");
    let labeled = create_issue(&workspace, "labeled target");
    let label_target = create_issue(&workspace, "label target");
    let blocked = create_issue(&workspace, "blocked target");
    let blocker = create_issue(&workspace, "blocker target");
    let with_ref = create_issue(&workspace, "ref target");
    let ref_target = create_issue(&workspace, "ref add target");
    let with_data = create_issue(&workspace, "data target");
    let data_target = create_issue(&workspace, "data set target");
    let in_progress = create_issue(&workspace, "release target");
    let to_close = create_issue(&workspace, "close target");
    let closed = create_issue(&workspace, "reopen target");
    let resolve_target = create_issue(&workspace, "resolve target");
    let watchdog_target = create_issue(&workspace, "watchdog target");
    let redact_target = create_issue(&workspace, "redaction target");

    bead(&workspace)
        .args(["update", &in_progress, "--status", "in_progress"])
        .assert()
        .success();
    bead(&workspace)
        .args(["close", &closed, "--reason", "fixture setup"])
        .assert()
        .success();
    bead(&workspace)
        .args([
            "update",
            &watchdog_target,
            "--status",
            "in_progress",
            "--assignee",
            "watchdog-contract-probe",
        ])
        .assert()
        .success();
    bead(&workspace)
        .args(["label", "add", &labeled, "--label", "contract-probe-label"])
        .assert()
        .success();
    bead(&workspace)
        .args([
            "ref",
            "add",
            "--id",
            &with_ref,
            "--namespace",
            "github",
            "--key",
            "probe",
            "--value",
            "contract-probe-1",
        ])
        .assert()
        .success();

    // Historical findings cannot be seeded through public argv because the
    // rejection boundary correctly blocks them. Model a pre-feature record
    // directly, advance the forensic event sequence, then retain only the
    // scanner fingerprint for the command registry.
    let shaped = ["AK", "IA", "7Q9W2E4R6T8Y1U3I"].concat();
    let conn = rusqlite::Connection::open(workspace.join(".beads/beads.db")).unwrap();
    conn.execute(
        "UPDATE issues
         SET description = ?1, revision = revision + 1,
             updated_at = '2026-09-03T00:00:00Z'
         WHERE id = ?2",
        rusqlite::params![&shaped, &redact_target],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO events (issue_id, kind, actor, time, detail)
         VALUES (?1, 'historical_fixture_seed', 'contract-probe',
                 '2026-09-03T00:00:00Z', '{}')",
        [&redact_target],
    )
    .unwrap();
    conn.execute(
        "UPDATE issues SET updated_at = '2020-01-01T00:00:00Z' WHERE id = ?1",
        [&watchdog_target],
    )
    .unwrap();
    let redaction_fingerprint = bead_rs::service::secret_diagnostics::scan_live_findings(&conn)
        .unwrap()
        .into_iter()
        .find(|finding| {
            finding.rule_id == "aws-access-key-id"
                && finding.field_path == "description"
                && finding.is_blocking_match()
        })
        .expect("historical fixture must produce one blocking finding")
        .fingerprint;
    drop(conn);
    bead(&workspace)
        .args([
            "data",
            "set",
            "--id",
            &with_data,
            "--namespace",
            "cfg",
            "--schema-ref",
            "probe:v1",
            "--value",
            "{\"setup\": true}",
        ])
        .assert()
        .success();
    bead(&workspace)
        .args([
            "recurrence",
            "create",
            "--id",
            "probe-template",
            "--title",
            "Contract Probe",
            "--base-title-template",
            "Contract Probe {n}",
        ])
        .assert()
        .success();

    // Freeze the setup generation outside the live workspace. Earlier
    // mutating commands in the registry publish newer generations and may
    // tombstone old objects; restore must keep selecting this exact immutable
    // generation throughout the sweep.
    bead(&workspace)
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let restore_source = tempfile::tempdir().expect("restore source tempdir");
    let restore_checkpoint = restore_source.path().join("checkpoint");
    copy_tree(&workspace.join(".beads/checkpoint"), &restore_checkpoint);
    let restore_pointer: serde_json::Value =
        serde_json::from_slice(&std::fs::read(restore_checkpoint.join("current.json")).unwrap())
            .unwrap();
    let restore_generation = restore_pointer["generation_id"]
        .as_str()
        .unwrap()
        .to_string();

    // A second workspace supplies the checkpoint that `sync import-only`
    // merges in: one foreign issue, flushed.
    let foreign = tempfile::tempdir().expect("foreign tempdir");
    let foreign_ws = foreign.path().to_path_buf();
    bead(&foreign_ws)
        .args(["init", "--prefix", "probe"])
        .assert()
        .success();
    create_issue(&foreign_ws, "foreign issue");
    bead(&foreign_ws)
        .args(["sync", "flush-only"])
        .assert()
        .success();
    let foreign_checkpoint = foreign_ws.join(".beads/checkpoint/forensic.jsonl");

    Fixture {
        _dirs: vec![dir, foreign, restore_source],
        workspace,
        update_target,
        labeled,
        label_target,
        blocked,
        blocker,
        with_ref,
        ref_target,
        with_data,
        data_target,
        in_progress,
        to_close,
        closed,
        resolve_target,
        redaction_fingerprint,
        template: "probe-template".to_string(),
        spare_template: "probe-spare-template".to_string(),
        foreign_checkpoint,
        restore_checkpoint,
        restore_generation,
    }
}

/// The live event sequence: the same `MAX(sequence)` definition the shipped
/// code uses for its dirtiness signal.
fn live_sequence(workspace: &Path) -> i64 {
    let conn = rusqlite::Connection::open(workspace.join(".beads/beads.db"))
        .expect("open fixture database");
    conn.query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
        row.get(0)
    })
    .expect("read live event sequence")
}

/// Public-surface fingerprint of the issue set, used to catch a command that
/// changes issues without appending an event.
fn issue_fingerprint(workspace: &Path) -> String {
    let output = bead(workspace)
        .args(["list", "--json", "--limit", "999999"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(output).unwrap()
}

/// Every visible leaf command path in the live `clap` tree.
fn leaf_command_paths() -> Vec<String> {
    let mut leaves = Vec::new();
    walk_leaves(&bead_rs::cli::Cli::command(), "bead", &mut leaves);
    leaves.sort();
    leaves
}

fn walk_leaves(cmd: &clap::Command, prefix: &str, leaves: &mut Vec<String>) {
    let visible: Vec<&clap::Command> = cmd
        .get_subcommands()
        .filter(|sub| !sub.is_hide_set() && sub.get_name() != "help")
        .collect();

    if visible.is_empty() {
        leaves.push(prefix.to_string());
        return;
    }

    for sub in visible {
        walk_leaves(sub, &format!("{prefix} {}", sub.get_name()), leaves);
    }
}

const CLASSIFICATION_HELP: &str = "every visible leaf command must declare its event contract in \
     this test's registry:\n  - a command that performs a semantic mutation (state the forensic \
     checkpoint carries: issues, lifecycle, claims, labels, dependencies, external references, \
     structured data) is Mutating and must append an audit event in its own transaction (plan \
     6.2.1 P3, ADR-003);\n  - anything else is NonMutating and must carry a reason.\n\
     Adding a mutating command without classifying it -- or without its event -- fails this suite.";

#[test]
fn every_leaf_command_is_classified() {
    let registry = registry();
    let classified: std::collections::HashSet<&str> = registry.iter().map(|c| c.path).collect();
    let leaf_paths = leaf_command_paths();
    let leaves: std::collections::HashSet<&str> = leaf_paths.iter().map(String::as_str).collect();

    let unclassified: Vec<&str> = leaves.difference(&classified).copied().collect();
    assert!(
        unclassified.is_empty(),
        "unclassified command(s) {:?}: {}\n(tree: {:?})",
        unclassified,
        CLASSIFICATION_HELP,
        leaf_paths,
    );

    let stale: Vec<&str> = classified.difference(&leaves).copied().collect();
    assert!(
        stale.is_empty(),
        "registered command(s) {:?} no longer exist in the clap tree; remove or rename their \
         registry entries",
        stale,
    );

    // The registry must be exhaustive without duplicates.
    assert_eq!(
        registry.len(),
        registry
            .iter()
            .map(|c| c.path)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        "duplicate registry entries"
    );
}

#[test]
fn section_5_command_table_is_enforced() {
    let registered = registry();
    let registry: std::collections::HashMap<&str, &RegisteredCommand> =
        registered.iter().map(|c| (c.path, c)).collect();

    for path in SECTION_5_MUTATING {
        let entry = registry.get(*path).unwrap_or_else(|| {
            panic!("plan section 5 mutating command {path:?} is missing from the registry")
        });
        assert_eq!(
            entry.class,
            CommandClass::Mutating,
            "plan section 5 lists {path} as a mutating command; the registry must classify it \
             Mutating"
        );
    }

    for path in SECTION_5_READ_ONLY {
        let entry = registry.get(*path).unwrap_or_else(|| {
            panic!("plan section 5 read-only command {path:?} is missing from the registry")
        });
        assert_eq!(
            entry.class,
            CommandClass::NonMutating,
            "plan section 5 lists {path} as non-mutating; the registry must classify it \
             NonMutating"
        );
    }
}

#[test]
fn every_mutating_command_advances_the_live_event_sequence() {
    let fixture = build_fixture();

    for command in registry()
        .into_iter()
        .filter(|c| c.class == CommandClass::Mutating)
    {
        let args = (command.invoke)(&fixture);
        let before = live_sequence(&fixture.workspace);

        let assert = bead(&fixture.workspace).args(&args).assert();
        if !assert.get_output().status.success() {
            panic!(
                "mutating command {:?} failed against the fixture (args {:?}): {}",
                command.path,
                args,
                String::from_utf8_lossy(&assert.get_output().stderr)
            );
        }

        let after = live_sequence(&fixture.workspace);
        assert!(
            after > before,
            "mutating command {:?} committed without advancing the live event sequence \
             ({} -> {}): the event sequence is the checkpoint dirtiness signal (plan 6.2.1 \
             P3, ADR-003), so this mutation is invisible to `sync flush-only`. Append its \
             audit event in the mutation's own transaction. [{}]",
            command.path,
            before,
            after,
            command.reason,
        );
    }
}

#[test]
fn non_mutating_commands_do_not_advance_the_live_event_sequence() {
    let fixture = build_fixture();

    for command in registry()
        .into_iter()
        .filter(|c| c.class == CommandClass::NonMutating)
    {
        let args = (command.invoke)(&fixture);
        let before = live_sequence(&fixture.workspace);
        let fingerprint_before = issue_fingerprint(&fixture.workspace);

        let assert = bead(&fixture.workspace).args(&args).assert();
        if !assert.get_output().status.success() {
            panic!(
                "non-mutating command {:?} failed against the fixture (args {:?}): {}",
                command.path,
                args,
                String::from_utf8_lossy(&assert.get_output().stderr)
            );
        }

        let after = live_sequence(&fixture.workspace);
        assert_eq!(
            after, before,
            "non-mutating command {:?} advanced the live event sequence ({} -> {}); only \
             semantic mutations may append events. [{}]",
            command.path, before, after, command.reason,
        );

        let fingerprint_after = issue_fingerprint(&fixture.workspace);
        assert_eq!(
            fingerprint_before, fingerprint_after,
            "non-mutating command {:?} changed the visible issue set without appending an \
             event; if it is actually a semantic mutation, classify it Mutating and give it \
             an event. [{}]",
            command.path, command.reason,
        );
    }
}
