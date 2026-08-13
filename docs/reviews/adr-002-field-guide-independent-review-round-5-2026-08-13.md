# ADR-002 native field guide independent review — round 5 (targeted)

Date: 2026-08-13
Reviewer: Claude (Anthropic). Authored the round-1 through round-4 reviews;
authored neither the original artifact nor any correction, is not the schema
implementation author, and did not author the checkpoint fixes offered as
evidence.
Artifact: `research/specs/native-field-guide-v1.md` at commit `805c7de`
Specification SHA-256:
`8d26bb1297d91e147cb60a230a2f3653bed6b78d4518b5bc02c3d2d07834ad0e`
Implementation under evidence: `92701b7`
Prior round: `770a2d5` (accepted with required revisions, `819fd3c1…`), addendum
`63e2126`
Tracking bead: `bf-57wtd`

Scope: **targeted**. This review verifies only round-four R15, R16 and R17 and
the four behaviors named in the review request — forensic issue validation,
repeated merges over an identical event prefix with a new suffix, explicit
empty-projection deletion propagation, and legacy omission preservation. No
other section was re-reviewed; every prior closure stands as recorded in rounds
2 through 4.

Decision: **accepted with required revisions** (outcome 2 of section 10). R15
and R17 are closed. R16 is closed as a behavior and **not** closed as a claim:
repeated merges now succeed, but the sentence added to resolve it asserts an
import rule the implementation does not have. One new required revision, R18, is
named below. The status header does **not** become `accepted normative
specification`.

## Independence and provenance

Method as in prior rounds: every claim was checked against `src/`, `tests/`, and
empirical runs of a build of `805c7de` in disposable workspaces. No source,
tests, fixtures, SQL, or internal documentation from any other bead
implementation was inspected. No clean-room contamination was found.
`PROVENANCE.md` carries a round-five correction-provenance entry that disclaims
acceptance.

Repository state at review: working tree clean; `HEAD` = `origin/main` =
`805c7de`; Forgejo divergence `0 0`. `cargo fmt --check` clean.
`cargo clippy --all-targets -- -D warnings` clean. `cargo test` exit 0 with
**635 passed, 0 failed, 0 ignored across 36 suites**; no `#[ignore]` anywhere in
`src/` or `tests/`. All three conformance tests pass, including the two extended
by `92701b7`.

## R15 — forensic issue validation: closed

`92701b7` added a per-issue validation loop at the head of
`validate_forensic_checkpoint` (`src/service/checkpoint.rs:1438-1442`), which is
on the path `cmd_sync_import_only` → `import_forensic_checkpoint` →
`validate_forensic_checkpoint`. `Issue::validate()` has been bidirectional since
`0375fdc`, so both directions of the closed-metadata invariant are now rejected
before activation. Reproduced on a build of `805c7de`, forging each direction
into an otherwise valid checkpoint:

```text
=== base_status=open carrying closed_at + close_reason ===
restore-into-empty: Issue 'bead-85e58523' failed validation:
                    Non-closed issues must not have closed_at or close_reason
                    rc=1   issues restored: 0
--merge:            same message, rc=1, issues: 0

=== base_status=closed carrying neither ===
restore-into-empty: Issue 'bead-85e58523' failed validation:
                    Closed issues must have a close_reason
                    rc=1   issues restored: 0
--merge:            same message, rc=1, issues: 0
```

Both import modes reject, and nothing is written. Section 9's replacement text —
"every native forensic restore and merge now performs bidirectional issue
validation before activation" — is accurate, and moving the operator's
remediation deadline from "before restore" to "before flush" follows correctly.
The excluded section 4 sentence from round four is now true as written and the
carve-out is lifted.

Observation, not a finding: the rejection surfaces as
`bead: Internal error: …` at exit 1 rather than the integrity classification
(exit 5) that section 1 assigns to malformed or integrity-invalid documents.
Section 1's taxonomy is scoped to `bead schema *`, and the artifact promises no
exit code for import validation, so this is outside the accepted text. It is
worth aligning when the schema commands land.

## R17 — explicit empty projection deletion propagation: closed

`92701b7` made the exporter emit the three projections unconditionally:
`external_references` and `comments` are inserted without the `is_empty` guard
(`:4518-4541`), and `data` became `Some(Object)` unconditionally (`:4591`). New
native output therefore always carries them, which drives the
replace-when-present branch and lets a source-side deletion reach the target.

Reproduced on a build of `805c7de`:

```text
new-producer record for a bead with nothing set:
  data= {}   external_references= []   comments= []

source holds one of each → merge → target: data=1 refs=1 comments=1
source deletes all three, reflushes (record carries {} / [] / [])
  → merge → target: data=0 refs=0 comments=0
```

The revised section 4 entries for `data`, `comments` and `external_references`
match: optional for legacy input, required in new native output, with `{}` and
`[]` as the empty forms. No "absent when empty" language survives anywhere in the
artifact. Section 3.2's 22-member declaration is unaffected and still correct.

## Legacy omission preservation: closed

The preserve-when-absent path is intact and is what protects checkpoints from
producers that cannot emit the projections. Reproduced by stripping `data`,
`external_references` and `comments` from the same generation and merging it
into a target holding one of each:

```text
target before: data=1 refs=1 comments=1
$ bead sync import-only --input <stripped checkpoint> --merge --actor a
  Merge completed: 0 inserted, 1 updated, 0 retained
target after:  data=1 refs=1 comments=1
scalar content advanced: notes = "gen2"
```

Live collections survive while scalar content follows the newer `updated_at`,
exactly as section 4 describes. `92701b7` also added
`remove_projected_collections` to the conformance suite and applied it to
`test_checkpoint_merge_advances_revision_when_replacing_newer_live_content`, so
the legacy case is now modeled explicitly rather than incidentally.

## R16 — repeated merges: behavior fixed, claim not

The hard failure is gone. `validate_event_prefix` (`:1673-1710`) replaced both
prior guards and accepts an existing event identity whose `issue_id`, `kind`,
`actor`, `time` and `detail` all match, so a later full checkpoint no longer
collides with itself. Verified across three successive merges of a growing
source: all three returned exit 0 and the target's scalar content advanced
(`work 1` → `work 2` → `work 3`).

Content conflict handling is also correct and was verified. Tampering with the
`actor` of an already-imported event produced:

```text
bead: Internal error: Event identity conflict: (e6db4b69-…, 1) has different content
  rc=1
target title after: "conflict probe"   (unchanged)
target events from origin: 1           (unchanged)
```

Section 4's "an identity with different content is a conflict and leaves the
target unchanged" is accurate.

### R18. Repeated merges re-import the identical prefix instead of skipping it

Section 4 now states that "Repeated merges accept an identical event-history
prefix and **import only its new suffix**." The suffix-only half is false. Every
merge re-inserts the whole prefix as additional event rows.

`import_events` uses `INSERT OR IGNORE` (`:1938-1955`) and supplies no
`sequence`. The `events` table's only uniqueness is
`sequence INTEGER PRIMARY KEY AUTOINCREMENT` (`src/store/migrations.rs:121-129`),
which is auto-assigned and can never collide; `(origin_store_uuid,
origin_event_sequence)` carries a **non-unique** index
(`src/store/migrations.rs:236`). There is therefore no constraint for `OR IGNORE`
to act on, and no other guard skips already-present events — `validate_event_prefix`
only decides whether the merge may proceed.

Measured on a build of `805c7de`, one source workspace merged three times into
one target as it gained one event per round:

```text
after merge 1:  origin events in target = 1     seq 1 ×1
after merge 2:  origin events in target = 3     seq 1 ×2, seq 2 ×1
after merge 3:  origin events in target = 6     seq 1 ×3, seq 2 ×2, seq 3 ×1
                (source has 3 events)
rows carrying an origin identity = 6, distinct identities = 3
```

Growth is quadratic in the number of merges. The duplication is consumer-visible
through the public change feed: `bead changes --since 0 --json` on that target
reports `total_available: 9`, listing each duplicated origin event as its own
mutation with its own local sequence.

It also falsifies two section 6 statements that were verified in earlier rounds
and are still in the accepted text: that events are "ordered by
`(origin_store_uuid, origin_event_sequence)`" as an identity, and that the
sequence is "a positive, monotonically contiguous integer within the origin
store". After two merges neither holds in the target.

The conformance suite does not catch it because
`test_checkpoint_merge_replaces_projected_collections_when_present` asserts only
that the three collections reach zero after the second merge; it never counts
events.

Required, either:

1. **Make the dedupe real.** Add a partial unique index —
   `CREATE UNIQUE INDEX events_origin_identity_unique ON events
   (origin_store_uuid, origin_event_sequence) WHERE origin_store_uuid IS NOT NULL`
   — so `INSERT OR IGNORE` behaves as the code already assumes. The partial
   predicate is necessary: locally generated events carry NULL origin columns
   (verified: 3 NULL-origin rows alongside 6 origin-carrying rows in the probe
   target), and SQLite treats NULLs as distinct anyway. Note that the index
   **cannot be created on already-duplicated data** — attempting it on the probe
   target fails with `UNIQUE constraint failed: events.origin_store_uuid,
   events.origin_event_sequence` — so the migration needs a dedupe step for any
   workspace that has already merged twice. Alternatively skip in `import_events`
   the identities `validate_event_prefix` already matched.
2. **Or correct the claim** — but then section 6's identity and contiguity
   statements need the same treatment, and the change feed duplication needs a
   `known_implementation_deviations` entry. Option 1 keeps more of the accepted
   text true.

Add an event-count assertion to the repeated-merge conformance coverage either
way; without one this regresses silently.

## Accepted baseline

The whole artifact is accepted except the section 4 clause "and import only its
new suffix". Do not derive suffix-only import semantics, an event-identity
uniqueness guarantee in a merge target, or a `known_implementation_deviations`
set from it. Every other sentence verified in this round — R15's validation
claim, R17's explicit-empty claims, the legacy-omission claim, the
content-conflict claim, and section 9's replacement text — is accurate.

All round-two, round-three and round-four carve-outs are lifted, including the
round-four exclusions on the section 4 `closed_at` sentence and the section 9
"import validation is bidirectional" clause. R18 is the only open item against
this artifact.

## Re-review conditions

R18 is one clause plus either a five-line migration or a skip in
`import_events`, and one assertion. A revision addressing it receives a new
SHA-256, a new `PROVENANCE.md` entry, and an unconditional-acceptance review
scoped to that single item.
