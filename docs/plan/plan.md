# bead-rs 0.1 implementation plan

Status: implementation-ready clean-room plan.

This is the execution blueprint for the first usable `bead-rs` release. The
installed executable is `bead`. SQLite is its authoritative live store,
`.beads/issues.jsonl` is the portable recovery backup and interchange artifact,
and the initial compatibility target is NEEDLE v1. SQLite provides the live
ACID working state. This plan defines an independent native architecture; it is
not a translation of another bead implementation.

## 1. Authority and clean-room boundary

Implementers may use only `AGENTS.md`, `PROVENANCE.md`, this plan, normative
files in `research/specs/`, independently authored `research/fixtures/`, public
standards, and public dependency API documentation.

`research/specs/observed-behavior-v1.md` contains sanitized process-boundary
facts. It does not authorize inspection or reproduction of another
implementation's source, tests, fixtures, SQL, schema, internal names, help
prose, or error prose. Implementation work must not inspect `beads_rust`,
`bead-forge`, or another bead implementation. If prohibited material becomes
visible, stop the affected component and append the exposure to
`PROVENANCE.md` before proceeding.

Normative specifications prevail if this plan contradicts them. Resolve such a
conflict by correcting the plan, never by silently changing the requirement.

## 2. Release definition

Version 0.1 is complete when F001-F014 in `.marathon/feature_list.json` pass
and every release gate in `.marathon/instruction.md` succeeds.

In scope:

- workspace initialization and versioned native SQLite migrations;
- issue CRUD, assignment, lifecycle, labels, notes, and dependencies;
- deterministic readiness and atomic server-selected claiming;
- deterministic checkpoint import/export and unknown-field preservation;
- explicit per-bead public schema identification;
- diagnostics and narrowly scoped repair;
- machine-readable capabilities and NEEDLE v1 subprocess compatibility;
- explicit `native-v1`, `needle-v1`, `br-v1`, and `bf-v1` profiles;
- migration dry-run and receipts;
- an Apache-2.0 Rust crate providing the `bead` binary.

Out of scope:

- reading or modifying another implementation's SQLite database;
- daemon mode, network sync, or multi-host consensus;
- Git automation or automatic commits by the application;
- fuzzy dependency-direction inference;
- silent recovery from malformed JSONL;
- crates.io publication;
- native SQLite backup/restore formats; JSONL is the supported backup boundary;
- compatibility claims without corresponding conformance evidence.

## 3. Canonical domain model

### 3.1 Identity

Imported issue IDs are opaque nonempty UTF-8 strings. Reject control
characters, leading/trailing whitespace, path separators, NUL, and values over
255 bytes. Preserve valid imported IDs byte-for-byte.

Native creation generates:

```text
<workspace-prefix>-<16 lowercase hexadecimal characters>
```

The suffix is 64 random bits from the operating-system CSPRNG. Insert under a
unique constraint and retry a collision up to five times. The immutable
workspace prefix defaults to `bead` and must match
`[a-z][a-z0-9-]{0,31}`. This is an independent ID design; do not imitate
another tool's suffix algorithm.

### 3.2 Issue fields

| Field | Native invariant |
| --- | --- |
| `id` | immutable opaque identifier |
| `title` | required, 1 to 4,096 UTF-8 bytes |
| `description` | defaults empty, at most 4 MiB |
| `notes` | defaults empty, at most 4 MiB |
| `priority` | signed integer; native CLI accepts 0 through 4 |
| `issue_type` | nonempty string, default `task` |
| `base_status` | `open`, `in_progress`, `deferred`, or `closed` |
| `manual_blocked` | explicit operator block, separate from graph blockers |
| `assignee` | absent or nonempty UTF-8 string |
| `created_at` | immutable UTC instant |
| `updated_at` | UTC instant advanced by every semantic mutation |
| `closed_at` | present only for closed issues |
| `close_reason` | nonblank for closed issues; required by `close` |
| `source_repo` | optional source-workspace descriptor |
| `profile` | origin profile for extension round trips |
| `schema_ref` | absolute URI naming the immutable public schema governing this bead representation |
| `extensions` | unknown top-level JSON values keyed by original name |

Native v1 records use
`urn:bead-rs:schema:issue:native-v1`. A schema reference describes the public
JSON representation, never the private SQLite layout. Unknown references are
preserved during inspection/migration but fail closed for activation unless an
explicit profile adapter declares compatibility.

Labels, dependency edges, comments, claim telemetry, and audit events are
normalized child records. Reads assemble them into an interchange view.

### 3.3 Lifecycle and effective status

Allowed base transitions are:

| From | To | Operation |
| --- | --- | --- |
| `open` | `in_progress` | successful claim or explicit update |
| `open` | `deferred` | update |
| `open` | `closed` | close |
| `in_progress` | `open` | release/update |
| `in_progress` | `deferred` | update |
| `in_progress` | `closed` | close |
| `deferred` | `open` | reopen/update |
| `deferred` | `closed` | close |
| `closed` | `open` | reopen |

`blocked` is an effective status, not a terminal base state. An unfinished
`blocks` edge or `manual_blocked = true` makes a nonclosed issue report
`blocked`. `update --status blocked` sets the manual flag while retaining its
base state. `update --status open` clears the flag and sets the base state to
open. Finishing the last graph blocker reveals the stored base status unless
the manual flag remains set.

Profiles map `done` and `completed` explicitly to `closed`. Unknown imported
status values are retained in extensions and rejected for activation unless
the selected profile defines a mapping; they are never treated as open.

`close` requires a nonblank reason, sets `closed`, clears manual blocking, and
sets `closed_at`; assignment is retained. `reopen` sets `open`, clears closure
metadata and manual blocking, and retains assignment. Release sets `open` and
clears assignment.

### 3.4 Dependencies and readiness

Every edge is canonicalized as `(blocked_issue_id, blocker_issue_id, kind)`.
Both IDs must exist. Self-edges are invalid. Duplicate adds and removal of an
absent edge succeed idempotently. Version 0.1 supports `blocks` and
`relates_to`; only `blocks` affects readiness.

Reject a `blocks` edge if it creates a directed cycle, with detection and
insertion in the same transaction. `relates_to` cycles are allowed.

An issue is ready exactly when it is base `open`, not manually blocked,
unassigned, and has no unfinished `blocks` blocker. A blocker is unfinished
unless its base state is `closed`. Compute readiness from authoritative rows;
version 0.1 has no readiness cache.

### 3.5 Claim selection

Eligible issues sort by lower priority, then earlier `created_at`, then lexical
ID. Claim starts `BEGIN IMMEDIATE`, selects one ready issue, changes it to
`in_progress`, assigns the requested actor, writes telemetry and an audit
event, and commits before emitting success. With no eligible issue, return exit
0 and `{}` in JSON mode without mutation.

Model, harness, and harness-version flags are telemetry only. Twenty competing
processes must never receive the same successful issue ID.

## 4. Workspace and independent SQLite design

### 4.1 Layout and discovery

```text
.beads/
  beads.db          authoritative native SQLite database
  issues.jsonl      portable recovery backup, present after first flush
  config.json       nonsecret workspace configuration
  receipts/         migration receipts created on request
  .gitignore        ignores journals and temporary files
```

`bead init [--prefix PREFIX]` creates this workspace without modifying
unrelated files. Repeating it with the same prefix succeeds; a conflicting
prefix fails without mutation. Use user-only write permissions where
supported.

Workspace discovery walks from the current directory toward the filesystem
root until `.beads/config.json` is found. Never follow a `.beads` symlink
outside the selected workspace for a mutation.

### 4.2 Connection policy

Every connection enables foreign keys and a five-second busy timeout.
Initialize in WAL mode with `synchronous=NORMAL`. Mutations use explicit
transactions; multi-row semantic changes use `BEGIN IMMEDIATE` so validation
and mutation share one serialization point. Use parameterized statements
behind the store boundary, never ad hoc shell access.

### 4.3 Semantic schema

Migration 1 creates this independently authored schema. Exact SQL is an
implementation task and must be written without consulting another bead
database definition.

| Table | Required data and constraints |
| --- | --- |
| `schema_migrations` | integer version PK, applied time, migration checksum |
| `workspace` | singleton store UUID, prefix, layout version, creation time |
| `issues` | canonical scalars, ID PK, lifecycle checks, timestamps |
| `issue_extensions` | issue ID + key PK, canonical JSON, origin profile |
| `labels` | issue ID + label PK, issue FK cascade |
| `dependencies` | blocked + blocker + kind PK, two issue FKs cascade, no self-edge |
| `comments` | random ID, issue ID, author, body, creation time |
| `claim_telemetry` | issue ID, claim time, assignee, optional model/harness/version |
| `events` | integer sequence, issue ID, kind, actor, time, canonical JSON detail |
| `checkpoint_state` | singleton last hash, event sequence, export time |

Add only indexes justified by v0.1 queries:

- issues on `(base_status, manual_blocked, assignee, priority, created_at, id)`;
- dependencies by blocker and by blocked issue;
- labels by label and issue;
- comments/events by issue plus time/sequence.

Do not add caches, tombstones, recovery subsystems, or compatibility-shaped
columns without a measured requirement and new migration.

### 4.4 Migrations

On open, compare the database version with the newest embedded migration.
Apply pending migrations in one exclusive transaction and record checksums.
Refuse a database newer than the executable or a checksum mismatch. A failed
migration rolls back fully. Test version 0, current, corrupt,
checksum-mismatched, and future databases.

## 5. Command and process contract

Use `clap` derive for parsing and keep domain logic independent of CLI types.
Human output may evolve; named-profile machine output is stable.

| Command | Version 0.1 behavior |
| --- | --- |
| `bead init [--prefix P]` | initialize or verify workspace |
| `bead create --title T --description D [--label L]...` | create and print only ID plus LF |
| `bead list --json [--status S] [--assignee A] --limit N` | records in claim order; limit 0-999999 |
| `bead show ID --json` | one-element JSON array for NEEDLE v1 |
| `bead claim --assignee A [telemetry] --json` | atomic claim; one JSON object |
| `bead update ID [--status S] [--assignee A] [--notes N]` | atomically apply supplied changes |
| `bead reopen ID` | restore open lifecycle |
| `bead close ID --reason TEXT` | finish with retained reason |
| `bead label add ID --label L` | idempotent presence |
| `bead label remove ID --label L` | idempotent absence |
| `bead dep add BLOCKED BLOCKER --type KIND` | add canonical edge |
| `bead dep remove BLOCKED BLOCKER [--type KIND]` | remove matching edge(s) |
| `bead sync --flush-only [--profile P]` | atomic checkpoint export |
| `bead sync --import-only [--profile P]` | transactional reconciliation |
| `bead doctor [--repair]` | diagnose; optionally perform safe repairs |
| `bead capabilities --format json --profile P` | versioned capabilities and supported schema references |
| `bead schema list --format json` | list supported public document schemas |
| `bead schema show SCHEMA_REF --format json` | emit the exact versioned JSON Schema document |
| `bead migrate --from P --input I --output O [--dry-run]` | transform without overwriting input |

The native command is `bead`. Do not create a `br` executable; that name is a
deprecated compatibility shim in the surrounding environment. Alternate
legacy spellings must be explicit adapters, never silent native behavior.

### 5.1 Machine output

Diagnostics go only to stderr. JSON stdout is valid UTF-8 with no progress
text. Serialize stable field order although consumers must not rely on object
key order. NEEDLE issue JSON always includes:

```json
{
  "id": "bead-0123456789abcdef",
  "title": "Example",
  "description": "",
  "priority": 2,
  "status": "open",
  "assignee": null,
  "dependencies": [],
  "created_at": "2026-08-07T12:00:00Z",
  "updated_at": "2026-08-07T12:00:00Z",
  "labels": []
}
```

Native dependencies use explicit `blocked`, `blocker`, and `kind` keys.
Profile writers may add required aliases but must never reverse direction.

List emits one compact object per line; show emits a one-element array; claim
emits `{"bead_id":"...","assignee":"..."}` on success and `{}` when empty.
These are valid NEEDLE v1 envelopes and make empty results explicit.

### 5.2 Exit taxonomy

| Exit | Meaning |
| --- | --- |
| 0 | success, including empty claim queue |
| 2 | CLI usage or validation error |
| 3 | workspace or not-found error |
| 4 | conflict, invalid transition, or dependency cycle |
| 5 | integrity, import, or migration failure |
| 6 | transient database busy or I/O failure |
| 1 | uncategorized internal failure |

Use structured internal errors and print one concise diagnostic at the CLI
boundary. Do not expose SQL, secrets, environment values, or backtraces by
default.

## 6. JSONL backup and compatibility profiles

### 6.1 Canonical JSONL

JSONL is UTF-8 with one compact object and LF per issue. Blank lines may be
ignored. Malformed or non-object lines fail with a one-based line number.
Reject duplicate IDs before activating any state.

Canonical export order is ID ascending. Known fields follow the interchange
specification, optional known fields follow, and extension keys sort lexically.
Labels sort lexically and are unique. Dependencies sort by blocker ID, kind,
then blocked ID. Comments retain creation order with ID as tie-breaker.
Timestamps emit UTC RFC 3339 and retain imported fractional precision while
the represented instant is unchanged.

Known fields win over same-name extension keys. Report that collision as a
transformation; never emit duplicate JSON keys silently.

### 6.2 Backup flush algorithm

SQLite is authoritative between flushes because it supplies transactional live
operation. `.beads/issues.jsonl` is the supported portable backup at the last
successful flush and the source for disaster recovery into a newly initialized
store. The CLI and documentation must call out its recorded snapshot sequence
and freshness; they must never imply that an older backup contains unflushed
mutations. There is no separate native SQLite backup format.

1. Open a read transaction and capture the event sequence.
2. Assemble all records from that single committed snapshot.
3. Serialize to a uniquely named temporary sibling in `.beads/`.
4. Flush and `sync_all` the file.
5. Atomically rename it over `.beads/issues.jsonl`.
6. Sync the parent directory where supported.
7. Record SHA-256, snapshot sequence, and time in a short write transaction.

A write after step 1 may make the checkpoint an older committed snapshot; its
recorded sequence makes this explicit. Never truncate the prior checkpoint in
place. On failure preserve it and remove only this operation's temporary file.

### 6.3 Import reconciliation

`sync --import-only` fully parses and validates `.beads/issues.jsonl` before
activation. Default safety limits are 1 million records, 16 MiB per line, 4
GiB total, and `serde_json`'s bounded nesting behavior.

In one write transaction:

- insert IDs absent from native state;
- replace only when imported `updated_at` is later;
- retain native state when its timestamp is later;
- treat equal timestamps with unequal semantic content as a conflict and roll
  back the entire import;
- never delete native issues because they are absent from the checkpoint;
- validate endpoints and cycles against the final staged graph;
- preserve unknown values under their source profile.

After commit, report inserted, updated, retained, and conflicted counts.
Dry-run performs the same staging and conflict analysis without activation.

### 6.4 Profile rules

- `native-v1`: canonical fields and explicit dependency objects; native export
  default.
- `needle-v1`: the normative consumer CLI/output contract.
- `br-v1`: enabled only after independently captured and approved fixtures
  specify it.
- `bf-v1`: enabled only after independently captured and approved fixtures
  specify it.

F012 requires for each external profile: a field-presence matrix, status
mapping, dependency-direction declaration, null/absent behavior, timestamp
rules, independent fixtures, and loss report. Unsupported profiles fail
closed.

The observed `bf` spelling `dep add BLOCKER --blocks BLOCKED` belongs only to
an explicit future CLI adapter. Native and NEEDLE syntax remains
`dep add BLOCKED BLOCKER --type blocks`.

Each emitted native bead includes its `schema_ref`. Profiles explicitly map,
preserve, or report omission of the reference. Supported public schemas use
immutable absolute identifiers and JSON Schema Draft 2020-12; the schema
document's `$id` equals the reference. See
`research/specs/schema-identification-v1.md`.

### 6.5 Migration receipts

Migration takes explicit, distinct input and output paths. Resolve aliases and
reject identical files before opening output. Write through a temporary sibling
and atomic rename.

The canonical JSON receipt includes tool version, UTC time, profiles, input
and output SHA-256, record counts, transformation counts, warnings, and dry-run
state. It excludes user content, credentials, absolute home paths, and
environment values.

## 7. Diagnostics and recovery

`doctor` is read-only and checks:

- workspace/config parsing and permissions;
- database open, SQLite `quick_check`, foreign keys, schema version, and
  migration checksums;
- lifecycle and timestamp invariants;
- dangling or cyclic blocking edges;
- extension JSON validity;
- checkpoint parseability and recorded-hash freshness;
- orphaned temporary files owned by `bead-rs`.

Warnings begin exactly `WARN `; healthy lines may use `OK `. Failed integrity
checks exit nonzero.

`doctor --repair` diagnoses first and may repair only stale checkpoint
metadata, proven-stale operation-owned temporary files, missing safe indexes,
or a missing checkpoint by normal atomic flush after database integrity passes.
Every repaired line begins `FIXED `.

Version 0.1 never reconstructs issue rows, drops unknown tables, deletes the
database, rewrites a corrupt checkpoint, or alters lifecycle/dependency data
automatically. Diagnose those cases and recommend explicit manual recovery.

## 8. Rust architecture

Start with one package containing a library and binary. Split crates only when
a measured build or API boundary requires it.

```text
src/
  lib.rs              orchestration API
  main.rs             parsing, rendering, exit mapping
  cli.rs              clap definitions
  model.rs            validated domain types and transitions
  store/
    mod.rs            transaction boundary
    sqlite.rs         rusqlite implementation
    migrations.rs     independently authored migrations
  service/
    issues.rs         CRUD and lifecycle
    claim.rs          readiness and claim
    dependencies.rs   graph validation
    checkpoint.rs     snapshot/import
    doctor.rs         diagnosis and repair
    migrate.rs        profile conversion and receipts
  profile/
    mod.rs            adapter interface
    native_v1.rs
    needle_v1.rs
  output.rs           deterministic rendering
  error.rs            error taxonomy
tests/
  cli/                isolated subprocess tests
  conformance/        normative lanes
  concurrency/        multiprocess tests
research/fixtures/    independent fixtures and manifests
```

Suggested dependencies, subject to Rust 1.75 verification:

- `clap` 4 with derive;
- `rusqlite` with bundled SQLite;
- `serde` and `serde_json`;
- `time` or `chrono` for RFC 3339;
- `rand` with the OS random source;
- `sha2` for hashes;
- `thiserror`, with `anyhow` only at the binary boundary if useful;
- `tempfile`, `assert_cmd`, and `predicates` for development.

Commit `Cargo.lock` because this package ships a binary. Verify selected
versions on Rust 1.75 before accepting F001. Put
`#![forbid(unsafe_code)]` in project crates.

## 9. Verification design

Every filesystem/subprocess test receives a new temporary workspace and HOME.
Never point tests at `/home/coding`, a contributor's real `.beads`, or another
implementation's database. Capture stdout, stderr, status, and filesystem
effects. Every fixture manifest records author, date, requirement, independent
creation method, and SHA-256.

Required layers:

1. Unit tests for validation, transitions, profile mapping, deterministic
   serialization, and cycle detection.
2. Store tests for transactions, migrations, constraints, rollback, snapshot
   isolation, and interruption recovery.
3. CLI subprocess tests for every NEEDLE invocation and output envelope.
4. Multiprocess tests with at least 20 simultaneous claimers.
5. Independently generated property tests for Unicode round trips and acyclic
   graphs, if property testing is added.
6. Package tests installing the `.crate` into a temporary Cargo root.

Critical scenarios beyond the conformance specification:

- empty list versus malformed output; empty claim versus database busy;
- priority/timestamp ties and two processes claiming the last issue;
- blocker closure/reopen in chains and diamonds;
- manual blocking coexisting with graph blocking;
- direct and indirect cycles; idempotent label and edge mutations;
- invalid transitions and close without reason;
- Unicode, multiline text, quotes, NUL rejection, and size limits;
- checkpoint failure before rename and writers during snapshot;
- malformed line N, duplicate IDs, future statuses, dangling edges, and equal
  timestamp conflicts;
- known/extension key collision;
- future schema refusal and migration rollback;
- symlink and path-alias attempts in migration;
- no diagnostics or secrets on JSON stdout.

Concurrency tests assert semantic results, not timing alone. Use barriers or
child-process coordination, bounded deadlines, and final inspection through
the public library API.

## 10. Marathon execution order

The feature ledger remains release authority. Execute it in these increments:

### Phase A: trustworthy native core

1. **F001:** package, config, connection policy, migration 1, idempotent init,
   future-version refusal.
2. **F002:** validated types, IDs, timestamps, transition matrix, extensions.
3. **F003:** create/list/show and stable machine output.
4. **F004:** readiness, transactional claim, empty result, 20-process test.

### Phase B: work coordination

5. **F005:** atomic update/release, close/reopen, audit events.
6. **F006:** labels, canonical edges, cycles, derived blocking, graph tests.
7. **F007:** single-snapshot deterministic, crash-safe flush.
8. **F008:** staged import, extension preservation, reconciliation/rollback.
9. **F009:** read-only doctor and narrow repair allowlist.
10. **F010:** immutable capability document.

### Phase C: compatibility and release

11. **F011:** full NEEDLE subprocess matrix in isolated workspaces.
12. **F012:** external profile matrices, independent fixtures, loss reports.
13. **F013:** dry-run, path safety, atomic migration output, receipts.
14. **F014:** package/install smoke test, licensing, provenance verification.

One Marathon iteration implements one coherent increment, runs targeted and
repository gates, changes a feature's `passes` and evidence only after all its
acceptance criteria succeed, appends a handoff, and commits. A large feature
may take multiple iterations and remains false until complete.

## 11. Capability document

`bead capabilities --format json --profile needle-v1` returns at least:

```json
{
  "contract": "needle-v1",
  "implementation": "bead-rs",
  "version": "0.1.0",
  "store_layout": 1,
  "atomic_claim": true,
  "statuses": ["blocked", "closed", "deferred", "in_progress", "open"],
  "checkpoint_modes": ["flush-only", "import-only"],
  "schemas": ["urn:bead-rs:schema:issue:native-v1"],
  "commands": ["claim", "close", "create", "dep", "doctor", "label", "list", "reopen", "schema", "show", "sync", "update"]
}
```

Arrays are lexically stable. Additive fields are allowed. An unsupported
profile exits nonzero instead of returning mislabeled native capabilities.

## 12. Adopted post-0.1 roadmap

These features are accepted for development after the F001-F014 release core.
They require their own normative specifications, conformance scenarios, and
future Marathon ledger entries before implementation begins.

### R001 — Explain claim and readiness decisions

Add a nonmutating, machine-readable decision trace with versioned semantic
reason codes for lifecycle, assignment, blockers, manual blocking, resource
conflicts, and other eligibility rules. This makes empty queues and surprising
selection behavior diagnosable without revealing SQL or private store details.

### R002 — Fenced claim leases

Add opt-in expiring claims, renewals, and monotonically increasing fencing
tokens. A stale worker must be unable to update or close work after expiry and
reassignment. This provides safe recovery from crashed or disconnected agents
without weakening the simple nonleased claim path.

### R003 — Logical revision guards

Give each bead a monotonically increasing logical revision and accept an
`--if-revision` precondition on mutations. This prevents silent lost updates
across concurrent humans and workers without depending on wall-clock ordering.
Profiles must state whether and how they preserve the revision.

### R004 — Safe query language and saved views

Define a small, versioned, typed query grammar for supported fields,
dependency/readiness predicates, deterministic sorting, projections, and named
local views. It must never expose raw SQL or the private schema. A deliberately
limited first grammar replaces fragile shell filtering while keeping query
cost and compatibility bounded.

### R005 — Machine-readable schemas and per-bead schema references

Ship immutable JSON Schema documents for public issue records, capabilities,
migration receipts, decision traces, and future bulk/error formats. Every
native bead carries `schema_ref`, allowing consumers to identify exactly which
public schema governs it. `bead schema` resolves supported identifiers, while
capabilities and migration receipts enumerate them. This gives interoperating
tools an explicit validation contract without making SQLite an API.

## 13. Release gates

Before `.marathon/COMPLETE`:

- F001-F014 have concrete passing evidence;
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo test` pass on the release commit;
- native, interchange, NEEDLE, concurrency, and migration lanes pass;
- `cargo package` succeeds from a clean checkout;
- the packaged crate installs into a temporary root and its `bead` completes
  init, create, list, claim, update, dependency, flush/import, doctor,
  capabilities, and close smoke workflows;
- `LICENSE`, `NOTICE`, `README.md`, `PROVENANCE.md`, specs, and fixture
  manifests are accurate and packaged as intended;
- no `br` shim, upstream-derived artifact, credential, real workspace, or
  disposable research database is packaged;
- compatibility claims name exact profiles and known losses;
- publication remains separately human-authorized.

## 14. Deferred feature notes

The following candidates remain intentionally deferred in
`docs/notes/ideas-ledger.md` and are not roadmap commitments:

- atomic resource locks;
- atomic bulk transaction manifests;
- mutation idempotency keys;
- worker capability declarations.

Native SQLite backup/restore is rejected. Deterministic JSONL flush/import is
the backup and recovery contract; SQLite exists primarily to provide ACID live
operation.

## 15. Inputs still required for external profiles

The core can proceed now. F012 still needs complete independently approved
field/nullability/status/dependency fixtures for `br-v1` and `bf-v1`. F014
also needs a consumer-side NEEDLE run if its deployment harness imposes a
requirement absent from the v1 contract.

Do not guess missing details. Record new sanitized observable facts in a
versioned `research/specs/` file, review them independently, then extend only
the relevant adapter and fixture. These gaps do not block F001-F011, but
F012-F014 cannot be declared complete without their evidence.
