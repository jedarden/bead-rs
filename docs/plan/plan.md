# bead-rs 0.1 implementation plan

Status: core implementation-ready; the 0.1 release remains externally blocked
on the independently approved `br-v1` and `bf-v1` fixtures required by F012.

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

Version 0.1 is complete when F001-F017 in `.marathon/feature_list.json` pass
and every release gate in `.marathon/instruction.md` succeeds.

“Core implementation-ready” means F001-F011, F013, and F015-F017 have enough
clean-room specification to proceed. It is not a release-readiness claim.
F012 cannot start its external-profile conformance implementation until the
inputs in section 15 exist, and F014/package release cannot pass while F012 is
blocked. No profile, fixture, evidence, or gate may be waived to turn that
external dependency into a nominal 0.1 release.

In scope:

- workspace initialization and versioned native SQLite migrations;
- issue CRUD, assignment, lifecycle, labels, notes, and dependencies;
- deterministic readiness and atomic server-selected claiming;
- deterministic monolithic or sharded checkpoint import/export, complete
  forensic history, and unknown-field preservation;
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
| `priority` | native P0-P4 urgency class; lower is more urgent, default P2 |
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
| `data` | namespaced, schema-bound JSON values for portable structured extensions |
| `extensions` | unknown top-level JSON values keyed by original name |

Native v1 records use
`urn:bead-rs:schema:issue:native-v1`. A schema reference describes the public
JSON representation, never the private SQLite layout. Unknown references are
preserved during inspection/migration but fail closed for activation unless an
explicit profile adapter declares compatibility.

Labels, dependency edges, comments, claim telemetry, and audit events are
normalized child records. Reads assemble them into an interchange view.
Recovery-backup views always include complete comments and structured data.
Ordinary list/show views omit comment bodies by default and accept
`--comments unresolved|all`; this makes conversational context optional without
making the backup incomplete.

Native priority taxonomy is:

| Value | Name | Scheduling intent |
| --- | --- | --- |
| P0 / `0` | urgent | immediate incident, safety, or release-blocking work |
| P1 / `1` | critical | essential work that should precede ordinary delivery |
| P2 / `2` | high | important planned work and the native default |
| P3 / `3` | normal | ordinary work with no elevated urgency |
| P4 / `4` | aspirational/backlog | speculative, someday, or low-urgency work, claimable only when policy permits |

Native create/update rejects values outside 0-4. JSON stores the integer and
human output may show the `P` name. The P0-P4 range intentionally matches the
observed bead ecosystem and avoids priority clamping or lossy transformations
at compatibility boundaries. Profiles still state their supported range and
must report any narrower mapping.

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
metadata and manual blocking, and retains assignment. `release ID` accepts an
`in_progress` bead, sets its base state to `open`, clears its assignment, and
leaves its last-claim sequence and attempt history intact. Releasing an already
open, unassigned bead succeeds idempotently without advancing `updated_at` or
appending an event. Releasing an open but assigned bead, or a `deferred` or
`closed` bead, is an invalid-transition conflict (exit 4) and makes no change;
callers must use the explicit lifecycle operation appropriate to that state.
A semantic release appends one durable `released` audit event containing the
bead ID, prior assignee, actor when known, and resulting base state, then prints
the released ID plus LF. Validation, state change, and event append occur in
one write transaction.

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

Claim is a server-selected scheduling operation, not a client-side list followed
by update. In 0.1, selection, final eligibility validation, assignment, the
minimal claim audit record, and the `fifo-v1` tie-break state commit in one
`BEGIN IMMEDIATE` transaction. Version 0.1 has no lease request, expiry,
renewal, or fencing-token fields. Twenty competing processes must never receive
the same successful issue ID.

Version 0.1 implements only `fifo-v1`: eligible issues sort by declared priority
ascending, `created_at` ascending, then ID ascending. With no eligible issue,
claim returns exit 0 and `{}` in JSON mode without mutation. The richer policies
below are adopted post-0.1 behavior and must not silently change `fifo-v1`.

Sections 3.5.1-3.5.9 specify the R019 extension unless a paragraph explicitly
labels a `fifo-v1` invariant. Their intelligent-policy fields, counters,
explanations, caches, retry state, and outcome classifications are not 0.1
schema requirements. References there to leases describe composition with the
separate R002 extension; R019 alone neither implements nor requires leases.

#### 3.5.1 Scheduling pipeline

Every policy uses these stages in order:

1. **Eligibility:** require ready base lifecycle, no assignment, no active
   blocker, no manual block, satisfied worker constraints, expired retry delay,
   and a prompt projection the requesting worker can consume.
2. **Policy ranking:** calculate a deterministic lexicographic tuple using only
   committed state, the transaction's captured selection instant, and the
   versioned workspace policy.
3. **Final validation:** re-read the winner and relevant dependency rows, plus
   lease rows only when the separately implemented R002 capability is active,
   under the write transaction. Cached metrics may rank candidates but never
   establish eligibility.
4. **Commit:** assign the actor, move to `in_progress`, increment the workspace
   claim sequence, record the attempt/policy/factor breakdown, create a lease
   only when the separately implemented R002 capability is requested, and
   append the audit event.
5. **Respond:** emit the small compatibility result or an explicitly requested
   prompt projection only after commit.

Model, harness, and harness-version remain telemetry hints unless a future
capability-matching specification explicitly promotes them to scheduling
inputs. Policy name and version are workspace configuration and appear in
capabilities and every successful claim event.

#### 3.5.2 Ready age

The primary fairness clock is `ready_since`, not `created_at`. Set it when a
bead enters ready state and clear it when the bead becomes unready. If a newly
closed blocker makes a long-lived bead ready, its waiting age starts then; it
does not inherit years of artificial preference from its creation timestamp.
`created_at` remains the stable late tie-breaker.

Age promotion uses integer buckets:

```text
age_promotions = min(max_promotions, floor(ready_age / aging_interval))
effective_priority = max(0, declared_priority - age_promotions)
```

Defaults for an eventual `aging-v1`/`balanced-v1` policy are a 24-hour interval
and at most two promotions. The exact values are versioned configuration. A
captured selection instant makes the calculation internally consistent and the
claim event records the resulting bucket. Aging never bypasses eligibility.
P4 remains eligible native work, but a workspace may require an explicit
`include_aspirational` policy flag before automatic workers claim it. Aging may
promote old P4 work within the configured cap; it never rewrites the declared
priority.

#### 3.5.3 Completion-unlock impact

Impact measures what successful completion would unlock, not raw dependent
count and not what becomes unblocked merely by claiming. Candidate `A`
immediately unlocks dependent `B` only when:

- `A` is an active unfinished blocker of `B`;
- every other active required blocker of `B` is finished; and
- `B` would otherwise be ready after `A` closes: open, unassigned, not manually
  blocked, and permitted by its active conditional dependencies.

Diamonds are deduplicated by dependent ID. Calculate:

- `immediate_unlock_count`;
- the best and ordered priority distribution of immediately unlocked beads;
- `downstream_reach`, the count of unique transitive descendants benefiting
  from completion;
- `critical_path_reduction`, a bounded integer measure of blocking-chain depth.

Use integer tuple components rather than an opaque floating-point score. Raw
fan-out must not beat a bead that is the final blocker for fewer but critical
tasks. `impact-v1` ranks inside effective-priority bands by unlocked priority,
immediate count, critical-path reduction, unique downstream reach, then normal
fairness tie-breakers.

#### 3.5.4 Rotation and least-recently-served fairness

Maintain a monotonically increasing workspace `claim_sequence`. Each bead
records its last successful claim sequence and attempt count. Within comparable
effective priority and attempt tier, rank:

1. never-claimed beads;
2. least recently claimed beads;
3. older ready-age bucket;
4. older creation instant;
5. lexical ID.

When a claim is released, its last-claim sequence remains, so comparable work
gets a turn before it is served again. Rotation uses logical sequence rather
than wall-clock time and never overrides lifecycle, dependencies, leases,
capabilities, resource constraints, or retry/quarantine state.

#### 3.5.5 Failure-aware attempt tiers

Distinguish outcomes before changing scheduling state:

| Outcome | Bead penalty |
| --- | --- |
| bead-scoped failure: invalid assumptions, repeatable build/test failure, inability to satisfy the bead | increment consecutive bead failures |
| infrastructure failure: worker crash, provider outage, rate limit, network loss | no bead penalty |
| claim race | no bead penalty |
| context projection overflow | record separately; no bead-quality penalty |
| stale/expired lease | normally worker/infrastructure failure |
| explicit human release | no penalty unless explicitly classified |

Attempt tiers within the current ready epoch are:

```text
0  unproven: no bead-scoped failure
1  retryable: one bead-scoped failure
2  struggling: multiple failures below quarantine threshold
3  quarantined: ineligible for automatic claim
```

Within the same effective-priority band, unproven open work always ranks ahead
of failed work. A failed priority-0 bead may still rank ahead of an unproven
priority-2 bead; a strict workspace policy may instead compare attempt tier
before effective priority. The selected ordering mode is explicit and
versioned.

Default retry behavior is:

- first bead-scoped failure: defer until comparable unproven work has had an
  opportunity;
- second: set `retry_after_claim_sequence` so a configured number of other
  claims must occur first;
- third consecutive bead-scoped failure: quarantine by default;
- no automatic claim of quarantined work.

To prevent failed work from starving under a continuously replenished queue,
`balanced-v1` reserves a bounded retry lane, initially one eligible retry for
every ten successful normal claims. The retry cadence is persisted in scheduler
state and advanced atomically. A retry slot does not admit quarantined work and
does not override declared/effective priority policy.

Failure counters belong to a readiness/revision epoch. A material mutation to
description, acceptance criteria, structured task data, or dependencies starts
a new attempt epoch and may reset consecutive bead failures while retaining
lifetime attempt history. Cosmetic changes do not reset it.

#### 3.5.6 Versioned policies

- `fifo-v1`: declared priority, creation time, ID; initial compatibility mode.
- `aging-v1`: bounded ready-age promotion, then FIFO tie-breakers.
- `impact-v1`: effective priority, completion-unlock impact, ready age,
  rotation, creation time, ID.
- `rotation-v1`: effective priority, attempt tier, never/least recently served,
  ready age, creation time, ID.
- `balanced-v1`: effective priority, attempt tier, ready-age bucket,
  completion-unlock tuple, least-recently-served sequence, creation time, ID,
  plus the bounded retry lane.

The eventual recommended intelligent default is `balanced-v1`; `fifo-v1`
remains available for reproducibility and compatibility. A released policy
version is immutable. Changing constants or tuple order creates a new version.

#### 3.5.7 NEEDLE and bounded initial context

Current NEEDLE usage claims atomically, then fetches the claimed bead and places
its description verbatim into the model prompt alongside workspace
instructions, context files, and skills. bead-rs therefore separates stored
content from the bounded initial claim projection:

- JSONL backup remains complete and never truncates bead content;
- the `needle-v1` selection view contains only fields required to rank/filter;
- the default claimed-bead view excludes comments, audit history, telemetry,
  and structured data not explicitly selected;
- a compact task brief contains the executable task, acceptance criteria, and
  references to supplementary context;
- large content is listed in a context manifest and retrieved through bounded,
  cursor-based `context list|get|search` operations;
- `--max-initial-bytes` and an optional named token estimator constrain the
  initial projection; no command silently truncates required instructions or
  emits partial JSON.

A worker advertising `bead.context.lazy-v1` may claim beads whose full content
is large when their brief fits. A legacy worker without lazy retrieval receives
only a bead whose complete legacy description fits the configured hard
compatibility ceiling. Context overflow is an explicit eligibility/explanation
reason, not a bead-scoped execution failure.

The native extension may return the selected prompt projection inline with the
claim result, eliminating a second subprocess. The v1 result containing only
`bead_id` remains valid. NEEDLE still owns the final model-specific prompt
budget because it adds context after bead retrieval.

#### 3.5.8 Explainability and observability

Every claim stores the policy version, captured selection instant, effective
priority, ready-age bucket, attempt tier, failure counts, retry-lane decision,
unlock metrics, last claim sequence, context-fit result, and final stable
tie-breakers. `explain-ready` can show why the winner ranked ahead and why
others were ineligible or deferred, using semantic reason codes rather than SQL
or private query plans.

Diagnostics report starvation, repeatedly bypassed work, retry-lane health,
quarantine counts, stale scheduling metrics, excessive claim contention, and
context-fit failures. Telemetry must not include bead bodies or secret
structured data.

#### 3.5.9 Performance and correctness

Ranking uses a hybrid write-maintained/read-finalized design. Do not recalculate
the complete queue after every mutation or scan and fully score every bead for
every claim.

The rankable population is the **ready frontier**, not every bead in the
dependency graph. A frontier bead is open, unassigned, not manually blocked,
and has no active unfinished blocker. Closed, deferred, assigned, manually
blocked, and graph-blocked interior beads never enter candidate ranking.
“Frontier” is used instead of “leaf” because leaf/root terminology reverses
with graph drawing convention. Closing or reopening a blocker and adding or
removing an edge incrementally removes or exposes only affected beads at this
frontier.

Relevant issue, dependency, lifecycle, condition, failure, or structured-data
mutations update inexpensive authoritative inputs such as ready state,
`ready_since`, active-blocker count, attempt tier, retry sequence, and graph
revision in the same transaction. Expensive graph metrics are either updated
for the bounded affected subgraph or marked dirty there. Unrelated beads are
not reranked or invalidated.

At claim time, an indexed authoritative query reads only the ready frontier and
produces a bounded conservative candidate set. The bound and query shape are
policy-versioned: the shortlist must not exclude a frontier bead that could win
that policy. The policy then calculates
time- and request-dependent inputs—age promotion, retry-lane position,
least-recently-served order, worker compatibility, and prompt fit—only for that
set. It scores the complete workspace only when the policy cannot prove a safe
shortlist, and that fallback is observable in diagnostics and benchmarks.

Small stores may calculate graph metrics with bounded SQLite recursive queries.
Large stores may use a derived `scheduling_metrics` cache keyed by graph and
issue revisions. Dependency, lifecycle, condition, or relevant structured-data
mutations update or invalidate affected metrics in the same transaction. Dirty
metrics may be recomputed lazily before ranking, but recomputation must be
bounded; a documented simpler policy fallback is preferable to holding the
claim transaction for an unbounded graph rebuild.

A stale or missing cache may reduce ranking quality but can never make an
ineligible bead claimable. The winner's readiness, active conditions, worker
constraints, retry state, and prompt fit are revalidated from authoritative
rows under the claim transaction.

Required tests include chains, fan-out, diamonds, multiple remaining blockers,
conditional edges, priority conflicts, age-bucket boundaries, released-bead
rotation, continuously arriving work, every failure class, retry cadence,
quarantine, revision-epoch reset, context overflow, cache invalidation, and at
least twenty concurrent claimers. Repeating a claim against identical state,
captured time, request capabilities, and policy must select the same bead.
Priority tests cover every P0-P4 boundary, aspirational opt-in, bounded
promotion, and profile-range validation.

#### 3.5.10 Rapid-fire lifecycle capacity benchmarks

Ship a deterministic, noninteractive stress harness that exercises the real
store and service/CLI paths with isolated temporary workspaces. Dataset setup is
timed and reported separately from the steady-state workload. The harness must
accept at least:

```text
--beads 100|1000|10000|100000|1000000
--workers 1..200
--policy fifo-v1|POLICY
--seed INTEGER
--duration DURATION
--workload claim-close|claim-release|mixed|dependency-churn
--output-json PATH
```

The canonical scale matrix is 100, 1,000, 10,000, 100,000, and 1,000,000
beads. At each scale run 1, 2, 4, 8, 16, 24, 32, 48, 64, 96, 128, 160, and 200
concurrent agents. These approximately logarithmic steps retain useful points
around common fleet sizes while covering the full requested range. Continue
through 200 even after the default capacity profile fails so the report shows
the complete degradation curve; a caller may request any integer from 1 to 200
for targeted reproduction. A run that cannot be completed because of memory,
disk, or time limits records a structured `resource_limited` result rather than
silently omitting the scale.

Total bead count and ready-frontier width are independent benchmark dimensions.
At every scale, deterministic dataset families include:

- **independent:** every open bead is on the frontier, the worst case for the
  number of rankable candidates;
- **chains:** long dependency chains expose approximately one bead per chain;
- **wide DAGs:** many initial frontier beads converge into blocked interior
  layers and expose new waves as blockers close;
- **diamonds:** shared downstream beads test deduplicated unlock metrics; and
- **mixed lifecycle:** realistic proportions of ready, assigned, deferred,
  closed, manually blocked, and graph-blocked beads.

Each report records total beads, edge count, graph depth, ready-frontier width
and density, and the number of beads entering or leaving the frontier per
mutation. Capacity conclusions must identify the dataset family; a million-bead
store with a frontier of ten is not equivalent to a million independent ready
beads.

Required workloads are:

- **claim-close:** atomically claim ready work and immediately close it;
- **claim-release:** repeatedly claim and release, stressing rotation and
  reassignment without exhausting the queue;
- **mixed:** deterministic weighted create, claim, show, update, dependency,
  close, reopen, and release operations;
- **dependency-churn:** close/reopen blockers and add/remove valid edges while
  other workers claim, exercising incremental metric invalidation.

Reports include schema version, commit, build profile, Rust/SQLite versions,
OS, CPU count/model where available, memory, filesystem, journal/synchronous
mode, seed, dataset shape, policy/configuration, worker model (processes or
threads), warmup, duration, and every command line. For each operation report
attempted/succeeded/conflicted/busy counts, throughput, p50/p95/p99/max latency,
transaction duration, shortlist size, full-scan fallbacks, cache hit/dirty/
recompute counts, database/WAL sizes, and peak memory/CPU where measurable.

The harness deliberately discovers saturation, but normal bead operations must
not use SQLite as a compute engine for unbounded ranking or graph work. Every
ordinary operation uses indexed, bounded queries; avoids per-row transaction
loops and accidental N+1 reads; prepares/reuses statements where practical;
keeps `BEGIN IMMEDIATE` sections limited to final validation and mutation; and
does not perform a full graph traversal, cache rebuild, JSON serialization, or
prompt construction while holding the writer lock. WAL readers remain
concurrent with a claimant except at SQLite's unavoidable commit boundaries.

Performance tests capture transaction hold time, busy-handler invocations and
wait duration, rows visited/returned where observable, statements per
operation, WAL growth/checkpoint time, database growth, and bytes written per
semantic mutation. Representative query-plan tests at every scale reject
unexplained full scans of the million-row issue table for single-bead CRUD or
frontier claims. A policy fallback that scans the complete ready frontier is
reported explicitly and cannot be the normal path for large stores. Tests use
bounded retry with jitter outside the transaction; they never hide saturation
through unbounded waits or retry storms.

Correctness is unconditional: duplicate successful claims, lost committed
mutations, invalid readiness, or an unreconciled final-state count fails the
run at every scale. Capacity is machine-relative. The default `interactive-v1`
profile defines an agent count as supported when, after warmup, correctness
holds, at least 99.9% of operations avoid terminal busy/I/O failure, claim p95
is at most 250 ms, and all-mutation p99 is at most 1 second. Reports show the
highest supported agent count and the complete saturation curve through 200 for every
scale; users may supply and name other threshold profiles.

Benchmarks are not ordinary unit tests. CI runs a fast 100/1,000-bead smoke
matrix; scheduled or explicitly provisioned performance runs execute all five
scales. Results are descriptive across machines and must not be compared unless
their environment and capacity profile are compatible. Preserve JSON reports
as build artifacts, not source-controlled performance claims.

## 4. Workspace and independent SQLite design

### 4.1 Layout and discovery

```text
.beads/
  beads.db          authoritative native SQLite database
  issues.jsonl      small-workspace portable checkpoint
  checkpoint/
    current.json    authoritative checkpoint-mode/generation pointer
    manifest.json   current sharded-checkpoint manifest when sharding is active
    previous.json   immediately previous verified manifest for crash recovery
    objects/        content-addressed issue and event JSONL shards
  config.json       nonsecret workspace configuration
  receipts/         migration receipts created on request
  .gitignore        ignores journals and temporary files
```

`bead init [--prefix PREFIX]` creates this workspace without modifying
unrelated files. Repeating it with the same prefix succeeds; a conflicting
prefix fails without mutation. Use user-only write permissions where
supported.

The generated `.beads/.gitignore` ignores SQLite, WAL/journal files, locks, and
operation-owned temporaries, but does not ignore `issues.jsonl`, current or
previous checkpoint manifests, or referenced checkpoint objects. Those files are
deterministic project artifacts intended to be committed. `bead-rs` never runs
Git commands or creates commits; the surrounding repository workflow flushes
before commit and pushes to its authoritative host, from which a configured
mirror may publish the history to GitHub.

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
| `issues` | canonical scalars, ID PK, lifecycle checks, timestamps; no lease, intelligent-policy, retry, quarantine, or revision-guard columns in migration 1 |
| `issue_extensions` | issue ID + key PK, canonical JSON, origin profile |
| `labels` | issue ID + label PK, issue FK cascade |
| `dependencies` | blocked + blocker + kind PK, optional canonical condition JSON, two issue FKs cascade, no self-edge |
| `comments` | random ID, issue ID, author, immutable body, reply-to ID, resolution state, creation time |
| `issue_data` | issue ID + namespace PK, schema reference, canonical JSON value, issue FK cascade |
| `claim_telemetry` | issue ID, claim time, assignee, optional model/harness/version |
| `claim_attempts` | minimal immutable successful-claim audit identity, issue, actor, time, and `fifo-v1` tie-break snapshot; richer outcomes/lease/context fields require R002 or R019 migrations |
| `scheduler_state` | only state proven necessary for deterministic `fifo-v1`; intelligent-policy sequence, retry cadence, graph revision, and configuration require R019 |
| `scheduling_metrics` | optional derived issue/graph revision, unlock/critical-path metrics; never authoritative for eligibility |
| `events` | local ingestion sequence plus immutable origin store UUID, origin sequence, event hash, optional issue ID, kind, actor, time, canonical JSON detail, and import provenance |
| `checkpoint_state` | singleton current generation/mode/root hash, snapshot and event sequences, export time, tombstone/changed-path state |

Add only indexes justified by v0.1 queries:

- issues on `(base_status, manual_blocked, assignee, priority, created_at, id)`;
- dependencies by blocker and by blocked issue;
- labels by label and issue;
- comments/events by issue plus time/sequence.

Do not add caches, tombstones, recovery subsystems, or compatibility-shaped
columns without a measured requirement and new migration. The post-0.1
`scheduling_metrics` cache is permitted only under the correctness rules in
section 3.5.9.

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
| `bead create --title T [--description D] [--label L]...` | create with an empty description when omitted; print only ID plus LF |
| `bead list --json [--status S] [--assignee A] --limit N` | records in claim order; limit 0-999999 |
| `bead show ID --json` | one-element JSON array for NEEDLE v1 |
| `bead claim --assignee A [telemetry] --json` | atomic claim; one JSON object |
| `bead update ID [--status S] [--assignee A] [--notes N]` | atomically apply supplied changes |
| `bead release ID` | atomically return claimed work to open and unassigned; print ID plus LF |
| `bead reopen ID` | restore open lifecycle |
| `bead close ID --reason TEXT` | finish with retained reason |
| `bead label add ID --label L` | idempotent presence |
| `bead label remove ID --label L` | idempotent absence |
| `bead dep add BLOCKED BLOCKER --type KIND` | add canonical edge |
| `bead dep remove BLOCKED BLOCKER [--type KIND]` | remove matching edge(s) |
| `bead sync --flush-only [--profile P]` | atomic checkpoint export |
| `bead sync --import-only (--restore-into-empty\|--merge) [--profile P] [--dry-run]` | validated restore or transactional reconciliation/analysis |
| `bead sync --status --format json` | freshness, root hash, mode, and Git-trackable changed paths |
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

### 5.3 Help and manual contract

The `clap` command tree is the single structural source of truth for public CLI
documentation. Every public command path supports both:

```text
bead COMMAND [SUBCOMMAND ...] --help
bead help COMMAND [SUBCOMMAND ...]
```

Help must work without a workspace, database, network, or writable current
directory; it exits zero and performs no mutation. Every visible command,
subcommand, positional argument, option, flag, alias, enumerated value, default,
conflict, requirement, and repeatability rule has nonempty help. Short help is
scannable; long help includes behavior, safety consequences, machine-output
notes, and at least one realistic example for nontrivial leaf commands. Hidden
implementation options are excluded from the public contract.

Root help introduces the product before presenting the command inventory. Its
short form contains a compact intended workflow:

```text
init workspace -> create/import beads -> add blocking relationships
-> inspect ready work -> claim -> update/release -> close -> flush JSONL backup
```

Root long help and `bead(1)` explain the lifecycle in plain language: `open`
beads may be ready; manual blocking or unfinished `blocks` edges remove them
from the ready frontier; claim atomically assigns one ready bead and moves it to
`in_progress`; release returns it to open/unassigned work; close requires a
reason and may expose dependents; reopen restores an intentionally closed bead
to open. They distinguish base state from effective `blocked` status and state
that SQLite is authoritative live state while `issues.jsonl` is the portable
backup only as of its last successful flush. The root page includes a minimal
end-to-end command example, points automation to `--json` and capabilities, and
links each lifecycle operation to its command page.

Root help also states that checkpoint files are designed to be Git-tracked and
that users or automation should run `bead sync --flush-only` before committing
the repository. It must not imply that `bead-rs` performs the commit or push.

Generate section-1 manual pages from that same command tree and structured
long-form documentation. Ship `bead(1)` plus one page for every public command
and nested leaf, using hyphenated names such as `bead-claim(1)`,
`bead-dep(1)`, and `bead-dep-add(1)`. Parent pages summarize their children;
leaf pages completely document their arguments and options. Each page contains
the applicable NAME, SYNOPSIS, DESCRIPTION, OPTIONS, EXIT STATUS, FILES,
ENVIRONMENT, EXAMPLES, and SEE ALSO sections. It identifies the exact bead-rs
release and machine-output/profile stability where relevant.

Generated roff lives under `man/man1/`, is included in source and release
packages, and is reproducible byte-for-byte for a fixed release. Distribution
packages install it into the platform man path. Because `cargo install` does
not install ancillary man files, document a supported command or release-script
path that copies the packaged pages into an explicitly selected man root; never
write a system directory implicitly. Man generation and installation are
offline and noninteractive.

Tests recursively walk the public `clap::Command` tree and fail when any command
or argument lacks required help, when a leaf lacks an example, when either help
spelling fails, or when generated man-page names/content differ from committed
artifacts. Snapshot tests normalize only the version/date fields explicitly
declared variable. A packaging test verifies every expected page is present in
the `.crate` and cross-links resolve to an existing page.

The public lifecycle inventory includes `release` as its own root command.
Consequently root help, both help spellings, generated `bead-release(1)`, the
end-to-end example, package-content checks, and the capability command list all
name `release ID` explicitly. This native addition does not alter NEEDLE v1's
required invocation matrix; NEEDLE consumers that do not call release remain
compatible, while capability-aware consumers can discover it additively.

## 6. JSONL backup and compatibility profiles

### 6.1 Canonical JSONL

JSONL is UTF-8 with one compact object and LF per issue. In native monolithic
mode each issue record includes its complete durable audit-event array in event
sequence order. Blank lines may be ignored. Malformed or non-object lines fail
with a one-based line number. Reject duplicate IDs before activating any state.

Canonical export order is ID ascending. Known fields follow the interchange
specification, optional known fields follow, and extension keys sort lexically.
Labels sort lexically and are unique. Dependencies sort by blocker ID, kind,
then blocked ID. Comments retain creation order with ID as tie-breaker.
Timestamps emit UTC RFC 3339 and retain imported fractional precision while
the represented instant is unchanged.

Known fields win over same-name extension keys. Report that collision as a
transformation; never emit duplicate JSON keys silently.

The native recovery corpus is historical, not merely a queue of unfinished
work. It retains open, in-progress, deferred, and closed beads; complete
dependency, comment, schema-bound data, and unknown-extension content; and the
complete ordered public audit-event stream needed to reconstruct lifecycle,
assignment, dependency, and other semantic mutations. Claim telemetry or
private diagnostic material is included only when its public schema explicitly
marks it durable and nonsecret. Normal cleanup never drops closed beads or
historical events from the portable corpus. Any future pruning operation must
be explicit, separately specified, and produce a forensic receipt.

#### 6.1.1 Adaptive sharded checkpoint set

Small workspaces use `.beads/issues.jsonl` for maximum compatibility. Native
defaults switch to a sharded checkpoint when the monolith would exceed 50,000
issue records, 64 MiB total, or 8 MiB for any history-bearing issue line; all
thresholds are versioned configuration and recorded in the manifest. Operators
may force monolithic or sharded output, but forcing a monolith never bypasses
record/byte safety limits.

All native checkpoints use `.beads/checkpoint/current.json` as the sole
authoritative discovery pointer. It canonically records a generation ID,
`monolithic` or `sharded` mode, store UUID, snapshot sequence, active-root path
and SHA-256, and a deterministic set of paths added, replaced, and deleted by
that generation. In monolithic mode the active root is `.beads/issues.jsonl`;
in sharded mode it is `.beads/checkpoint/manifest.json`. Files not selected by
`current.json` are inactive even if they remain after a crash. Import never
chooses between roots by existence or modification time.

The sharded manifest records format/schema version, store UUID, snapshot and
maximum event sequence, creation time, profile, complete record/event counts, partition
algorithm and thresholds, and every referenced object path, byte length,
SHA-256, record range, and semantic role. The manifest itself has a canonical
SHA-256 reported by `sync --status`. Import rejects missing, extra-referenced,
duplicate, overlapping, mispartitioned, miscounted, or hash-mismatched data
before activating any state.

Issue shard assignment is deterministic:

```text
key = SHA-256(UTF-8 bead ID)
partition = the manifest-declared leading hexadecimal prefix of key
```

Begin with a shallow prefix. When one shard exceeds its record or byte target,
split only that prefix into its sixteen next-hex-digit children. Do not
automatically merge shards on later flushes: avoiding oscillation and wholesale
Git diffs is more valuable than recovering a few small files. An explicit
future compaction operation may produce a new partition plan and receipt.
Records within each issue shard sort by bead ID.

Audit events are stored separately from issue snapshots in immutable,
contiguous sequence-range JSONL objects. Seal an event object at 100,000 events
or 64 MiB, then start the next range. A flush may replace only the unsealed tail;
sealed objects never change. This makes forensic history append-friendly and
prevents a frequently updated bead from rewriting its entire history-bearing
issue shard.

Object filenames contain their content SHA-256 and live under
`.beads/checkpoint/objects/`; identical content is reused. Before publishing a
new root, preserve the old sharded manifest as `previous.json`. Publish each
data root only after all of its content is durable and verified, then atomically
replace `current.json` as the commit point. A mode transition publishes a new
generation whose changed-path set includes the new root and objects, the
pointer replacement, and tombstones (deletion entries) for the formerly active
root and any objects referenced by neither the new generation nor the retained
previous manifest. Only after the pointer is durable may those tombstoned paths
be removed. Thus a crash before the pointer leaves the old mode authoritative;
a crash afterward leaves the new mode authoritative and cleanup safely
repeatable. `sync --status` reports unresolved tombstones as not ready to
commit, and its changed paths include deletions as well as additions and
modifications. One external Git commit must contain that entire set. Git
history retains previously committed roots and objects; `bead-rs` itself never
runs Git.

The monolithic and sharded representations are semantically equivalent. A
fresh store restored from either must produce the same canonical public state
and audit-event history. External compatibility profiles that require one file
may export a monolith explicitly to a caller-selected path without changing the
native checkpoint mode.

### 6.2 Backup flush algorithm

SQLite is authoritative between flushes because it supplies transactional live
operation. The monolithic `.beads/issues.jsonl` or sharded checkpoint manifest
is the supported portable backup at the last successful flush and the source
for disaster recovery into a newly initialized store. The CLI and documentation
must call out its recorded snapshot sequence and freshness; they must never
imply that an older backup contains unflushed mutations. There is no separate
native SQLite backup format.

1. Open a read transaction and capture the event sequence.
2. Assemble all issue records and durable audit events from that single
   committed snapshot.
3. Select monolithic or sharded mode from explicit configuration or recorded
   thresholds; retain an existing valid partition plan and split only
   overflowing shards.
4. Serialize new content-addressed objects or a uniquely named monolithic
   temporary; reuse already verified objects without rewriting them.
5. Flush and `sync_all` every new file, verify lengths/hashes/counts, then sync
   its parent directory where supported.
6. Publish the monolith or canonical manifest, verify it, then atomically
   replace `current.json` as the generation commit point and sync the parent
   directory. Apply only the pointer-declared tombstones afterward. Never
   expose an authoritative pointer referencing an incomplete root or object
   set.
7. Record root hash, snapshot sequence, event range, mode, partition plan, and
   time in a short write transaction.
8. Emit machine-readable freshness and changed-path information so an external
   Git workflow can verify that every checkpoint mutation is included in its
   commit.

A write after step 1 may make the checkpoint an older committed snapshot; its
recorded sequence makes this explicit. Never truncate the prior checkpoint in
place. On failure preserve it and remove only this operation's temporary file.

Every committed semantic mutation advances the live event sequence and makes
checkpoint status dirty until a successful flush covers that sequence.
`sync --status --format json` reports live and flushed sequences, root hash,
mode, changed paths, and whether the checkpoint is ready to commit. Repository
automation must treat a dirty checkpoint as a failed pre-commit/release gate,
run `sync --flush-only`, and include every reported path in the same Git commit
as the related project change. This workflow preserves forensic material on the
remote history without making the bead CLI a Git client.

### 6.3 Import reconciliation

`sync --import-only` requires exactly one explicit semantic mode:
`--restore-into-empty` or `--merge`. It follows `checkpoint/current.json` and
fully validates the selected root before activation. A legacy monolith without
a pointer may be accepted only through an explicit caller-selected input path;
simultaneous files are never heuristically ranked. Default safety limits are 1
million issue records, 16 MiB per line, 4 GiB total, and `serde_json`'s bounded
nesting behavior. Event limits are independently configured and never inferred
from the issue-record limit.

Sharded import streams objects in manifest order, verifies each content hash
and partition membership, and rejects duplicate issue IDs or event sequences
across shards. Validation of the entire manifest, graph, event continuity, and
semantic state completes before the activation transaction.

Every native event has immutable identity
`(origin_store_uuid, origin_event_sequence, event_sha256)`. The hash covers the
canonical public event excluding local import-envelope fields. Native events
use the local store UUID as their origin. The manifest declares the maximum
sequence retained for every represented origin, and a checkpoint contains
exactly one event for every sequence from 1 through each declared maximum; a
repeated `(origin_store_uuid, origin_event_sequence)` with a different hash is
divergence, not a timestamp conflict. Imported events retain origin
identity and order. When merged into another store they also receive a local
monotonic ingestion sequence and provenance containing source root hash,
source store UUID, import receipt ID, importing actor, and import time. That
envelope is itself audited without rewriting the imported actor or time.

`--restore-into-empty` requires a newly initialized store with no semantic
mutations. In one transaction it adopts the checkpoint store UUID, restores
issues and the exact contiguous native event sequence, verifies that replayed
event outcomes equal the checkpoint snapshot, records recovery provenance in a
separate local restore receipt, and activates the result. The receipt does not
alter the restored historical sequence. Any nonempty target, UUID ambiguity,
sequence gap, replay mismatch, or root-hash mismatch fails without mutation.

`--merge` preserves the target store UUID and never presents foreign history
as locally originated. For a same-UUID checkpoint, target and input event
streams must share an identical hash prefix; input may extend the target, but a
gap, rewrite, or different event at the same origin sequence rejects the whole
import. An older identical prefix is an auditable no-op. For a different UUID,
origin identities must be new or byte-identical to events already ingested
from that origin; any identity/hash mismatch is divergence. After those
history checks, one write transaction:

- insert IDs absent from native state;
- replace only when imported `updated_at` is later;
- retain native state when its timestamp is later;
- treat equal timestamps with unequal semantic content as a conflict and roll
  back the entire import;
- never delete native issues because they are absent from the checkpoint;
- validate endpoints and cycles against the final staged graph;
- preserve unknown values under their source profile;
- append accepted origin events in origin order with their provenance
  envelopes, followed by one local import-summary audit event containing only
  counts, source identity/root hash, receipt ID, and reconciliation result.

Snapshot timestamps never authorize discarding, synthesizing, or reordering
events. If snapshot reconciliation would produce state inconsistent with the
accepted event stream, import reports semantic divergence and rolls back.
After commit, report inserted, updated, retained, conflicted, duplicate-event,
and imported-event counts plus source/target UUIDs and the receipt ID.
With `--dry-run`, perform the same input discovery, limits, parsing, hash and
schema validation, staging, event replay/provenance checks, graph checks, and
reconciliation/conflict analysis without entering an activation transaction.
It must not change SQLite rows,
events, sequences, checkpoint metadata or files, receipts, or any other durable
workspace state; operation-owned scratch material is removed before return.
Dry-run emits one canonical JSON summary on stdout, with `dry_run: true` and
the inserted, updated, retained, and conflicted counts that would result; a
real import reports the same fields with `dry_run: false` in its selected
renderer. A clean analysis exits 0. A reconciliation conflict exits 4, and
malformed/integrity-invalid input exits 5; when analysis reaches the
reconciliation-report stage, its JSON summary remains valid even on exit 4,
and diagnostics remain on stderr. Tests
compare dry-run counts with an immediate real import against unchanged state
and assert byte-for-byte workspace immutability after both successful and
failed dry-runs.

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
  docs.rs             structured long help and manual supplements
man/man1/             reproducible generated section-1 manual pages
tests/
  cli/                isolated subprocess tests
  conformance/        normative lanes
  concurrency/        multiprocess tests
  stress/             deterministic rapid-fire lifecycle correctness harness
benches/
  lifecycle.rs        scale/concurrency benchmark driver and JSON reports
research/fixtures/    independent fixtures and manifests
```

Suggested dependencies, subject to Rust 1.75 verification:

- `clap` 4 with derive;
- `clap_mangen` or an equivalently bounded roff generator sharing the `clap`
  command tree;
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
7. Rapid-fire lifecycle stress and benchmark harnesses covering the matrix and
   report contract in section 3.5.10.
8. Recursive CLI documentation coverage, help snapshots, reproducible man-page
   generation, cross-link checks, and package-content verification.

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

The fast verification lane runs deterministic benchmark smoke cases at 100 and
1,000 beads with 1, 4, and 20 workers for `claim-close`, `claim-release`, and a
short mixed workload. The full performance lane runs 100 through 1,000,000
beads and the worker saturation sweep. Harness self-tests verify deterministic
dataset generation, percentile calculation, schema-stable JSON, resource-limit
reporting, duplicate-claim detection, and final-state reconciliation. Benchmark
setup may use an independently implemented fixture generator through the public
store/service boundary; it must not copy another implementation's database or
measure fixture creation as claim latency.

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
14. **F015:** deterministic lifecycle stress harness, fast matrix, full-scale
    benchmark driver, capacity calculation, and schema-stable reports.
15. **F016:** complete help tree, generated man pages, drift/coverage tests, and
    documented explicit installation.
16. **F017:** adaptive deterministic sharding, complete forensic event history,
    semantic restore equivalence, and Git-trackable checkpoint verification.
17. **F014:** package/install smoke test, licensing, provenance verification.

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
  "priorities": {"min": 0, "max": 4, "default": 2, "p4_aspirational_requires_opt_in": true},
  "statuses": ["blocked", "closed", "deferred", "in_progress", "open"],
  "checkpoint_modes": ["flush-only", "import-only"],
  "checkpoint_formats": ["monolithic-jsonl-v1", "sharded-jsonl-v1"],
  "schemas": ["urn:bead-rs:schema:issue:native-v1"],
  "commands": ["capabilities", "claim", "close", "create", "dep", "doctor", "init", "label", "list", "migrate", "release", "reopen", "schema", "show", "sync", "update"]
}
```

Arrays are lexically stable. `commands` is the lexical, duplicate-free set of
every application-defined visible public root subcommand in the same
`clap::Command` tree used for help, including discovery and administrative
commands but excluding clap's generated `help` pseudo-command; `--help` and
`--version` are flags, not root commands. Nested paths are described by
additive structured fields when needed rather than replacing their root entry.
Tests compare that array directly with the visible application root command
tree, so a command cannot ship undiscoverably. Additive fields are
allowed. An unsupported profile exits nonzero instead of returning mislabeled
native capabilities. For `needle-v1`, this complete native inventory is an
additive handshake: the commands required by the normative NEEDLE v1 contract
remain present with their specified syntax and envelopes, and extra native
commands do not imply that NEEDLE must invoke them.

## 12. Adopted post-0.1 roadmap

Items marked **extension** are accepted only after the F001-F017 release core
and require their own normative specifications, conformance scenarios,
migrations where applicable, and future Marathon ledger entries. Items marked
**core incorporated** name requirements already owned by F001-F017; they are
traceability notes, not permission to defer or reimplement the core subset.

### R001 — Explain claim and readiness decisions

Add a nonmutating, machine-readable decision trace with versioned semantic
reason codes for lifecycle, assignment, blockers, manual blocking, policy
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

### R005 — Machine-readable schemas and per-bead schema references (core incorporated)

F010 and the 0.1 domain/command contract already require immutable schemas for
native issue records, capabilities, and migration receipts, per-bead
`schema_ref`, schema resolution, and capability/receipt enumeration. R005 is
superseded for that subset. Its extension is limited to schemas for R001
decision traces and future bulk/error documents.

### R006 — Semantic backup completeness proof (core incorporated)

F017 already requires restore/re-export semantic equivalence for every 0.1
durable fact, including lifecycle, dependencies, comments, structured data,
schema references, unknown extensions, and forensic events. R006 is superseded
for that subset. Its extension covers fields introduced after 0.1, such as
R003 revisions, without deferring the F017 proof.

### R007 — Atomic versioned backup generations (core incorporated)

F017 and sections 6.1.1-6.2 already require verified generations, an atomic
mode/generation pointer, a retained previous sharded manifest, and the
monolithic compatibility representation. R007 is superseded for 0.1. Its
extension is retention beyond one recovery generation and explicit
compaction/retention receipts.

### R008 — Backup freshness contract (core incorporated)

The 0.1 `sync --status` already exposes live and backed-up sequences, age, root
hash, mode, verification/readiness, tombstones, and Git-trackable changed
paths. R008 is superseded for visibility. Its extension is intentionally
configured maximum-age/event-gap enforcement and explicit backup preconditions
for selected risky mutations.

### R009 — Schema negotiation catalog

Capabilities declare exact readable and writable schema URN sets. Producers
and consumers negotiate only an exact mutual identifier and report read-only or
lossy support explicitly. Do not infer compatibility from similar names or
schema structure.

### R010 — Comment mutation and richer threaded-comment workflow (core incorporated, extension scoped)

Version 0.1 already preserves imported immutable comment bodies, authors,
stable IDs, reply relationships, and resolution state as normalized child
records; includes their complete ordered history in JSONL backup and restore;
and projects them read-only through `list` and `show`. Those commands default
to `--comments none` (metadata/counts may remain visible) and accept
`--comments unresolved` or `--comments all`, so a retriever controls whether
conversation bodies enter its prompt. Import, export, and projection must not
drop or rewrite comments merely because 0.1 has no comment mutation command.

This roadmap item adds the first public comment mutation operations (including
create, reply, and resolution changes), their authorization/validation rules,
audit events, help/man pages, and stable machine results. Until that separate
specification and ledger work lands, comments are portable and readable but
cannot be created, edited, resolved, or deleted through the native 0.1 CLI.

### R011 — Namespaced external references

Attach generic `(namespace, key, value)` references such as tracker IDs and
commit identifiers without replacing native bead IDs or resolving anything
over the network. Optional namespace-scoped uniqueness supports reliable
deduplication and cross-tool recognition without title heuristics.

### R012 — Schema-bound typed annotations and structured data (core incorporated)

The 0.1 model and backup already require namespaced `data` envelopes with an
immutable `schema_ref`, JSON values, round-trip preservation, and nonexecuting
validation. R012 is superseded for that subset. Its extension is limited to
issue-type constraints over allowed namespaces and schemas; public CRUD remains
separately scoped by R018.

### R013 — Cursor-based local change feed

Emit deterministic public mutation records after a cursor, including snapshot
identity and explicit gap detection. Consumers must resynchronize from JSONL
after a gap. This supports incremental local indexes and adapters without a
daemon, network service, or dependency on private event tables.

### R014 — Complete import diagnostic report

Collect a bounded, deterministically ordered set of validation failures with
line number, JSON Pointer, schema keyword, semantic code, and a truncation
marker. No state activates. This replaces repeated one-error-per-import repair
cycles without allowing unbounded memory consumption or cascading noise.

### R015 — Disposable recovery rehearsal

Build a temporary workspace from the current JSONL generation, run integrity
and schema diagnostics, re-export for semantic comparison, record a nonsecret
report, and remove only the operation-owned temporary workspace. This exercises
the real disaster-recovery path without overwriting live state.

### R016 — Scoped doctor and diagnostic mode

Extend `doctor` with `store`, `backup`, `schema`, `dependencies`, `comments`,
and `all` scopes plus stable JSON diagnostics. It checks backup generations and
freshness, schema/data validity, conditional predicates and latent cycles,
comment threads, change-feed gaps, and recovery provenance. Repairs stay
narrowly allowlisted and never rewrite user semantic data.

### R017 — Conditional dependencies

Allow an edge to carry a bounded declarative predicate over stored fields,
labels, issue type, priority, assignee presence, and schema-bound data on the
blocked or blocker bead. Conditions use typed `all`/`any`/`not` composition and
comparison/set operators—never scripts, SQL, wall-clock, environment, network,
comments, or recursively derived readiness. Treat every conditional blocking
edge as potentially active during cycle detection.

### R018 — Structured bead data

Expose atomic `data set|get|list|remove` operations for namespaced JSON values,
each governed by its own immutable schema reference. Unknown schemas remain
preservable for interchange but fail closed for native mutation. This is the
general mechanism for adding structured information to a bead JSON object
without turning arbitrary fields or the SQLite layout into an API.

### R019 — Intelligent, aging, rotating, failure-aware claim scheduling (extension)

Core incorporates only atomic eligibility and immutable `fifo-v1`
priority/creation/ID ordering with its minimal claim audit. R019 implements the
post-0.1 portions of section 3.5: graph-unlock
impact, bounded ready-age promotion, least-recently-served rotation, unproven
work preference, classified failure tiers, retry cadence, quarantine, context
fit, atomic selection, and semantic explanations. Ship `fifo-v1` unchanged,
then independently specify and conform `aging-v1`, `impact-v1`, `rotation-v1`,
and `balanced-v1` before enabling them. `balanced-v1` becomes a default only
through an explicit release/configuration decision, never silently. R019 adds
no lease or fencing fields; those belong exclusively to R002 and compose only
when both capabilities are installed.

### R020 — Cross-profile semantic comparison

Add a read-only comparison that renders selected native records through two
explicit installed profiles and reports preserved, transformed, omitted, and
unsupported semantic fields by canonical field path. Compare meaning rather
than incidental JSON formatting, bound the record count, and never write either
representation. This lets an operator understand interoperability loss before
running migration instead of learning only from the resulting receipt.

### R021 — Workspace policy lint

Add `bead policy check --format json` to diagnose contradictory, unreachable,
redundant, and ineffective scheduling or retention configuration without
changing it. Every stable diagnostic is bound to exact policy and configuration
schema versions; an unknown version fails closed rather than applying guessed
rules. Policy lint is advisory and cannot make a bead eligible or ineligible.

### R022 — General mutation dry-run

Extend the existing migration/import dry-run concept to ordinary semantic
mutations. `update`, `close`, `reopen`, and dependency mutations accept a
consistent `--dry-run` mode that performs normal authorization, validation,
cycle analysis, and derived-status calculation, then emits a canonical
before/after semantic delta without committing rows, events, revisions, or
checkpoint metadata. The result records the observed revision and workspace
sequence and is explicitly advisory; callers use R003 revision guards if the
subsequent real mutation must apply to that same state.

### R023 — Unified `why` explanation facade

Add a read-only `bead why ID` command that explains effective status,
readiness, active blockers, claim-ranking factors, and currently legal next
operations in human and stable machine-readable forms. It must call the same
domain evaluators and reason codes used by R001 and R019, never implement a
parallel policy engine. This gives humans and agents one entry point for the
question “why is this bead in this state, and what can happen next?”

### R024 — Explicit recurring-bead materialization

Store immutable, nonexecuting recurrence-template versions and create the next
occurrence only through an explicit command. Each occurrence carries a stable
series reference, selected copied fields, and an idempotent materialization
receipt. Core `bead-rs` never wakes, polls, interprets wall-clock schedules, or
creates work autonomously; an external caller may decide when to invoke the
operation.

## 13. Release gates

Before `.marathon/COMPLETE`:

- F001-F017 have concrete passing evidence;
- F012 includes independently approved `br-v1` and `bf-v1` fixture manifests,
  full matrices, and loss reports; absence of either external input blocks the
  release rather than narrowing its profile claims;
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo test` pass on the release commit;
- native, interchange, NEEDLE, concurrency, and migration lanes pass;
- the rapid-fire benchmark smoke matrix passes and a full 100-to-1,000,000-bead
  run either completes or records explicit resource-limited results for every
  uncompleted scale;
- every public command path passes recursive help coverage and every generated
  section-1 man page is current, cross-linked, and present in the package;
- monolithic and sharded restore-equivalence tests pass; the checkpoint covers
  every bead and durable audit event; and no referenced checkpoint artifact is
  ignored by Git packaging rules;
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
- predeclared file-intent manifests, file-derived dependency serialization,
  edit fencing, and post-diff path enforcement;
- atomic bulk transaction manifests;
- mutation idempotency keys;
- worker capability declarations.
- sensitive-content linting for backup-bound fields;
- portable execution-outcome envelopes.

Workers are not required to predict or declare files before claiming or
starting a bead. `bead-rs` does not gate edits on an accepted read/write set,
base revision, intent hash, or planning phase. A future file-writing
coordination mechanism may reuse the deferred research, but it needs a separate
product decision and normative specification before becoming a roadmap item.

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
