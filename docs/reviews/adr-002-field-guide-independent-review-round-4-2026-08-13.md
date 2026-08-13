# ADR-002 native field guide independent review — round 4

Date: 2026-08-13
Reviewer: Claude (Anthropic). Authored the round-1, round-2 and round-3 reviews;
authored neither the original artifact nor any correction, is not the schema
implementation author, and did not author the checkpoint fixes offered here as
evidence.
Artifact: `research/specs/native-field-guide-v1.md` at commit `9953b66`
Specification SHA-256:
`819fd3c16ff1e298dad1c4b2254aad61a76208196e839b09984b365cfd5bde27`
Implementation under evidence: `0375fdc`, plus `2ce61ce`, `02d4d62`, `1943551`,
`9836b07` carried forward
Prior rounds: `0009b32` (rejected as written, `32dea941…`); `febf041` (accepted
with required revisions, `3a5a5228…`); `78a3d32` (accepted with required
revisions, `6b7c6da0…`)
Tracking bead: `bf-57wtd`

Decision: **accepted with required revisions** (outcome 2 of section 10). The
status header does **not** become `accepted normative specification`. R12, R13
and R14 are closed and verified. Three new required revisions — R15, R16, R17 —
are named below.

## Independence and provenance

Method as in prior rounds: every claim was checked against `src/`, `tests/`, and
empirical runs of a build of `9953b66` in disposable workspaces. No source,
tests, fixtures, SQL, or internal documentation from any other bead
implementation was inspected. No clean-room contamination was found.
`PROVENANCE.md` carries a round-four correction-provenance entry that disclaims
acceptance.

Repository state at review: working tree clean; `HEAD` = `origin/main` =
`9953b66`; Forgejo divergence `0 0`. `cargo fmt --check` clean.
`cargo clippy --all-targets -- -D warnings` clean. `cargo test` exit 0 with
**635 passed, 0 failed, 0 ignored across 36 suites**. No `#[ignore]` exists
anywhere in `src/` or `tests/`. All three conformance tests
(`test_checkpoint_round_trip_fidelity_comprehensive`,
`test_checkpoint_merge_advances_revision_when_replacing_newer_live_content`,
`test_checkpoint_merge_replaces_projected_collections_when_present`) are
committed, active, and passing.

## Decision rationale

The round-three findings are genuinely resolved, and the author went past what
the review required: rather than documenting the merge data-loss defect as a
deviation, `0375fdc` fixed it. Merge now preserves the projected collections an
incoming record omits, the revision token advances so a pre-merge
`--if-revision` holder cannot mutate replaced content, and the conformance suite
covers both merge directions. Every one of those claims was independently
reproduced.

It is not accepted unconditionally for one reason of the same class this gate
has caught in each prior round: the correction added a present-tense claim about
implementation behavior that is false in the safety direction, on the path the
artifact itself designates for cutover. Section 4's `closed_at` entry now states
that "import validation rejects the invariant in both directions." The forensic
import path that `sync import-only` actually uses performs no such validation in
either direction, and I restored an invalid record through it twice. Section 9
then instructs operators to rely on that rejection.

Two further merge behaviors are consumer-visible, verified, and undescribed. All
three findings are text changes; none requires touching the model, the section
structure, or the implementation.

## Round-three required revisions — all closed

### R12. Merge preserves external references, comments, and structured data — closed

`0375fdc` replaced the unconditional deletes with replace-when-present and
preserve-when-absent gates (`issue.data.is_some()`,
`issue.extensions.contains_key("external_references")`,
`…("comments")`, `src/service/checkpoint.rs:2192-2209`). Reproduced on a build
of `9953b66`, target holding one of each, incoming record carrying none of them:

```text
TARGET before: rev=4 data=1 refs=1 comments=1 labels=live-label
$ bead sync import-only --input <src checkpoint> --merge --actor reviewer
  Merge completed: 0 inserted, 1 updated, 0 retained
TARGET after:  rev=5 data=1 refs=1 comments=1 labels=incoming-label,live-label
```

Replacement when present was reproduced separately: an incoming record carrying
`data`, `external_references` and `comments` overwrote all three
(`incoming` / `source-7` / `in-c` replacing `old` / `old-1` / `old-c`).

### R13. Conformance test is cutover evidence for merge — closed

`test_checkpoint_merge_replaces_projected_collections_when_present` asserts the
present case. `test_checkpoint_merge_advances_revision_when_replacing_newer_live_content`
asserts the absent case — its source bead carries no collections and it asserts
`COUNT(*) == 1` for all three target tables with the message "merge erased
{table} omitted by the incoming checkpoint". Both are active and passing.
Section 9's evidence sentence is now accurate.

### R14. Merge semantics documented — closed

Both halves are stated in section 4 and both were reproduced:

1. Additive labels/dependencies versus replaced collections: the live label
   survived alongside the incoming one (`incoming-label,live-label`) while the
   live reference was replaced when the incoming record carried one.
2. Revision divergence: `resulting_revision = if existing >= incoming { existing
   + 1 } else { incoming }` (`src/service/checkpoint.rs:2149-2154`). Verified in
   both branches — live 4 / incoming 2 → **5**; live 1 / incoming 7 → **7**. A
   stale checkpoint cannot roll the token backward, and a holder of the
   pre-merge token is invalidated. Section 4's wording matches the code exactly.

A stale merge is also correctly inert: an incoming record with an older
`updated_at` produced `0 inserted, 0 updated, 1 retained` and left title,
revision and references untouched, consistent with "scalar issue content follows
the newer `updated_at`".

## Implementation claims re-verified

Restore fidelity, one fully-populated bead flushed and restored into a fresh
empty workspace on a build of `9953b66`:

| Surface | Source | After restore |
|---|---|---|
| `revision` | 2 | 2 |
| `issue_data` (nested JSON incl. null) | 1, byte-identical | 1, byte-identical |
| `external_references` | 1 | 1 |
| `comments` | 1 | 1 |
| `labels` / `dependencies` / `events` | 1 / 1 / 1 | 1 / 1 / 1 |
| null `description` | NULL | NULL |

Section 3.2's 22-member declaration is complete. A fully-populated active bead
emits exactly `assignee, base_status, comments, created_at, data, dependencies,
external_references, id, issue_type, labels, manual_blocked, notes, priority,
profile, revision, schema_ref, title, updated_at`; the remaining four —
`description`, `closed_at`, `close_reason`, `source_repo` — are the members
correctly documented as absent when unset. `external_references`, `comments` and
`data` are each injected only when non-empty
(`src/service/checkpoint.rs:4523-4551`, `4600`), matching "absent when empty".

## New required revisions

### R15. Forensic import does not validate the closed-metadata invariant in either direction

Section 4's `closed_at` entry states: "Import validation rejects the invariant in
both directions and diagnostic activation remains a no-op when any invalid
record is reported." Neither clause holds for `sync import-only`.

`0375fdc` did make `Issue::validate()` bidirectional (`src/model.rs:302-306`).
But `Issue::validate()` is called from exactly two places —
`stage_import` (`src/service/checkpoint.rs:2553`) and
`stage_import_with_diagnostics` (`:2765`) — and `sync import-only` reaches
neither. `cmd_sync_import_only` calls `service::import_forensic_checkpoint`
(`src/main.rs:1027`), which stages through `stage_forensic_checkpoint` (`:792`)
and checks `validate_forensic_checkpoint` (`:1432`). No issue-level invariant
check exists on that path.

Reproduced on a build of `9953b66`, forging each direction into an otherwise
valid checkpoint and restoring into a fresh workspace:

```text
=== restore-into-empty, base_status=open with closed_at + close_reason ===
Forensic import completed:          rc=0   issues restored: 1
  restored row: bead-730c12bd|open|'2026-08-13T00:00:00.000000000Z'|'stale'

=== restore-into-empty, base_status=closed with neither ===
Forensic import completed:          rc=0   issues restored: 1
  restored row: bead-730c12bd|closed|NULL|NULL
```

`doctor` flags both afterwards — detection works, rejection does not. The second
clause fails for a different reason: the diagnostics gate that returns
`inserted: 0` without activating (`:543-559`) lives in
`import_checkpoint_with_diagnostics`, and `cmd_sync_import_only` never passes
`opts.diagnostics` to anything, so `sync import-only --diagnostics` is still
parsed and dropped.

This matters because section 9 leans on the claim: "cutover validation must
reject or explicitly remediate them before restore." An operator following the
artifact believes restore is the enforcement point. It is not.

Required: scope the sentence to the plain import path, or state plainly that
forensic restore and merge do not validate the closed-metadata invariant, that
`--diagnostics` is inert on `sync import-only`, and that cutover must therefore
run `doctor` on the source before flush **and** on the target after restore.
Record it as a `known_implementation_deviations` entry.

### R16. A second merge from the same origin store fails

Merge is not repeatable once any event from the incoming origin is already
present in the target. `validate_different_uuid_merge`
(`src/service/checkpoint.rs:1689-1718`) rejects any event-identity collision,
and a full checkpoint always re-emits its events from sequence 1. The
same-UUID path (`:1676-1684`) is the mirror image: it requires the checkpoint to
extend local history, which a full checkpoint never does.

Minimal fleet sequence, reproduced on a build of `9953b66`:

```text
worker A: create + update      → 1 event; flush
worker B: merge                → Merge completed: 1 inserted, 0 updated   rc=0
worker A: update               → 2 events; flush
worker B: merge                → bead: Internal error: Different-UUID merge has
                                 event identity conflict: (9dfd4f1f-…, 1) already exists
                                 rc=1
          target notes:        → "work 1"  (second round of work not applied)
```

Because every `update`, `claim`, `close`, `release` and `reopen` appends an
event, this is the second iteration of any realistic fleet loop, not an edge
case. The failure is loud and leaves the target unmodified, so it is not data
loss — but the artifact describes merge's per-collection, scalar and revision
rules in detail while saying nothing about when merge is permitted at all, and
`0375fdc` is titled "make fleet merges lossless". The source comments
("For now, require that checkpoint extends existing history", "need to check
hash equality") show the constraint is known and provisional.

Required: state the precondition and the observed failure, note that the
artifact's cutover procedure in section 9 uses `--restore-into-empty` rather
than repeated merge, and record it as a deviation. The exit-1
`Internal error` classification is also worth naming, since a collision is a
state conflict rather than an internal fault.

### R17. Deleting a projected collection does not propagate through merge

`preserve-when-absent` combined with "absent when empty" means a deletion on the
source side cannot reach a merge target. Verified: the source removed its only
external reference and re-flushed; merging that checkpoint into a target holding
a stale value for the same key left the stale value in place.

```text
source refs after removal: 0
target refs before merge:  stale-target-value
$ bead sync import-only --input <src checkpoint> --merge --actor reviewer
  Merge completed: 0 inserted, 1 updated, 0 retained
target refs after merge:   stale-target-value
```

This is the correct consequence of the rule the artifact states, not a
contradiction — but it is the most likely operational misreading of
"replace-when-present and preserve-when-absent", and the artifact's own format
requires a common mistake for exactly this kind of trap. The same applies to
`comments` and `data`.

Placement compounds it: the entire cross-cutting merge rule set lives inside the
`external_references` field entry, while the `comments` and `data` entries never
mention merge at all. A reader consulting `data` learns nothing about how it
merges.

Required: state that merge cannot propagate a deletion of `data`,
`external_references` or `comments`, and cross-reference the merge rules from
the `comments` and `data` entries, or lift them into their own subsection.

## Findings confirmed correct in this round

Recorded so a fifth round does not regress them. All independently reproduced
against a build of `9953b66` unless noted.

Every round-two and round-three item that closed remains closed: `show --json`
array versus per-line `list --json`; the 22-member checkpoint declaration;
event universality, the six-kind set and the create/dep/label audit gap;
§3.3's normative-target framing; the `status` example; `description`'s absent
creation default; provenance receipts and `schema explain` behavior for other
advertised identities; null-versus-absence for defined members; the
`claim_result` projection; `detail` and `actor` import tolerance; the five
document names and the catalog booleans.

Also re-verified this round: priority names, ordering and default `2`; the `id`,
`title` and 4 MiB text limits; RFC 3339 nanosecond timestamps; `issue_type`,
`profile` and `schema_ref` defaults; every owning operation; `--if-revision` on
update/release/close/reopen and not on claim; `manual_blocked` set by
`update --status blocked` and cleared by `close`/`reopen`; the transition graph
including `deferred -> in_progress` as an exit-4 conflict; the readiness
predicate and `list --ready`; dependency orientation, kinds, self-edge and cycle
rejection, and `--condition`; `fifo-v1` ordering and the R019 policy names;
lease fencing; the `bead changes` cursor; comments readable-but-not-creatable
with inert flags; `ref` and `data` subcommands; `recurrence materialize`; the
2/3/4/5 exit taxonomy; and the section 9 rehydration boundary.

## Accepted baseline

Implementation of `bead schema list|show|explain` may proceed against the whole
artifact — sections 1 through 8 and 10 as written, and section 9 apart from the
sentence identified below — and must track R15, R16 and R17.

- **Excluded:** the section 4 `closed_at` sentence beginning "Import validation
  rejects the invariant in both directions", and the section 9 clause "import
  validation is bidirectional and diagnostic activation is all-or-nothing". Do
  not derive an import-time enforcement guarantee or a
  `known_implementation_deviations` entry from either.
- **Additive, nothing excluded:** R16 and R17 require new text, not corrections
  to existing text. The merge rules already present are accurate.

Both round-two carve-outs and the round-three carve-out on section 9's
preservation sentence are lifted: preservation through merge is now real and
verified.

Fleet rehydration is no longer blocked by `bf-4hr30`, `bf-xq4ds` or `bf-53b91`,
all of which are fixed and verified on both restore and merge. It is blocked by
`bf-3siqo` only in the narrow sense R15 describes: released `0.1.1` can mint
invalid closed rows and no import path rejects them, so cutover must gate on
`doctor` rather than on import.

## Re-review conditions

R15 is one sentence in section 4 plus one clause in section 9 plus a deviation
entry. R16 and R17 are additive text. None touches the model or the section
structure. A revision addressing the three receives a new SHA-256, a new
`PROVENANCE.md` entry, and an unconditional-acceptance review scoped to those
three items. On the evidence of this round the artifact is one text pass from
acceptance; the implementation behind it is materially sound.
