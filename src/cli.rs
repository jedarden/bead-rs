//! CLI command definitions for bead-rs
//!
//! This module uses clap derive to define all command-line interface commands.

use clap::{Parser, Subcommand};

/// Main CLI structure for bead-rs
#[derive(Parser, Debug)]
#[command(name = "bead")]
#[command(
    author = "Jed Arden <github@jedarden.com>",
    version = env!("BEAD_VERSION_STRING"),
    about = "Clean-room task coordination for agent fleets",
    long_about = "bead-rs is an independent Rust task-coordination system.

The intended workflow is:
  init workspace -> create beads -> add blocking relationships
  -> inspect ready work -> claim -> update/release -> close
  -> checkpoint published automatically with every successful mutation

The ready frontier can be inspected with `bead list --ready --json --limit N`,
which uses claim order but does not reserve the displayed beads. Use `bead claim`
to atomically assign work.

SQLite (.beads/beads.db) is the authoritative live state and is not committed.
The checkpoint under .beads/checkpoint/ is the portable, durable copy and is
what Git tracks; every successful mutation publishes it automatically after its
transaction commits, so it is never silently behind the database. `bead sync
flush-only` remains an explicit idempotent check, and `--no-auto-flush` or
`checkpoint.auto_flush` in .beads/config.json suppresses automatic publication,
leaving the checkpoint to be flushed by hand.

Recovery is explicit: `bead restore --source .beads/checkpoint --generation
<GENERATION> --actor <WHO>` verifies one named immutable generation before it
creates or replaces any live state. `bead doctor` may recommend that command but
never runs it.

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

    /// Admit only findings whose exact scanner fingerprint matches this
    /// value. Repeat for multiple findings. Every successful mutation using
    /// an acknowledgment records a same-transaction audit event; there is no
    /// blanket bypass flag.
    #[arg(long = "acknowledge-secret", global = true, value_name = "FINGERPRINT")]
    pub acknowledge_secret: Vec<String>,

    /// Let workspace discovery continue past the first `.beads` directory
    /// when it is not a bead-rs workspace (no `.beads/config.json`), so a
    /// bead-rs workspace farther up the tree can be used. Discovery
    /// otherwise stops there and fails closed: it never silently skips a
    /// `.beads` it does not recognize to operate on an unrelated parent
    /// workspace, and it never writes into the unrecognized directory --
    /// including under this flag, which only widens the search
    #[arg(long, global = true)]
    pub skip_foreign_workspace: bool,

    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Command,
}

/// Available commands
#[derive(Subcommand, Debug)]
pub enum Command {
    Init(InitOptions),

    /// Restore one named, verified checkpoint generation
    #[command(
        about = "Restore one named, verified checkpoint generation",
        long_about = "Restore one named immutable checkpoint generation as an explicit recovery operation.

The source must be a checkpoint-set directory containing current.json or
previous.json, or one of those pointer files directly. --generation must match
the selected pointer exactly. Before touching the target, bead-rs verifies the
pointer, content-addressed root, every sharded object, record counts, canonical
ordering, event continuity, and graph integrity. Bare forensic.jsonl files,
unverified artifacts, and checkpoint-archaeology views are refused.

The target is the current bead-rs workspace. A missing or uninitialized native
database is initialized by this command after source verification. A target
with semantic state is refused unless --allow-non-empty is supplied; that
override atomically replaces native semantic state while preserving unknown
tables. Every successful restore records the actor in a durable provenance
receipt and reports the selected generation, root hash, UUID, and exact issue,
event, and receipt counts.

EXAMPLES:
  bead restore --source .beads/checkpoint --generation gen-abc123 --actor admin
  bead restore --source /backups/current.json --generation gen-abc123 --actor admin
  bead restore --source /backups/checkpoint --generation gen-abc123 --actor admin --allow-non-empty
  bead restore --source /backups/checkpoint --generation gen-abc123 --actor admin --format json

This is the authoritative operator recovery command. `sync import-only` remains
the lower-level interchange and reconciliation primitive; doctor stays
read-only and never invokes restore automatically."
    )]
    Restore(RestoreOptions),

    Create(CreateOptions),

    List(ListOptions),

    Show(ShowOptions),

    Update(UpdateOptions),

    Release(ReleaseOptions),

    Close(CloseOptions),

    Reopen(ReopenOptions),

    /// Resolve an execution attempt
    #[command(
        name = "resolve",
        about = "Record an execution attempt outcome atomically",
        long_about = "Resolve an execution attempt by recording its outcome and applying
a lifecycle transition in one atomic operation. This implements the
attempt-outcome-v1 specification with exactly-once semantics."
    )]
    Resolve(ResolveOptions),

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

    /// Manage workspace-local resource declarations
    #[command(
        subcommand,
        long_about = "Declare local resource keys for atomic claim exclusion.

Resource locks are scheduling exclusions inside this one native workspace. They
are not distributed locks and do not coordinate different stores.

  bead resource add <ID> --key <KEY>
  bead resource remove <ID> --key <KEY>
  bead resource list <ID>"
    )]
    Resource(ResourceCommand),

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

    /// Materialize a bulk transaction manifest
    #[command(
        subcommand,
        long_about = "Apply a versioned JSON manifest of existing command primitives
as one all-or-none transaction (R033).

A manifest composes creates, updates, label and dependency mutations, and
closes into a single atomic unit: every operation is exactly one existing
command's semantics, applied in array order inside one SQLite transaction,
publishing at most one checkpoint generation for the whole manifest instead
of one per command. Newly created beads are named with a `local_id` other
operations reference as `$name`.

  bead manifest dry-run --input plan.json     # report the semantic delta, mutate nothing
  bead manifest commit  --input plan.json     # apply everything or nothing
  bead manifest commit  --input plan.json --format json   # machine-readable result map

Version 1 refuses any semantics a single existing command does not already
have: no control flow, no query-scoped mutation, no new field semantics.
Schema and rules: research/specs/bulk-manifests-v1.md."
    )]
    Manifest(ManifestCommand),

    /// Synchronize checkpoint operations
    #[command(
        subcommand,
        long_about = "Publish and ingest the durable checkpoint.

SQLite holds live state and is not committed; the checkpoint under
.beads/checkpoint/ is what Git tracks. Every successful mutation publishes
it automatically after its transaction commits, so it stays current without
a remembered command.

  bead sync flush-only                                 # idempotent check: database -> checkpoint
  bead sync import-only --input <PATH> --restore-into-empty --actor <WHO>
  bead sync import-only --input <PATH> --merge --actor <WHO>

For disaster recovery use `bead restore --source <CHECKPOINT-SET> --generation
<GENERATION> --actor <WHO>`. `sync import-only` remains the lower-level
interchange and reconciliation primitive."
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

    /// Analyze bead exclusion from ready frontier
    #[command(
        about = "Analyze why beads are excluded from the ready frontier",
        long_about = "Analyze beads and determine why they are excluded from the ready frontier.
Runs the exact same filtering logic as 'bead list --ready' and provides detailed
per-bead breakdown of which exclusion rules matched and why.

This helps diagnose 'starvation' situations where open beads exist but the ready
frontier is empty, indicating that all open beads are excluded by one or more rules.

EXCLUSION RULES (evaluated in order):
  1. Status filter: must be 'open' (excludes: in_progress, deferred, closed)
  2. Assignee filter: must be unassigned (excludes: beads with assignee)
  3. Manual block: must not be manually blocked (excludes: manual_blocked = 1)
  4. Dependencies: must have no unclosed blocking dependencies
  5. Resource locks: must have no conflicts with held locks by other active issues

EXAMPLES:
  bead analyze-exclusion                         # Analyze all open beads
  bead analyze-exclusion --json                 # Machine-readable JSON output
  bead analyze-exclusion --attach tunnel-abc123  # Attach analysis as bead comment
  bead analyze-exclusion --limit 50              # Analyze first 50 open beads

The command also reports:
  - Total open beads vs. ready frontier size
  - Per-rule exclusion counts
  - 'Open but invisible' beads (excluded open beads)

This is useful for automated monitoring: run this when 'bead list --ready' returns
empty but open beads exist, and attach the output to a diagnostic bead."
    )]
    AnalyzeExclusion(AnalyzeExclusionOptions),

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

    /// Watch for and automatically release stale bead claims
    #[command(
        about = "Monitor bead claim duration and auto-release stuck claims",
        long_about = "Watchdog monitors in_progress beads and automatically releases those that appear stuck.

A bead is considered stuck if:
  - It has been in_progress for longer than the threshold (default: 4 hours)
  - The assigned worker process is no longer running

The watchdog checks process liveness by searching for the assignee name in the
process table. If the worker is dead, the bead is automatically released back to
the ready frontier. All auto-releases are logged to .beads/watchdog-releases.jsonl.

This prevents starvation where a crashed or hung worker holds beads indefinitely.

EXAMPLES:
  bead watchdog --dry-run                                 # Report what would happen
  bead watchdog --threshold 8h                            # Use 8-hour threshold
  bead watchdog --threshold 2h --force                     # Release beads stale >2h
  bead watchdog --json                                     # Machine-readable output

The watchdog is designed to run as a periodic systemd service (bead-watchdog.timer)
but can also be run manually for ad-hoc cleanup."
    )]
    Watchdog(WatchdogOptions),

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

/// Options for explicit verified restore (R036)
#[derive(Parser, Debug)]
pub struct RestoreOptions {
    /// Checkpoint-set directory or current.json/previous.json pointer
    #[arg(long, alias = "input", value_name = "PATH")]
    pub source: String,

    /// Exact generation ID recorded by the selected immutable pointer
    #[arg(long, value_name = "GENERATION")]
    pub generation: String,

    /// Actor responsible for this recovery operation
    #[arg(long, value_name = "ACTOR")]
    pub actor: String,

    /// Atomically replace native semantic state when the target is non-empty
    #[arg(long, alias = "force", alias = "force-non-empty")]
    pub allow_non_empty: bool,

    /// Prefix used only when this command initializes a workspace with no config
    #[arg(long, default_value = "bead")]
    pub prefix: String,

    /// Output format: text or json
    #[arg(long, default_value = "text")]
    pub format: String,
}

/// Options for creating a new issue
#[derive(Parser, Debug)]
#[command(
    about = "Create a new issue",
    long_about = "Create a new issue in the workspace.

Creates a new issue with the specified title and optional metadata.
The issue ID is automatically generated and printed on success.
Fresh creates print only the issue ID followed by a newline; idempotent
reference hits use the explicit result prefixes described below.

IDEMPOTENT CREATION:
  --unique-ref NAMESPACE:KEY atomically binds a stable external identity.
  A repeated create returns `EXISTING ID`; a repeated create whose binding
  points to a closed issue returns `EXISTING_CLOSED ID` so callers can stop
  retrying finished work.

PRIORITIES:
  0 = urgent (immediate incident, safety, or release-blocking)
  1 = critical (essential work preceding ordinary delivery)
  2 = high (important planned work, default)
  3 = normal (ordinary work with no elevated urgency)
  4 = aspirational/backlog (speculative or low-urgency work)

EXAMPLES:
  bead create --title \"Fix authentication bug\" --priority 0
  bead create --title \"Update documentation\" --priority 2 --label docs
  bead create --title \"Build image\" --resource-key docker:daemon --resource-key gpu:0
  bead create --title \"Materialize tracker work\" --unique-ref github:issue-123
  bead create --title \"Add search feature\" --assignee alice --label feature --label backend
  bead create --title \"Code review PR-123\" --description \"Review changes for user auth\"

ISSUE TYPES:
  Common types include: task, bug, feature, improvement, documentation
  Custom types can be specified as needed (no validation is performed).

RESOURCE KEYS:
  --resource-key declares a normalized, case-sensitive key used only for
  scheduling exclusion inside this native workspace. It is not a distributed
  lock. Claims acquire every declared key atomically; release, close, and
  expired leases return keys. Use `bead resource add|remove|list` after create."
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

    /// Workspace-local resource key (can be specified multiple times)
    #[arg(long = "resource-key")]
    pub resource_keys: Vec<String>,

    /// Idempotency binding in NAMESPACE:KEY form
    #[arg(long = "unique-ref")]
    pub unique_ref: Option<String>,

    // Hidden flags for R037 near-miss detection
    /// Near-miss trap: create has no --status; a new issue starts open.
    /// Set a status after creation with 'bead update <id> --status'.
    #[arg(long, hide = true)]
    pub status: Option<String>,

    /// Near-miss trap: create has no --notes; supply notes later with
    /// 'bead update <id> --notes'.
    #[arg(long, hide = true)]
    pub notes: Option<String>,
}

/// Options for listing issues
#[derive(Parser, Debug)]
#[command(
    about = "List issues",
    long_about = "List issues with optional filtering and comment projection.

Supports filtering by status, assignee, ready frontier, and manual block. Uses claim
ordering (priority ASC, created_at ASC, id ASC) for deterministic results.
Ready frontier uses the same ordering as 'bead claim' but is read-only and
does not reserve work.

EXAMPLES:
  bead list --json --limit 10                      # First 10 issues as JSON
  bead list --status open --assignee alice        # Open issues assigned to alice
  bead list --ready --limit 5                      # Next 5 ready candidates
  bead list --blocked                               # Manually blocked open issues
  bead list --comments unresolved --json --limit 20  # Issues with unresolved comments

FILTERS:
  --status VALUE    Filter by base status: open, in_progress, deferred, closed
                    (Special case: 'blocked' is an alias for --blocked)
  --assignee NAME   Filter by assignee (exact match)
  --ready           Show only ready frontier issues (open, unassigned, not blocked)
  --blocked         Show only manually blocked open issues
  --limit N         Maximum results (0-999999, default: 100)

COMMENT PROJECTION:
  --comments none         Show only counts and resolution metadata (default)
  --comments unresolved   Include bodies for unresolved comments only
  --comments all          Include all comment bodies in canonical order

OUTPUT:
  Without --json: human-readable table format
  With --json: one compact JSON object per line (compact JSONL format)
  JSON output includes: id, title, priority, status, manual_blocked, effective_status,
  assignee, dependencies, created_at, updated_at, and labels based on comment projection."
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

    /// Show only manually blocked open issues
    #[arg(long)]
    pub blocked: bool,

    /// Comment projection: none, unresolved, or all (default: none)
    #[arg(long, default_value = "none")]
    pub comments: String,

    /// Maximum number of issues to return (0-999999)
    #[arg(long, default_value = "100")]
    pub limit: i64,

    /// Show diagnostic information about filtering (SQL query, counts, exclusion reasons)
    #[arg(long)]
    pub verbose: bool,
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
  - Manual block: manual_blocked flag (true when explicitly blocked)
  - Effective status: effective_status (shows 'blocked' when manual_blocked is true)
  - Timestamps: created_at, updated_at, closed_at (if closed)
  - Dependencies: blocked_by (blocking issues), blocking (issues this blocks)
  - Labels: all assigned labels
  - Comments: based on --comments projection
  - Metadata: issue_type, close_reason (if closed)"
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
  bead claim --assignee 'Team Backend'        # Claim with team assignee
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

RESOURCE LOCKS:
  A claim acquires every declared key as one workspace-local set. A ready issue
  needing a held key is skipped; `--why` and `bead why --json` report reason code
  `resource_conflict`. This has no effect outside this native store and is not a
  distributed lock.

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

    // Hidden flags for R037 near-miss detection
    /// Near-miss trap: title is immutable after create; this update flag
    /// does not exist. Set the title at creation time.
    #[arg(long, hide = true)]
    pub title: Option<String>,

    /// Near-miss trap: description is immutable after create; this update
    /// flag does not exist. Set the description at creation time.
    #[arg(long, hide = true)]
    pub description: Option<String>,

    /// Near-miss trap: priority is immutable after create; this update
    /// flag does not exist. Set the priority at creation time.
    #[arg(long, hide = true)]
    pub priority: Option<i64>,

    /// Near-miss trap: issue type is immutable after create; this update
    /// flag does not exist. Set the issue type at creation time.
    #[arg(long = "issue-type", hide = true)]
    pub issue_type_hidden: Option<String>,

    /// Near-miss trap: labels are managed by 'bead label add|remove', not
    /// by update.
    #[arg(long, hide = true)]
    pub label: Option<String>,
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
    #[arg(long, required = false)]
    pub reason: Option<String>,

    /// Expected revision for optimistic concurrency control
    #[arg(long)]
    pub if_revision: Option<i64>,

    /// Fencing token for lease validation (advanced usage)
    #[arg(long)]
    pub fencing_token: Option<i64>,

    /// Dry run: show what would happen without making changes
    #[arg(long)]
    pub dry_run: bool,

    // Hidden flag for R037 near-miss detection (--body should be --reason)
    /// Near-miss trap: close takes --reason, not --body. Pass the closing
    /// reason with 'bead close <id> --reason'.
    #[arg(long, hide = true)]
    pub body: Option<String>,
}

/// Options for reopening an issue
#[derive(Parser, Debug)]
#[command(
    about = "Reopen a closed issue",
    long_about = "Restore a closed issue to open lifecycle status.

Reopen transitions a closed issue back to open status, clearing the
assignee so the issue becomes claimable again. This is the only valid
way to cross from closed to open status (generic update cannot do this).

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
  - Clears assignee (makes issue claimable)
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

Reopen makes closed issues ready frontier candidates
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

/// Options for resolving an attempt
#[derive(Parser, Debug)]
#[command(
    about = "Record an execution attempt outcome atomically with lifecycle transition",
    long_about = "Resolve an execution attempt by recording its outcome and applying
a lifecycle transition in one atomic operation.

This command implements the attempt-outcome-v1 specification, providing
exactly-once semantics for attempt resolution under concurrent access and
crash recovery.

EXAMPLES:
  bead resolve bead-123abc456789def --attempt-id urn:needle:attempt:abc123 --outcome verified_success --action close
  bead resolve bead-123abc456789def --attempt-id urn:needle:attempt:abc123 --outcome work_failure --action quarantine --reason \"Tests failing\"
  bead resolve bead-123abc456789def --attempt-id urn:needle:attempt:abc123 --outcome infrastructure_failure --action release

OUTCOMES:
  verified_success       - Work completed successfully (no tier penalty)
  work_failure          - Bead-scoped failure (advances tier)
  infrastructure_failure - Worker crash, outage, rate limit (no tier penalty)
  cancelled             - Explicit cancellation by operator
  indeterminate          - Unable to determine outcome

ACTIONS:
  close       - Set closed_at, store close_reason
  release     - Clear assignee, retain state
  quarantine  - Set attempt_tier=3, set retry_after
  block       - Set manual_blocked=true
  none        - No lifecycle transition

SEMANTICS:
  - Atomic: outcome and lifecycle transition commit together
  - Idempotent: identical replay returns original receipt
  - Conflict: divergent replay with same attempt_id fails

EXIT CODES:
  0 - Success (including idempotent replay)
  2 - Usage or validation error
  3 - Issue not found
  4 - Conflict (revision, fencing, outcome divergence)
  5 - Integrity failure
  6 - Database busy or transient I/O error

This operation requires the attempt-outcome capability. Check with:
  bead capabilities --format json | jq -e '.attempt_outcome.supported'"
)]
pub struct ResolveOptions {
    /// Issue ID
    pub id: String,

    /// Attempt ID (required)
    #[arg(long)]
    pub attempt_id: String,

    /// Outcome classification (required)
    #[arg(long)]
    pub outcome: String,

    /// Lifecycle action to apply
    #[arg(long)]
    pub action: Option<String>,

    /// Human-readable reason for the action
    #[arg(long)]
    pub reason: Option<String>,

    /// Expected revision for optimistic concurrency control
    #[arg(long)]
    pub if_revision: Option<i64>,

    /// Fencing token for lease validation
    #[arg(long)]
    pub fencing_token: Option<String>,

    /// Evidence references (NAMESPACE:VALUE format)
    #[arg(long)]
    pub evidence_ref: Vec<String>,

    /// Actor identity
    #[arg(long, default_value = "unknown")]
    pub actor: String,

    /// Model identifier for telemetry
    #[arg(long)]
    pub model: Option<String>,

    /// Harness name for telemetry
    #[arg(long)]
    pub harness: Option<String>,

    /// Harness version for telemetry
    #[arg(long)]
    pub harness_version: Option<String>,

    /// Output format
    #[arg(long, default_value = "text")]
    pub format: String,
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
  bead sync flush-only                                  # Idempotent check; publishes nothing new when current
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
  A checkpoint represents the database as of its publication. Every successful
  mutation publishes a new generation automatically after its transaction
  commits, so the checkpoint is never silently behind. `--no-auto-flush` and
  the checkpoint.auto_flush configuration key suppress that publication,
  leaving the checkpoint dirty exactly as an unflushed mutation; this command
  then closes the gap. With a current checkpoint it publishes no new
  generation and exits 0.

ATOMICITY:
  - A read transaction captures the snapshot
  - The temporary file is written and verified
  - An atomic rename replaces the previous checkpoint
  - Checkpoint state advances in the same transaction

GIT INTEGRATION:
  Mutations publish automatically; run 'bead sync flush-only' before committing
  the repository as an explicit idempotent check that the checkpoint is current.
  bead-rs never runs Git commands itself. Recover with the named-generation
  verifier: 'bead restore --source .beads/checkpoint --generation <GEN> --actor <WHO>'."
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
  bead sync import-only --input forensic.jsonl --diagnostics --dry-run

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

DIAGNOSTICS MODE (R014):
  With --diagnostics: enables detailed validation failure collection (R014).
  Incompatible with --restore-into-empty and --merge (uses simple import only).
  Reports line numbers, JSON pointers, schema keywords, and semantic codes
  for validation failures. Useful for debugging malformed checkpoint files.

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

RECOVERY BOUNDARY:
  This command remains public for interchange, compatibility automation,
  diagnostic dry-runs, and merge. It may accept standalone artifacts without a
  generation pointer. Use `bead restore` for operator recovery from a named,
  fully verified immutable generation; doctor recommends only that path.

Use --dry-run to validate checkpoints before risking database mutation."
    )]
    ImportOnly(SyncImportOptions),

    /// Reconcile a remote-advanced checkpoint into the live store (R027)
    #[command(
        name = "reconcile",
        about = "Merge a pulled, verified, ahead-of-live checkpoint into this workspace",
        long_about = "Reconcile the durable checkpoint into the live store (R027).

The Git-transported workflow commits .beads/checkpoint/ and not
.beads/beads.db, so after pulling another machine's flush the checkpoint can
contain work the live database does not -- the remote-advanced state. This
command recognizes exactly that state and merges the checkpoint into the live
store through the same machinery `sync import-only --merge` uses: one
transaction, conflict detection, issue reconciliation by timestamp, an
actor-attributed merge provenance receipt, and a merge summary event.

QUALIFICATION:
  Reconcile acts on the workspace's own durable checkpoint against the
  workspace's own live store; there is no --input. It proceeds only when
  the checkpoint is remote-advanced:
  - the pointer verifies (supported mode, hashes, no unresolved tombstones,
    agreeing compatibility view)
  - the selected generation stages and passes forensic validation
  - the checkpoint's store UUID equals this workspace's UUID
  - every live event appears in the checkpoint with identical public content
  - the recorded checkpoint state does not claim more history than the
    live store holds
  Any other checkpoint-ahead-of-live shape is an integrity failure and is
  refused without mutation. bead-rs never runs Git; it observes the store
  relationship, not the transport that produced it.

PUBLICATION:
  Reconcile never publishes a checkpoint generation by its own action. Under
  the automatic publication default the post-commit chokepoint publishes the
  generation covering the merge; with publication suppressed the workspace
  is left dirty for `bead sync flush-only`, exactly like any other committed
  mutation. Reconcile is idempotent at the state level: once reconciled and
  published the relationship is no longer remote-advanced and a second run
  reports nothing to reconcile.

EXAMPLES:
  bead sync reconcile --actor jed                        # after git pull
  bead sync reconcile --actor jed --dry-run              # prospective counts only

EXIT CODES:
  0 - Reconcile committed (or dry-run validated)
  2 - Nothing to reconcile: relationship is absent, aligned, or behind
      (behind names `bead sync flush-only`)
  5 - Covered-ahead integrity failure; the first failed qualifier is named

WORKFLOW:
  Pull, reconcile, then work. Mutating before reconciling is operator error:
  the local event lands under a pointer that already claims its sequence and
  the next classification fails closed instead of silently succeeding."
    )]
    Reconcile(SyncReconcileOptions),

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

  Under the automatic publication default a not-ready checkpoint means
  publication was suppressed by `--no-auto-flush` or checkpoint.auto_flush,
  or failed after a committed mutation. Repository automation must treat a
  not-ready checkpoint as a failed pre-commit gate: run `bead sync
  flush-only` and include every reported changed path in the same Git commit.

EXAMPLES:
  bead sync status                     # Human-readable summary
  bead sync status --format json       # Machine-readable status

OUTPUT:
  --format json prints one JSON object with checkpoint_present, mode,
  generation_id, live_sequence, covered_sequence, relationship, dirty,
  root_path, root_hash, root_verified, view_agrees, unresolved_tombstones,
  changed_paths, ready_to_commit, and not_ready_reasons.

RELATIONSHIP (R027):
  The sync relationship between the live store and the durable checkpoint:
  absent, behind (live has unflushed work; run `bead sync flush-only`),
  aligned, remote-advanced (a pulled checkpoint is a verified superset ahead
  of the live store; run `bead sync reconcile --actor <you>`), or
  covered-ahead-integrity-failure (the checkpoint is ahead but failed its
  qualification; the first failed qualifier is named in the reasons)."
    )]
    Status(SyncStatusOptions),

    /// Diff two verified checkpoint generations without import
    #[command(
        about = "Diff two verified checkpoint generations",
        long_about = "Compare two retained checkpoint generations without importing either one.

Each argument may be a generation pointer, a pointer-selected sharded manifest,
or a pointer-selected monolithic object. bead-rs resolves manifests and objects
back to current.json or previous.json, then runs the complete named-generation
verifier before materializing an ephemeral read-only view. The JSON response
contains issue- and event-level added, removed, and changed semantic records.
It is explicitly non-importable.

EXAMPLES:
  bead sync diff .beads/checkpoint/previous.json .beads/checkpoint/current.json
  bead sync diff old/manifests/<HASH>.json new/objects/<HASH>.jsonl"
    )]
    Diff(SyncDiffOptions),

    /// Search a verified checkpoint generation series with a safe predicate
    #[command(
        about = "Search a verified checkpoint generation series",
        long_about = "Evaluate one safe query predicate against a caller-ordered series of retained checkpoint artifacts.

Repeat --checkpoint in chronological order. Each artifact may be a generation
pointer, a pointer-selected manifest, or a pointer-selected monolithic object.
Every artifact is completely verified before it is served. The command reports
every generation with one or more matching issues; it deliberately scans rather
than assuming an arbitrary predicate stays monotonic across history. Output is
an explicitly non-importable archaeology view.

EXAMPLES:
  bead sync bisect --checkpoint old/current.json --checkpoint new/current.json --file query.json
  bead sync bisect --checkpoint .beads/checkpoint/previous.json --checkpoint .beads/checkpoint/current.json --file query.json"
    )]
    Bisect(SyncBisectOptions),

    /// Fork workspace identity (R028)
    #[command(
        name = "fork",
        about = "Fork workspace identity under a new UUID",
        long_about = "Create a new workspace UUID derived from the current workspace.

This command implements R028: an explicit operator command that re-origins a
cloned workspace under a new store UUID recorded in a provenance-chained receipt.
Forking enables clones of one repository to become distinct origins whose event
streams merge composably under existing different-UUID rules instead of being
rejected as same-UUID divergence.

WHEN TO FORK:
  - After git cloning a repository with an existing bead workspace
  - Before making changes that should not merge back as same-UUID events
  - When multiple independent workers need separate event streams

WHAT FORK DOES:
  1. Validates workspace has a clean checkpoint (not dirty)
  2. Generates new UUID with provenance to parent UUID
  3. Records fork receipt in provenance_receipts table
  4. Updates workspace.uuid in database
  5. Creates summary event in events table
  6. Marks checkpoint dirty (requiring flush after fork)

AFTER FORKING:
  - Run 'bead sync flush-only' to publish the forked checkpoint
  - Commit both .beads/config.json (new UUID) and checkpoint to git
  - The forked workspace is now a distinct origin
  - Can merge back into parent using 'bead sync import-only --merge'

EXAMPLES:
  bead sync fork --actor 'team-lead' --reason 'Forking for experimental branch'
  bead sync fork --actor 'team-lead'

NOT FORKING:
  Forking is never implicit or inferred. Only this explicit command creates a
new workspace UUID. Running commands in a cloned workspace without forking will
fail on same-UUID divergence during merge."
    )]
    Fork(SyncForkOptions),
}

/// Options for flushing checkpoint
#[derive(Parser, Debug)]
pub struct SyncFlushOptions {
    /// Export an issue-only copy to this path instead of only updating .beads/checkpoint/
    #[arg(long)]
    pub output: Option<String>,
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

    /// Enable diagnostic mode (R014) with detailed validation failure collection
    #[arg(long)]
    pub diagnostics: bool,
}

/// Options for checkpoint status
#[derive(Parser, Debug)]
pub struct SyncStatusOptions {
    /// Output format: text or json
    #[arg(long, default_value = "text")]
    pub format: String,
}

/// Options for reconciling a remote-advanced checkpoint (R027)
#[derive(Parser, Debug)]
pub struct SyncReconcileOptions {
    /// Actor performing the reconcile (required, attributed in the merge receipt)
    #[arg(long)]
    pub actor: String,

    /// Perform dry-run validation without mutating the live store
    #[arg(long)]
    pub dry_run: bool,
}

/// Positional artifacts for one historical semantic delta.
#[derive(Parser, Debug)]
pub struct SyncDiffOptions {
    /// Earlier checkpoint artifact (pointer, manifest, or monolithic object)
    pub from: String,

    /// Later checkpoint artifact (pointer, manifest, or monolithic object)
    pub to: String,
}

/// Predicate-driven search across caller-supplied historical checkpoints.
#[derive(Parser, Debug)]
pub struct SyncBisectOptions {
    /// Checkpoint artifact, repeat in chronological order
    #[arg(long = "checkpoint", required = true)]
    pub checkpoints: Vec<String>,

    /// Query specification as inline JSON
    #[arg(long, conflicts_with = "file")]
    pub query: Option<String>,

    /// Query specification file (JSON format)
    #[arg(long, conflicts_with = "query")]
    pub file: Option<String>,
}

/// Options for forking workspace identity (R028)
#[derive(Parser, Debug)]
pub struct SyncForkOptions {
    /// Actor performing the fork operation (required)
    #[arg(long)]
    pub actor: String,

    /// Human-readable explanation for the fork (optional, max 4096 bytes)
    #[arg(long)]
    pub reason: Option<String>,

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

/// Workspace-local resource declaration commands.
#[derive(Subcommand, Debug)]
pub enum ResourceCommand {
    /// Add one or more normalized local resource keys
    #[command(long_about = "Add normalized resource declarations to an issue.

The keys are scheduling exclusions in this native workspace only. They are
not distributed locks. A claimed issue holds every declared key as one
atomic set; adding keys to an in-progress issue requires its lease fencing
token when applicable.

EXAMPLE:
  bead resource add <ID> --key gpu:0 --key docker:daemon")]
    Add(ResourceAddOptions),

    /// Remove one or more local resource keys
    #[command(long_about = "Remove normalized resource declarations from an issue.

This changes scheduling metadata in the current native workspace only; it
does not coordinate or unlock resources in another store. Removing keys from
an in-progress issue requires its lease fencing token when applicable.

EXAMPLE:
  bead resource remove <ID> --key gpu:0")]
    Remove(ResourceRemoveOptions),

    /// List declared local resource keys
    #[command(long_about = "List normalized resource declarations for an issue.

The result describes scheduling keys in this native workspace only. Resource
declarations are not distributed-lock state and do not describe another
workspace.

EXAMPLE:
  bead resource list <ID> --json")]
    List(ResourceListOptions),
}

#[derive(Parser, Debug)]
pub struct ResourceAddOptions {
    /// Issue ID
    pub id: String,
    /// Resource key; repeat for multiple keys
    #[arg(long = "key", required = true)]
    pub keys: Vec<String>,
    /// Fencing token for a leased in-progress issue
    #[arg(long)]
    pub fencing_token: Option<i64>,
}

#[derive(Parser, Debug)]
pub struct ResourceRemoveOptions {
    /// Issue ID
    pub id: String,
    /// Resource key; repeat for multiple keys
    #[arg(long = "key", required = true)]
    pub keys: Vec<String>,
    /// Fencing token for a leased in-progress issue
    #[arg(long)]
    pub fencing_token: Option<i64>,
}

#[derive(Parser, Debug)]
pub struct ResourceListOptions {
    /// Issue ID
    pub id: String,
    /// Output JSON
    #[arg(long)]
    pub json: bool,
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
With --starvation-recovery, automatically diagnoses and repairs common
starvation causes that prevent workers from claiming beads.

EXAMPLES:
  bead doctor                           # Read-only diagnostics
  bead doctor --repair                  # Diagnose and attempt repairs
  bead doctor --rehearse                # Test disaster recovery with temporary workspace
  bead doctor --starvation-recovery     # Diagnose and repair starvation issues
  bead doctor --starvation-check        # Diagnose starvation issues without repair

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
  - Recreate missing workspace structure directories (receipts)
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

STARVATION RECOVERY (with --starvation-recovery):
  - Run SQLite integrity checks on beads.db
  - Verify checkpoint/current.json matches database state
  - Identify and fix beads with inconsistent status (e.g., assigned-but-open)
  - Reset stale_since timestamps on beads that appear stuck
  - Clear stale assignees from open beads
  - Release stale in-progress claims
  - Log all repairs to .beads/doctor-recovery.log for audit

STARVATION CHECK (with --starvation-check):
  - Diagnose beads that are open but not appearing in the ready frontier
  - Identify reasons for exclusion (assignees, manual blocks, dependencies, conflicts)
  - Generate detailed report without performing repairs

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

    /// Run starvation check: diagnose beads that are open but not appearing in the ready frontier
    #[arg(long)]
    pub starvation_check: bool,

    /// Run starvation recovery: automatically diagnose and repair common starvation causes
    #[arg(long)]
    pub starvation_recovery: bool,

    /// Run visibility check: compare open bead count against ready frontier query and log discrepancies
    #[arg(long)]
    pub visibility_check: bool,

    /// Force mutations during starvation recovery (default: recommendation-only)
    #[arg(long, requires = "starvation_recovery")]
    pub force: bool,

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
  - auto_flush: reports that this binary publishes a checkpoint
    generation after every successful semantic mutation

AUTO_FLUSH:
  The additive auto_flush field reports the compiled default, not
  workspace state: a workspace that disables publication through
  checkpoint.auto_flush and an invocation passing --no-auto-flush
  change what the binary does, never what it advertises. The field is
  present and true under the automatic default. Consumers that require
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
  bead query --checkpoint .beads/checkpoint/previous.json --file open_high.json
  bead query --file alice_work.json --output-json      # Machine-readable results
  bead query --file open_high.json --save-as hot       # Save as a named view
  bead query --view hot --output-json                  # Run a saved view
  bead query --list-views                              # List saved views

Note: --json supplies the query document itself. Use --output-json to render
results as JSON. --checkpoint selects a verified historical generation and
always renders an explicitly non-importable JSON archaeology view; saved-view
operations are deliberately unavailable against historical state.

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

    /// Verified historical checkpoint artifact (pointer, manifest, or monolithic object)
    #[arg(long)]
    pub checkpoint: Option<String>,

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

/// Options for analyze-exclusion command
#[derive(Parser, Debug)]
#[command(
    about = "Analyze bead exclusion from ready frontier",
    long_about = "Analyze beads and determine why they are excluded from the ready frontier."
)]
pub struct AnalyzeExclusionOptions {
    /// Output in JSON format
    #[arg(long)]
    pub json: bool,

    /// Maximum number of open beads to analyze (0-999999)
    #[arg(long, default_value = "100")]
    pub limit: i64,

    /// Attach analysis as a comment to the specified bead ID
    #[arg(long)]
    pub attach: Option<String>,

    /// Include SQL query in output
    #[arg(long)]
    pub show_sql: bool,

    /// Suppress auto-checkpoint publication for this invocation
    #[arg(long)]
    pub no_auto_flush: bool,
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

/// Manifest bulk-transaction commands (R033)
#[derive(Subcommand, Debug)]
pub enum ManifestCommand {
    /// Report a manifest's full semantic delta without mutating anything
    #[command(
        name = "dry-run",
        about = "Report a manifest's full semantic delta without mutating anything",
        long_about = "Validate and execute a manifest inside one transaction that is
always rolled back.

The report is exact: document validation failures, not-found targets,
transition refusals, revision and lease guards, and cycle detection all fire
against the same pinned snapshot a commit would run on, so a dry-run that
succeeds commits. Nothing is written: no event is appended durably, no
sequence advances, no checkpoint generation is published. Created IDs in the
report are provisional -- only a commit's result map carries real IDs;
correlate through the create's local_id.

  bead manifest dry-run --input plan.json
  bead manifest dry-run --input plan.json --format json"
    )]
    DryRun(ManifestOpOptions),

    /// Apply every manifest operation in one all-or-none transaction
    #[command(
        about = "Apply every manifest operation in one all-or-none transaction",
        long_about = "Execute a manifest's operations in array order inside one SQLite
transaction and commit exactly once.

Any failure rolls back everything: no partial issue, label, edge, event,
lock, or revision survives. Because the whole manifest is one command
invocation committing one transaction, the automatic post-commit chokepoint
publishes at most one checkpoint generation covering the entire manifest --
N commands would publish N generations, one manifest publishes one. A
manifest that changes nothing semantic publishes no generation at all.
Publication failures after the commit are the standard split outcome: the
manifest stays committed, exit 1, and 'bead sync flush-only' is the remedy.

  bead manifest commit --input plan.json
  bead manifest commit --input plan.json --format json   # result map with real IDs"
    )]
    Commit(ManifestOpOptions),
}

/// Options shared by `manifest dry-run` and `manifest commit`: the two
/// subcommands differ only in what the transaction does at the end, so
/// they take the same arguments.
#[derive(Parser, Debug)]
pub struct ManifestOpOptions {
    /// Path to the manifest JSON file
    #[arg(long)]
    pub input: String,

    /// Output format: text or json
    #[arg(long, default_value = "text")]
    pub format: String,
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

/// Options for the watchdog command
#[derive(Parser, Debug)]
#[command(
    about = "Monitor bead claim duration and auto-release stuck claims",
    long_about = "Watchdog monitors in_progress beads and automatically releases those that appear stuck.

A bead is considered stuck if:
  - It has been in_progress for longer than the threshold (default: 4 hours)
  - The assigned worker process is no longer running

The watchdog checks process liveness by searching for the assignee name in the
process table. If the worker is dead, the bead is automatically released back to
the ready frontier. All auto-releases are logged to .beads/watchdog-releases.jsonl.

EXAMPLES:
  bead watchdog --dry-run                                 # Report what would happen
  bead watchdog --threshold 8h                            # Use 8-hour threshold
  bead watchdog --threshold 2h --force                     # Release beads stale >2h
  bead watchdog --json                                     # Machine-readable output"
)]
pub struct WatchdogOptions {
    /// Maximum age of an in_progress bead before it's considered stale (default: 4h)
    #[arg(long, default_value = "4h")]
    pub threshold: String,

    /// Don't actually release beads, just report what would happen
    #[arg(long)]
    pub dry_run: bool,

    /// Force release of stale beads even if assignee process appears alive
    #[arg(long)]
    pub force: bool,

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
