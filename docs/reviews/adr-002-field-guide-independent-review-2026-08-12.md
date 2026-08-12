# ADR-002 native field guide independent review

Date: 2026-08-12
Reviewer: Claude (Anthropic), operating independently from the OpenAI Codex
2026-08-12 authoring session
Artifact: `research/specs/native-field-guide-v1.md` at commit `f4a31c8`
Specification SHA-256:
`32dea9411f49a2897ac72e3df0ebdfda35b819bc5af159794daf92b14d50fa52`
Tracking bead: `bf-57wtd`
Decision: **rejected as written**; concrete corrections required below. A
corrected revision receives a new hash and a new review. Implementation of
`bead schema *` and fleet rehydration remain blocked, per the artifact's own
L231 and `docs/plan/plan.md` section 15.

## Independence and provenance

The reviewer did not author the artifact. `PROVENANCE.md` records the authoring
session and explicitly disclaims self-approval. This review used only this
repository, its installed `bead 0.1.1` binary, and disposable scratch
workspaces created for the purpose. No source, tests, fixtures, SQL, or internal
documentation from any other bead implementation was inspected. No evidence of
clean-room contamination was found; every defect below is a specification
accuracy or completeness problem, not a provenance problem.

Method: the artifact's claims were checked against (a) `src/` and `tests/`,
(b) the generated man pages under `man/man1/`, (c) sibling specifications under
`research/specs/` and `docs/`, and (d) empirical runs of `bead 0.1.1` in a
throwaway workspace. Where those sources disagreed with each other, the running
binary was treated as authoritative for present-tense claims, and the
disagreement is recorded as a separate finding.

## Decision rationale

The artifact's model of a bead is largely sound, and several of its most
error-prone sections — dependency orientation, the readiness predicate, the base
status enum, the transition graph — are exactly right and independently
reproduced. It is rejected for one structural reason and a set of factual ones.

The structural reason: the artifact claims at L11-12 to define "the complete
public, agent-facing meaning of a native bead," but it describes only the
checkpoint issue record. bead-rs exposes two different public issue documents
with different member sets and a different lifecycle key, and the one agents
actually consume is the one the artifact does not describe.

## Blocking findings

### B1. `status` versus `base_status`: the documented surface is not the consumed surface

The artifact documents `base_status` and never mentions `status`. Observed from
`bead 0.1.1`:

```jsonc
// bead list --json  /  bead show --json   (src/main.rs:1259-1285, to_needle_json)
{ "assignee": null, "created_at": "2026-08-12T22:51:52.301697398Z",
  "dependencies": [], "description": "", "id": "bead-18409c0e",
  "labels": [], "priority": 2, "revision": 1,
  "status": "open",
  "title": "review probe", "updated_at": "2026-08-12T22:51:52.301697398Z" }

// .beads/checkpoint/objects/*.jsonl
{ "record_type": "issue", "issue": {
    "base_status": "closed", "close_reason": "probe done",
    "closed_at": "...", "created_at": "...", "id": "bead-18409c0e",
    "issue_type": "task", "manual_blocked": false, "notes": "",
    "priority": 2, "profile": "native-v1", "revision": 1,
    "schema_ref": "urn:bead-rs:schema:issue:native-v1", "title": "review probe",
    "updated_at": "..." } }
```

Ten of the artifact's nineteen documented fields — `notes`, `manual_blocked`,
`issue_type`, `closed_at`, `close_reason`, `source_repo`, `profile`,
`schema_ref`, `data`, `extensions` — never appear in the CLI projection. The
`{record_type, issue|event}` envelope is never mentioned. `interchange-v1.md:18`
and `needle-cli-contract-v1.md:41-44` both require `status`;
`checkpoint-set-v1.md:35` emits `base_status`.

Required: name both documents, give each its own member list, and specify the
projection mapping including `status` = derived overlay of `manual_blocked` on
`base_status`.

### B2. Priority level names are wrong for P1, P2, and P3

L80-81 states "P0 urgent, P1 high, P2 normal, P3 low, P4
aspirational/backlog". `src/cli.rs:231-236` and `docs/plan/plan.md:198-202`
both give: 0 urgent, 1 critical, 2 high **and the native default**, 3 normal,
4 aspirational/backlog. Three of five names are shifted one rung and "low" is
invented. An agent following the artifact files ordinary work at P3 believing
it is "normal", placing it below the default.

The default value is also never stated. It is `2`
(`bead-create.1`: `--priority [default: 2]`, confirmed live).

### B3. Default values for `description` and `notes` are wrong

L68 and L73-74 say "nullable, default absent". Observed: `description` is
emitted as `""` by the CLI and omitted from the checkpoint; `notes` defaults to
`""` at the DB layer (`src/store/migrations.rs:105`) and is never absent. The
artifact's own L39-40 rule — "Empty strings are values, not null" — makes the
contradiction sharper rather than resolving it.

### B4. Null and absence are flattened, against the producer and three siblings

L38-39 asserts absent and `null` "have the same native meaning". The producer
emits `"assignee": null` explicitly rather than omitting the member.
`profile-loss-report-v1.md:59-60`, `br-v1-profile.md:75-76`, and
`interchange-v1.md:38` all require the distinction to be preserved. The rule
also defeats the artifact's own lossless `extensions` round-trip requirement
(L153-158), which cannot be honoured without distinguishing the two.

### B5. Five fields name the wrong owning operations

| Field | Artifact | Actual |
|---|---|---|
| `title` | `create` and `update` | `create` only; `--title` on update is a usage error (`src/cli.rs:492-495`) |
| `description` | `create` and `update` | `create` only (`src/cli.rs:254-255`) |
| `priority` | `create` and `update` | `create` only (`src/cli.rs:257-259`) |
| `issue_type` | `create` and `update` | `create` only (`src/cli.rs:261-263`) |
| `notes` | `create` and `update` | `update` only; `create` has no `--notes` |

`assignee` (L96-99) additionally omits `create --assignee` (`src/cli.rs:265-267`)
and `update --clear-assignee`, which is the only way to clear an assignee on an
open bead.

### B6. `claim` has no revision guard

L182 states `claim` "honors a revision guard". `ClaimOptions`
(`src/cli.rs:437-465`) exposes `assignee, json, why, policy, lease_ttl,
renew_lease, fencing_token` and no `if_revision`; `bead claim --assignee w1
--if-revision 1` exits 2 with `unexpected argument`. Guards exist on
`update`, `release`, `close`, and `reopen` only. Claim's concurrency safety
comes from the atomic transaction and lease fencing tokens.

### B7. Events are absent entirely

The string "event" does not appear in the artifact. Events are a durable public
record family with their own schema identity
(`urn:bead-rs:schema:event:native-v1`), present in every checkpoint:

```jsonc
{ "record_type": "event", "event": {
    "$schema": "urn:bead-rs:schema:event:native-v1", "actor": "system",
    "detail": { "prior_base_status": "open", "reason": "probe done" },
    "issue_id": "bead-18409c0e", "kind": "closed",
    "origin_event_sequence": 1, "origin_store_uuid": "...", "time": "..." } }
```

ADR-002 L41 requires the guide to state interactions with "lifecycle, readiness,
dependencies, **events**, and revision"; `plan.md:1489-1490` repeats it. Note
also that event instances identify themselves with `$schema` while issue
instances use `schema_ref` — a live inconsistency with
`schema-identification-v1.md:36-39`, which the artifact should either resolve or
record, since it claims authority over `schema_ref`.

## Required corrections, non-blocking

1. **Undefined terms that block generation.** "Timestamp string" (L108, L113,
   L118) — the real format is RFC 3339 UTC with nanosecond precision,
   `2026-08-12T22:51:52.301697398Z`. "Disallowed control character" (L52) —
   `src/model.rs:29-34` exempts `\t`, `\n`, `\r`. "Active unfinished blocker"
   (L187) — `bead-dep-add.1` defines unfinished as "not in closed state", so a
   deferred blocker still blocks. `issue_type` value space (L101-104) — free
   string, no validation. `revision` initial value — `1`. `close_reason` has no
   length bound while `title`, `description`, and `notes` do.
2. **`extensions` is a field entry describing a rule.** L153-158 reads as
   `additionalProperties`, not a member named `extensions`. The completeness
   test at L26-27 cannot be authored until this is settled.
3. **The field-guide document's own shape is unspecified.** Only `schema_ref`
   and `describes_schema_ref` are named (L19-21), yet `schema show` must return
   a Draft 2020-12 schema for it (L22-23). `describes_schema_ref` is introduced
   here and defined in no sibling.
4. **Per-field examples are required and absent.** L44-45 promises type,
   nullability/default, ownership, owning operations, invariants, and a common
   mistake — no example. Checklist item 2 (L222-224) then requires the reviewer
   verify "every type, nullability, default, owner, operation, invariant,
   example, and mistake". `plan.md:1486-1489` requires per-field minimal
   examples. The artifact fails its own gate.
5. **The `derived` ownership class is defined and never applied**, because every
   derived surface was omitted. `base_status` (L85-86) and `source_repo`
   (L128-129) state no ownership class at all. `source_repo` has no writer
   anywhere in `src/`, so "supplied only where a public operation permits" is
   vacuous — state that it is unreachable in 0.1.
6. **Omitted public surface.** Leases and fencing tokens, including that an
   expired lease blocks update/release/close; the `bead changes` cursor feed and
   snapshot identity, whose invalidation on restore is directly relevant to the
   artifact's own rehydration procedure; `recurrence`; claim selection policy
   (`fifo-v1`: priority ASC, created_at ASC, id ASC) and R019 ranking;
   conditional dependencies (`dep add --condition`); `list --ready`;
   `update --status blocked|open`, which is the only way to set or clear
   `manual_blocked`; `close` clearing `manual_blocked`; the `blocked_by` and
   `blocking` derived arrays; and the exit-code taxonomy (conflict = 4,
   not-found = 3) behind "fail closed" (L24).
7. **Comments are mis-hedged, not missing.** L172-173 defers them "until a
   public command is advertised". `bead list --comments` and
   `bead show --comments none|unresolved|all` ship today, and `PROVENANCE.md`
   records an accepted 2026-08-07 decision on comment handling. The accurate
   statement is that comments are publicly readable and not publicly creatable.
   (See I5 — the flag is currently inert.)
8. **`release` is described as a reopen.** L99 and L183. `release` is
   `in_progress -> open` only; other states are exit-4 conflicts; an open
   assigned bead requires `update --clear-assignee`.
9. **`closed_at` "present exactly when closed"** (L119) is correct as intent and
   false in practice — see I1. State the invariant and note that it is not
   currently enforced.
10. **`schema_ref` nullability.** L140-141 invents an "older accepted native
    record" class. `schema-identification-v1.md:10-11` requires every
    `native-v1` record to carry a nonempty `schema_ref`, and ADR-002 kept only
    the exact native checkpoint version.
11. **Stale migration references.** L231 "and migration" and the example title
    at L195, "Verify migration rehearsal", refer to scope ADR-002 removed.

## Findings confirmed correct

Recorded so a later revision does not regress them. Dependency orientation,
including the endpoint-reversal warning, matches `bead-dep-add.1` and
`src/service/dependencies.rs` exactly; only `blocks` and `relates_to` exist.
The readiness predicate — open, unassigned, not manually blocked, no unfinished
`blocks` blocker — matches all three independent implementations
(`src/service/issues.rs:238-241`, `src/service/claim.rs:638-666`,
`src/service/why.rs:302-305`) and was reproduced empirically. The `base_status`
enum and "blocked and ready are derived, never stored" are correct
(`src/store/migrations.rs:108`). The transition graph matches
`src/model.rs:167-187`, including `closed -> open` only via `reopen`. The `id`
and `title` size limits match `src/model.rs:19-69` exactly. Priority range and
direction are correct. `profile` handling and "version 0.1 accepts no external
checkpoint profile" hold (`src/service/checkpoint.rs:620-624`). The
caller/system/derived/preserved vocabulary is a sound abstraction. The
rehydration boundary (L202-215) is in scope and matches ADR-002 point for point;
it does not resurrect the removed cross-tool migration.

## Implementation defects found while verifying

These are not specification findings and do not affect the decision. They are
recorded because three of them are silent data loss.

- **I1.** `bead update --status closed` bypasses `close`, producing
  `base_status=closed` with NULL `closed_at` and NULL `close_reason`, despite
  `src/cli.rs:474-476` claiming otherwise. `doctor` passes it, flush exports it,
  and `import-only --restore-into-empty` accepts it. Directly violates the
  invariant at artifact L119 and L124.
- **I2.** `revision` does not survive a checkpoint. Live values increment
  correctly (traced 1, 2, 3, 4 across create/update/update/close), but export
  hardcodes `revision: Some(1)` (`src/service/checkpoint.rs:4337`) and no import
  writer includes the column. A workspace at `4, 1, 2, 1` restores as
  `1, 1, 1, 1`, destroying every optimistic-concurrency guard.
- **I3.** `bead data` content does not survive a checkpoint; `issue_data` count
  is 0 after restore. The issue document also always carries `data: None`.
- **I4.** `dep add --kind parent-child` reports success and writes nothing —
  `INSERT OR IGNORE` swallows the CHECK violation
  (`src/service/dependencies.rs:128-137`). Kind validation exists only on the
  dry-run path.
- **I5.** `--comments` is validated on `list`/`show` but never affects output.
- **I6.** `sync flush-only --profile` is accepted and ignored;
  `import-only --diagnostics` is parsed and never read.
- **I7.** `bead capabilities` advertises a `schema` command that does not exist
  and omits eight that do (`ref`, `data`, `query`, `changes`, `why`, `compare`,
  `recurrence`, `policy`).
- **I8.** `plan.md:803` and `needle-cli-contract-v1.md:32` specify
  `dep add --type`; the shipped CLI accepts only `--kind` and rejects `--type`.
  The artifact matches the binary; the NEEDLE contract does not. This is a live
  consumer-contract break independent of this review.

## Process findings

The artifact's review clause (L217-232) is weaker than this repository's own
precedent and should be tightened before the next round.

1. L219 says the reviewer records "this file's exact hash" — no algorithm, no
   location, no artifact. The precedent is SHA-256, a dated disposition appended
   to `PROVENANCE.md`, and a findings document under `docs/reviews/`.
2. The scheme is binary: accept, or L231 "rejection keeps implementation and
   migration blocked". `checkpoint-set-v1-independent-review-guide.md:76-102`
   defines four outcomes including approve-with-required-revisions, which is the
   proportionate outcome for an artifact of this quality. Adopt it.
3. Independence is defined only as "did not author this artifact" (L14), which
   would permit the implementer to review it — the exact gap behind the recorded
   F017 violation.
4. No status-header end state is defined (L3, L7).

## Sibling staleness

Not the artifact's fault, but the next reviewer will re-derive the same
conflicts unless it is addressed. `3122b85` removed the adapters and left their
specifications in place still labelled normative candidates:
`bf-v1-profile.md`, `br-v1-profile.md`, `profile-loss-report-v1.md`,
`conformance-v1.md:10` (lanes 2 and 5), `schema-identification-v1.md:46-48`,
`interchange-v1.md:53-72`, and `checkpoint-set-v1.md:332`, which still registers
the removed migration-receipt identity. Separately, `plan.md` still scopes
`revision` (R003), `ref` (R011), and `data` CRUD (R018) as post-0.1 while all
three ship today; on those points the plan is stale and the artifact is right.

Mark the removed-scope specifications `Superseded by ADR-002` or move them out
of `research/specs/`.

## Re-review conditions

A revision addressing B1 through B7 and correction items 1 through 4 is
sufficient for acceptance as the implementation baseline. Items 5 through 11 and
the process findings should be folded into the same revision to avoid a third
round. The corrected artifact receives a new SHA-256, a new entry in
`PROVENANCE.md`, and a review by a reviewer who authored neither the original
nor the correction.
