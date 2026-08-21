//! CLI command definitions for bead-rs
//!
//! This module uses clap derive to define all command-line interface commands.

use clap::{Parser, Subcommand};

/// Main CLI structure for bead-rs
#[derive(Parser, Debug)]
#[command(name = "bead")]
#[command(
    author = "Jed Arden <github@jedarden.com>",
    version = env!("CARGO_PKG_VERSION"),
    about = "Clean-room task coordination for agent fleets",
    long_about = "bead-rs is an independent Rust task-coordination system.

The intended workflow is:
  init workspace -> create beads -> add blocking relationships
  -> inspect ready work -> claim -> update/release -> close -> flush checkpoint

The ready frontier can be inspected with `bead list --ready --json --limit N`,
which uses claim order but does not reserve the displayed beads. Use `bead claim`
to atomically assign work.

SQLite (.beads/beads.db) is the authoritative live state between flushes, and is
not committed. The checkpoint under .beads/checkpoint/ is the portable, durable
copy and is what Git tracks; flush it with `bead sync flush-only` before
committing the repository. Mutating commands never flush implicitly.

Lifecycle transitions:
  - open beads may be ready if unassigned and not manually blocked
  - unfinished `blocks` edges remove beads from the ready frontier
  - claim atomically assigns one ready bead and moves it to in_progress
  - release returns claimed work to open/unassigned
  - close requires a reason and may expose dependents
  - reopen restores a closed bead to open

EXIT CODES:
  0  success
  1  internal failure
  2  CLI usage or validation error
  3  workspace, issue, or file not found
  4  conflict (invalid transition, revision guard, cycle)
  5  malformed input or integrity failure

Run `bead <COMMAND> --help` for the full description of any command."
)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Suppress automatic post-commit checkpoint publication for this one
    /// invocation, leaving the checkpoint dirty for a later explicit
    /// `bead sync flush-only`; overrides the `checkpoint.auto_flush`
    /// workspace configuration key
    #[arg(long, global = true)]
    pub no_auto_flush: bool,

    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Command,
}

/// Available commands
#[derive(Subcommand, Debug)]
pub enum Command {
    Init(InitOptions),

    Create(CreateOptions),

    List(ListOptions),

    Show(ShowOptions),

    Update(UpdateOptions),

    Release(ReleaseOptions),

    Close(CloseOptions),

    Reopen(ReopenOptions),

    Claim(ClaimOptions),

    /// Manage labels
    #[command(
        subcommand,
        long_about = "Add or remove labels on an issue.

Labels are free-form, case-sensitive strings used for categorization. Both
operations are idempotent, so they are safe to run repeatedly.

  bead label add <ID> --label <LABEL>
  bead label remove <ID> --label <LABEL>"
    )]
    Label(LabelCommand),

    /// Manage dependencies
    #[command(
        subcommand,
        long_about = "Add or remove dependency edges between issues.

An edge is written blocked-first: `bead dep add <BLOCKED> <BLOCKER>` means
BLOCKER must close before BLOCKED can become ready. `blocks` edges affect the
ready frontier and may not form cycles; `relates_to` edges are informational
only and may.

  bead dep add <BLOCKED> <BLOCKER> [--kind blocks|relates_to]
  bead dep remove <BLOCKED> <BLOCKER> [--kind KIND]"
    )]
    Dep(DepCommand),

    /// Manage external references
    #[command(
        subcommand,
        long_about = "Attach namespaced (namespace, key, value) references to issues.

References link a bead to an identifier in another system -- a tracker ticket, a
commit, a pull request -- without resolving anything over the network. `ref find`
locates every issue carrying a given value, which supports cross-tool dedup.

  bead ref add --id <ID> --namespace <NS> --key <KEY> --value <VALUE>
  bead ref remove --id <ID> --namespace <NS> --key <KEY>
  bead ref list --id <ID>
  bead ref find --namespace <NS> --value <VALUE>"
    )]
    Ref(RefCommand),

    /// Synchronize checkpoint operations
    #[command(
        subcommand,
        long_about = "Publish and ingest the durable checkpoint.

SQLite holds live state and is not committed; the checkpoint under
.beads/checkpoint/ is what Git tracks. Nothing flushes implicitly, so a
checkpoint is only as current as the last explicit flush.

  bead sync flush-only                                 # database -> checkpoint
  bead sync import-only --input <PATH> --restore-into-empty --actor <WHO>
  bead sync import-only --input <PATH> --merge --actor <WHO>

Recovering a fresh clone is `bead init` then `import-only --restore-into-empty`."
    )]
    Sync(SyncCommand),

    Doctor(DoctorOptions),

    Capabilities(CapabilitiesOptions),

    #[command(
        subcommand,
        about = "Inspect public schemas",
        long_about = "Inspect the immutable public document-schema catalog.\n\nThe catalog is workspace-independent and is the same typed registry returned by `bead capabilities`."
    )]
    Schema(SchemaCommand),

    Query(QueryOptions),

    Changes(ChangesOptions),

    Why(WhyOptions),

    Compare(CompareOptions),

    /// Manage structured bead data
    #[command(
        subcommand,
        long_about = "Attach schema-governed JSON documents to an issue.

Each document lives under a namespace and declares an immutable schema
reference, which lets consumers negotiate on content they understand while
unknown namespaces are preserved untouched through interchange.

  bead data set --id <ID> --namespace <NS> --schema-ref <REF> --value <JSON>
  bead data get --id <ID> --namespace <NS>
  bead data list --id <ID>
  bead data remove --id <ID> --namespace <NS>"
    )]
    Data(DataCommand),

    /// Manage recurrence templates
    #[command(
        subcommand,
        long_about = "Define templates that mint repeat issues on demand.

bead-rs never creates occurrences on a schedule of its own. An external
scheduler calls `recurrence materialize` when it wants the next one, and each
occurrence records its series reference and sequence number.

  bead recurrence create --id <ID> --title <T> --base-title-template <T {n}>
  bead recurrence materialize --id <ID> [--actor <WHO>]
  bead recurrence list | show --id <ID> | history --id <ID> | delete --id <ID>"
    )]
    Recurrence(RecurrenceCommand),

    /// Validate workspace policy and scheduling configuration
    #[command(
        subcommand,
        long_about = "Lint scheduling and retention configuration.

Policy lint is advisory only: it reports contradictory, unreachable, redundant,
or ineffective settings and never changes whether a bead is claimable.

  bead policy check [--format text|json]"
    )]
    Policy(PolicyCommand),
}

/// Options for workspace initialization
#[derive(Parser, Debug)]
#[command(
    about = "Initialize a new workspace",
    long_about = "Initialize a new bead workspace in the current directory.

Creates a .beads directory with SQLite database and configuration.
Repeated initialization with the same prefix is safe and deterministic.
The workspace prefix is used as the default prefix for generated bead IDs.

EXAMPLES:
  bead init                          # Use default prefix 'bead'
  bead init --prefix myteam          # Use custom prefix 'myteam'
  bead init --prefix taskforce       # Use custom prefix 'taskforce'

The workspace must be initialized before creating or managing issues.
Once initialized, all bead commands will operate on this workspace."
)]
pub struct InitOptions {
    /// Custom prefix for bead IDs (default: bead)
    #[arg(long, default_value = "bead")]
    pub prefix: String,
}

/// Options for creating a new issue
#[derive(Parser, Debug)]
#[command(
    about = "Create a new issue",
    long_about = "Create a new issue in the workspace.

Creates a new issue with the specified title and optional metadata.
The issue ID is automatically generated and printed on success.
Prints only the issue ID followed by a newline on success.

PRIORITIES:
  0 = urgent (immediate incident, safety, or release-blocking)
  1 = critical (essential work preceding ordinary delivery)
  2 = high (important planned work, default)
  3 = normal (ordinary work with no elevated urgency)
  4 = aspirational/backlog (speculative or low-urgency work)

EXAMPLES:
  bead create --title \"Fix authentication bug\" --priority 0
  bead create --title \"Update documentation\" --priority 2 --label docs
  bead create --title \"Add search feature\" --assignee alice --label feature --label backend
  bead create --title \"Code review PR-123\" --description \"Review changes for user auth\"

ISSUE TYPES:
  Common types include: task, bug, feature, improvement, documentation
  Custom types can be specified as needed (no validation is performed)."
)]
pub struct CreateOptions {
    /// Issue title (required)
    #[arg(long)]
    pub title: String,

    /// Issue description (optional, defaults to empty)
    #[arg(long)]
    pub description: Option<String>,

    /// Issue priority (0-4, default: 2)
    #[arg(long, default_value = "2")]
    pub priority: i64,

    /// Issue type (default: task)
    #[arg(long)]
    pub issue_type: Option<String>,

    /// Assignee (optional)
    #[arg(long)]
    pub assignee: Option<String>,

    /// Labels to add (can be specified multiple times)
    #[arg(long)]
    pub label: Vec<String>,
}

/// Options for listing issues
#[derive(Parser, Debug)]
#[command(
    about = "List issues",
    long_about = "List issues with optional filtering and comment projection.

Supports filtering by status, assignee, and ready frontier. Uses claim
ordering (priority ASC, created_at ASC, id ASC) for deterministic results.
Ready frontier uses the same ordering as 'bead claim' but is read-only and
does not reserve work.

EXAMPLES:
  bead list --json --limit 10                      # First 10 issues as JSON
  bead list --status open --assignee alice        # Open issues assigned to alice
  bead list --ready --limit 5                      # Next 5 ready candidates
  bead list --comments unresolved --json --limit 20  # Issues with unresolved comments

FILTERS:
  --status VALUE    Filter by base status: open, in_progress, deferred, closed
  --assignee NAME   Filter by assignee (exact match)
  --ready           Show only ready frontier issues (open, unassigned, not blocked)
  --limit N         Maximum results (0-999999, default: 100)

COMMENT PROJECTION:
  --comments none         Show only counts and resolution metadata (default)
  --comments unresolved   Include bodies for unresolved comments only
  --comments all          Include all comment bodies in canonical order

OUTPUT:
  Without --json: human-readable table format
  With --json: one compact JSON object per line (compact JSONL format)
  JSON output includes: id, title, priority, status, assignee, dependencies,
  created_at, updated_at, and labels based on comment projection."
)]
pub struct ListOptions {
    /// Output in JSON format
    #[arg(long)]
    pub json: bool,

    /// Filter by status
    #[arg(long)]
    pub status: Option<String>,

    /// Filter by assignee
    #[arg(long)]
    pub assignee: Option<String>,

    /// Show only ready frontier issues
    #[arg(long)]
    pub ready: bool,

    /// Comment projection: none, unresolved, or all (default: none)
    #[arg(long, default_value = "none")]
    pub comments: String,

    /// Maximum number of issues to return (0-999999)
    #[arg(long, default_value = "100")]
    pub limit: i64,
}

/// Options for showing a single issue
#[derive(Parser, Debug)]
#[command(
    about = "Show a single issue",
    long_about = "Display detailed information about a single issue.

Shows complete issue details including dependencies, labels, and optionally
comments. Output format depends on --json flag.

EXAMPLES:
  bead show bead-123abc456789def                    # Human-readable output
  bead show ID --json                                # JSON output for NEEDLE
  bead show ID --comments unresolved                 # With unresolved comments
  bead show ID --comments all --json                 # Full issue with all comments

OUTPUT FORMAT:
  Without --json: Human-readable detailed view
  With --json: One-element JSON array for NEEDLE v1 compatibility

COMMENT PROJECTION:
  --comments none         Show only counts and resolution metadata (default)
  --comments unresolved   Include bodies for unresolved comments only
  --comments all          Include all comment bodies in canonical order

NEEDLE COMPATIBILITY:
  JSON output format is a one-element array containing the issue object.
  This matches NEEDLE v1 expectations for subprocess output.

ISSUE DETAILS INCLUDE:
  - Basic fields: id, title, description, priority, status
  - Assignment: assignee (if any)
  - Timestamps: created_at, updated_at, closed_at (if closed)
  - Dependencies: blocked_by (blocking issues), blocking (issues this blocks)
  - Labels: all assigned labels
  - Comments: based on --comments projection
  - Metadata: issue_type, manual_blocked flag, close_reason (if closed)"
)]
pub struct ShowOptions {
    /// Issue ID
    pub id: String,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,

    /// Comment projection: none, unresolved, or all (default: none)
    #[arg(long, default_value = "none")]
    pub comments: String,
}

/// Options for claiming an issue
#[derive(Parser, Debug)]
#[command(
    about = "Claim an issue from the ready frontier",
    long_about = "Atomically claim and assign one issue from the ready frontier.

Claim performs server-side selection from ready issues (open, unassigned,
not manually blocked, no unfinished blockers) using fifo-v1 policy:
priority ASC, created_at ASC, id ASC.

Selection and assignment occur in one atomic transaction. With no eligible
issues, returns exit code 0 and an empty result ({} in JSON mode).

Twenty competing claimants must never receive the same successful issue ID.

EXAMPLES:
  bead claim --assignee alice --json          # Claim with JSON output
  bead claim --assignee \"Team Backend\"       # Claim with team assignee
  bead claim --assignee worker-1              # Claim for automated worker

READY FRONTIER:
  Issues are ready when: base status is open, no assignee, not manually blocked,
  and no unfinished 'blocks' dependency edges exist.

  Use 'bead list --ready --json --limit N' to inspect ready candidates without
  reserving them. List uses the same ordering as claim but is read-only.

LEASED CLAIMS:
  --lease-ttl SECONDS enables opt-in leased claims with fencing tokens.
  Leased claims expire after the specified seconds, preventing stale workers
  from mutating work after expiry. Each lease has a monotonically increasing
  fencing token for safe recovery from crashed or disconnected agents.

  --renew-lease renews an existing lease instead of claiming new work.
  This requires the issue to already have an active lease for the assignee.

  --fencing-token explicitly specifies a fencing token for advanced use cases.
  Normally fencing tokens are auto-generated and incremented on each claim/renewal.

  Standard non-leased claims remain the default and maintain backward compatibility.
  Leased claims add safety for distributed fleets and crash recovery scenarios.

SINGLE-CLAIM GUARD:
  --single-claim refuses the claim when this assignee already holds an
  in_progress issue in this workspace. The refusal fails with exit code 4
  and the machine-readable reason code 'assignee_has_active_claim', naming
  the blocking issue ID. Without the flag, an assignee may hold any number
  of simultaneous claims (default unchanged).

  The guard checks this workspace's own store only, inside the same atomic
  transaction as selection and assignment. It bounds claim accumulation
  (one in_progress issue per assignee per workspace); it does not detect or
  reap stale claims -- combine with --lease-ttl to bound how long a stale
  claim can persist.

  Lease renewal (--renew-lease) is not guarded: it operates on an issue the
  assignee already holds. Assigning via 'bead update --assignee' is also
  unaffected.

CLAIM SEMANTICS:
  - Atomic: selection, assignment, and audit record are one transaction
  - Deterministic: same state always produces same result (fifo-v1)
  - Safe: concurrent claimants never receive duplicate successful IDs
  - Empty queue: returns {} with exit 0, not an error

LEASE EXPIRY:
  Once a lease expires, the assignee cannot update, release, or close the issue
  until the lease is renewed or a new claim is made. This prevents stale workers
  from corrupting work that has been reassigned to other agents."
)]
pub struct ClaimOptions {
    /// Assignee name (required)
    #[arg(long)]
    pub assignee: String,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,

    /// Explain the claim decision with a machine-readable trace
    #[arg(long)]
    pub why: bool,

    /// Scheduling policy for claim selection (fifo-v1, aging-v1, impact-v1, rotation-v1, balanced-v1)
    #[arg(long, default_value = "fifo-v1")]
    pub policy: String,

    /// Request a leased claim with time-to-live in seconds (optional)
    #[arg(long)]
    pub lease_ttl: Option<u64>,

    /// Renew an existing lease instead of claiming new work
    #[arg(long)]
    pub renew_lease: bool,

    /// Explicit fencing token for lease validation (advanced usage)
    #[arg(long)]
    pub fencing_token: Option<i64>,

    /// Refuse the claim if this assignee already holds an in_progress issue
    /// in this workspace (opt-in guard; fails with reason code
    /// assignee_has_active_claim and exit code 4)
    #[arg(long)]
    pub single_claim: bool,
}

/// Options for updating an issue
#[derive(Parser, Debug)]
#[command(
    about = "Update an issue",
    long_about = "Update issue fields atomically.

Updates one or more issue fields in a single atomic transaction.
All validations and mutations occur together; any failure leaves the
issue unchanged. Changing status to 'closed' requires the 'close' command
instead. Use 'reopen' to transition from closed to open.

EXAMPLES:
  bead update ID --status in_progress                # Start working on an issue
  bead update ID --assignee alice                    # Assign to alice
  bead update ID --notes \"Investigated root cause\"   # Add investigation notes
  bead update ID --clear-assignee                    # Clear assignment (open only)
  bead update ID --status in_progress --if-revision 3 # With revision guard

STATUS TRANSITIONS:
  Valid transitions depend on the current base status:
  - open can transition to: in_progress, deferred
  - in_progress can transition to: open, deferred, closed (via 'close' command)
  - deferred can transition to: open, closed (via 'close' command)
  - closed can only transition to: open (via 'reopen' command)

FIELDS THIS COMMAND DOES NOT CHANGE:
  Title, description, priority, issue type, and labels are not editable here.
  Set them at creation time with 'bead create', and use 'bead label add' or
  'bead label remove' for labels. Passing --priority or --title is a usage error.

ASSIGNMENT:
  --assignee and --clear-assignee are mutually exclusive.
  --clear-assignee only works for open assigned issues.
  For in_progress issues, use 'bead release' to return to open/unassigned.

BLOCKED STATUS:
  Setting --status blocked sets manual_blocked=true and retains base status.
  Setting --status open clears manual blocking and sets base status to open.
  Closed issues cannot use --status open (use 'reopen' instead).

REVISION GUARDS:
  --if-revision N provides optimistic concurrency control.
  If the issue's current revision doesn't match N, the update fails with
  exit code 4 and a conflict message. This prevents silent lost updates
  when multiple agents or humans modify the same issue concurrently.

All updates are atomic. Invalid transitions or conflicts exit with code 4
without changing any fields or timestamps."
)]
pub struct UpdateOptions {
    /// Issue ID
    pub id: String,

    /// New status
    #[arg(long)]
    pub status: Option<String>,

    /// New assignee
    #[arg(long)]
    pub assignee: Option<String>,

    /// Clear assignee (only for open assigned issues)
    #[arg(long)]
    pub clear_assignee: bool,

    /// New notes
    #[arg(long)]
    pub notes: Option<String>,

    /// Expected revision for optimistic concurrency control
    #[arg(long)]
    pub if_revision: Option<i64>,

    /// Fencing token for lease validation (advanced usage)
    #[arg(long)]
    pub fencing_token: Option<i64>,

    /// Dry run: show what would happen without making changes
    #[arg(long)]
    pub dry_run: bool,
}

/// Options for releasing an issue
#[derive(Parser, Debug)]
#[command(
    about = "Release a claimed issue",
    long_about = "Atomically release a claimed issue back to open/unassigned status.

The release command returns an in_progress issue to open and unassigned state.
This is the proper way to stop working on an issue without closing it.
For open assigned issues, use 'update --clear-assignee' instead.

EXAMPLES:
  bead release bead-123abc456789def    # Release claimed issue
  bead release ID                       # Release by issue ID
  bead release ID --if-revision 4       # With revision guard

SEMANTICS:
  - in_progress → open, unassigned (semantic release)
  - open/unassigned → no-op, idempotent
  - Other states → conflict (exit 4)

REVISION GUARDS:
  --if-revision N provides optimistic concurrency control.
  If the issue's current revision doesn't match N, the release fails with
  exit code 4 and a conflict message. This prevents silent lost updates
  when multiple agents or humans modify the same issue concurrently.

Release is atomic with proper audit event recording. The prior assignee
and resulting state are recorded in the 'released' audit event.
The issue's updated_at timestamp advances on semantic release.

Use release when:
  - You need to return a claimed issue to the ready frontier
  - Work was started but won't continue (not completed)
  - You want another agent/worker to be able to claim the issue

Use close when:
  - The work is complete and verified
  - The issue should be removed from the active frontier
  - Dependent issues can now become unblocked"
)]
pub struct ReleaseOptions {
    /// Issue ID
    pub id: String,

    /// Expected revision for optimistic concurrency control
    #[arg(long)]
    pub if_revision: Option<i64>,

    /// Fencing token for lease validation (advanced usage)
    #[arg(long)]
    pub fencing_token: Option<i64>,

    /// Dry run: show what would happen without making changes
    #[arg(long)]
    pub dry_run: bool,
}

/// Options for closing an issue
#[derive(Parser, Debug)]
#[command(
    about = "Close an issue",
    long_about = "Close an issue with a required reason.

Close transitions an issue to closed status and records the close reason.
The reason must be non-empty and is preserved for audit and debugging.
Closing clears manual blocking and may expose dependent issues.

EXAMPLES:
  bead close bead-123abc456789def --reason \"Completed successfully\"
  bead close ID --reason \"Fixed authentication bug\"
  bead close ID --reason \"Duplicate of ID-456\"
  bead close ID --reason \"Done\" --if-revision 3    # With revision guard

SEMANTICS:
  - All non-closed states → closed (semantic close)
  - closed with matching reason → no-op, idempotent
  - closed with different reason → conflict (exit 4)

REQUIREMENTS:
  --reason TEXT must be non-empty after trimming whitespace

EFFECTS:
  - Sets base_status to closed
  - Clears manual_blocked flag
  - Sets closed_at to current time
  - Stores close_reason
  - Advances updated_at
  - Increments revision
  - Appends 'closed' audit event

REVISION GUARDS:
  --if-revision N provides optimistic concurrency control.
  If the issue's current revision doesn't match N, the close fails with
  exit code 4 and a conflict message. This prevents silent lost updates
  when multiple agents or humans modify the same issue concurrently.

IDEMPOTENCY:
  Repeating the same close command with the same reason is idempotent
  and does not change timestamps or append duplicate events.
  Changing the reason is a conflict (use update with new reason if needed).

Dependent issues may become ready when their last blocker is closed."
)]
pub struct CloseOptions {
    /// Issue ID
    pub id: String,

    /// Close reason (required)
    #[arg(long)]
    pub reason: String,

    /// Expected revision for optimistic concurrency control
    #[arg(long)]
    pub if_revision: Option<i64>,

    /// Fencing token for lease validation (advanced usage)
    #[arg(long)]
    pub fencing_token: Option<i64>,

    /// Dry run: show what would happen without making changes
    #[arg(long)]
    pub dry_run: bool,
}

/// Options for reopening an issue
#[derive(Parser, Debug)]
#[command(
    about = "Reopen a closed issue",
    long_about = "Restore a closed issue to open lifecycle status.

Reopen transitions a closed issue back to open status while preserving
its assignment and other metadata. This is the only valid way to cross
from closed to open status (generic update cannot do this).

EXAMPLES:
  bead reopen bead-123abc456789def     # Reopen closed issue
  bead reopen ID                        # Reopen by issue ID
  bead reopen ID --if-revision 5        # With revision guard

SEMANTICS:
  - closed → open (semantic reopen)
  - open → no-op, idempotent
  - in_progress/deferred → conflict (exit 4)

EFFECTS:
  - Sets base_status to open
  - Clears closed_at and close_reason
  - Clears manual_blocked flag
  - Preserves existing assignee
  - Advances updated_at
  - Increments revision
  - Appends 'reopened' audit event

REVISION GUARDS:
  --if-revision N provides optimistic concurrency control.
  If the issue's current revision doesn't match N, the reopen fails with
  exit code 4 and a conflict message. This prevents silent lost updates
  when multiple agents or humans modify the same issue concurrently.

IDEMPOTENCY:
  Repeating reopen on an open issue succeeds without changing timestamps
  or appending duplicate events.

USE CASES:
  - Issue was closed prematurely and needs more work
  - New information suggests the issue should be revisited
  - Closed issue was determined to not be actually complete

For unassigned closed issues, reopen makes them ready frontier candidates
(if they have no unfinished blockers)."
)]
pub struct ReopenOptions {
    /// Issue ID
    pub id: String,

    /// Expected revision for optimistic concurrency control
    #[arg(long)]
    pub if_revision: Option<i64>,

    /// Fencing token for lease validation (advanced usage)
    #[arg(long)]
    pub fencing_token: Option<i64>,

    /// Dry run: show what would happen without making changes
    #[arg(long)]
    pub dry_run: bool,
}

/// Sync commands
#[derive(Subcommand, Debug)]
pub enum SyncCommand {
    /// Flush checkpoint to JSONL file
    #[command(
        name = "flush-only",
        about = "Flush checkpoint to JSONL file",
        long_about = "Atomically publish a checkpoint from the current database state.

Captures one committed snapshot and writes deterministically ordered JSONL.
The operation is crash-safe: it writes to a temporary file, verifies it, then
atomically renames. Checkpoint state is updated only after a successful flush.

EXAMPLES:
  bead sync flush-only                                  # Flush the workspace checkpoint
  bead sync flush-only --output /path/to/backup.jsonl   # Export a copy elsewhere

OUTPUT:
  Without --output, writes the forensic checkpoint set into .beads/checkpoint/:
    current.json            pointer to the active generation, plus counts
    forensic.jsonl          or objects/ + manifests/ when sharded
  The forensic checkpoint carries issues, events, provenance receipts, and the
  dependency and label graph. This is the directory Git should track.

  With --output, exports an issue-only JSONL copy to the given path. The path
  must not already exist and must be outside .beads/. This is a side export; it
  does not update the workspace checkpoint or its freshness state.

CHECKPOINT FRESHNESS:
  A checkpoint represents the database as of flush time. Any mutation afterwards
  makes it stale, and nothing flushes implicitly. Flush before committing, and
  periodically during long sessions -- otherwise a fresh clone of the repository
  reproduces the last flushed state, not the current one.

ATOMICITY:
  - A read transaction captures the snapshot
  - The temporary file is written and verified
  - An atomic rename replaces the previous checkpoint
  - Checkpoint state advances in the same transaction

GIT INTEGRATION:
  Run 'bead sync flush-only' before committing the repository. bead-rs never
  runs Git commands itself. Restore a fresh clone with
  'bead init' followed by 'bead sync import-only --restore-into-empty'."
    )]
    FlushOnly(SyncFlushOptions),

    /// Import forensic checkpoint with restore or merge
    #[command(
        name = "import-only",
        about = "Import forensic checkpoint with restore or merge",
        long_about = "Import and validate forensic checkpoint with atomic activation.

Stages and validates complete forensic checkpoint before any state mutation.
Supports both monolithic and sharded checkpoint-set formats with full validation.

MODES:
  Exactly one of --restore-into-empty or --merge must be specified.

  --restore-into-empty: Restore into empty initialized workspace
    - Target must be newly initialized with no semantic mutations
    - Adopts checkpoint store UUID and event sequence
    - Validates all records: issues, events, receipts, hashes, counts
    - Replays events and verifies resulting state matches checkpoint
    - Creates durable 'restore' provenance receipt
    - Atomically activates in one transaction

  --merge: Merge checkpoint into existing workspace
    - Same-UUID: extends local history with compatible checkpoint events
    - Different-UUID: merges foreign checkpoint with conflict detection
    - Validates event identities, hashes, and continuity
    - Handles conflicts via timestamp comparison with rollback
    - Creates durable 'merge' provenance receipt
    - Never deletes native issues absent from checkpoint

EXAMPLES:
  bead sync import-only --input checkpoint/ --restore-into-empty --actor admin
  bead sync import-only --input backup.jsonl --merge --actor admin --dry-run
  bead sync import-only --input .beads/checkpoint --merge --actor system

VALIDATION PERFORMED:
  - Record type validation (issue, event, provenance_receipt)
  - Canonical ordering verification
  - Hash and count validation
  - UUID continuity checks
  - Event replay verification (restore)
  - Conflict detection (merge)
  - Graph validation (cycles, dangling references)

DRY RUN:
  With --dry-run: performs complete validation and reconciliation without
  activating state. Reports prospective counts, conflicts, and receipt preview.

PROVENANCE:
  Both operations create immutable receipts stored in database and exported
  in subsequent checkpoints. Receipts record: operation kind, source/target
  UUIDs, source root hash, actor, counts, and result.

EXIT CODES:
  0 - Successful operation (or successful dry-run)
  2 - CLI usage or validation error
  3 - Workspace or file not found
  4 - Reconciliation conflict
  5 - Malformed input or integrity failure

Use --dry-run to validate checkpoints before risking database mutation."
    )]
    ImportOnly(SyncImportOptions),

    /// Report checkpoint status and readiness to commit
    #[command(
        name = "status",
        about = "Report checkpoint status and readiness to commit",
        long_about = "Report checkpoint freshness, verification, and readiness to commit.

Reads the authoritative current.json pointer, verifies the root object it
selects, checks the forensic.jsonl compatibility view (monolithic mode),
reapplies no changes, and lists any pointer-declared tombstones that are
still unresolved on disk.

READINESS:
  ready_to_commit holds only when every check passes:
  - the pointer's root object exists and hashes to the declared SHA-256
  - the checkpoint covers the live event sequence (not dirty)
  - no pointer-declared tombstone remains on disk
  - the forensic.jsonl view is byte-identical to the root object
  - the recorded checkpoint state agrees with the pointer

  Repository automation must treat a not-ready checkpoint as a failed
  pre-commit gate: run `bead sync flush-only` and include every reported
  changed path in the same Git commit.

EXAMPLES:
  bead sync status                     # Human-readable summary
  bead sync status --format json       # Machine-readable status

OUTPUT:
  --format json prints one JSON object with checkpoint_present, mode,
  generation_id, live_sequence, covered_sequence, dirty, root_path,
  root_hash, root_verified, view_agrees, unresolved_tombstones,
  changed_paths, ready_to_commit, and not_ready_reasons."
    )]
    Status(SyncStatusOptions),
}

/// Options for flushing checkpoint
#[derive(Parser, Debug)]
pub struct SyncFlushOptions {
    /// Export an issue-only copy to this path instead of only updating .beads/checkpoint/
    #[arg(long)]
    pub output: Option<String>,

    /// Profile for checkpoint format (not supported for issue-only export)
    #[arg(long)]
    pub profile: Option<String>,
}

/// Import operation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    RestoreIntoEmpty,
    Merge,
}

impl ImportMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImportMode::RestoreIntoEmpty => "restore-into-empty",
            ImportMode::Merge => "merge",
        }
    }
}

/// Options for importing checkpoint
#[derive(Parser, Debug)]
pub struct SyncImportOptions {
    /// Input file or directory path
    #[arg(long)]
    pub input: String,

    /// Profile for import (default: native-v1)
    #[arg(long, default_value = "native-v1")]
    pub profile: String,

    /// Restore checkpoint into empty workspace
    #[arg(long, conflicts_with = "merge")]
    pub restore_into_empty: bool,

    /// Merge checkpoint into existing workspace
    #[arg(long, conflicts_with = "restore_into_empty")]
    pub merge: bool,

    /// Actor performing the import operation (required for restore/merge)
    #[arg(long)]
    pub actor: Option<String>,

    /// Perform dry-run without activating state
    #[arg(long)]
    pub dry_run: bool,
}

/// Options for checkpoint status
#[derive(Parser, Debug)]
pub struct SyncStatusOptions {
    /// Output format: text or json
    #[arg(long, default_value = "text")]
    pub format: String,
}

/// Label management commands
#[derive(Subcommand, Debug)]
pub enum LabelCommand {
    /// Add a label to an issue
    #[command(
        about = "Add a label to an issue",
        long_about = "Add a label to an issue (idempotent).

Adds the specified label to the issue. If the label already exists,
the command succeeds without making changes. Labels are optional
metadata used for categorization and filtering.

EXAMPLES:
  bead label add bead-123abc456789def --label bug
  bead label add ID --label \"needs-review\"

LABELS:
  - Any non-empty string is valid
  - Labels are case-sensitive
  - Adding an existing label is idempotent (no-op)
  - One label per invocation; run the command again for a second label
    (`bead create` accepts a repeatable --label at creation time)

Common labels: bug, feature, improvement, documentation, urgent,
help-wanted, work-in-progress, needs-review, blocked, etc."
    )]
    Add(LabelAddOptions),

    /// Remove a label from an issue
    #[command(
        about = "Remove a label from an issue",
        long_about = "Remove a label from an issue (idempotent).

Removes the specified label from the issue. If the label does not exist,
the command succeeds without making changes.

EXAMPLES:
  bead label remove bead-123abc456789def --label bug
  bead label remove ID --label urgent

IDEMPOTENCY:
  Removing a non-existent label succeeds without error.
  This makes label management safe and declarative."
    )]
    Remove(LabelRemoveOptions),
}

/// Options for adding a label
#[derive(Parser, Debug)]
pub struct LabelAddOptions {
    /// Issue ID
    pub id: String,
    /// Label to add
    #[arg(long)]
    pub label: String,
}

/// Options for removing a label
#[derive(Parser, Debug)]
pub struct LabelRemoveOptions {
    /// Issue ID
    pub id: String,
    /// Label to remove
    #[arg(long)]
    pub label: String,
}

/// Dependency management commands
#[derive(Subcommand, Debug)]
pub enum DepCommand {
    /// Add a dependency edge
    #[command(
        about = "Add a dependency edge",
        long_about = "Add a dependency relationship between two issues.

Creates a directional dependency edge from blocked issue to blocker issue.
The 'blocks' kind affects readiness; 'relates_to' does not affect readiness
but allows tracking related work.

EXAMPLES:
  bead dep add BLOCKED BLOCKER                       # Add blocks dependency
  bead dep add task-1 task-2 --kind blocks            # Explicit blocks
  bead dep add feature-a bug-fix --kind relates_to     # Non-blocking relationship

DEPENDENCY KINDS:
  - blocks: BLOCKED is blocked until BLOCKER is closed (affects readiness)
  - relates_to: Related issues without blocking semantics (cycles allowed)

CYCLE DETECTION:
  'blocks' dependencies cannot create cycles.
  Adding an edge that creates a directed cycle will fail with exit code 4.
  'relates_to' edges can form cycles (no restriction).

READINESS IMPACT:
  Only 'blocks' dependencies affect ready frontier:
  - Issue is ready when: open, unassigned, not manually blocked, no unfinished blockers
  - Blocker is unfinished when: not in closed state
  - Finishing a blocker may expose dependent issues to ready frontier

IDEMPOTENCY:
  Adding an existing dependency succeeds without error.
  Self-edges (blocked == blocker) are rejected (exit code 4).

ORIENTATION:
  Syntax is: bead dep add <BLOCKED> <BLOCKER>
  BLOCKED depends on BLOCKER completing first
  BLOCKER must finish before BLOCKED can be ready"
    )]
    Add(DepAddOptions),

    /// Remove a dependency edge
    #[command(
        about = "Remove a dependency edge",
        long_about = "Remove a dependency relationship between two issues.

Removes dependency edge(s) matching the given blocked, blocker, and kind.
Without --kind, removes all dependency kinds between the two issues.

EXAMPLES:
  bead dep remove BLOCKED BLOCKER                    # Remove all dependencies
  bead dep remove task-1 task-2 --kind blocks        # Remove specific kind
  bead dep remove feature-a bug-fix --kind relates_to # Remove relates_to edge

IDEMPOTENCY:
  Removing a non-existent dependency succeeds without error.
  Removing multiple edges succeeds as long as at least one matched.

USE CASES:
  - Dependency was determined to be incorrect
  - Work relationship changed
  - Issue relationship is being tracked differently
  - Cleaning up after issue completion"
    )]
    Remove(DepRemoveOptions),
}

/// Options for adding a dependency
#[derive(Parser, Debug)]
pub struct DepAddOptions {
    /// Blocked issue ID
    pub blocked: String,
    /// Blocker issue ID
    pub blocker: String,
    /// Dependency kind (default: blocks)
    #[arg(long, default_value = "blocks")]
    pub kind: String,
    /// Conditional dependency expression as JSON (optional)
    #[arg(long)]
    pub condition: Option<String>,
    /// Dry run: show what would happen without making changes
    #[arg(long)]
    pub dry_run: bool,
}

/// Options for removing a dependency
#[derive(Parser, Debug)]
pub struct DepRemoveOptions {
    /// Blocked issue ID
    pub blocked: String,
    /// Blocker issue ID
    pub blocker: String,
    /// Dependency kind (optional, removes all kinds if not specified)
    #[arg(long)]
    pub kind: Option<String>,
    /// Dry run: show what would happen without making changes
    #[arg(long)]
    pub dry_run: bool,
}

/// Options for doctor command
#[derive(Parser, Debug)]
#[command(
    about = "Diagnose workspace integrity and optionally perform repairs",
    long_about = "Perform read-only integrity checks on the workspace.

Doctor validates workspace configuration, database integrity, checkpoint
state, and filesystem without modifying data. With --repair, performs
safe automatic repairs for diagnosed issues. With --rehearse, performs
a disposable recovery rehearsal to verify disaster recovery procedures.

EXAMPLES:
  bead doctor                           # Read-only diagnostics
  bead doctor --repair                  # Diagnose and attempt repairs
  bead doctor --rehearse                # Test disaster recovery with temporary workspace

CHECKS PERFORMED:
  - Workspace configuration and permissions
  - Database integrity (SQLite PRAGMA checks)
  - Schema version and migration checksums
  - Checkpoint state and file consistency
  - Lifecycle and timestamp invariants
  - Dependency graph (no cycles or dangling references)
  - Orphaned temporary files

REPAIRS PERFORMED (with --repair):
  - Remove proven-stale operation-owned temporary files
  - Rebuild checkpoint views from authoritative database state
  - Create missing safe indexes
  - Repair checkpoint state through atomic flush

RECOVERY REHEARSAL (with --rehearse):
  - Create temporary workspace from current checkpoint
  - Run full diagnostics on temporary workspace
  - Re-export checkpoint from temporary workspace
  - Compare semantic equivalence between original and re-exported content
  - Generate comprehensive report with comparison results
  - Clean up only operation-owned temporary files

DOCTOR OUTPUT:
  OK  - Check passed
  WARN - Non-critical issue detected
  FIXED - Issue was repaired (with --repair)

EXIT CODES:
  0 - All checks passed (or repairs successful)
  1 - Internal failure
  3 - Workspace not found or inaccessible
  5 - Integrity failure detected

Doctor never rewrites user data, drops tables, or performs speculative repairs.
For serious integrity issues, recommendations are provided for explicit recovery."
)]
pub struct DoctorOptions {
    /// Attempt automatic repairs
    #[arg(long)]
    pub repair: bool,

    /// Run disposable recovery rehearsal: create temp workspace, run diagnostics, re-export, compare semantic equivalence
    #[arg(long)]
    pub rehearse: bool,

    /// Diagnostic scopes: store, backup, schema, dependencies, comments, all (default: all)
    #[arg(long, value_delimiter = ',')]
    pub scope: Option<Vec<String>>,

    /// Output diagnostics in JSON format
    #[arg(long)]
    pub json: bool,
}

/// Options for capabilities command
#[derive(Parser, Debug)]
#[command(
    about = "Show capabilities and supported features",
    long_about = "Display versioned capabilities and supported feature sets.

Outputs machine-readable capabilities document describing contract version,
implementation, store layout, supported commands, priorities, statuses,
checkpoint modes and formats, and schema catalog.

EXAMPLES:
  bead capabilities                            # Native capabilities as JSON
  bead capabilities --profile needle-v1        # NEEDLE v1 contract
  bead capabilities --profile native-v1        # Explicit native profile

Output is always JSON on stdout.

CAPABILITY INFORMATION:
  - Contract version and implementation identifier
  - Store layout version
  - Atomic claim support
  - Priority range and P4 claimability
  - Supported statuses and transitions
  - Checkpoint modes and formats
  - Schema catalog with validation and operation support
  - Complete command inventory
  - auto_flush: present only once this binary publishes a checkpoint
    generation after every successful semantic mutation

AUTO_FLUSH:
  The additive auto_flush field reports the compiled default, not
  workspace state: a workspace that disables publication through
  checkpoint.auto_flush and an invocation passing --no-auto-flush
  change what the binary does, never what it advertises. The field is
  absent until the compiled default is enabled. Consumers that require
  a current checkpoint must still read `bead sync --status`, which
  remains the only authority on whether this workspace is clean.

PROFILES:
  - native-v1: Full native capabilities (default)
  - needle-v1: NEEDLE subprocess compatibility contract

SCHEMA CATALOG:
  Each schema entry shows:
  - schema_ref: Immutable schema identifier
  - document_kind: Type of document
  - validate: Schema available for 'bead schema show'
  - consume: Operations accepting this document type
  - emit: Operations producing this document type

Use this command for capability negotiation and feature detection."
)]
pub struct CapabilitiesOptions {
    /// Profile for capabilities output
    #[arg(long, default_value = "native-v1")]
    pub profile: String,
}

#[derive(Subcommand, Debug)]
pub enum SchemaCommand {
    #[command(
        about = "List public schemas",
        long_about = "List every supported public document schema as deterministic JSON.\n\nEntries are sorted by exact schema identity and include document kind, readability, writability, validation support, and consuming or emitting operations."
    )]
    List(SchemaListOptions),
    #[command(
        about = "Show a public JSON Schema",
        long_about = "Emit the immutable JSON Schema Draft 2020-12 document for an exact catalog identity.\n\nSchema resolution is workspace-independent. Unknown identities are usage errors."
    )]
    Show(SchemaShowOptions),
    #[command(
        about = "Explain a public schema",
        long_about = "Explain an exact public schema identity as deterministic typed JSON or Markdown.\n\nThe explanation describes ownership, transport, supported operations, and the schema's public members."
    )]
    Explain(SchemaExplainOptions),
}

#[derive(Parser, Debug)]
pub struct SchemaListOptions {
    /// Output format
    #[arg(long, value_parser = ["json"], default_value = "json")]
    pub format: String,
}

#[derive(Parser, Debug)]
pub struct SchemaShowOptions {
    /// Exact schema identity
    pub schema_ref: String,

    /// Output format
    #[arg(long, value_parser = ["json"], default_value = "json")]
    pub format: String,
}

#[derive(Parser, Debug)]
pub struct SchemaExplainOptions {
    /// Exact schema identity
    pub schema_ref: String,

    /// Output format
    #[arg(long, value_parser = ["json", "markdown"], default_value = "json")]
    pub format: String,
}

/// Options for query language execution
#[derive(Parser, Debug)]
#[command(
    about = "Execute safe query language queries",
    long_about = "Execute queries using the safe query language (R004).

Queries are specified as JSON files with predicates, sorting, and projection.
This provides a powerful, type-safe alternative to shell filtering while
never exposing raw SQL or private schema details.

QUERY LANGUAGE FEATURES:
  - Predicates: field comparisons (equals, contains, greater than, etc.)
  - Sorting: deterministic multi-field ordering
  - Projection: select specific fields to return
  - Versioned grammar: ensures forward compatibility
  - Safe execution: only whitelisted fields and operators

SUPPORTED FIELDS:
  - id, title, description, notes, priority (0-4)
  - status (open, in_progress, deferred, closed)
  - blocked, assignee, type, created, updated, closed, close_reason

EXAMPLES:
  bead query --file open_high.json                     # Query from a file
  bead query --json '{\"version\":\"v1\"}'                 # Inline query text
  bead query --file alice_work.json --output-json      # Machine-readable results
  bead query --file open_high.json --save-as hot       # Save as a named view
  bead query --view hot --output-json                  # Run a saved view
  bead query --list-views                              # List saved views

Note: --json supplies the query document itself. Use --output-json to render
results as JSON.

QUERY FORMAT:
  {
    \"version\": \"v1\",
    \"predicates\": [
      {\"field\": \"status\", \"operator\": \"=\", \"value\": \"open\"},
      {\"field\": \"priority\", \"operator\": \">=\", \"value\": 2}
    ],
    \"sort\": [
      {\"field\": \"priority\", \"direction\": \"asc\"}
    ],
    \"limit\": 100
  }

OPERATORS:
  - =, !=, >, <, >=, <=  (numeric and string comparison)
  - contains, starts_with, ends_with  (string matching)
  - is_null, not_null  (null checking)

The query command never exposes raw SQL or internal schema details.
All field names are validated against a public whitelist."
)]
pub struct QueryOptions {
    /// Query specification file (JSON format)
    #[arg(long)]
    pub file: Option<String>,

    /// Query specification as inline JSON
    #[arg(long)]
    pub json: Option<String>,

    /// Output results in JSON format
    #[arg(long)]
    pub output_json: bool,

    /// Save query as a named view
    #[arg(long)]
    pub save_as: Option<String>,

    /// List all saved views
    #[arg(long)]
    pub list_views: bool,

    /// Execute a saved view
    #[arg(long)]
    pub view: Option<String>,

    /// Delete a saved view
    #[arg(long)]
    pub delete_view: Option<String>,
}

/// Options for the changes command
#[derive(Parser, Debug)]
#[command(
    about = "Access cursor-based change feed for incremental local synchronization",
    long_about = "Read the append-only change feed so a consumer can catch up incrementally.

Each mutation appends an event with a monotonically increasing sequence number.
A cursor records how far a consumer has read, so it can resume without rescanning
the whole workspace. Cursors are only meaningful against the snapshot identity
that issued them; if the workspace is restored from a checkpoint the identity
changes and old cursors must be revalidated.

EXAMPLES:
  bead changes --latest                  # Current cursor position
  bead changes --since 42 --json         # Events after sequence 42
  bead changes --snapshot                # Snapshot identity of this workspace
  bead changes --validate <CURSOR>       # Check a cursor for gaps

Exactly one of --latest, --since, --snapshot, or --validate is expected.
This feed is local to the workspace; bead-rs performs no network access."
)]
pub struct ChangesOptions {
    /// Get changes since this cursor position (sequence number or cursor string)
    #[arg(long)]
    pub since: Option<String>,

    /// Get the latest cursor position for tracking
    #[arg(long)]
    pub latest: bool,

    /// Get the current snapshot identity
    #[arg(long)]
    pub snapshot: bool,

    /// Validate a cursor and check for gaps
    #[arg(long)]
    pub validate: Option<String>,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

/// Options for the why explanation command (R023)
#[derive(Parser, Debug)]
#[command(
    about = "Explain issue state, readiness, blockers, and legal operations",
    long_about = "Provide comprehensive explanation of why an issue is in its current state,
what operations are legal, and what factors affect its claim ranking.

This command gives humans and agents one entry point for understanding issue state,
readiness, active blockers, claim-ranking factors, and legal next operations. It calls
the same domain evaluators and reason codes used by R001 (decision traces) and
R019 (intelligent scheduling) to ensure consistency across all diagnostic interfaces.

EXAMPLES:
  bead why --id bead-123abc456789def           # Human-readable explanation
  bead why --id bead-123abc456789def --json    # Machine-readable JSON output

EXPLANATION COVERAGE:
  - Effective vs base status (blocked vs open/in_progress/closed)
  - Ready frontier analysis (unassigned, no manual block, no active blockers)
  - Active blocker analysis with blocker details
  - Claim ranking factors (priority, age, rotation, attempt tier, graph impact)
  - Legal operations list with validity checks and command examples
  - Reuse of R001 reason codes and R019 scheduling metrics

OUTPUT FORMAT:
  Human-readable: Multi-section text with clear headers and actionable insights
  JSON (--json): Structured WhyExplanation with all analysis components

The command fails with exit code 3 if the issue is not found."
)]
pub struct WhyOptions {
    /// Issue ID to explain
    #[arg(long)]
    pub id: String,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

/// Cross-profile semantic comparison (R020)
#[derive(clap::Args, Debug)]
#[command(
    about = "Compare issue representation across two profiles",
    long_about = "Compare how an issue is represented in two different profiles.
Reports preserved, transformed, omitted, and unsupported semantic fields.
This is a read-only operation that never writes to the database.

EXAMPLES:
  bead compare --id bead-1234567890abcdef --source native-v1 --target needle-v1
  bead compare --id bead-1234567890abcdef --source needle-v1 --target native-v1 --json

The command fails with exit code 3 if the issue is not found,
or exit code 2 if either profile is not supported."
)]
pub struct CompareOptions {
    /// Issue ID to compare
    #[arg(long)]
    pub id: String,

    /// Source profile name
    #[arg(long)]
    pub source: String,

    /// Target profile name
    #[arg(long)]
    pub target: String,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

/// External reference management commands (R011)
#[derive(Subcommand, Debug)]
pub enum RefCommand {
    /// Add an external reference to an issue
    #[command(
        about = "Add an external reference to an issue",
        long_about = "Add a namespaced external reference to an issue (idempotent).

Adds a generic (namespace, key, value) reference such as tracker IDs or
commit identifiers. Does not replace native bead IDs or resolve anything
over the network. Namespace-scoped uniqueness supports reliable deduplication
and cross-tool recognition.

EXAMPLES:
  bead ref add --id bead-123abc456789def --namespace github --key issue-number --value 12345
  bead ref add --id ID --namespace gitlab --key mr-iid --value 42
  bead ref add --id ID --namespace jira --key ticket --value PROJ-001

NAMING RULES:
  - Namespace: 1-64 chars, lowercase letters/numbers/hyphens/underscores, must start with letter
  - Key: 1-128 chars, no control characters
  - Value: 1-512 chars, no control characters

COMMON NAMESPACES:
  - github: GitHub issues, PRs, commits
  - gitlab: GitLab merge requests, issues
  - jira: JIRA tickets
  - tracker: Generic issue trackers
  - vcs: Version control systems

USE CASES:
  - Link issues to external tracker tickets
  - Reference commit hashes or PR numbers
  - Cross-tool issue correlation and deduplication
  - External system integration without network dependencies

The command validates all inputs and creates the reference atomically."
    )]
    Add(RefAddOptions),

    /// Remove an external reference from an issue
    #[command(
        about = "Remove an external reference from an issue",
        long_about = "Remove a namespaced external reference from an issue (idempotent).

Removes the specified external reference from the issue. If the reference
does not exist, the command succeeds without making changes.

EXAMPLES:
  bead ref remove --id bead-123abc456789def --namespace github --key issue-number
  bead ref remove --id ID --namespace gitlab --key mr-id

IDEMPOTENCY:
  Removing a non-existent reference succeeds without error.
  This makes external reference management safe and declarative.

NAMESPACE-SCOPED:
  References are identified by (issue_id, namespace, key) combination.
  The same namespace can have different keys for the same issue."
    )]
    Remove(RefRemoveOptions),

    /// List external references for an issue
    #[command(
        about = "List external references for an issue",
        long_about = "List all external references for an issue.

Shows all namespaced external references attached to the issue, sorted
by namespace and key for consistent output.

EXAMPLES:
  bead ref list --id bead-123abc456789def
  bead ref list --id ID --json

OUTPUT FORMAT:
  Human-readable format shows namespace, key, and value for each reference.
  JSON format provides structured data for automation.

  --json: Output in compact JSON format"
    )]
    List(RefListOptions),

    /// Find issues by external reference
    #[command(
        about = "Find issues by external reference",
        long_about = "Find issues that have a specific external reference value.

Supports cross-tool recognition by finding all issues that reference the
same external identifier. This enables deduplication and correlation across
different tools and namespaces.

EXAMPLES:
  bead ref find --namespace github --value 12345
  bead ref find --namespace jira --value PROJ-001 --json

CROSS-TOOL RECOGNITION:
  - Multiple issues can reference the same external identifier
  - Enables detection of duplicate work across systems
  - Supports correlation without network access
  - Namespace-scoped to avoid false matches

OUTPUT:
  Returns a list of issue IDs that have the specified reference.
  Empty list if no matching issues are found."
    )]
    Find(RefFindOptions),
}

/// Options for adding an external reference
#[derive(Parser, Debug)]
pub struct RefAddOptions {
    /// Issue ID
    #[arg(long)]
    pub id: String,

    /// Reference namespace (e.g., github, gitlab, jira)
    #[arg(long)]
    pub namespace: String,

    /// Reference key (e.g., issue-number, mr-id, ticket)
    #[arg(long)]
    pub key: String,

    /// Reference value (e.g., 12345, abc123, PROJ-001)
    #[arg(long)]
    pub value: String,
}

/// Options for removing an external reference
#[derive(Parser, Debug)]
pub struct RefRemoveOptions {
    /// Issue ID
    #[arg(long)]
    pub id: String,

    /// Reference namespace
    #[arg(long)]
    pub namespace: String,

    /// Reference key
    #[arg(long)]
    pub key: String,
}

/// Options for listing external references
#[derive(Parser, Debug)]
pub struct RefListOptions {
    /// Issue ID
    #[arg(long)]
    pub id: String,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

/// Options for finding issues by external reference
#[derive(Parser, Debug)]
pub struct RefFindOptions {
    /// Reference namespace
    #[arg(long)]
    pub namespace: String,

    /// Reference value
    #[arg(long)]
    pub value: String,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

/// Structured data commands
#[derive(Subcommand, Debug)]
pub enum DataCommand {
    Set(DataSetOptions),

    Get(DataGetOptions),

    List(DataListOptions),

    Remove(DataRemoveOptions),
}

/// Options for setting structured data
#[derive(Parser, Debug)]
#[command(
    about = "Set a structured data value for an issue",
    long_about = "Set or replace a JSON value for a specific namespace with schema governance.

Sets or replaces the structured data value for the specified namespace and issue.
Each namespace is governed by its own immutable schema reference, enabling controlled
extension of issue data without arbitrary field proliferation.

EXAMPLES:
  bead data set --id bead-123abc456789def --namespace config --schema-ref schema:v1 --value '{\"setting\": \"value\"}'
  bead data set --id ID --namespace metrics --schema-ref schema:metrics --value '{\"count\": 42}'

SCHEMA GOVERNANCE:
  - Each namespace has an immutable schema reference identifier
  - Unknown schemas are preserved for interchange but fail closed for mutation
  - Schema references enable validation and consumer negotiation

NAMING RULES:
  - Namespace: 1-64 chars, lowercase letters/numbers/hyphens/underscores, must start with letter
  - Schema reference: 1-512 chars, non-empty

ATOMICITY:
  - Set operations are atomic and crash-safe
  - Replaces existing values for the same namespace
  - Validates that the issue exists before setting data

USE CASES:
  - Add structured configuration data to issues
  - Store metrics and measurements
  - Attach schema-governed metadata
  - Extend issue data without API changes"
)]
pub struct DataSetOptions {
    /// Issue ID
    #[arg(long)]
    pub id: String,

    /// Data namespace (e.g., config, metrics, state)
    #[arg(long)]
    pub namespace: String,

    /// Schema reference (immutable identifier for the data schema)
    #[arg(long)]
    pub schema_ref: String,

    /// JSON value to set
    #[arg(long)]
    pub value: String,
}

/// Options for getting structured data
#[derive(Parser, Debug)]
#[command(
    about = "Get a structured data value from an issue",
    long_about = "Retrieve the JSON value and schema reference for a specific namespace.

Returns both the schema reference and the JSON value for the specified namespace.
If the namespace does not exist for the issue, returns a not-found error.

EXAMPLES:
  bead data get --id bead-123abc456789def --namespace config
  bead data get --id ID --namespace metrics --json

OUTPUT FORMAT:
  Human-readable format shows the schema reference and formatted JSON value.
  JSON format provides structured data with schema_ref and value fields."
)]
pub struct DataGetOptions {
    /// Issue ID
    #[arg(long)]
    pub id: String,

    /// Data namespace
    #[arg(long)]
    pub namespace: String,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

/// Options for listing structured data
#[derive(Parser, Debug)]
#[command(
    about = "List all structured data namespaces for an issue",
    long_about = "List all namespaces and their schema references for an issue.

Shows all structured data namespaces attached to the issue, sorted alphabetically
by namespace for consistent output. Each entry includes the namespace and its
governing schema reference.

EXAMPLES:
  bead data list --id bead-123abc456789def
  bead data list --id ID --json

OUTPUT FORMAT:
  Human-readable format shows namespace and schema reference for each entry.
  JSON format provides structured array of objects with namespace and schema_ref."
)]
pub struct DataListOptions {
    /// Issue ID
    #[arg(long)]
    pub id: String,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

/// Options for removing structured data
#[derive(Parser, Debug)]
#[command(
    about = "Remove a structured data value from an issue",
    long_about = "Remove a structured data value from an issue (idempotent).

Removes the structured data value for the specified namespace and issue.
If the namespace does not exist, the command succeeds without making changes.

EXAMPLES:
  bead data remove --id bead-123abc456789def --namespace config
  bead data remove --id ID --namespace metrics

IDEMPOTENCY:
  Removing a non-existent namespace succeeds without error.
  This makes structured data management safe and declarative.

ATOMICITY:
  - Remove operations are atomic and crash-safe
  - Only affects the specified namespace
  - Validates that the issue exists before removal"
)]
pub struct DataRemoveOptions {
    /// Issue ID
    #[arg(long)]
    pub id: String,

    /// Data namespace
    #[arg(long)]
    pub namespace: String,
}

/// Recurrence template commands
#[derive(Subcommand, Debug)]
pub enum RecurrenceCommand {
    Create(RecurrenceCreateOptions),

    Show(RecurrenceShowOptions),

    List(RecurrenceListOptions),

    Delete(RecurrenceDeleteOptions),

    Materialize(RecurrenceMaterializeOptions),

    History(RecurrenceHistoryOptions),
}

/// Policy validation subcommands
#[derive(Subcommand, Debug)]
pub enum PolicyCommand {
    /// Validate workspace policy and scheduling configuration
    #[command(
        name = "check",
        about = "Validate workspace policy and scheduling configuration",
        long_about = "Diagnose contradictory, unreachable, redundant, and ineffective scheduling
or retention configuration without making any changes to the workspace.

This command performs comprehensive validation of:
  - Scheduling policy configuration and parameters
  - Retention and aging settings
  - Dependency and readiness rules
  - Version compatibility issues

Policy lint is purely advisory and will never make a bead eligible or ineligible.
It only identifies potential configuration issues that may affect behavior.

EXAMPLES:
  bead policy check
  bead policy check --format json"
    )]
    Check(PolicyCheckOptions),
}

/// Options for policy check command
#[derive(Parser, Debug)]
pub struct PolicyCheckOptions {
    /// Output format (text or json)
    #[arg(long, default_value = "text")]
    pub format: String,

    /// Scheduling policy to validate (defaults to current workspace policy)
    #[arg(long)]
    pub policy: Option<String>,

    /// Policy version to validate (defaults to current workspace version)
    #[arg(long)]
    pub policy_version: Option<String>,
}

/// Options for creating a recurrence template
#[derive(Parser, Debug)]
#[command(
    about = "Create a new recurrence template",
    long_about = "Create an immutable recurrence template that defines how recurring issues should be created.

Templates define the structure for recurring issues including title templates, default priority,
issue type, and labels. Individual occurrences are created explicitly through the materialize
command, not automatically on schedules.

EXAMPLES:
  bead recurrence create --id template-001 --title 'Daily Review' --base-title-template 'Daily Review {n}' --priority 2
  bead recurrence create --id weekly-planning --title 'Weekly Planning' --base-title-template 'Week {n} Planning' --labels 'weekly,planning'"
)]
pub struct RecurrenceCreateOptions {
    /// Template ID
    #[arg(long)]
    pub id: String,

    /// Template title
    #[arg(long)]
    pub title: String,

    /// Template description
    #[arg(long)]
    pub description: Option<String>,

    /// Title template for occurrences (use {n} for sequence number)
    #[arg(long)]
    pub base_title_template: String,

    /// Description template for occurrences
    #[arg(long)]
    pub base_description: Option<String>,

    /// Priority for created issues (0-4, default 2)
    #[arg(long)]
    pub priority: Option<i64>,

    /// Issue type for created issues (default 'task')
    #[arg(long)]
    pub issue_type: Option<String>,

    /// Comma-separated list of labels to apply to occurrences
    #[arg(long)]
    pub labels: Option<String>,
}

/// Options for showing a recurrence template
#[derive(Parser, Debug)]
#[command(
    about = "Show a recurrence template",
    long_about = "Display detailed information about a specific recurrence template including
its configuration and materialization history."
)]
pub struct RecurrenceShowOptions {
    /// Template ID
    #[arg(long)]
    pub id: String,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

/// Options for listing recurrence templates
#[derive(Parser, Debug)]
#[command(
    about = "List all recurrence templates",
    long_about = "List all recurrence templates in the workspace with their basic configuration
and occurrence counts."
)]
pub struct RecurrenceListOptions {
    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

/// Options for deleting a recurrence template
#[derive(Parser, Debug)]
#[command(
    about = "Delete a recurrence template",
    long_about = "Delete a recurrence template and its materialization history.
This operation is irreversible and will remove all tracking of the template's occurrences."
)]
pub struct RecurrenceDeleteOptions {
    /// Template ID
    #[arg(long)]
    pub id: String,
}

/// Options for materializing the next occurrence
#[derive(Parser, Debug)]
#[command(
    about = "Materialize the next occurrence from a template",
    long_about = "Create the next occurrence in the recurrence series as a new issue.
Each occurrence carries a stable series reference and the created issue ID.

This is an explicit operation - bead-rs never automatically creates occurrences on schedules.
External schedulers should call this command when they want a new occurrence created.

EXAMPLES:
  bead recurrence materialize --id template-001
  bead recurrence materialize --id daily-review --actor scheduler-1"
)]
pub struct RecurrenceMaterializeOptions {
    /// Template ID
    #[arg(long)]
    pub id: String,

    /// Actor performing the materialization
    #[arg(long)]
    pub actor: Option<String>,
}

/// Options for showing materialization history
#[derive(Parser, Debug)]
#[command(
    about = "Show materialization history for a template",
    long_about = "Display the complete materialization history for a recurrence template,
showing all created occurrences with their sequence numbers and timestamps."
)]
pub struct RecurrenceHistoryOptions {
    /// Template ID
    #[arg(long)]
    pub id: String,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing() {
        // Test init command
        let cli = Cli::try_parse_from(["bead", "init"]).unwrap();
        matches!(cli.command, Command::Init(_));

        // Test init with custom prefix
        let cli = Cli::try_parse_from(["bead", "init", "--prefix", "custom"]).unwrap();
        if let Command::Init(opts) = cli.command {
            assert_eq!(opts.prefix, "custom");
        } else {
            panic!("Expected Init command");
        }
    }

    #[test]
    fn test_cli_help() {
        // Just ensure help doesn't panic
        let _ = Cli::try_parse_from(["bead", "--help"]);
    }
}
