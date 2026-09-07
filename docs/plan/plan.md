# bead-rs Current Product and Software Factory Plan

Plan revision: 15

As of: 2026-09-06

Status owner: bead-rs maintainers

Status: v0.2.6 was tagged at `d9a32b3` and published as a GitHub release on
2026-09-06 **ahead of its release evidence**: BR-T18 remains open, the
CHANGELOG carries no 0.2.6 entry, and gates 11–14 have no recorded proof, so
per this plan's own rule the tag is a mid-transition snapshot, not an
evidenced release. Independent checks of the published x86_64 artifact
(recorded in `beadrs-559d3bfe` notes) confirm checksum, exact-commit version
string, enforce-mode rejection without value disclosure, ruleset-v3
capability advertisement, and the `redact` surface. BR-T18 acceptance must
either bind its evidence to this exact tag or supersede it with an evidenced
cut. Attempt resolution, secret rejection, and historical-redaction recovery
are implemented on `main`, including the ruleset-v3 Garage
credential-identifier extension. Exact-source release conformance remains
open on the explicit gate defects recorded in sections 1.2, 6, and 7. Revision 14 makes the durable
claim-epoch transition and its NEEDLE consumer canary explicit as BR-T23
through BR-T27 rather than leaving that release-blocking work implicit in
other beads. BR-T28 captures the dispatch race discovered while materializing
that graph and makes the existing transactional manifest path the required
planner workflow: dependent work must not become claimable before its
dependency and resource declarations commit.

## 0. How to read this plan

Sections 0–8 are the current normative product and transition plan. The former
0.1/Marathon plan follows as a historical appendix. Its bootstrap sequence,
release status, completion gates, roadmap statuses, and deferred-feature list
are retained for provenance but do not describe the current release unless
sections 0–8 explicitly adopt them.

Behavioral authority remains:

1. independently reviewed normative specifications under `research/specs/`;
2. released source, migrations, and public conformance fixtures;
3. accepted ADRs for architectural decisions;
4. this plan for delivery state and future work;
5. native beads for mutable work state.

Marathon artifacts are a frozen historical evidence set. They must not be
silently rewritten to claim evidence for requirements added after their
recorded scope.

This revision accepts:

- [ADR-010: Store attempt facts, not learning or orchestration policy](../adr/010-store-attempt-facts-not-learning-policy.md)
- [ADR-011: Resolve an attempt and its lifecycle transition atomically](../adr/011-atomic-idempotent-attempt-resolution.md)
- [ADR-012: Roll out attempt resolution through versioned capabilities](../adr/012-capability-gated-attempt-contract-rollout.md)
- [ADR-014: Hard-reject mutations that would publish a detectable secret](../adr/014-hard-reject-secret-bearing-mutations.md)
- [ADR-015: Audited historical redaction over hand-edited recovery artifacts](../adr/015-audited-historical-redaction.md)
- [ADR-016: Keep workspace probes observational](../adr/016-observational-workspace-probes.md)
- [R038 specification acceptance for the exact submitted hashes](../reviews/r038-specification-acceptance-2026-09-03.md)

## 1. Product boundary and current reality

bead-rs is an independent, clean-room Rust task-coordination system. SQLite is
the authoritative local live store. Mutations are transactional and audited;
successful semantic mutations publish the Git-trackable checkpoint by default.
The installed binary is `bead`, and `native-v1` plus `needle-v1` expose public
process contracts.

The latest tagged package is 0.2.4, edition 2024, MSRV 1.85. The current
checkout declares package version 0.2.6 and the installed fleet binary reports
0.2.6, but no matching tag is present in this checkout. The repository has
continued to receive diagnostic, attempt-resolution, and starvation-recovery
work after the tag; those checkout artifacts are not release evidence until
their specifications, capabilities, tests, tag, and release report agree.

### 1.1 Shipped capability baseline

| Capability | Current artifact |
| --- | --- |
| Workspace and lifecycle | Native workspace discovery, SQLite migrations, issue CRUD, assignment, manual blocking, close reason/`closed_at`, reopen, labels, dependencies, conditional readiness |
| Atomic coordination | Server-selected claim, optional single-claim policy, leases, monotonically fenced recovery, revision guards, and atomic resource locks |
| Scheduling | FIFO plus aging, impact, rotation, balanced, attempt tiers, retry/quarantine state, and readiness explanations |
| Structured extension | Public schema catalog, schema-bound bead data, namespaced references, unique-reference creation, safe queries, change feed, recurrence, and atomic bulk manifests |
| Recovery | Read-only-by-default doctor, explicit verified restore, monolithic/sharded checkpoints, generation pointers, archaeology/reconcile support, and checkpoint auto-flush |
| Sensitive-content safety | Current `main` rejects high-confidence findings before mutation, reports advisory findings without values, and provides fingerprint-selected audited historical redaction with anti-resurrection state; 0.2.4 predates these capabilities and BR-T18 release conformance remains open |
| Consumer contract | Machine-readable capabilities and the `needle-v1` profile |
| Explanation | `why`, policy validation, compare, scoped diagnostics, and readiness/exclusion reporting |

The current implementation contains the primitives NEEDLE needs for safe task
state: atomic claim, revision, lease/fencing, close/release/reopen,
failure-aware scheduling, audit events, durable checkpoint publication, and a
first-class portable attempt receipt. `bead resolve` atomically records an
attempt outcome, updates failure state, and performs the requested lifecycle
transition. That path is implemented but is not release-backed until BR-T10
and BR-T11 complete against one exact source commit and pinned binary.

### 1.2 Governance and artifact mismatches

- The former plan still says version 0.1 is incomplete while versions 0.1.x
  and 0.2.x have been tagged and 0.2.4 is current.
- `.marathon/COMPLETE` declares 41/41 features through R024 but retains pending
  artifact/evidence hashes. Other Marathon status files predate completion and
  still say the sentinel does not exist.
- The historical plan adopts R025–R037 after the frozen 41-feature ledger, so
  the sentinel is not evidence for those later requirements.
- Several specifications that the historical status calls missing now exist,
  including checkpoint-set, native field-guide, resource-lock, bulk-manifest,
  and verified-restore contracts.
- The current checkout exposes `resolve`, `watchdog`, and `analyze-exclusion`,
  but the exhaustive command/event contract does not classify them. Resolve
  and watchdog have semantic mutation paths; `analyze-exclusion --attach`
  writes a checkpoint-carried comment without a same-transaction audit event.
- Some post-release starvation recovery uses OS-process or fallback heuristics
  and separate log artifacts. Those paths require normative review against
  leases, fencing, atomic mutation, audit, and the boundary in ADR-010; their
  presence in a checkout is not approval of their semantics.
- Claim service layers accept some harness/model metadata internally, but the
  public CLI does not yet supply a durable attempt identity across claim and
  outcome.
- The installed 0.2.6 development binary now advertises secret ruleset v3 and
  historical redaction, but no tagged release carries that contract. The
  motivating NEEDLE records have been redacted and their exposed credential
  rotated; immutable Git history still requires rotation as containment.
- Many integration suites create workspaces below the host temporary root and
  assume no unrelated ancestor `.beads` exists. They pass under a neutral
  temporary root but fail under the shared `/home/coding` layout. A shared
  fixture must opt into `--skip-foreign-workspace` only for deliberate test
  initialization; production discovery remains fail-closed.
- `bead init` auto-migrates through `SqliteStore::with_path` before capturing
  its prior schema version, so its pending-migration report is unreachable.
  Exact-source `schema_upgrade_on_init` exposes the mismatch.
- Workspace probing was itself mutating because it used the auto-migrating
  connection path. ADR-016 and commit `36432b2` make probing observational;
  exact-source store discovery and all nine R036 tests pass.

## 2. Current design principles

1. **Task truth is small and deterministic.** bead-rs owns task fields,
   relationships, lifecycle, concurrency guards, factual attempt receipts,
   audit, and recovery—not orchestration or learning policy.
2. **Every mutation is atomic, auditable, and concurrency-tested.** A feature
   that cannot state its transaction, idempotency, conflict, event, and
   checkpoint behavior is not ready to implement.
3. **Capabilities govern consumers.** A consumer uses semantic capability and
   schema negotiation, never version-string folklore or command probing.
4. **Recovery follows stored facts.** Revision, lease, fencing, timestamps,
   events, and declared relationships are authoritative. Title similarity,
   process-name searches, and undeclared intent do not silently mutate work.
5. **No policy smuggling.** Labels, notes, or generic data cannot become an
   unversioned substitute for a required typed store contract.
6. **Checkpoint publication follows committed state.** SQLite commits first;
   automatic publication then advances a verified generation. Publication
   failure never invents or repeats the semantic mutation.
7. **Clean-room provenance is a release property.** New behavior begins with
   an independently reviewable normative contract and fixtures.
8. **Historical evidence is immutable except for verified erasure.** New
   requirements receive new release evidence rather than expanding an old
   completion sentinel retroactively. Detectable sensitive bytes may be
   destroyed only through the typed redaction operation in section 5.1; record
   identity and a nonsecret redaction receipt remain immutable.
9. **Secret removal is not ordinary editing.** No general event editor, issue
   purge, database patch, checkpoint rewrite, or broad scanner exemption is
   introduced as a side effect of redaction.

## 3. Software-factory extension boundary

The combined factory requires two new bead-rs concepts: a portable, immutable
attempt outcome that can be committed atomically with the issue's failure tier
and lifecycle transition, and an exceptional audited redaction operation that
can remove sensitive bytes already accepted by an older store without becoming
a general history editor.

### 3.1 What bead-rs stores

- caller-provided `attempt_id`, actor, issue ID, and contract version;
- expected revision and lease/fencing token when applicable;
- bounded harness/tool identity metadata;
- caller-selected outcome class and requested lifecycle action;
- reason plus bounded opaque evidence/side-effect receipt references;
- canonical request hash, resulting issue revision/state/attempt tier, event
  identity, and checkpoint publication state.

### 3.2 What bead-rs does not decide or store

- prompts, full transcripts, hidden model reasoning, or memory embeddings;
- whether tests or other evidence actually prove the requested work;
- lesson extraction, confidence, reinforcement, experiments, or promotion;
- AGENTS.md/CLAUDE.md precedence or policy content;
- provider budgets, fleet routing, controller scheduling, or human-escalation
  meaning;
- external side-effect execution.

NEEDLE supplies the semantic classification after applying its verifier and
policy. bead-rs validates the request's schema and concurrency conditions and
commits the factual result. Evidence references are opaque identifiers;
bead-rs does not fetch or judge them.

### 3.3 Historical-redaction boundary

Secret prevention and historical repair are different contracts. ADR-014
rejects newly supplied detectable secrets before a semantic transaction.
Historical redaction handles bytes already present in SQLite, issue snapshots,
audit events, comments, structured data, receipts, or retained checkpoint
generations. The operation is native maintenance/recovery behavior; NEEDLE may
request or monitor it but cannot implement it by editing bead-rs artifacts.

Redaction preserves semantic identity and removes content. It retains issue and
event IDs, ordering, lifecycle state, dependencies, revisions, and attempt
facts; replaces only the exact fingerprinted field or byte range with a fixed
nonsecret marker; and appends a receipt containing actor, reason, rule,
selector, prior-content fingerprint, and resulting checkpoint identity. It
never stores the removed value in the receipt, diagnostic, command line, or
temporary report.

## 4. Portable attempt-outcome contract

The exact wire contract must be authored as
`research/specs/attempt-outcome-v1.md`, independently reviewed, and accompanied
by fixtures before implementation. The following requirements constrain that
specification.

### 4.1 Outcomes and actions are orthogonal

At minimum, the versioned outcome vocabulary distinguishes:

- verified success asserted by the caller;
- work/bead-scoped failure;
- infrastructure failure;
- cancellation/interruption;
- indeterminate result.

Lifecycle actions include close, release, block/manual-block, quarantine,
none, and only those additional transitions adopted by the normative model.
The outcome determines attempt-tier accounting; the action determines issue
state. Invalid combinations fail validation before mutation.

### 4.2 Exactly-once receipt semantics

One `attempt_id` binds one canonical request hash per workspace:

- first valid resolution commits the receipt and transition;
- identical replay returns the original receipt and resulting state without a
  new event or revision;
- different replay conflicts;
- expected-revision or fencing mismatch conflicts without recording a false
  outcome;
- store transaction failure changes nothing;
- checkpoint publication failure reports a committed-but-unpublished state and
  is repaired by existing publication machinery, not by repeating resolution.

### 4.3 Atomic transaction

One transaction validates ownership/concurrency, inserts the immutable
attempt-outcome receipt, updates failure epoch/tier if required, performs the
legal lifecycle transition, releases resource locks where existing lifecycle
rules require it, appends audit/change-feed events, and returns the
authoritative resulting state.

Claim gains additive attempt correlation only if the normative specification
proves the behavior and compatibility. Caller-provided attempt IDs allow
NEEDLE to create correlation before dispatch; bead-rs remains free to expose a
server receipt ID as well.

## 5. Capability and compatibility contract

`bead capabilities` will advertise the attempt-outcome schema, atomic resolve,
idempotent replay, claim correlation, revision/fencing support, allowed outcome
and action enums, checkpoint representation, and schema references.

Capability absence means unsupported. NEEDLE may use a tested sequence of
existing close/release/data/event operations, followed by authoritative
reconciliation, but must label that resolution non-atomic. It may not infer
support from `bead --version` or from help text.

The transition must preserve:

- old clients reading checkpoints produced by new clients according to the
  declared forward-compatibility rules;
- new clients operating against older bead-rs releases through an explicit
  fallback;
- existing lifecycle commands and idempotence;
- automatic-flush and explicit suppression behavior;
- the clean-room `needle-v1` consumer conformance boundary.

If additive changes cannot satisfy those guarantees, publish a new contract
profile rather than silently changing `native-v1` or `needle-v1`.

### 5.1 Exceptional historical redaction

The normative `historical-redaction-v1` and `secret-rejection-v1` contracts were
accepted at their exact submitted hashes in the R038 review record. They define
a two-step, output-redacted flow:

1. `bead doctor --scope secrets --format json` scans every operator-supplied
   text field plus the current and retained recovery generations and returns
   only rule IDs, stable finding fingerprints, record selectors, field paths,
   ranges, and severity. It never returns matched bytes.
2. `bead redact --finding FINGERPRINT --actor ACTOR --reason REASON` acquires
   the workspace maintenance and checkpoint-publication locks, revalidates the
   finding against current state, and performs one idempotent redaction epoch.
   The finding fingerprint selects the bytes; the bytes themselves are never
   accepted on argv or stdout. A fixed typed redaction marker replaces them.

Redaction is deliberately destructive and therefore narrower than ordinary
mutation:

- only scanner-produced findings against supported text fields are eligible;
- issue/event identities, origin sequence, status, graph, and unrelated bytes
  cannot change;
- SQLite changes and the nonsecret redaction receipt commit atomically;
- publication creates a sanitized generation set without retaining a
  secret-bearing `previous.json` root and tombstones every superseded dirty
  object only after the sanitized pointer is durable;
- interruption before the SQLite commit changes nothing; interruption after
  it leaves a resumable committed-but-unpublished redaction epoch;
- identical replay returns the original receipt; a stale or changed target
  conflicts without mutation;
- import, merge, reconcile, and restore give a known redaction receipt
  precedence over older matching content so a stale checkpoint cannot
  resurrect removed bytes; and
- diagnostics, receipts, events, temporary files, tracing, and errors contain
  fingerprints and selectors only, never matched content.

False positives use a separate audited acknowledgment from ADR-014. An
acknowledgment is scoped to one finding fingerprint and rule. There is no
checkpoint-directory, field-class, or scanner-wide exemption. Recovery input
that predates a redaction may be inspected and reported, but it cannot replace
a sanitized live workspace without explicitly applying every known redaction
receipt.

`bead capabilities` advertises `secret_scan` and
`historical_redaction = { contract, doctor_findings, atomic_redact,
anti_resurrection, sanitized_generation_set }`. Capability absence means an
operator must stop; hand-editing SQLite or checkpoint JSON is never a fallback.

## 6. Artifact-by-artifact transition ledger

| ID | Artifact(s) | Change | Acceptance evidence | Status |
| --- | --- | --- | --- | --- |
| BR-T01 | `docs/plan/plan.md`, ADR README, Marathon status docs | Re-baseline 0.2.4 reality; distinguish frozen Marathon evidence from later releases | Internal link/status audit; no retroactive ledger edits | current for plan/ADRs; status-doc reconciliation pending |
| BR-T02 | post-0.2.4 exclusion/starvation/watchdog code and help | Inventory behavior; retain read-only explanation; remove, disable, or normatively redesign heuristic mutation outside lease/fencing rules | Spec trace, atomic/audit tests, capability inventory parity | implemented; exhaustive event-contract gate remains in BR-T20 |
| BR-T03 | `research/specs/attempt-outcome-v1.md` plus independent fixtures/review | Define portable receipt, canonical hashing, outcomes/actions, conflicts, events, and checkpoint form | Recorded independent approval and fixture hashes | accepted/current |
| BR-T04 | model and public schema catalog | Add versioned attempt outcome request/receipt types and bounded metadata | schema validation and compatibility fixtures | implemented/current |
| BR-T05 | SQLite migration and checkpoint import/export | Add immutable attempt identity/receipt storage and lossless checkpoint representation | migration, round-trip, unknown-field, restore, and corruption tests | implemented/current |
| BR-T06 | service transaction | Atomically dedupe receipt, update attempt tier, apply lifecycle, audit, and return state | concurrent replay, hash conflict, stale revision/fencing, crash-boundary tests | implemented/current |
| BR-T07 | `bead resolve` CLI and optional claim correlation | Expose machine-readable request/receipt without leaking orchestrator policy | recursive help, JSON contract, exit-code, and installed-binary tests | implemented/current |
| BR-T08 | capabilities and `needle-v1` contract | Advertise exact atomic-attempt semantics and schema references; reconcile complete command inventory | capability snapshots and old/new profile fixtures | implemented; exact release matrix remains open |
| BR-T09 | audit/change feed/why/doctor | Surface receipt and resulting state; diagnose inconsistent legacy sequences without inventing facts | explanation and non-destructive diagnostic tests | implemented/current |
| BR-T10 | release evidence and governance status | Produce current feature/capability matrix tied to tag, commit, spec and test hashes | noninteractive verifier passes; status documents agree | in progress (`beadrs-15fcfce1`) |
| BR-T11 | NEEDLE consumer conformance | Test atomic path, unknown-result replay, and older-backend fallback | pinned NEEDLE + old/new bead-rs integration matrix | blocked by BR-T10 (`beadrs-eb52656d`) |

| BR-T12 | mutating CLI commands (`close`, `release`, `reopen`, `update`, `claim`, `dep`, `label`, `comments`) and `service::issues` | Accept `--actor` and the `BEAD_ACTOR` environment variable on every mutating command and record it in the audit event instead of the `system` default; additive and profile-neutral | forensic fixtures show the caller actor on `closed`, `released` and `reopened`; old clients unaffected; `needle-v1` capability snapshot updated | transition (independent of BR-T03 to BR-T08) |
| BR-T13 | ADR-014, ADR-015, `research/specs/secret-rejection-v1.md`, `research/specs/historical-redaction-v1.md`, independent review and fixtures | Freeze secret detection, fingerprints, selectors, fixed marker, acknowledgment, redaction receipt, failure, and anti-resurrection semantics before code | [Accepted exact-hash review](../reviews/r038-specification-acceptance-2026-09-03.md); no live-format committed samples | accepted/current |
| BR-T14 | `src/scan/`, configuration, doctor, dry-run, mutation service boundaries | Add the offline versioned blocking/advisory scanner and reject detectable secrets before every operator-text mutation | command inventory coverage, provider/placeholder fixtures, redacted diagnostics, scan-cost benchmark | implemented/current; ruleset v3 adds context-bound Garage key-ID assignments |
| BR-T15 | model, public schemas, SQLite migration, checkpoint grammar | Add finding, acknowledgment, redaction receipt/epoch, field selector, and durable anti-resurrection tombstone records | migration, schema, unknown-field, checkpoint round-trip, old-reader, and restore tests | implemented/current |
| BR-T16 | transactional redaction service | Revalidate one fingerprint, replace only selected bytes with the fixed marker, preserve semantic identities, and commit one idempotent receipt | stale target, exact replay, concurrent mutation, crash boundary, unchanged-field, and no-value-output tests | implemented/current |
| BR-T17 | `bead redact`, checkpoint publication, import/merge/reconcile/restore | Expose the maintenance command; publish a sanitized generation set; tombstone dirty roots; prevent known removed content from reappearing | current/previous/forensic scan, publication interruption/resume, anti-resurrection, and recursive-help tests | implemented/current |
| BR-T18 | conformance, recovery rehearsal, packaging, NEEDLE incident remediation, release evidence | Prove zero unacknowledged findings, semantic restore equivalence, safe install, and cleanup of the motivating records without disclosure | exact-release gitleaks report, restored counts/graph/events, Forgejo push acceptance, NEEDLE cross-reference and credential-rotation receipt | in progress; release gates remain open |
| BR-T19 | integration-test workspace fixtures | Centralize deliberate workspace initialization so tests remain hermetic beneath an unrelated ancestor store without weakening production discovery | affected suites pass with both the host default and a neutral temporary root | partial in `9f02c7a`; blocked on claim-epoch overlap (`beadrs-94fd9fc2`) |
| BR-T20 | command/event contract, attempt outcomes, watchdog, exclusion comments | Classify every visible command and require each semantic path to advance audit state transactionally | real-effect contract probes cover resolve, watchdog release, and exclusion attachment; read-only modes remain unchanged | blocked on BR-T19 and claim-epoch overlap (`beadrs-d0cd90d1`) |
| BR-T21 | workspace discovery and doctor | Make probing observational so diagnostics never initialize or migrate a store | missing database remains absent; store discovery and R036 suites pass | implemented in `36432b2` (`beadrs-e498fb31`) |
| BR-T22 | init and migration API | Capture and report the schema transition owned by explicit init without disabling migrate-on-open for normal commands | pending/current migration tests pass and older rows survive | blocked on claim-epoch overlap (`beadrs-5c27b273`) |
| BR-T23 | claim model, SQLite migration, claim response and checkpoint grammar | Mint a durable monotonically advancing credential for every leased or ordinary claim epoch and return its exact projection to the claimant | migration/restore round trip, no-duplicate issuance under contention, and old-checkpoint compatibility | transition; `beadrs-bd985270`, umbrella `beadrs-8c343a7c` |
| BR-T24 | claimant-owned lifecycle mutations and operator override | Require the exact current claim-epoch credential for claimant mutation; make any operator override explicit, reason-bearing, and separately audited | stale, missing, mismatched, replayed and override transaction tests with no false audit event | blocked by BR-T23; `beadrs-9d740f26`, `beadrs-dc8df464` |
| BR-T25 | lease expiry, recovery and stale-worker fencing | Add atomic compare-and-reap and guarantee that an older process cannot mutate after release, expiry, reassignment or recovery | deterministic two-worker and crash-boundary tests; valid current lease is never reaped | blocked by BR-T24; `beadrs-3be4bf40`, `beadrs-0d0cb036` |
| BR-T26 | ADR-017, `needle-v1` specification, schemas, recursive help, capabilities and concurrency fixtures | Freeze and advertise the claim-epoch contract, then prove every command and profile obeys it without weakening older profile behavior | accepted ADR/spec review, capability snapshots, installed-binary help and twenty-claimant concurrency fixture | transition chain through `beadrs-eec200d1`, `beadrs-24a3a27b` |
| BR-T27 | exact-source packaging and NEEDLE consumer conformance | Build one pinned artifact and run the old/new consumer matrix plus duplicate-worker replay before release | source/binary hashes, archive-build proof, restore rehearsal, NEEDLE canary and rollback receipt agree | blocked by BR-T23–BR-T26; `beadrs-41b9130e` |
| BR-T28 | existing manifest transaction, planner guidance, dependency graph and resource declarations | Make manifest-based atomic materialization the required/default planner path; retain assigned-staging only for shapes the manifest cannot express | concurrent claimer observes zero wins before graph commit; create resource keys are present at first visibility; cycle, missing-ID and replay failures leave no partial issue or edge | transition; `beadrs-57c668be` |

General mutation idempotency remains a separate potential feature. BR-T03–T08
adopt idempotency only for the attempt-resolution boundary required by the
combined factory.

BR-T13–T18 are R038. They are security/recovery work, not learning policy and
not a relaxation of the immutable-event model. The motivating NEEDLE incident
is tracked as `needle-27ec0073`; cross-repository references provide
traceability, while dependency truth remains inside each native workspace.

**Binary builds never mutate the shared checkout (added 2026-09-02).** The pinned-binary
work (BR-T10 evidence, beadrs-8eb168ca and children) built older commits by stashing, resetting
and checking out inside the single shared NEEDLE checkout; 17 stash entries and several
reset/checkout moves on 2026-09-01/02 erased another worker's uncommitted hour of work
(beadrs-e167fde8). The sanctioned way is `scripts/build-from-archive.sh <sha>`: extract with
`git archive` into a scratch directory, build there, copy the binary and metadata out, remove
the directory on success. Bead: beadrs-5a0dc962.

**Attribution (added 2026-09-01, revision 10).** `service::issues` already
accepts an optional actor and defaults to `system` (`src/service/issues.rs`);
only the CLI omits the flag, and the claim path is the sole command that
records a caller identity today. Because `git-activity-exporter` and the
NEEDLE attempt ledger (NEEDLE plan section 4.4) both consume
`.beads/checkpoint/forensic.jsonl`, a `system` actor on close makes every
closure unattributable fleet-wide (measured 0 of all `closed`, `released`,
`updated`, `reopened` events). BR-T12 closes that gap now, without waiting
for the attempt-outcome tranche; the receipt in BR-T03 to BR-T06 carries the
same actor field once it exists.

## 7. Release gates for the extension

The attempt-outcome capability is releasable only when:

1. the normative specification and fixtures have independent approval;
2. migration and checkpoint round trips preserve every receipt and reject
   duplicate IDs with conflicting hashes;
3. concurrent identical resolve calls produce one mutation and one receipt;
4. work failure updates attempt scheduling exactly once, while infrastructure
   failure does not penalize the issue;
5. close/release/quarantine results obey existing revision, fencing, resource
   lock, and audit contracts;
6. a client retry after an unknown response returns the original result;
7. capability documents and the actual CLI command inventory agree;
8. old and new checkpoint/consumer compatibility fixtures pass;
9. complete fmt, Clippy, tests, packaging, installed-binary, restore, and
   NEEDLE conformance gates pass against the exact release commit;
10. the release evidence report contains exact commit, binary, specification,
    fixture, and report hashes;
11. the secret scanner covers every operator-text mutation and never prints a
    matched value, with no blanket bypass or workspace-supplied blocking rule;
12. redaction removes the selected bytes from SQLite, `current.json`,
    `previous.json`, the forensic view, every referenced object, and any
    operation-owned temporary while preserving unrelated semantic state;
13. restore and merge of pre-redaction input cannot resurrect a finding known
    to the destination, and a clean empty-target restore remains semantically
    equivalent after applying redaction receipts; and
14. a redacted exact-release artifact passes the same repository gitleaks
    configuration and a guarded Forgejo push without an allowlist broader than
    one independently classified false-positive fingerprint.

Current exact-source gate snapshot (2026-09-04): the redaction unit,
transaction, publication, recovery, installed-binary, package, and NEEDLE
remediation checks recorded in
[`brt18-ruleset3-remediation-2026-09-04.md`](../verification/brt18-ruleset3-remediation-2026-09-04.md)
pass. A neutral-temporary-root integration sweep passes every suite after
`schema_upgrade_on_init`; the two independent failures are the exhaustive
command/event registry (BR-T20) and pending-migration reporting (BR-T22).
Under the shared host temporary root, additional suites fail because they do
not yet use the BR-T19 fixture. Complete fmt and Clippy remain required against
one clean exact source commit; unrelated shared-checkout work is not release
evidence.

## 8. Combined-factory success measures

bead-rs reports facts useful to, but does not optimize, the learning system:

- unique attempt receipts and idempotent replays;
- outcome classes and committed lifecycle actions;
- revision/fencing conflicts and stale ownership;
- failure-tier transitions by readiness epoch;
- time from claim to durable resolution;
- checkpoint publication state and recovery;
- atomic versus caller-reconciled resolution capability;
- share of lifecycle events whose actor is a caller identity rather than
  `system` (BR-T12);
- rejected blocking-tier findings by rule and field, never by matched value;
- acknowledged false positives and completed redaction epochs by fingerprint;
- time from historical finding to rotation, sanitized generation, and
  accepted checkpoint publication; and
- attempted resurrection conflicts from stale recovery input.

NEEDLE owns verified-closure yield, lesson effectiveness, recurrence,
experiments, cost, and policy rollback. bead-rs must not change scheduling or
lifecycle because a lesson appears beneficial; it applies only explicit,
versioned requests that pass its invariant checks.

---

# Historical bead-rs 0.1 Implementation Plan (Superseded)

> The remainder of this file is retained as architecture and delivery history.
> Its version status, Marathon gates, roadmap completion, and deferred-feature
> statements are not the current plan.

Plan revision: 8

As of: 2026-08-21

Revision 8 change: R026 activates. The compiled automatic-flush default
flips to on, the capability document advertises `auto_flush`, and the
never-implicit-flush documentation reverses in the same commit across the
README, root help, the section 5.3 workflow summary, generated man pages, and
AGENTS.md, satisfying the section 13 handshake-and-documentation criterion.
Section 13 gate evidence is recorded against that commit; a failing criterion
reverts the compiled default. The surrounding environment's agent
instructions (home CLAUDE.md) carry the matching update outside this
repository, noted in the release rather than left silently stale.

Revision 7 change: ADR-004 raises the MSRV from Rust 1.75 to 1.85 with
edition 2024, corrects the section 8 dependency-verification wording and the
section 10 risk-register lane to cite it, and requires a pinned MSRV
verification lane in CI. Bootstrap-era 1.75 references in provenance and
review records remain as history. Migration is tracked by beads;
`Cargo.toml` stays authoritative for the shipped floor until they close.

Revision 6 change: adopts the run-4 ideation tranche R027-R034 (multi-clone
transport, checkpoint archaeology, self-defending discovery, resource locks,
idempotent creation, bulk manifests, stale-claim diagnostics) per the
2026-08-15 ideas-ledger product decision; moves resource locks and bulk
transaction manifests out of the section 14 deferred list and adds the
caller-owned stdio session to it. These items postdate `.marathon/COMPLETE`
and do not retroactively alter the recorded R001-R026 gates.

Revision 5 change: ADR-003 adopts automatic checkpoint flush on successful
mutation as the eventual default, adds section 6.2.1 and R026, and gates
activation on four prerequisites that make checkpoint publication incremental.
Explicit `sync flush-only` remains the default until that gate passes.

Revision 4 change: ADR-002 removes cross-tool profiles and migration tooling
and replaces them with a native field guide plus agent-guided rehydration.

Status owner: bead-rs release owner

Decision authority: accepted ADRs explain choices; normative files under
`research/specs/` define behavior; this plan defines delivery; after the final
G4 handoff, native beads and generated release evidence define work state.

Status: bootstrap MVP and Gates G1-G4 complete. The final handoff is recorded
at commit `ccb2c4e4304f7d69ecf0d9fedbe45d6c03e4c3f3`; native beads are now the
sole mutable work-state authority and the Marathon ledger is frozen audit
input. Version 0.1 remains incomplete: the ADR-002 replacements for F012/F013,
F015, F016, F017, and F014 still require passing evidence. F017 is
specification-blocked pending an independently reviewed normative
`research/specs/checkpoint-set-v1.md`; the field-guide replacement is blocked
on an independently reviewed native semantics contract.

This is the execution blueprint for the first usable `bead-rs` release. The
installed executable is `bead`. SQLite is its authoritative live store,
the monolithic or manifest-backed JSONL checkpoint is its portable recovery
backup and interchange artifact,
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
prose, or error prose. Implementation work must not inspect any other bead
implementation. If prohibited material becomes
visible, stop the affected component and append the exposure to
`PROVENANCE.md` before proceeding.

Normative specifications prevail if this plan contradicts them. Resolve such a
conflict by correcting the plan, never by silently changing the requirement.

## 2. Release definition

Version 0.1 is complete when F001-F017 have concrete passing evidence in the
active traceability system and every final release gate succeeds. Before G4,
`.marathon/feature_list.json` is that work-state system. After G4, the reviewed
native-bead mapping and evidence are authoritative; the frozen Marathon ledger
is an audit input, not a second mutable status store.

G4 completed on 2026-08-08. The bootstrap artifact passed the governed
self-hosting handoff with 181 tests, including ready-frontier regression
coverage. This establishes an MVP artifact suitable for NEEDLE execution, but
does not satisfy G5 or authorize a `0.1.0` compatibility/release claim.

Delivery has three distinct milestones that must not be conflated:

1. **Bootstrap MVP:** F001-F011 provide a native store, issue CRUD and graph
   operations, atomic claiming, the pre-F017 issue-only checkpoint, diagnostics,
   capabilities, and the complete `needle-v1` provider contract. The bootstrap
   is an installed, pinned development artifact used to prove self-hosting; it
   is not version 0.1 and does not authorize cross-tool checkpoint
   compatibility or migration claims.
2. **Self-hosted execution:** after the bootstrap gates pass, the installed
   `bead` binary materializes the remaining reviewed plan into a fresh native
   bead workspace. After graph reconciliation and a one-worker canary, clean-room
   NEEDLE workers use that workspace as execution authority. This changes how
   work is coordinated; it does not make any materialized feature complete.
3. **Version 0.1:** NEEDLE workers replace F012/F013's external-profile work
   with the ADR-002 native field guide and rehydration runbook, then complete
   F015, F016, F017, and
   finally F014 packaging, and every final release gate passes. Post-0.1
   R-items remain outside this release unless an accepted ADR, normative
   specification, and ledger change explicitly promote one.

The bootstrap MVP is successful when an implementer unfamiliar with the
internals can install the pinned artifact into a temporary root, initialize an
isolated workspace, create and relate work, obtain distinct atomic claims from
20 competing processes, round-trip the issue-only checkpoint, diagnose the
workspace, negotiate `needle-v1` capabilities, and complete a consumer-side
NEEDLE canary without touching another implementation's store. The intended
users are local coding-agent operators and NEEDLE workers that need a
deterministic, recoverable, auditable work queue. Materializing plan prose as
beads is representation evidence only, never implementation evidence.

The remaining implementation-ready work includes F015, F016, and the ADR-002
field guide and rehydration runbook. Cross-tool migration and external profile
work are removed rather than treated as release blockers. This is not a
release-readiness claim. F017 is a
design proposal only: implementation must not begin until the
new normative `research/specs/checkpoint-set-v1.md` exists and has been
independently reviewed. Plan prose cannot substitute for that specification.
The field guide cannot pass until its completeness tests cover every public
native issue field and lifecycle value. Plan prose cannot substitute for the
versioned guide emitted by the installed binary.

In scope:

- workspace initialization and versioned native SQLite migrations;
- issue CRUD, assignment, lifecycle, labels, notes, and dependencies;
- deterministic readiness and atomic server-selected claiming;
- deterministic monolithic or sharded checkpoint import/export, complete
  forensic history, and unknown-field preservation;
- explicit per-bead public schema identification;
- diagnostics and narrowly scoped repair;
- machine-readable capabilities and NEEDLE v1 subprocess compatibility;
- explicit `native-v1` recovery and `needle-v1` subprocess contracts;
- a versioned native field guide and agent-guided rehydration runbook;
- an Apache-2.0 Rust crate providing the `bead` binary.

Out of scope:

- reading or modifying another implementation's SQLite database;
- parsing or transforming another implementation's checkpoint schema;
- direct agent writes to native SQLite or synthesized recovery checkpoints;
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
`--comments none|unresolved|all`, defaulting to `none`; `none` may retain only
counts and resolution metadata, `unresolved` includes bodies only for
unresolved comments plus the minimum parent metadata needed to identify their
threads, and `all` includes every comment body in canonical comment order.
This makes conversational context optional without making the backup
incomplete. The flag and those meanings apply to both native and `needle-v1`
list/show parsing. A NEEDLE invocation that omits the flag therefore retains
its v1 envelope and receives no comment bodies; capability-aware callers may
request either additive projection explicitly.

Native priority taxonomy is:

| Value | Name | Scheduling intent |
| --- | --- | --- |
| P0 / `0` | urgent | immediate incident, safety, or release-blocking work |
| P1 / `1` | critical | essential work that should precede ordinary delivery |
| P2 / `2` | high | important planned work and the native default |
| P3 / `3` | normal | ordinary work with no elevated urgency |
| P4 / `4` | aspirational/backlog | speculative, someday, or low-urgency work; eligible under 0.1 `fifo-v1` |

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
base state. For a nonclosed bead, `update --status open` clears the flag and
sets the base state to open. It is invalid for a closed bead: only `reopen ID`
may cross `closed` to `open`, clear closure metadata, and append the reopen
audit event. Thus generic update can never bypass reopen validation or its
forensic record. Finishing the last graph blocker reveals the stored base
status unless the manual flag remains set.

Native input accepts only the lifecycle values defined by its exact schema.
Unknown imported status values fail validation; they are never treated as open
or silently retained for activation.

The dedicated lifecycle commands have this complete base-state contract. A
conflict changes no fields or timestamps, appends no event, prints no success
payload, and exits 4. Every success prints the ID plus LF and exits 0.

| Command | `open` | `in_progress` | `deferred` | `closed` |
| --- | --- | --- | --- | --- |
| `close --reason R` | semantic close | semantic close | semantic close | idempotent only when stored reason equals normalized `R`; otherwise conflict |
| `reopen` | idempotent | conflict | conflict | semantic reopen |
| `release` | idempotent only when unassigned; assigned is conflict | semantic release | conflict | conflict |

`close` always validates a nonblank normalized reason, including on a closed
bead. A semantic close sets `closed`, clears manual blocking, sets `closed_at`
and `updated_at` to the transaction instant, retains assignment, stores the
reason, and appends exactly one durable `closed` event with the prior base
state, reason, actor when known, and resulting base state. Its idempotent case
does not change either timestamp or advance the event sequence. `reopen`'s
semantic case sets `open`, clears `closed_at`, `close_reason`, and manual
blocking, retains assignment, sets `updated_at` to the transaction instant,
and appends exactly one durable `reopened` event with the prior and resulting
base states and actor when known. Its idempotent case changes no timestamp and
appends no event.

A semantic `release` sets an `in_progress` bead to `open`, clears its
assignment, sets `updated_at` to the transaction instant, and appends exactly
one durable `released` event containing the bead ID, prior assignee, actor when
known, and resulting base state. Migration 1 has no last-claim sequence or
attempt history to preserve; R019 defines preservation of those fields after
its migration. Validation, state change, and event append for each semantic
lifecycle operation occur in one write transaction.

`update ID --clear-assignee` is the explicit operation for an assigned bead
whose base state is already `open`; it conflicts with `--assignee`. On an open,
assigned bead it clears the assignee and appends exactly one durable
`assignment_cleared` event containing the bead ID, prior assignee, actor when
known, and unchanged base state, in the same write transaction. On an open,
already-unassigned bead it succeeds idempotently without changing
`updated_at`, advancing the event sequence, or appending an event. On an
`in_progress`, `deferred`, or `closed` bead it is an invalid-transition
conflict (exit 4): use `release`, an explicit lifecycle update, or `reopen` as
appropriate rather than silently changing assignment under that lifecycle.

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
by update. In 0.1, selection, final eligibility validation, assignment, and the
minimal claim audit record commit in one
`BEGIN IMMEDIATE` transaction. Version 0.1 has no lease request, expiry,
renewal, or fencing-token fields. Twenty competing processes must never receive
the same successful issue ID.

Version 0.1 implements only `fifo-v1`: eligible issues sort by declared priority
ascending, `created_at` ascending, then ID ascending. With no eligible issue,
claim returns exit 0 and `{}` in JSON mode without mutation. The richer policies
below are adopted post-0.1 behavior and must not silently change `fifo-v1`.
`fifo-v1` persists no claim sequence, last-claim value, attempt counter,
ready-age value, retry state, or ranking-factor snapshot. Its successful claim
audit is exactly one `events` row whose kind is `claimed`, issue ID is the
selected issue, actor is the assignee, time is the claim instant, and detail is
the canonical object `{"policy":"fifo-v1","resulting_base_status":"in_progress"}`;
the event table supplies its normal origin identity and ingestion sequence.
Optional model, harness, and harness-version values are telemetry only in the
separate `claim_telemetry` row and do not enter selection or the audit detail.

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

R019 adds a monotonically increasing workspace `claim_sequence`. Each bead
then records its last successful claim sequence and R019 attempt state. Within comparable
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
  issues.jsonl      issue-per-line interchange checkpoint
  checkpoint/
    current.json    authoritative checkpoint-mode/generation pointer
    previous.json   immediately previous verified generation pointer
    forensic.jsonl  nonauthoritative view of an active forensic monolith
    manifests/      immutable generation-named sharded manifests
    objects/        content-addressed issue and event JSONL shards
  config.json       nonsecret workspace configuration
  receipts/         native recovery and reconciliation receipts
  .gitignore        ignores journals and temporary files
```

`bead init [--prefix PREFIX]` creates this workspace without modifying
unrelated files. Repeating it with the same prefix succeeds; a conflicting
prefix fails without mutation. Use user-only write permissions where
supported.

The generated `.beads/.gitignore` ignores SQLite, WAL/journal files, locks, and
operation-owned temporaries, but does not ignore `issues.jsonl`, forensic
checkpoint views, current/previous pointers, immutable manifests, or referenced
checkpoint objects. Those files are
deterministic project artifacts intended to be committed. `bead-rs` never runs
Git commands or creates commits; the surrounding repository workflow flushes
before commit and pushes to its authoritative host, from which a configured
mirror may publish the history to GitHub.

Workspace discovery walks from the current directory toward the filesystem
root and stops at the first `.beads` directory it encounters (R030). When that
directory carries the bead-rs workspace fingerprint — `.beads/config.json` —
it is the selected workspace. When it does not, discovery fails closed with a
diagnostic that names the directory and claims only that it is not a bead-rs
workspace; which foreign format occupies it is neither inspected nor named,
keeping the clean-room boundary intact. Discovery never continues past a
foreign store to an unrelated parent workspace, and `bead init` equally refuses
to write into a `.beads` directory lacking the fingerprint. Legitimate
nesting under an unrecognized `.beads` is opt-in: the global
`--skip-foreign-workspace` flag continues the search past it and widens only
the search — it never authorizes writing into the skipped directory. Never
follow a `.beads` symlink outside the selected workspace for a mutation.

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
| `issues` | ID PK; title, description, notes, priority, issue type, base status, manual-blocked flag, nullable assignee, created/updated/closed timestamps, nullable close reason/source repo, origin profile, and schema reference, with section 3 invariants as checks where SQLite can enforce them |
| `issue_extensions` | issue ID + key PK, canonical JSON, origin profile |
| `labels` | issue ID + label PK, issue FK cascade |
| `dependencies` | blocked + blocker + kind PK, two issue FKs cascade, no self-edge; migration 1 has no condition column |
| `comments` | random ID, issue ID, author, immutable body, reply-to ID, resolution state, creation time |
| `issue_data` | issue ID + namespace PK, schema reference, canonical JSON value, issue FK cascade |
| `claim_telemetry` | claimed-event sequence PK/FK plus nullable model, harness, and harness-version hints; absent when no hint was supplied |
| `events` | integer sequence, optional issue ID, kind, actor, time, and canonical JSON detail |
| `checkpoint_state` | singleton last issue-interchange hash, covered event sequence, and export time for the pre-F017 `.beads/issues.jsonl` checkpoint |

Add only indexes justified by v0.1 queries:

- issues on `(base_status, manual_blocked, assignee, priority, created_at, id)`;
- dependencies by blocker and by blocked issue;
- labels by label and issue;
- comments/events by issue plus time/sequence.

Migration 1 contains exactly the tables listed above: it has no
`claim_attempts`, `scheduler_state`, or `scheduling_metrics` table. Successful
claims are audited only through `events`, with optional request telemetry in
`claim_telemetry`. Do not add conditional-dependency, intelligent-policy,
claim-sequence, attempt, retry, ready-age, graph-metric, cache, lease,
revision-guard, recovery-subsystem, provenance-receipt, origin-event-identity,
tombstone, changed-path, or compatibility-shaped columns without the owning
feature's new migration. A post-0.1 `scheduling_metrics`
cache is permitted only under the correctness rules in section 3.5.9.

Before F017 is available, migration 1 and F007/F008 implement the complete
issue-only checkpoint contract: `bead sync --flush-only` atomically replaces
`.beads/issues.jsonl` from one read snapshot, and `bead sync --import-only`
stages and validates exactly one explicitly named issue-per-line JSONL file
before applying it transactionally. The migration-1 `checkpoint_state` row is
sufficient for that contract: its covered event sequence identifies the live
snapshot represented by the issue projection even though the file contains no
event records. No pointer, manifest, shard, forensic event, or provenance
receipt is created or required. F017 later upgrades these commands to the
native forensic checkpoint-set grammar and storage described provisionally in
section 6; that upgrade owns its additional migration and must preserve the
working issue-only import/export path as `issues-jsonl-v1` interchange.

After `research/specs/checkpoint-set-v1.md` is independently accepted, F017
owns a new core migration—not migration 1—that adds immutable event origin
identity/hash and local ingestion ordering, `provenance_receipts`, and the
generation/mode/root, pending-tombstone, and changed-path checkpoint state
required by that normative specification. The migration must preserve and
deterministically assign origin identity to all migration-1 audit events before
F017 checkpoint publication can activate.

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
| `bead list --json [--status S] [--assignee A] [--ready] [--comments none\|unresolved\|all] --limit N` | matching records in claim order; filters intersect; `--ready` applies the exact section 3.4 predicate; limit 0-999999 |
| `bead show ID --json [--comments none\|unresolved\|all]` | one-element JSON array for NEEDLE v1 |
| `bead claim --assignee A [telemetry] --json` | atomic claim; one JSON object |
| `bead update ID [--status S] [--assignee A\|--clear-assignee] [--notes N]` | atomically apply supplied changes; closed-to-open requires `reopen` |
| `bead release ID` | atomically return claimed work to open and unassigned; print ID plus LF |
| `bead reopen ID` | restore open lifecycle |
| `bead close ID --reason TEXT` | finish with retained reason |
| `bead label add ID --label L` | idempotent presence |
| `bead label remove ID --label L` | idempotent absence |
| `bead dep add BLOCKED BLOCKER --kind KIND` | add canonical edge |
| `bead dep remove BLOCKED BLOCKER [--kind KIND]` | remove matching edge(s) |
| `bead sync --flush-only [--profile P] [--output PATH]` | before F017, atomically publish issue-only `.beads/issues.jsonl` when output is omitted or export that issue-only format to a new explicit path; after F017, use the upgraded forensic publication/export contract in section 6; under the R026 automatic default it remains supported and is idempotent against a clean checkpoint |
| `--no-auto-flush` (global) | suppress R026 automatic post-commit publication for one invocation, leaving the checkpoint dirty; overrides the `checkpoint.auto_flush` workspace configuration key |
| `--skip-foreign-workspace` (global) | R030: let discovery continue past the first `.beads` directory when it lacks the bead-rs fingerprint, so a bead-rs workspace farther up can be selected; widens the search only and never authorizes writing into the skipped directory |
| `bead sync --import-only --input PATH [--profile P] [--dry-run]` | before F017, stage and transactionally replace an empty store from exactly one explicitly named issue-only JSONL file; after F017, this base grammar is extended with the required `(--restore-into-empty\|--merge) --actor ACTOR` forensic semantics in section 6.3 |
| `bead sync --status --format json` | before F017, issue-only checkpoint hash, covered/live sequence, time, and dirty state; after F017, the richer root/mode/changed-path status in section 6.2 |
| `bead doctor [--repair]` | diagnose; optionally perform safe repairs |
| `bead capabilities --format json --profile P` | versioned capabilities and supported schema references |
| `bead schema list --format json` | list supported public document schemas |
| `bead schema show SCHEMA_REF --format json` | emit the exact versioned JSON Schema document |
| `bead schema explain SCHEMA_REF --format json\|markdown` | emit the versioned field semantics, ownership, invariants, examples, and common mistakes for an agent; JSON and Markdown derive from one typed source |

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
`list --ready` returns exactly the ready frontier defined in section 3.4,
intersects with any other supplied filters, and uses the same deterministic
priority/creation-time/ID order as `fifo-v1` claim. It is a read-only
inspection and never reserves work; concurrent callers must still use `claim`
to obtain atomic ownership. Comment projection does not change record
selection, ordering, or the enclosing NEEDLE envelopes.

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
-> inspect ready work -> claim -> update/release -> close
-> checkpoint published automatically with every successful mutation
```

Here “inspect ready work” is the concrete nonmutating command
`bead list --ready --json --limit N`; it uses claim order but does not reserve
the displayed beads.

Root long help and `bead(1)` explain the lifecycle in plain language: `open`
beads may be ready; manual blocking or unfinished `blocks` edges remove them
from the ready frontier; claim atomically assigns one ready bead and moves it to
`in_progress`; release returns it to open/unassigned work; close requires a
reason and may expose dependents; reopen restores an intentionally closed bead
to open. They distinguish base state from effective `blocked` status and state
that SQLite is authoritative live state, `issues.jsonl` is the issue-only
interchange checkpoint, and the F017 forensic checkpoint is the complete
portable backup, published automatically after every successful mutation. The
root page includes a minimal
end-to-end command example, points automation to `--json` and capabilities, and
links each lifecycle operation to its command page.

Root help also states that checkpoint files are designed to be Git-tracked and
that users or automation should run `bead sync --flush-only` before committing
the repository as an explicit idempotent check. It must not imply that
`bead-rs` performs the commit or push.

When R026 activated (plan revision 8), this help text, the workflow summary
above, the generated man pages, the README, and AGENTS.md reversed together in
the same commit that flipped the default: the closing step became automatic
publication rather than a remembered command, and `sync --flush-only` is
described as an explicit idempotent check plus the `--no-auto-flush` escape
hatch. Documentation must never describe a flush default the shipped binary
does not have, and no partial reversal is permitted.

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

### 6.1 Proposed native forensic checkpoint records

The issue-per-line interchange defined by the existing normative specifications
remains a distinct format: every nonblank line is one issue representation and
`.beads/issues.jsonl` retains that interchange meaning. It does not acquire
event, provenance-receipt, pointer, manifest, or shard records from this plan.
The multi-record historical corpus and sharded layout below are the proposed
native `forensic-checkpoint-set-v1` format, not the interchange format and not
an implemented compatibility claim.

Sections 6.1-6.3 are nonnormative F017 design input until an independently
reviewed `research/specs/checkpoint-set-v1.md` defines the format, immutable
schema identities, canonicalization, validation, restore/merge behavior, and
conformance fixtures. Under `AGENTS.md`, no F017 implementation may be derived
from these sections alone. If that future specification differs, it prevails
and this plan must be corrected before implementation.

Until that specification and F017 migration land, F007/F008 use only the
issue-per-line grammar: flush output is compact UTF-8 JSON, one issue per line
in bytewise issue-ID order with a final LF; empty state is a zero-byte file;
known/extension ordering and collision rules below apply, but forensic record
envelopes do not. Import accepts blank lines, treats every nonblank line as one
issue object, rejects malformed/non-object lines with a one-based line number,
duplicate IDs, invalid fields, dangling dependencies, and blocking cycles, and
activates only after the whole file validates. It requires an empty target,
preserves unknown fields and comments/data carried by the issue schema, and
replaces no nonempty live state. `--dry-run` performs identical staging and
validation without changing SQLite, checkpoint metadata, or files. This is the
complete migration-1 F007/F008 behavior, not an incomplete implementation of
the blocked forensic format. The pre-F017 grammar has no `--actor`,
`--restore-into-empty`, or `--merge`: import's only activation mode is replace
into an empty initialized store. Profile defaults to `native-v1`; publication
without `--output` permits only that profile, while an explicit new output path
may select an installed issue-only profile and returns its deterministic loss
report. Import uses a self-described installed issue profile or requires an
explicit matching `--profile`; ambiguity and mismatch fail closed. Flush writes
an operation-owned temporary sibling, verifies its hash/count, atomically
renames it, syncs the parent where supported, and only then updates
`checkpoint_state`. The pre-F017 activation transaction inserts all staged
issue state, then appends exactly one workspace-level `checkpoint_imported`
audit event (null issue ID) whose detail contains the profile, input SHA-256,
issue count, and `issues-jsonl-v1`; its event sequence is the activation
sequence. The target must be semantically empty before activation: no issues
and no prior semantic event other than initialization bookkeeping. On commit,
`checkpoint_state` records the input hash, the activation sequence as its
covered event sequence, and the transaction time. Thus immediate status has
`covered_sequence == live_sequence == activation_sequence` and is clean; any
later semantic event makes it dirty. The event attests activation of an
issue-only baseline, not source history, which this format cannot carry.
Import success reports profile, input hash, issue count, activation sequence,
covered sequence, live sequence, and `dirty: false`.

Dry-run performs the same empty-target check and validation, observes the
current live sequence, and reports the sequence activation would allocate as
`activation_sequence`, `covered_sequence`, and `live_sequence`, plus
`dirty: false`, `dry_run: true`, and `prospective: true`. Those sequence values
are advisory under concurrency; no event, checkpoint metadata, issue row, or
file is written.

The proposed forensic JSONL encoding is UTF-8 with one compact object and LF
per record. Its proposed monolithic representation uses exactly three
top-level record envelopes:

```json
{"record_type":"issue","issue":{}}
{"record_type":"event","event":{}}
{"record_type":"provenance_receipt","provenance_receipt":{}}
```

The envelope has exactly `record_type` and its matching payload key; the issue
or event public schema governs those payloads. Provenance receipts use
top-level `schema_ref`
`urn:bead-rs:schema:provenance-receipt:native-v1`, whose required fields are
`schema_ref`, `receipt_id`, `kind`, `source_store_uuid`, `target_store_uuid`,
`source_root_sha256`, `actor`, `created_at`, `counts`, `result`,
`summary_event_identity`, and `receipt_sha256`; the summary identity is null
for restore and the exact local event identity for merge. This represents
workspace events whose `issue_id` is absent without attaching them to a
synthetic issue.
Canonical monolith order is all issue records by issue ID ascending, followed
by all event records by `(origin_store_uuid, origin_event_sequence)` ascending,
then all provenance receipts by receipt ID ascending; UUID and receipt-ID
comparisons are bytewise UTF-8. The complete file contains one issue record per
issue, one event record per durable event, and one receipt record per durable
receipt. Blank lines may be ignored but do not count as records. An unknown
type, mismatched or extra envelope key, malformed or non-object line fails with
a one-based line number. Import rejects duplicate issue IDs, event identities,
or receipt IDs, origin-sequence gaps, and any declared per-type or total count
mismatch before activation.

Within an issue payload, known fields follow the interchange
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

Successful restore and merge provenance receipts are durable, nonsecret public
recovery facts and are part of that corpus. Dry-run reports, failed-operation
diagnostics, temporary staging data, busy/retry telemetry, absolute paths, and
environment details are nondurable and never exported.
External issue-only profile exports are distinct caller-selected artifacts and
must report omitted recovery records as profile loss; they are not complete
native backups.

#### 6.1.1 Adaptive sharded checkpoint set

Small workspaces use `.beads/checkpoint/forensic.jsonl` as the nonauthoritative
view of the pointer-selected forensic monolith. `.beads/issues.jsonl` remains
the separate issue-per-line interoperability checkpoint. Native forensic
defaults switch to a sharded checkpoint when the monolith would exceed 50,000
issue records, 64 MiB total, or 8 MiB for any individual record line; all
thresholds are versioned configuration and recorded in the manifest. Operators
may force monolithic or sharded output, but forcing a monolith never bypasses
record/byte safety limits.

All native checkpoints use `.beads/checkpoint/current.json` as the sole
authoritative discovery pointer. It canonically records a generation ID,
`monolithic` or `sharded` mode, store UUID, snapshot sequence, active-root path
and SHA-256, and a deterministic set of paths added, replaced, and deleted by
that generation, plus complete issue, event, provenance-receipt, and
total-record counts. In monolithic mode the active root is an immutable
content-addressed JSONL object under `.beads/checkpoint/objects/`, and
`.beads/checkpoint/forensic.jsonl` is a nonauthoritative byte-identical view.
In sharded mode the active root is an immutable manifest at
`.beads/checkpoint/manifests/<manifest-sha256>.json`; `current.json` gives the
separate generation ID and the canonical SHA-256 of the complete manifest
bytes, and the filename and recorded manifest digest must agree. No authoritative
manifest is ever overwritten or reused for different bytes. `manifest.json`,
if produced for legacy tooling, is only a nonauthoritative convenience view
and is never a discovery or recovery source unless a caller explicitly names
that exact file as standalone input. Files not selected by `current.json` are
inactive even if they remain after a crash. Import never chooses between roots
or views by existence or modification time.

The normalized checkpoint-set base is the directory containing
`current.json`, normally `.beads/checkpoint/`. Every relative reference in a
pointer or manifest is a slash-separated path relative to that same base—not
relative to the referring file's directory. Normalize before access; reject
absolute paths, empty or `.`/`..` components, backslashes, symlinks in any
component, and any resolved path outside the base. Thus a manifest under
`manifests/` references `objects/<sha256>.jsonl`, never `../objects/...`, and a
copied manifest cannot silently retarget sibling files. A standalone sharded
package is a newly created directory containing `current.json` plus every
referenced manifest/object at its base-relative path; it is written through an
operation-owned sibling directory and atomically renamed after closure/hash
verification. A bare manifest is valid only in its original checkpoint set;
portable single-file output remains the explicit standalone monolith. Import
applies the same base and traversal rules to both workspace and standalone
checkpoint-set directories.

The sharded manifest records format/schema version, store UUID, snapshot and
maximum local ingestion sequence, creation time, profile, complete issue/event/receipt counts, partition
algorithm and thresholds, and every referenced object path, byte length,
SHA-256, record range, and semantic role. The manifest itself has a canonical
SHA-256 reported by `sync --status`. Import rejects missing, extra-referenced,
duplicate, overlapping, mispartitioned, miscounted, or hash-mismatched data
before activating any state.

Every sharded JSONL line uses the same exact envelope grammar as section 6.1:
issue objects contain only `record_type` and `issue`, event objects only
`record_type` and `event`, and receipt objects only `record_type` and
`provenance_receipt`; each payload validates against the same immutable public
schema as its monolithic counterpart. Issue and receipt ordering remains bead
ID and receipt ID respectively. Across all event objects, canonical order is
bytewise `origin_store_uuid` ascending and then numeric
`origin_event_sequence` ascending. An event object's manifest range is the
inclusive composite pair
`[first_origin_store_uuid, first_origin_event_sequence]` through
`[last_origin_store_uuid, last_origin_event_sequence]`, never a local-ingestion
or timestamp range. The manifest also carries, for every represented origin,
its event count, minimum sequence (always 1), maximum sequence, and ordered
list of object/range intersections. Import verifies global composite ordering,
nonoverlap, exact per-origin continuity from 1 through the declared maximum,
payload hashes, and agreement of all per-origin and total counts.

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
contiguous composite-origin-range JSONL objects. Pack events in the canonical
origin/sequence order and seal an object at 100,000 events or 64 MiB. Because
all objects are content-addressed, a later flush writes a new tail object and
new immutable manifest rather than replacing any existing object; sealed
objects are reused byte-for-byte. This makes forensic history append-friendly and
prevents a frequently updated bead from rewriting its entire history-bearing
issue shard.

Sharded checkpoints likewise store provenance receipts in a separate
content-addressed JSONL object set declared by the manifest with receipt count,
schema reference, byte length, and SHA-256. Receipt objects sort by receipt ID
and participate in root verification and restore-equivalence exactly like issue
and event objects.

Object filenames contain their content SHA-256 and live under
`.beads/checkpoint/objects/`; identical content is reused. This includes the
complete native monolith object, so publishing never overwrites a root named by
the current generation. Before publishing a new root, preserve the old
generation pointer as `previous.json`. Publish each data root only after all of
its content is durable and verified, then atomically replace `current.json` as
the sole authority commit point. `previous.json` is itself an immutable-root
pointer copy replaced atomically and must continue to reference the immediately
preceding verified immutable root. In monolithic mode, update the
nonauthoritative `.beads/checkpoint/forensic.jsonl` view only after
`current.json` commits. A crash may therefore leave that view absent, stale, or
partially staged without making authority ambiguous; readers, import, doctor,
and repair resolve native state from `current.json`, never from the view.
`sync --status` reports view agreement separately and is not ready to commit
until the view is byte-identical to the pointer-selected object. A mode transition publishes a new
generation whose changed-path set includes the new root and objects, the
pointer replacement, and tombstones (deletion entries) for the formerly active
root and any objects referenced by neither the new generation nor the retained
previous generation root. Only after the pointer is durable may those tombstoned paths
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

`sync --flush-only --output PATH` is that standalone export operation. `PATH`
must be an explicit regular-file destination outside `.beads/checkpoint`; after
resolving existing ancestors and aliases it must not name the live database,
configuration, authoritative checkpoint root/pointer, an input path, or any
other workspace-managed file. The destination must not already exist. Export
captures one read snapshot, enforces monolith limits, writes and verifies an
operation-owned temporary sibling, then atomically renames it to `PATH` and
syncs the parent where supported. It reports computed issue, event,
provenance-receipt, and total-record counts. It does not replace `current.json`, change checkpoint mode/state
or freshness, emit tombstones, or mutate SQLite. Failure removes only the
operation-owned temporary. `--output` is invalid with a request for native
adaptive/sharded publication; an explicit output is always a standalone
monolith.

#### 6.1.2 Known implementation gap: dependencies and labels are not serialized

Discovered 2026-08-11 while verifying that a `bead`-tracked repository can
actually be reconstituted from a fresh `git clone` plus its committed
checkpoint alone (the scenario Section 6.1's "complete dependency, comment,
schema-bound data, and unknown-extension content" language and the final
gate's "the checkpoint covers every bead" language both already require).

**FIXED 2026-08-11** by commit `01f6ed8` (bead `test-ff97dce9`).

The original gap: `Issue` (`src/model.rs`) has no `dependencies` or `labels` field.
`publish_monolithic_checkpoint` (`src/service/checkpoint.rs`) wrote each
`CheckpointRecord::Issue` by cloning the live `Issue` struct directly, so a
flushed checkpoint issue record could never carry dependency or label data
regardless of what the live SQLite `dependencies`/`labels` tables contained.
This was a one-sided defect: the import path
(`stage_monolithic_checkpoint`) already inspected each parsed issue object for
optional `dependencies`/`labels` arrays and would restore them if present --
they were simply never written. Section 6.1's specified canonical ordering
("Dependencies sort by blocker ID, kind, then blocked ID") had no code path
that produced it.

The fix extended both `publish_monolithic_checkpoint` and `publish_sharded_checkpoint`
to query each issue's current dependencies and labels from SQLite and embed them
into the issue record in the Section 6.1 canonical order:
- Dependencies sorted by blocker ID, kind, then blocked ID
- Labels sorted lexically and unique

Implementation added:
- `read_all_dependencies()` and `read_all_labels()` helpers to query SQLite
- `IssueGraphData` struct to hold graph data for checkpoint serialization
- `build_enriched_issue_object()` helper to construct enriched JSON with deps/labels
- Golden round-trip regression test `test_round_trip_dependencies_and_labels`
  in `tests/cli_sync_import.rs` that creates issues with real dependency edges
  and labels, flushes, restores into a fresh workspace, and asserts the graph
  and labels are identical after restore

Live SQLite state was unaffected during the bug -- claim/dependency-graph behavior
against an open workspace was correct. Only `bead sync flush-only` output (and
therefore `bead sync import-only --restore-into-empty` against it) lost dependency
edges and labels. A workspace reconstituted purely from a committed checkpoint
previously had every issue but a flat, dependency-free graph -- confirmed by
cloning a real tracked repository (`game-of-life`, 15 issues, 24 `blocks` edges)
fresh and restoring from its checkpoint: all 15 issues returned, all 24 edges did
not. This is now fixed.

### 6.2 Backup flush algorithm

SQLite is authoritative between flushes because it supplies transactional live
operation. The pointer-selected immutable forensic monolith or sharded
manifest is the supported portable backup at the last successful flush and the source
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
4. Serialize new content-addressed issue/event/receipt objects or a complete
   content-addressed monolith object; reuse verified objects without rewriting
   them.
5. Flush and `sync_all` every new file, verify lengths/hashes/counts, then sync
   its parent directory where supported.
6. Publish and verify the immutable generation-named manifest when sharded,
   atomically preserve the old verified pointer as `previous.json`, then
   atomically replace `current.json` as the sole generation commit point and
   sync the parent directory. When monolithic, materialize and verify the
   nonauthoritative `.beads/checkpoint/forensic.jsonl` view only after that
   pointer commit.
   Apply only the pointer-declared tombstones afterward. Never expose an
   authoritative pointer referencing an incomplete root or object set.
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
mode, changed paths, authoritative-pointer/root verification, compatibility-view
agreement, and whether the checkpoint is ready to commit. A missing, stale, or
malformed forensic compatibility view never changes which generation is authoritative;
it makes status not ready to commit and is repairable by rematerializing it
from the verified pointer-selected immutable monolith. If the view changed but
the pointer did not, status identifies it as an uncommitted/noncanonical view,
and import ignores it unless the caller explicitly names it as standalone
input. Repository
automation must treat a dirty checkpoint as a failed pre-commit/release gate,
run `sync --flush-only`, and include every reported path in the same Git commit
as the related project change. This workflow preserves forensic material on the
remote history without making the bead CLI a Git client.

### 6.2.1 Automatic flush on mutation

Authorized by ADR-003. Every successful semantic mutation publishes a
checkpoint generation covering its own committed sequence, so the durable
checkpoint is never silently behind the database. `bead-rs` still never invokes
Git: automatic flush writes the working tree, and committing remains the
caller's responsibility.

This default **was gated**: it activated with plan revision 8, which flipped
the compiled default, advertised `auto_flush` in the capability document, and
reversed the never-implicit-flush documentation in the same commit. Section 13
records its gate evidence against that activation commit, and a failing
criterion reverts the compiled default. The prerequisites below remain
normative invariants rather than optional performance work; a full-workspace
flush per mutation writes bytes quadratic in mutation count into a
Git-tracked directory.

- **P1 — content-addressed roots.** The monolithic root object is named by its
  content SHA-256, as section 6.1.1 already requires, not by generation
  identity. Two flushes producing identical bytes reuse one object. The
  monolithic writer currently names the root from the generation ID, so
  identical content is never reused.
- **P2 — applied tombstones.** Step 6 of section 6.2 is implemented: after the
  pointer commits, pointer-declared tombstones are removed, `current.json` is
  never itself declared deleted, and the retained object set is bounded by the
  generations `current.json` and `previous.json` reference. Unresolved
  tombstones keep `sync --status` not ready to commit.
- **P3 — sound dirtiness signal.** Every committed semantic mutation appends an
  audit event, so the live event sequence advances for issue creation,
  dependency, label, external-reference, and structured-data mutations as well
  as lifecycle and claim transitions. Automatic flush is skipped only when the
  covered sequence already equals the live sequence, so an unrecorded mutation
  would otherwise publish nothing and report success.
- **P4 — incremental publication.** Sharded mode is reachable and selected from
  the recorded section 6.1.1 thresholds rather than hardcoded to monolithic, so
  a single mutation rewrites one issue shard, appends one sealed event tail
  object, and publishes a new manifest and pointer. Publication cost tracks the
  delta, not the workspace.

Once gated in, the contract is:

1. Automatic flush runs **after** the mutation's transaction commits, never
   inside it. A publication failure must not roll back committed work.
2. It runs from one shared post-commit chokepoint, not per call site, so every
   present and future mutating command inherits identical behavior. Read-only
   commands never publish.
3. It is skipped when the checkpoint already covers the live event sequence, so
   a no-op mutation publishes no generation and creates no object.
4. Publication is serialized by a checkpoint lock distinct from the SQLite
   write path. A concurrent worker that finds a newer generation already
   published for a sequence at or beyond its own treats that as success, not
   conflict. A lost race never leaves a torn pointer or a partially applied
   tombstone set.
5. A mutation that commits and then fails to publish reports the split
   explicitly: the mutation's own success output is preserved, the publication
   failure goes to stderr, and the process exits 1. Machine consumers must be
   able to distinguish "the mutation did not happen" from "the mutation
   happened and the checkpoint did not advance." Exit 1 never implies the
   mutation was rolled back.
6. Automatic flush is silent on success. `bead create` still prints only the
   new ID plus LF, and no other machine-output contract in section 5.1 gains a
   field or a line.
7. `--no-auto-flush` suppresses publication for one invocation and the
   workspace configuration key `checkpoint.auto_flush` suppresses it durably;
   the flag wins over configuration. Either leaves the checkpoint dirty exactly
   as today, and `sync --status` reports it.
8. `bead sync flush-only` remains a supported explicit operation and is
   idempotent under the automatic default: with a clean checkpoint it publishes
   no new generation and exits 0.
9. `sync --import-only` publishes on the same rule as any other mutation.
   Standalone `--output` export is unchanged and never satisfies or disturbs
   automatic publication state.

The capability document advertises the resolved behavior so a fleet can detect
it rather than infer it from version numbers, and section 3.5.10's rapid-fire
lifecycle benchmarks bound the acceptable per-mutation publication cost: the
automatic default may not regress them beyond the recorded budget.

Explicit-flush workspaces remain fully supported. Automatic flush changes which
behavior is the default, not which behaviors exist.

### 6.3 Import reconciliation

`sync --import-only` requires `--input PATH`, `--actor ACTOR`, and exactly one
explicit semantic mode: `--restore-into-empty` or `--merge`. `ACTOR` is required
for both real and dry-run restore/merge, must be nonblank after trimming, must
be at most 255 UTF-8 bytes, and must contain no control characters. Invalid or
missing actor input fails CLI validation before input staging and produces no
receipt or mutation. `PATH` names exactly one regular
monolithic JSONL file, sharded manifest, or native `current.json` pointer. A
pointer is followed only from that named path, and every referenced relative
path is resolved from the normalized checkpoint-set base defined in section
6.1.1 after rejecting symlinks and traversal. An explicitly named bare manifest
is accepted only at `<base>/manifests/NAME`; its base is the parent of that
literal `manifests` directory. A manifest or pointer input receives full hash and
declared-count validation. A pointerless legacy monolith is accepted only when
its file is named by `--input`; sibling files are never discovered or
heuristically ranked, and its issue, event, provenance-receipt, and total counts
are computed and reported rather than trusted from external metadata. Input is opened read-only,
must not alias the target database or an operation output, and is never renamed,
deleted, repaired, or rewritten. Parsing and staging cause no durable mutation;
only the single activation transaction described below may mutate SQLite, and
failed validation leaves the workspace byte-for-byte unchanged except for
removed operation-owned scratch. Default safety limits are 1
million issue records, 16 MiB per line, 4 GiB total, and `serde_json`'s bounded
nesting behavior. Event limits are independently configured and never inferred
from the issue-record limit.

Sharded import streams objects in manifest order, verifies each content hash
and partition membership, and rejects duplicate issue IDs or event identities
across shards. Validation of the entire manifest, graph, per-origin event
continuity, canonical composite ordering, and
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
event outcomes equal the checkpoint snapshot, imports any prior provenance
receipts, inserts one new immutable `restore` receipt, and activates the
result. The receipt is stored in `provenance_receipts` and exported by the next
native flush, but it has no origin event sequence and does not alter or create
a gap in the restored historical sequence. Its uniqueness key is
`(kind, target_store_uuid, source_root_hash)`. Any nonempty target, UUID
ambiguity, sequence gap, replay mismatch, or root-hash mismatch fails without
mutation.

Restore equivalence is defined over the canonical public forensic corpus, not
SQLite rows or checkpoint bookkeeping. Let `S` be the fully validated source
corpus (issues, events, and provenance receipts) in canonical order. Immediately
after restore, render the target through the same canonical corpus encoder and
remove exactly the newly inserted restore receipt identified by the operation's
returned receipt ID; no other receipt, event, field, or record may be excluded.
The remaining bytes and per-type counts must equal `S`, and recomputing the
corpus root over them must equal the validated source root. The full target
corpus must therefore equal `S` plus exactly that one restore receipt in normal
receipt sort order. Local ingestion sequences, `checkpoint_state`, pointers,
manifests, generation metadata, changed paths, staging paths, and file mtimes
are operational metadata outside this comparator; any public issue/event/
receipt payload is inside it. Failure of either comparison rolls back the
restore and its receipt.

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
  envelopes, then allocate the next local-origin sequence for one local
  import-summary audit event containing only counts, source identity/root hash,
  receipt ID, and reconciliation result, and insert the linked immutable
  `merge` receipt in `provenance_receipts` before commit.

The merge receipt uniqueness key is
`(kind, target_store_uuid, source_root_hash)` and its receipt hash covers all
public fields except the hash itself. Repeating an already committed identical
merge is an auditable idempotent success that returns the existing receipt and
does not ingest events, allocate a local sequence, update timestamps, or append
a second summary. A key collision with different receipt content is integrity
failure. Imported events receive local ingestion sequence only; they retain
their foreign origin identity. The local import-summary is the sole event
sequence effect attributable to a semantic merge, while the linked receipt
provides operation provenance without masquerading as source history. Receipt
insert, imported-event ingestion, reconciliation changes, and summary-event
append are atomic.

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
renderer. Both include `receipt_preview`, the same canonical projection of
`kind`, source and target store UUIDs, source root hash, validated actor,
counts, and result. A dry-run never assigns a durable receipt ID, creation
time, receipt hash, or summary-event identity; those materialized fields are
absent from its preview. A successful real import returns the durable `receipt`
and a `receipt_preview` exactly equal to that receipt with those four
materialized fields removed. Thus an immediate real run against unchanged
state must match the dry-run preview, while making no promise that generated
identity or time values can be predicted. Conflicts may return the preview of
the rejected outcome but never a durable receipt. A clean analysis exits 0. A reconciliation conflict exits 4, and
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

Authoritative publication and disaster-recovery restore are always
`native-v1`. `sync --flush-only` rejects every other profile, and
`--restore-into-empty` accepts only a verified native pointer/manifest or
native monolith. `needle-v1` defines subprocess behavior, not a recovery or
cross-tool checkpoint format. All other profile names fail closed.

Native pointers and manifests self-describe `native-v1`; their profile cannot
be overridden. Native envelope JSONL is self-describing as `native-v1` only
after the exact envelope and payload schemas validate. Missing or conflicting
identification fails closed. `--profile` cannot reinterpret an artifact or
bypass its declared schema. Native and NEEDLE dependency syntax remains
`dep add BLOCKED BLOCKER --kind blocks`.

Each emitted native bead includes its `schema_ref`. Profiles explicitly map,
preserve, or reject the reference according to their native/NEEDLE contract.
Supported public schemas use
immutable absolute identifiers and JSON Schema Draft 2020-12; the schema
document's `$id` equals the reference. See
`research/specs/schema-identification-v1.md`.

### 6.5 Native field guide and agent-guided rehydration

Per ADR-002, version 0.1 has no cross-tool migration command and accepts no
external checkpoint profile. `bead schema explain SCHEMA_REF --format
json|markdown` explains the native model to an agent without exposing or
authorizing native storage internals.

For every public issue field the guide records semantic meaning, structural
type, nullability, default, allowed values, stored/derived/read-only ownership,
owning CLI operations, cross-field invariants, a minimal valid example, and
common interpretation mistakes. It separately explains lifecycle transitions,
derived readiness and blocked presentation, dependency direction, revisions,
events, unknown extensions, and which values an agent must never synthesize.
The JSON document carries `schema_ref` equal to
`urn:bead-rs:schema:field-guide:native-v1`; the Markdown form is a deterministic
rendering of the same typed source. Completeness tests compare the guide with
the public native issue schema and fail on an undocumented, duplicated, or
stale field or lifecycle value.

An agent moving work from another tracker treats that repository as read-only
source material. It creates a fresh native workspace and reconstructs useful
work only through public `bead` commands. It emits a separate reconciliation
report containing the source repository and commit plus one disposition for
every source identifier: a target native ID or explicit `omitted`, `merged`, or
`unresolved` status with rationale. The report is for review and is never
accepted as checkpoint input. The required runbook rehearses the work in a
disposable workspace, compares counts and dependency intent, runs `doctor`,
flushes a native checkpoint, and preserves the original source artifact as an
archive. Agents never write SQLite or manufacture native checkpoint records.

## 7. Diagnostics and recovery

Before F017, `doctor` is read-only and checks:

- workspace/config parsing and permissions;
- database open, SQLite `quick_check`, foreign keys, schema version, and
  migration checksums;
- lifecycle and timestamp invariants;
- dangling or cyclic blocking edges;
- extension JSON validity;
- `.beads/issues.jsonl` parseability, canonical issue ordering/content, and
  agreement between its SHA-256 and the migration-1 `checkpoint_state` hash;
- `checkpoint_state.covered_event_sequence <=` the current live event
  sequence, reporting clean only when the hash agrees and the sequences are
  equal; a missing file/row, hash mismatch, or sequence lag is dirty, while a
  covered sequence ahead of live state is an integrity failure;
- orphaned temporary files owned by `bead-rs`;
- open issues carrying an assignee, which are excluded from the ready frontier
  without being an active claim, reported with the `update --clear-assignee`
  remedy and never cleared automatically (ADR-005, R035).

Warnings begin exactly `WARN `; healthy lines may use `OK `. Failed integrity
checks exit nonzero.

Before F017, `doctor --repair` diagnoses first and may remove a proven-stale
operation-owned temporary, create a missing safe index, or repair checkpoint
state only by running the normal atomic issue-only flush from a verified live
database. That flush republishes `.beads/issues.jsonl` and records its hash,
current live event sequence, and export time together; repair never blesses an
existing file by merely copying its hash or sequence into `checkpoint_state`,
and never edits JSONL in place. A missing file/row, stale hash, or sequence lag
therefore converges through one flush to a clean pair. A covered sequence ahead
of live state, malformed checkpoint, or failed database integrity check is not
automatically reconcilable and fails closed. Every repaired line begins
`FIXED `.

After the independently reviewed F017 specification and migration activate,
the pre-F017 file/row branch is replaced by checkpoint-set diagnosis. It always
starts at `current.json`, verifies its immutable
root and (when present) `previous.json`, and reports compatibility-view drift as
a separate nonauthority problem. Post-F017 repair may regenerate
`.beads/checkpoint/forensic.jsonl` from the verified pointer-selected monolith
or remove an
operation-owned interrupted view temporary; it never advances/rolls back the
pointer based on view contents, timestamps, or filenames. If the pointer/root
is invalid, repair fails closed and recommends explicit restore from a named,
verified immutable generation rather than promoting the compatibility view.
The installed store migration/layout selects the branch, never whichever files
happen to exist. F017's normative specification must define any one-time
reconciliation of migration-1 `checkpoint_state` into its new pointer state.

Version 0.1 never reconstructs issue rows, drops unknown tables, deletes the
database, rewrites a corrupt checkpoint, or alters lifecycle/dependency data
automatically. Diagnose those cases and recommend explicit manual recovery.

R036 makes that explicit recovery executable as `bead restore`: the operator
must name a retained immutable generation and actor; the command verifies the
pointer and complete content-addressed closure before target mutation, refuses
a non-empty target unless `--allow-non-empty` is explicit, and writes a restore
event and provenance receipt. Doctor remains diagnostic and never invokes it.
The normative contract is `research/specs/verified-restore-v1.md`.

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
    field_guide.rs    typed native semantics and deterministic renderers
  profile/
    mod.rs            native recovery and NEEDLE contract selection
    native_v1.rs
    needle_v1.rs
  output.rs           deterministic rendering
  error.rs            error taxonomy
  docs.rs             structured long help and manual supplements
man/man1/             reproducible generated section-1 manual pages
docs/adr/              indexed architecture decision records and template
docs/runbooks/         bootstrap, materialization, NEEDLE cutover, rollback
docs/traceability/     reviewed requirement-to-bead mapping and gate evidence
tests/
  cli/                isolated subprocess tests
  conformance/        normative lanes
  concurrency/        multiprocess tests
  stress/             deterministic rapid-fire lifecycle correctness harness
benches/
  lifecycle.rs        scale/concurrency benchmark driver and JSON reports
research/fixtures/    independent fixtures and manifests
```

Suggested dependencies, subject to MSRV verification (Rust 1.75 at
bootstrap; Rust 1.85 per ADR-004 as of 2026-08-15):

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

ADR-004 (2026-08-15) raises the MSRV to Rust 1.85 with edition 2024. The
1.75 references above record the bootstrap-era floor and its discharged F001
verification. `Cargo.toml` remains authoritative for the shipped floor until
the ADR-004 migration beads close, the CI MSRV lane must pin the same version
the manifest declares, and every forward-looking MSRV statement changes in
the same commit that flips the manifest. A future MSRV advance requires a
plan revision citing a new or revised ADR; the floor never moves silently.

## 9. Verification design

Every filesystem/subprocess test receives a new temporary workspace and HOME.
Never point tests at `/home/coding`, a contributor's real `.beads`, or another
implementation's database. Capture stdout, stderr, status, and filesystem
effects. Every fixture manifest records author, date, requirement, independent
creation method, and SHA-256.

Required layers:

1. Unit tests for validation, transitions, native/NEEDLE projection, deterministic
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

Transition verification additionally uses only the installed bootstrap
artifact and public CLI in disposable paths. It must prove:

- every active F-item and explicitly adopted R-item has exactly one disposition
  in the materialization mapping: bootstrap-complete, materialized, deferred,
  blocked, rejected, or superseded;
- executable mapping rows and generated native bead IDs are one-to-one, and
  every materialized bead has a stable source locator, bounded outcome,
  acceptance evidence, milestone, and complete dependency mapping;
- the generated graph is acyclic, its ready frontier matches the reviewed
  expected frontier, and export/import preserves the mapping and graph;
- a consumer-side NEEDLE canary negotiates `needle-v1` capabilities against the
  pinned installed `bead`, atomically claims one bead, records bounded evidence
  and a close reason, and cannot duplicate a claim under a second worker; and
- stopping NEEDLE and restoring the last verified checkpoint/configuration
  returns the workspace to the pre-canary state without asking Marathon and
  NEEDLE to mutate the same execution state concurrently.

## 10. Marathon execution order

Phases 0-4 and Gates G0-G4 are complete and retained below as bootstrap and
handoff history. By owner decision on 2026-08-08, the attempted early cutover
is superseded: Marathon resumes on `main` as execution authority for Phase 5,
Phase 6, and full-project completion. The historical handoff record remains
evidence but is no longer a stop condition. NEEDLE may be used only as a
consumer-side compatibility canary until `.marathon/COMPLETE` exists.

The earlier design treated Marathon only as a bootstrap mechanism and Gate G4
as a one-way transfer to the native bead workspace. The 2026-08-08 owner
decision supersedes that execution boundary because the cutover was premature.
For the resumed run, the source plan and Marathon ledger are authoritative;
native bead state is derived implementation evidence and must not compete with
or stop the Marathon loop.

Before the bootstrap implementation began, synchronize `.marathon/feature_list.json`,
`.marathon/instruction.md`, the Marathon runner/watcher, and documentation with
these gates. A prose-only phase change is invalid. `.marathon/COMPLETE` retains
its existing meaning of a fully verified version 0.1; use a distinct committed
handoff record for G4 containing a state (`pending` or `final`), the bootstrap
commit, artifact hash, checkpoint hash, mapping hash, NEEDLE configuration
revision, and UTC state-transition time.

### Phase 0: governance and specification readiness

- Freeze the bootstrap scope at F001-F011 and map every bootstrap requirement
  to its normative source and acceptance evidence.
- Establish `docs/adr/README.md` and an ADR template before accepting new
  architectural or delivery decisions.
- Record an owner and independent reviewer for the ADR-002 field guide and the
  missing F017 specification; an unowned required input is still blocked.
- Update Marathon control files once so they stop at the handoff rather than
  waiting for full 0.1 before allowing self-hosting.
- Define `docs/traceability/release-evidence-v1.schema.json` and a versioned,
  noninteractive verifier used by the release watcher. The report is generated
  from the reviewed mapping, native bead states/evidence, commit and artifact
  hashes, and gate results; it is not a second hand-edited status store.

**Gate G0 — governed bootstrap:** no orphan bootstrap requirement; all required
normative inputs exist; clean-room worker configuration is documented; the
ledger and mission agree with this phase model. Evidence is the reviewed
traceability table, accepted phase-boundary ADR, and synchronized control-file
diff. Accountable role: release owner; independent review: clean-room reviewer.

### Phase 1: native bootstrap core under Marathon

1. **F001:** package, config, connection policy, migration 1, idempotent init,
   future-version refusal.
2. **F002:** validated types, IDs, timestamps, transition matrix, extensions.
3. **F003:** create/list/show and stable machine output.
4. **F004:** readiness, transactional claim, empty result, 20-process test.
5. **F005:** atomic update/release, close/reopen, audit events.
6. **F006:** labels, canonical edges, cycles, derived blocking, graph tests.

**Gate G1 — trustworthy native core:** a disposable workspace passes the
isolated init, create, dependency, readiness, 20-way claim, release, close, and
reopen workflow plus formatting, Clippy, and tests. No external implementation
store is opened or changed. Accountable role: bootstrap implementer; review:
clean-room reviewer; evidence: feature ledger commands and committed handoff.

### Phase 2: durable NEEDLE-capable bootstrap MVP under Marathon

7. **F007:** single-snapshot deterministic, crash-safe issue-only flush.
8. **F008:** staged issue-only import, extension preservation, and rollback.
9. **F009:** read-only doctor and narrow repair allowlist.
10. **F010:** provisional bootstrap capability document.
11. **F011:** full `needle-v1` provider subprocess matrix in isolated workspaces.
12. **Bootstrap packaging gate:** build and install the exact artifact into a
    temporary Cargo root, verify minimal complete recursive help for bootstrap
    commands, and record its SHA-256. This is not F014 final packaging and does
    not mark F016 complete.

**Gate G2 — self-hosting candidate:** the pinned installed artifact passes
issue-only flush/import recovery, doctor, capability negotiation, the provider
suite, a consumer-side NEEDLE test, package-content/provenance checks, and the
G1 workflow when invoked from the installed path. The F012/F013 replacements,
F015, F016,
F017, and F014 remain incomplete. Accountable role: release owner; review:
NEEDLE adapter owner and clean-room reviewer; evidence: artifact hash,
installation transcript, provider/consumer results, and checkpoint hash.

### Phase 3: materialize the remaining plan as native beads

Use only the pinned G2 `bead` binary, its public CLI, this plan, accepted ADRs,
normative specifications, the reviewed ideas-ledger dispositions, and the
Marathon ledger. Create a new disposable native workspace; never write another
tool's database and never convert directly into an active workspace.

This is a controlled translation of already reviewed work, not autonomous
LLM-generated decomposition. The ideas ledger rejected provider-coupled,
nondeterministic decomposition, and that decision remains in force. Likewise,
do not require predicted file read/write manifests: file-intent gating remains
deferred because discovery cannot reliably identify every eventual edit.

For every remaining F-item and every adopted R-item, record exactly one
disposition row with:

```text
milestone / outcome / normative source and locator / F-or-R ID / native bead ID
/ bounded deliverable / dependencies / priority / accountable role or worker class
/ code or artifact surface / acceptance command or evidence / gate / risks / ADRs
/ disposition and rationale
```

Create a native bead only for independently executable remaining work. An
R-item marked core-incorporated maps to its owning F-bead and is recorded as
covered rather than duplicated. Rejected, superseded, pending, and deferred
ideas receive explicit non-executable dispositions; translation does not
silently promote them.

External readiness is represented in the graph, not entrusted to prose. Create
separate prerequisite acceptance beads for the independently reviewed native
field-guide specification and `checkpoint-set-v1.md`, each satisfied only by
the accepted artifact hash and review evidence. Their implementation beads
remain `deferred` until a reviewer closes the prerequisite and performs the
recorded `deferred`-to-`open` activation. Dependent release work remains
transitively blocked. G3 asserts that no externally blocked bead appears in the
ready frontier.

Create beads first, record their generated IDs, then add dependencies using
canonical direction. Because the bootstrap has no atomic bulk-manifest feature,
any interrupted or invalid materialization discards the disposable workspace
and starts again; it never repairs a half-activated graph in place. Use
`description`, notes, labels, and dependencies without inventing unsupported
schema. Do not copy the entire plan into each bead: store concise executable
context and stable repository-relative source locators. A reviewer must resolve
every omission, duplicate, ambiguous dependency, rejected item, and superseded
requirement before activation.

**Gate G3 — reconciled execution graph:** every source item has exactly one
disposition, and executable mapping rows form a bijection with native bead IDs;
every bead is queryable; the graph is acyclic; the ready frontier equals the
reviewed expected frontier and excludes deferred external/profile/F017 and
post-0.1 roadmap work; round-trip export/import preserves IDs, fields, unknown
extensions, and edges;
`doctor` passes; and a deterministic checkpoint plus mapping hash is committed.
This proves representation only. Accountable role: materialization operator;
review: release owner and independent plan reviewer.

### Phase 4: Marathon-to-NEEDLE authority handoff

The preferred steady state is a native NEEDLE `bead` backend that discovers the
explicit provider, negotiates `bead capabilities --format json --profile
needle-v1`, and fails closed if a mandatory capability is absent. An explicitly
enabled compatibility alias may be used for a disposable pre-cutover canary,
but it is not the steady-state integration and must not silently impersonate a
different native tool.

Pin NEEDLE to the G2 artifact hash and canonical G3 workspace. Transfer every
clean-room restriction from `AGENTS.md` and the Marathon mission into worker
configuration: no prohibited repositories, CASS, inherited session history,
global memory, real external databases, or non-independent fixtures. Start one
worker against a disposable copy. After it succeeds, fence and stop Marathon,
verify it cannot restart or mutate work, and commit the handoff record in
`pending` state. The pending commit transfers provisional authority to the
native workspace before the first canonical mutation. Run one canonical
worker, then increase only to the concurrency demonstrated by the bootstrap
tests. On success, update and commit the record as `final`.

**Gate G4 — authority transferred:** the canary claims exactly one ready bead,
receives bounded source context, performs only permitted work, records concrete
verification and a close reason, flushes a verified checkpoint, and does not
duplicate a claim when a second worker competes. Before writing
the pending handoff record, stop Marathon and verify no Marathon process can
mutate execution state. From the pending record onward, native beads are the
only work-state authority; G4 completes only when the canonical canary evidence
and final handoff record are committed.

Before the pending record, rollback leaves Marathon authoritative. At or after
the pending record, rollback stops all NEEDLE workers, preserves diagnostics,
restores the last verified native checkpoint/configuration in a new empty
workspace, and either retries under provisional native authority or explicitly
returns authority to the frozen Marathon snapshot through a reviewed ADR and
committed reversal record. Never run Marathon and NEEDLE as concurrent writers
or merge their divergent work-state claims.

### Phase 5: complete version 0.1 under Marathon

1. **F012 replacement:** remove external profile adapters and compatibility
   claims under ADR-002; retain only native recovery and NEEDLE conformance.
2. **F013 replacement:** implement the complete native field guide, JSON and
   Markdown renderers, schema/guide completeness tests, and agent-guided
   rehydration runbook.
3. **F015:** deterministic lifecycle stress harness, fast matrix, full-scale
   benchmark driver, capacity calculation, and schema-stable reports.
4. **F016:** complete help tree, generated man pages, drift/coverage tests, and
   documented explicit installation.
5. **F017 (specification-blocked):** only after the independently reviewed
   normative checkpoint-set specification exists, implement the distinct
   forensic checkpoint set, adaptive deterministic sharding, complete event
   history, semantic restore equivalence, and Git-trackable verification.
6. **F014:** final package/install smoke test, licensing, provenance, and full
   0.1 release verification.

F017 changes checkpoint and capability surfaces, so it invalidates the
provisional F010/G2 capability evidence. Re-run capability, import/export,
doctor, NEEDLE provider/consumer, packaging, and traceability gates against the
final artifact through a distinct final-capability verification bead that
depends on F017 and blocks F014. Its acceptance evidence includes the final
artifact/spec hashes; the release-evidence verifier rejects provisional or
hash-stale F010 evidence. A large feature must expose reviewed intermediate evidence
gates even while its ledger `passes` value remains false; F017 at minimum uses
specification, migration, monolith, sharding, restore, merge, and conformance
sub-gates.

**Gate G5 — version 0.1:** every F001-F017 feature has concrete passing
evidence, every traceability row is satisfied or explicitly out of scope, and
no post-0.1 R-extension bead is ready. All section 13 final release gates pass.
G5 permits the version 0.1 milestone but not full-project
`.marathon/COMPLETE`, which additionally requires Phase 6.

### Phase 6: complete the adopted post-0.1 roadmap under Marathon

R001-R026 become executable after G5 in their listed order subject to explicit
dependencies and required specifications/ADRs. R026 is further gated on its own
section 13 activation gate and sequences after the `checkpoint-set-v1`
acceptance that blocks F017. Core-incorporated items receive
verified dispositions tied to their owning F-feature rather than duplicate
implementations. Marathon records each roadmap item in the release ledger,
implements every remaining extension, and continues until full-project gates
permit `.marathon/COMPLETE`.

### Decision records and change governance

Create an ADR for every decision that changes architecture, persistent or
public schema, compatibility profile, checkpoint semantics, security or
recovery boundary, MVP/release scope, or execution-authority transition. ADRs
are explanatory; they cannot override normative specifications. Each ADR has a
stable number and title, status (`proposed`, `accepted`, `superseded`, or
`rejected`), date, owner and deciders, context, considered options, decision,
consequences, evidence, revisit trigger, and links to requirements, features,
beads, and superseding ADRs.

Seed ADRs for SQLite live authority with JSONL recovery; the staged native
NEEDLE backend; the bootstrap/full-0.1 boundary; Marathon-to-native-beads-to-
NEEDLE authority transfer; ADR-002 agent-guided rehydration; ADR-003 automatic
checkpoint flush gated on incremental publication; and F017 placement.
Record rejected alternatives rather than silently reopening them, including
native SQLite backup, mandatory file-intent gates, live upstream database
adapters, automatic Git publication, daemon/network authority, and arbitrary
LLM-generated decomposition. Progress logs link ADRs but never substitute for
them.

This plan carries a revision and as-of date. A change that alters a gate,
requirement disposition, phase, profile claim, or authority source must update
the traceability mapping, affected ADR/spec/ledger links, plan revision, and a
short change note in one coherent review. Unknown or contradictory state fails
closed; no implementer resolves drift by choosing whichever artifact is most
convenient.

### Transition risk and responsibility register

| Risk | Prevention and detection | Trigger and response/rollback | Accountable role / review evidence |
| --- | --- | --- | --- |
| Clean-room contamination | Isolated worker config, permitted-source manifest, provenance review | Any prohibited exposure stops the affected component and records it in `PROVENANCE.md` before independent reassessment | Clean-room reviewer / signed G0 and G4 review |
| Omitted, duplicated, or invented materialized work | One disposition per source item, executable-row/bead-ID bijection, independent reconciliation | Count/hash mismatch or orphan row discards the disposable workspace and reruns materialization | Materialization operator + plan reviewer / G3 mapping |
| Reversed or cyclic dependencies | Canonical direction review, cycle rejection, expected ready-frontier fixture | Cycle or frontier mismatch blocks activation; correct source mapping and rebuild fresh | Plan reviewer / G3 graph report |
| Partial conversion or unknown-field loss | Fresh destination, public CLI only, checkpoint round trip and semantic comparison | Interrupted conversion or mismatch discards destination; never patch partial activation in place | Materialization operator / G3 checkpoint evidence |
| Dual Marathon/NEEDLE authority | Explicit stop, process check, one committed handoff record | Any concurrent writer stops cutover; restore pre-cutover workspace and reconcile through ADR | Release owner / G4 handoff record |
| NEEDLE capability mismatch or duplicate claim | Fail-closed handshake, provider/consumer suites, one-worker canary then bounded concurrency | Missing capability, duplicate ID, or state drift stops workers and restores last verified checkpoint | NEEDLE adapter owner / G2 and G4 results |
| Worker scope or context escape | Bounded source locators and inherited clean-room restrictions | Unauthorized source/tool access stops worker and invokes contamination procedure | Worker operator + clean-room reviewer / worker config and audit |
| Missing field-guide or F017 specification | Named author, independent approver, explicit blocked state | Missing approval keeps dependent beads blocked; never implement from plan prose | Specification owner + approver / accepted artifact hashes |
| Oversized F017 hides progress or failure | Reviewed sub-gates with independent evidence and capability invalidation | Any failed sub-gate leaves F017 false and rolls back only its bounded migration/activation step | F017 owner + recovery reviewer / sub-gate evidence |
| Packaged binary differs from tested binary | Pin commit and artifact hash; run tests from installed path | Hash mismatch invalidates all canary evidence and returns to G2 packaging | Release owner / artifact manifest |
| Checkpoint or cutover recovery fails | Verified pre-canary checkpoint and rehearsed empty-target restore | Failed restore blocks handoff/release and preserves source workspace for diagnosis | Recovery owner / restore-equivalence report |
| MSRV or dependency drift | Lockfile, pinned MSRV lane (1.85 per ADR-004; 1.75 at bootstrap), dependency verification at F001 and final package | MSRV failure blocks artifact promotion; choose compatible dependency through ADR if architectural | Build owner / G2 and G5 package evidence |

Roles may be held by agents or humans, but the accountable role and independent
review evidence must be named before its gate can pass. Dates are optional;
resolution owners and observable gate events are mandatory.

## 11. Capability document

Version 0.1 assigns these exact immutable public schema identities:

| Document | Schema identity |
| --- | --- |
| native issue representation | `urn:bead-rs:schema:issue:native-v1` |
| native audit event | `urn:bead-rs:schema:event:native-v1` |
| capability document | `urn:bead-rs:schema:capabilities:native-v1` |
| native field guide | `urn:bead-rs:schema:field-guide:native-v1` |

Every instance of one of these documents carries top-level `schema_ref` equal
to the identity above. Every schema document returned by `schema show` is JSON
Schema Draft 2020-12 and carries `$schema` equal to
`https://json-schema.org/draft/2020-12/schema` and `$id` equal to that same
immutable identity. Published schema contents are immutable; an incompatible
change receives a new identity. Proposed checkpoint pointer, manifest, shard,
and provenance-receipt schemas are deliberately absent from this catalog until
the normative F017 specification assigns them.

Before F017, `bead capabilities --format json --profile needle-v1` returns at
least the following **provisional pre-F017** example:

```json
{
  "contract": "needle-v1",
  "implementation": "bead-rs",
  "version": "0.1.0",
  "store_layout": 1,
  "atomic_claim": true,
  "priorities": {"min": 0, "max": 4, "default": 2, "p4_claimable_by_fifo": true},
  "statuses": ["blocked", "closed", "deferred", "in_progress", "open"],
  "checkpoint_modes": ["flush-only", "import-only"],
  "checkpoint_formats": ["issues-jsonl-v1"],
  "schema_ref": "urn:bead-rs:schema:capabilities:native-v1",
  "schemas": [
    {"schema_ref":"urn:bead-rs:schema:event:native-v1","document_kind":"audit_event","validate":true,"consume":[],"emit":[]},
    {"schema_ref":"urn:bead-rs:schema:issue:native-v1","document_kind":"issue","validate":true,"consume":["sync.import-only"],"emit":["sync.flush-only"]},
    {"schema_ref":"urn:bead-rs:schema:field-guide:native-v1","document_kind":"field_guide","validate":true,"consume":[],"emit":["schema.explain"]}
  ],
  "commands": ["capabilities", "claim", "close", "create", "dep", "doctor", "init", "label", "list", "release", "reopen", "schema", "show", "sync", "update"]
}
```

This example is not the final 0.1 capability document. The normative
`checkpoint-set-v1` specification must assign the forensic pointer, manifest,
shard, and provenance-receipt schema identities and the exact checkpoint format
names. F017 must then extend `checkpoint_modes`, `checkpoint_formats`, and the
schema catalog to advertise every format and immutable public schema that the
final implementation actually validates, consumes, or emits, following the exact
normative names and support semantics. F010 tests this provisional document;
the final F017/release tests replace the expected catalog with the normative
one and fail if the shipped checkpoint grammar or schema resolver is omitted.

R026 adds one additive boolean, `auto_flush`, reporting whether this binary
publishes a checkpoint generation after every successful semantic mutation
(section 6.2.1). It was absent until the R026 activation flipped the compiled
default and has been present and `true` since (plan revision 8), so a fleet
detects the behavior by handshake rather than inferring it from a version
number. The field reports the compiled default; a workspace that
disables publication through `checkpoint.auto_flush`, and an invocation that
passes `--no-auto-flush`, do not change what the binary advertises. Consumers
that require a current checkpoint must still read `sync --status`, which
remains the only authority on whether this workspace is actually clean.

Schema catalog entries sort lexically by exact identity and are duplicate-free.
`validate` means `schema show` supplies the schema and the implementation can
validate an instance; it does not claim that a public command consumes that
document. `consume` and `emit` are lexical, duplicate-free lists of concrete
public operation paths that respectively accept or produce that document;
empty lists are meaningful. Before F017, audit events are internal durable rows:
their public schema is resolvable and usable for validation, but no sync command
consumes or emits event documents. The field guide is emitted by `schema
explain` but is not accepted as store input. Issue documents are consumed and
emitted only by the issue-only sync operations shown. Lossy support uses an
additive entry naming the operation, direction, and explicit loss reason and
does not appear in the lossless `consume` or `emit` list. The capabilities
document's own schema is identified by its `schema_ref` rather than recursively
listing itself. `schema list --format json` returns the same catalog entries,
including identity, document kind, validation and operational support, and
profile provenance;
`schema show` resolves every listed identity byte-for-byte to its immutable
schema document. Migration and provenance receipt outputs retain their
`schema_ref`, so their producer/profile provenance is discoverable without
inferring it from filenames.

All other arrays are lexically stable. `commands` is the lexical, duplicate-free set of
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
native issue records, capabilities, and the native field guide, per-bead
`schema_ref`, schema resolution, and capability enumeration. R005 is
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
and consumers negotiate only an exact mutual identifier and report read-only
support explicitly. Do not infer compatibility from similar names or schema
structure.

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

### R025 — Declared verification edges and inverted-gate diagnosis

Add a third dependency kind, `verifies`. A `verifies` edge from V to I records
that V checks the work I performs; like `relates_to` it never affects
readiness, and cycles among `verifies` edges alone are permitted. Unknown kinds
stay preservable for interchange and fail closed for native mutation, as R018
requires of unknown schemas.

With the relationship declared, an inverted verification gate becomes decidable
rather than guessed: a `blocks` edge whose blocker also `verifies` its blocked
bead states that the check must close before the work it checks may start,
which no execution can satisfy. Report each such pair from the existing
dependencies doctor scope that already performs cycle detection. Cycle
rejection does not cover this case — an inverted gate is normally acyclic, so
§3.4's insertion-time check accepts it and readiness then correctly reports a
bead that can never become ready.

The diagnosis is advisory and never rejects the edge, matching R021: a
deliberate "prove the baseline green before touching it" ordering is
legitimate and structurally identical to the error, so only the author can
distinguish them. Never infer the relationship from issue titles. Title shape
is not a stable cross-tool signal, and prefix matching misclassifies titles
that merely contain a verification noun; the declared edge exists precisely so
that no such inference is needed. R023 reports a `verifies`-shaped blocker as a
distinct reason code rather than an ordinary blocker, since "blocked by the
bead that verifies it" is the answer that identifies the fault. Sequence after
R017, whose conditional edges must treat `verifies` consistently in cycle
analysis. Authorized by ADR-001.

### R026 — Automatic checkpoint flush on mutation (extension)

Make a successful semantic mutation publish a checkpoint generation covering
its own committed sequence, replacing explicit `sync flush-only` as the
default. The normative contract is section 6.2.1; `bead-rs` still never invokes
Git, and automatic Git publication remains rejected.

The item is gated on four prerequisites, each of which is the implementation
converging on section 6.1.1 and 6.2 as already written rather than new scope:
content-addressed monolithic roots (P1), applied pointer-declared tombstones
(P2), an audit event for every committed semantic mutation so the event
sequence is a sound dirtiness signal (P3), and reachable sharded publication so
flush cost tracks the delta rather than the workspace (P4). Sequence P1-P4
after the normative `research/specs/checkpoint-set-v1.md` acceptance that
already blocks F017, since all three of P1, P2, and P4 are constrained by it.

Without those prerequisites the feature is actively harmful rather than merely
slow: publication is linear in workspace size, each generation leaks a
full-size immutable object that is never collected, and most mutation kinds do
not advance the sequence that would tell the implementation a flush was needed.
Per-mutation publication multiplies all three by the mutation rate, inside a
directory whose contents are Git-tracked.

Then deliver the automatic path itself: one shared post-commit publication
chokepoint, defined split-failure semantics with the mutation preserved and
exit 1, a checkpoint publication lock separate from the SQLite write path,
`--no-auto-flush` and `checkpoint.auto_flush`, an `auto_flush` capability
field, and the documentation reversal across README, root help, generated man
pages, and `AGENTS.md`. Authorized by ADR-003.

### Run-4 tranche (adopted 2026-08-15)

R027-R034 were adopted from ideation run 4 after `.marathon/COMPLETE` was
recorded. They do not retroactively alter the recorded R001-R026 full-project
gates. Each follows this section's extension rules — its own normative
specification, conformance scenarios, migrations where applicable, and ledger
entries — before implementation evidence can be accepted. Tracking beads
`beadrs-de075bba` through `beadrs-90c9afc9` under genesis `beadrs-d6f98dab`
record per-item acceptance criteria.

### R027 — Remote-advanced checkpoint reconcile (extension)

Add `bead sync reconcile`, which recognizes the state where the committed,
pointer-verified checkpoint is ahead of the live database — the normal result
of pulling another machine's flush in the Git-transported workflow — and
merges that checkpoint into the live store through the existing `--merge`
machinery. The specification must define the state taxonomy precisely: only a
verified pointer whose event stream is a superset of live state qualifies as
remote-advanced; every other covered-ahead-of-live case remains a fail-closed
integrity failure exactly as today, and doctor reports the distinction rather
than a blanket failure. `bead-rs` still never runs Git.

### R028 — Fork identity for cloned workspaces (extension)

Add `bead sync fork`, an explicit operator command that re-origins a cloned
workspace under a new store UUID recorded in a provenance-chained receipt.
Clones of one repository currently share a store UUID, so independently
advanced histories are rejected as same-UUID divergence with no
reconciliation path; forked clones are distinct origins whose event streams
merge composably under the existing different-UUID rules. Doctor detects
same-UUID divergence and names the fork/reconcile remedy. Forking is never
implicit or inferred.

### R029 — Checkpoint archaeology (extension)

One verified read-only loader materializes a named checkpoint artifact
(pointer, manifest, or monolith) into an ephemeral view serving three
operations: query against a historical generation without import, a semantic
issue/event-level diff between two generations, and predicate-driven search
across a generation series. Views are never accepted as import input and
archaeology outputs are explicitly non-importable, so a partial view can
never be mistaken for a recovery source. Sequenced with R026, whose
per-mutation generations make committed history a queryable timeline.

### R030 — Self-defending workspace discovery

Workspace discovery stops at the first `.beads` directory encountered on the
upward walk and fails closed with a precise diagnostic when that directory
lacks the bead-rs workspace fingerprint, instead of continuing past a foreign
store to an unrelated parent workspace. The diagnostic claims only that the
directory is not a bead-rs workspace; identifying a foreign format remains
prohibited by the clean-room boundary. An explicit override permits
legitimate nesting. This corrects section 4.1's discovery rule and may be
scheduled ahead of the larger extensions.

Field evidence strengthens the case for scheduling it early: on 2026-08-14 a
workspace was structurally damaged because the operator could not tell which
tool owned a store, applied a foreign recovery procedure to it, and silently
reinitialized it with the wrong schema; the data survived only because that
workspace's checkpoint happened to be current. A fail-closed diagnostic naming
the directory as not-a-bead-rs-workspace is the intervention that stops this
class of loss, and it remains sayable without identifying the foreign format.

### R031 — Atomic resource locks (extension)

Issues may declare normalized local resource keys. A claim atomically
acquires every key its bead declares and excludes ready issues that need a
key currently held; release, close, and lease expiry return keys. Readiness
explanations gain a resource-conflict reason code. Scope is strictly one
native store: this is workspace-local scheduling exclusion, never distributed
locking, and naming and documentation must say so. Adopted from the run-1
deferral after NEEDLE ADR-015 made bead-level serialization the accepted
shared-checkout model.

### R032 — Idempotent create by unique reference (extension)

`create --unique-ref NAMESPACE:KEY` binds an R011 external reference inside
the insert transaction; when the reference is already bound, the command
returns the existing bead's identity instead of creating a duplicate. The
specification must define the closed-bead case explicitly — a distinct output
or a flagged conflict — so automation cannot loop on finished work. This
prevents duplicate beads at the source when dispatchers materialize work from
the same external identifier concurrently.

### R033 — Atomic bulk transaction manifests (extension)

Validate a versioned JSON manifest composed strictly of existing command
primitives — creates, updates, labels, dependencies, closes — with local
references for newly created IDs; show a dry-run diff; then commit all
operations in one transaction that publishes at most one checkpoint
generation under R026. Version 1 must refuse any semantics a single existing
command does not already have. Adopted from the run-1 deferral: per-mutation
publication makes an N-command materialization publish N generations, and an
interrupted materialization still discards an entire disposable workspace.

### R034 — Stale in-progress detection

Add a doctor scope that reports non-leased `in_progress` beads whose most
recent event is older than a configured interval, together with the exact
release remedy. Advisory only: doctor never releases work itself. This is the
immediately executable subset of R019's starvation diagnostics for the
non-leased majority of claims and must share reason codes with R001/R019
rather than inventing parallel semantics.

### R035 — Assignment-held readiness diagnosis

Report open beads that carry an assignee, which are excluded from the ready
frontier despite not being an active claim, together with the exact
`update --clear-assignee` remedy. Advisory only: doctor never clears an
assignee, because a parked reservation and abandoned residue are
indistinguishable at the schema level and only the operator knows which is
which. This is R034's sibling for the `open` lifecycle state; the two must
converge on shared R001 reason codes rather than each carrying prose. The
initial check shipped ahead of that convergence and currently emits prose, so
reason codes, a machine-readable id list, and any means of declaring an
intentionally-held assignment remain outstanding. Accepted in ADR-005; field
evidence 2026-08-16 found 583 such beads across 47 workspaces, ten of them with
an entirely empty ready frontier while workers starved and every diagnostic
reported clean.

### R036 — First-class verified restore (extension)

Turn the explicit restore that section 7 already recommends into one verified
command rather than an operator-reconstructed recipe: select and verify a named
immutable generation, refuse a non-empty target unless explicitly overridden,
attribute the actor, and report exactly what was restored. Restore stays
explicit and never automatic — doctor continues to recommend and never perform
it — so this changes only whether the recommended path is executable. The
specification must state plainly how it relates to `sync import-only`, and must
refuse R029 archaeology views as input, preserving their non-importability.
Adopted because the recovery path currently survives as lore: an operator under
pressure reaching for a remembered recipe is how a foreign tool's recovery
steps were applied to a native store on 2026-08-14, silently reinitializing it
with the wrong schema. Accepted in ADR-006.

Implemented by `bead restore` and the verified-restore-v1 conformance suite.
`sync import-only` remains public as the lower-level interchange/merge and
compatibility path; it is not equivalent to named verified recovery and is no
longer the path doctor recommends. R029 archaeology artifacts remain explicitly
non-importable.

### R037 — Command errors that name the remedy

Where a command rejects an argument a competent operator would expect to work,
state the domain rule and the remedy instead of surfacing a bare parser error.
Covers at minimum fields immutable after `create` (title, description,
priority, issue-type) and near-miss flag spellings on lifecycle commands,
notably `close --body` against `--reason`. No currently-rejected invocation
becomes accepted and no semantics change; this generalizes the standard
`release` already sets when it refuses an assigned open issue by naming
`update --clear-assignee`. Conformance scenarios must assert the remedy text so
the affordance cannot regress to a parser default. Accepted in ADR-007.

### R038 — Secret rejection and audited historical redaction

Reject high-confidence provider-formatted credentials before any
operator-supplied text mutation commits, report lower-confidence findings
without values, and expose one exceptional fingerprint-selected redaction
operation for sensitive bytes already stored by older versions. Redaction
preserves semantic identity, emits a nonsecret receipt, publishes a sanitized
generation set that retains no dirty previous root, and prevents a known
finding from being resurrected by import, merge, reconcile, or restore.

R038 is not a general edit/delete command and does not make audit events
mutable for ordinary purposes. ADR-014 governs prevention; ADR-015 and the
`historical-redaction-v1` specification govern destructive repair. BR-T13
through BR-T18 are the current delivery ledger. The requirement was promoted
from the deferred sensitive-content lint after a real NEEDLE checkpoint push
was rejected on 2026-09-03. Both normative specifications were independently
accepted at their exact submitted hashes on 2026-09-03. BR-T14 through BR-T17
are now implemented on `main`. Ruleset v3 adds a narrowly context-bound Garage
access-key-ID assignment detector after the motivating remediation proved that
credential identifiers can persist after a secret value is removed. BR-T18
remains open until the exact packaged artifact, full Rust gates, and NEEDLE
remediation evidence all pass together.

## 13. Release gates

### Bootstrap and handoff gates

G2 may promote a bootstrap artifact only when:

- F001-F011 have concrete passing evidence at the same commit;
- formatting, Clippy, tests, isolated package installation, minimum recursive
  help coverage, and provenance/package-content checks pass;
- the pre-F017 issue-only checkpoint survives crash-safe flush, empty-target
  import, semantic comparison, and `doctor` verification;
- the installed binary passes the complete provider-side `needle-v1` suite and
  a consumer-side NEEDLE canary using a pinned capability handshake; and
- artifact, commit, test, and checkpoint hashes are recorded together.

G4 may transfer execution authority only when:

- the G3 mapping has no missing, duplicate, invented, or unresolved active
  requirement and its dependency graph and expected ready frontier are verified;
- the field-guide prerequisite, F017, their transitive dependents, and all post-0.1 R-extension beads
  are absent from that frontier until their explicit activation conditions pass;
- the canonical native workspace was created only with the pinned public
  `bead` CLI and has a verified recovery checkpoint;
- NEEDLE's native `bead` backend and clean-room worker configuration are pinned
  and reviewed;
- a disposable canary passes; Marathon is then stopped/fenced and a pending
  handoff transfers provisional authority before the canonical one-worker
  canary or duplicate-claim competition mutates native state; and
- rollback is rehearsed and the final committed handoff record names the sole
  work-state authority.

Neither gate creates `.marathon/COMPLETE`, calls the artifact version 0.1, or
claims cross-tool checkpoint compatibility.

### Final version 0.1 gates

Before declaring version 0.1 complete:

- F001-F017 have concrete passing evidence;
- the schema-valid generated release-evidence report maps every F-item to
  satisfied native evidence at the final commit/artifact hash, passes the
  versioned noninteractive verifier, and is the sole machine-readable completion
  input consumed by the post-handoff release watcher;
- a final-capability verification bead completed after F017 and blocks F014
  until all capability, checkpoint, doctor, provider/consumer, and package
  evidence is fresh for the final artifact;
- the independently reviewed normative
  `research/specs/checkpoint-set-v1.md` exists before any F017 implementation
  evidence is accepted; plan prose alone is not implementation authority;
- the ADR-002 field guide describes every public native issue field and
  lifecycle value exactly once, and its JSON and Markdown renderings pass
  deterministic completeness tests;
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo test` pass on the release commit;
- native, field-guide, rehydration-runbook, NEEDLE, and concurrency lanes pass;
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

### R026 automatic-flush activation gate

The automatic flush default of section 6.2.1 may ship only when all of the
following hold at one commit. Until then the binary keeps the explicit-flush
default and the documentation that describes it.

- **Bounded objects.** A scripted run of at least 1,000 mutate-and-publish
  cycles leaves the retained object set bounded by the generations
  `current.json` and `previous.json` reference. Checkpoint bytes are within a
  recorded constant factor of the live workspace and do not grow with the
  number of publications. Two publications of identical content produce one
  object, verified by count.
- **Applied tombstones.** After that run every path declared in
  `deleted_paths` is absent from disk, `current.json` never declares itself
  deleted, and `sync --status` reports no unresolved tombstones.
- **Sound dirtiness.** For every mutating command in the section 5 contract,
  a single invocation advances the live event sequence, and a publication
  immediately afterward reports a covered sequence equal to it. A test
  enumerates the command tree so a newly added mutating command cannot ship
  without an event.
- **Incremental cost.** Publication after one mutation writes bytes
  proportional to the changed shard and event tail, not to the workspace,
  demonstrated across at least two workspace sizes an order of magnitude
  apart. Section 3.5.10's rapid-fire lifecycle benchmarks pass within their
  recorded budget with the automatic default enabled.
- **Concurrency.** Bounded concurrent workers mutating one workspace produce
  no torn pointer, no partially applied tombstone set, and no lost mutation;
  each worker's committed sequence is covered by some published generation at
  quiesce. A worker that loses a publication race exits 0.
- **Split-failure semantics.** With publication forced to fail, the mutation
  remains committed and visible, the failure is reported on stderr, and the
  process exits 1. A test asserts the mutation is not rolled back.
- **Escape hatches.** `--no-auto-flush` and `checkpoint.auto_flush` each
  suppress publication, the flag wins over configuration, and a suppressed
  workspace is reported dirty by `sync --status`.
- **Recovery equivalence.** `doctor --rehearse` passes against a workspace
  built entirely through automatic publication, and a restore from its
  checkpoint into an empty store is semantically equivalent to the source.
- **Handshake and documentation.** `capabilities` advertises `auto_flush`,
  and README, root help, generated man pages, and `AGENTS.md` describe the
  automatic default in the same commit that flips it, with no surviving
  never-implicit-flush wording.

### Full-project Marathon gates

Before `.marathon/COMPLETE`:

- every R001-R026 item has a ledger entry with either verified
  core-incorporated evidence or a passing extension implementation;
- R026 additionally satisfies its own gate below, or is recorded as
  deliberately not activated with the explicit-flush default intact;
- every roadmap specification, ADR, migration, conformance scenario, and
  documentation requirement is satisfied at the final commit;
- formatting, Clippy, the complete test suite, package installation, recovery,
  stress/capacity, recursive help/man-page, provenance, and consumer-side
  NEEDLE compatibility gates are rerun against the final artifact;
- the release-evidence report covers F001-F017 and R001-R026 and passes the
  noninteractive verifier with exact commit and artifact hashes;
- the working tree is clean and every coherent increment is pushed to
  Forgejo `origin/main`; and
- publication remains separately human-authorized.

## 14. Deferred feature notes

The following candidates remain intentionally deferred in
`docs/notes/ideas-ledger.md` and are not roadmap commitments:

- predeclared file-intent manifests, file-derived dependency serialization,
  edit fencing, and post-diff path enforcement;
- general mutation idempotency keys outside the attempt-resolution boundary
  adopted by current sections 3–7;
- worker capability declarations;
- portable execution-outcome envelopes moved into the current plan as the
  independently specified, atomic attempt-resolution contract;
- a caller-owned stdio session protocol (and MCP hosting atop it).

Atomic resource locks and atomic bulk transaction manifests left this list on
2026-08-15 when they were adopted as R031 and R033. Sensitive-content linting
left it on 2026-09-03 when prevention plus historical repair were adopted as
R038.

Workers are not required to predict or declare files before claiming or
starting a bead. `bead-rs` does not gate edits on an accepted read/write set,
base revision, intent hash, or planning phase. A future file-writing
coordination mechanism may reuse the deferred research, but it needs a separate
product decision and normative specification before becoming a roadmap item.

Native SQLite backup/restore is rejected. Deterministic JSONL flush/import is
the backup and recovery contract; SQLite exists primarily to provide ACID live
operation.

## 15. Inputs still required for version 0.1

The bootstrap core is complete. ADR-002 requires an independently reviewed
normative field-guide contract covering every public native issue field,
lifecycle value, derived state, and owning CLI operation. F017 still needs an
independently authored and reviewed normative `checkpoint-set-v1.md` plus
conformance fixtures. F014 also needs a consumer-side NEEDLE run if its
deployment harness imposes a requirement absent from the v1 contract.

Before activating either implementation from deferred state, the release owner
must confirm separate accountable authors and independent approvers for the
field-guide contract and `checkpoint-set-v1.md`. “Independent approval” means
the reviewer did not author the artifact, verifies its clean-room provenance
and requirement coverage, and records the reviewed hash and decision. The owner
rechecks each blocked input at Phase 5 entry and whenever its native source
schema changes; there is no time-based waiver. The NEEDLE adapter owner is
separately accountable for the consumer-side suite and native backend used at
G5.

Do not guess missing native semantics. Define them in a versioned normative
specification, review them independently, and generate both field-guide
renderings from one typed implementation source. The guide and runbook must be
complete before their replacement feature evidence or F014 can pass.
