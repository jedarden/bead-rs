# ADR-002 native field guide independent review — round 3

Date: 2026-08-13
Reviewer: Claude (Anthropic). Authored the round-1 and round-2 reviews; authored
neither the original artifact nor any correction, is not the schema
implementation author, and did not author the checkpoint fixes offered here as
evidence.
Artifact: `research/specs/native-field-guide-v1.md` at commit `e5141b9`
Specification SHA-256:
`6b7c6da0e99cd38350664a6b1484d563428a72f7703e4ae45bb8bc2fb923309f`
Implementation under evidence: `02d4d62`, `1943551`, `9836b07`
Prior rounds: `0009b32` (rejected as written, hash `32dea941…`); `febf041`
(accepted with required revisions, hash `3a5a5228…`)
Tracking bead: `bf-57wtd`

Decision: **accepted with required revisions** (outcome 2 of section 10). The
status header does **not** become `accepted normative specification`. Three new
required revisions are named below; all eleven round-two revisions are closed
and the two round-two carve-outs are lifted.

## Independence and provenance

Method as in prior rounds: every claim was checked against `src/`, `tests/`, and
empirical runs of a build of `e5141b9` in disposable workspaces, plus released
`bead 0.1.1` for cross-version behavior. No source, tests, fixtures, SQL, or
internal documentation from any other bead implementation was inspected. No
clean-room contamination was found. `PROVENANCE.md` carries a round-three
correction-provenance entry that explicitly disclaims acceptance.

Repository state at review: working tree clean, `HEAD` = `origin/main` =
`e5141b9`, Forgejo divergence `0 0`. `cargo fmt --check` clean; `cargo clippy
--all-targets -- -D warnings` clean; `cargo test` exit 0 with 634 tests passed,
0 failed, **0 ignored**, across 36 suites — the new conformance test is
committed and active, not `#[ignore]`d.

## Decision rationale

All eleven required revisions were applied correctly and independently
reproduced. The three checkpoint fixes are real and materially close the defects
this review gate was opened over: revision, structured data, external
references, and durable comments now survive `flush-only` →
`import-only --restore-into-empty` intact.

It is not accepted unconditionally because the artifact now makes two
present-tense claims about the **merge** import mode that are false in the
data-loss direction. `sync import-only --merge` deletes an issue's live external
references, comments, and structured data whenever the incoming checkpoint
record omits those members — which is exactly what every checkpoint produced by
released `0.1.1` does, since `0.1.1` cannot emit them. The artifact tells a
consuming agent that main "preserves the collection transactionally on restore
and merge," and points at a conformance test as cutover evidence for those
surfaces; the test asserts nothing about them on the merge path.

This is the same class of defect as the round-two carve-outs — a sentence
written to close a finding that overstates what the producer does — but it is
narrower, and the underlying model, structure, and every other verified claim
stand. Hence outcome 2 rather than a rejection.

## Round-two required revisions — all closed

| Item | Verification |
|---|---|
| R1 `show --json` shape | Closed. `show --json` → `[{…}]`; `list --json` → one object per line. Section 3.1 now states both. |
| R2 checkpoint member list | Closed. Section 3.2 declares all 22 members. A flush emits exactly `base_status, comments, created_at, data, description, dependencies, external_references, id, issue_type, labels, manual_blocked, notes, priority, profile, revision, schema_ref, title, updated_at` for a fully-populated bead; `extensions.insert` at `src/service/checkpoint.rs:4486,4505,4530` is the complete injection set, so the list is complete. |
| R3 event universality and kinds | Closed. "Checkpoints may contain zero or more"; a create-only workspace produced zero event records; example kind is `updated`; the six-kind set and the create/dep/label gap are stated. Two updates produced exactly two events. |
| R4 §3.3 vs §3.1 overlay | Closed. §3.3 is marked normative target with the v0.1 deviation named. |
| R5 `status` example | Closed. Example is `"open"`; `"blocked"` described as post-fix target. |
| R6 `description` creation default | Closed. Section 4 says absent. Verified: `create` without `--description` writes SQL NULL and the member is absent from the checkpoint. |
| R7 provenance receipts | Closed. New §6.1. Its twelve members match `ProvenanceReceipt` (`src/service/checkpoint.rs:151-165`) exactly, and `schema explain` behavior for other advertised identities is specified. |
| R8 null vs absence for defined members | Closed. Section 5 states the `skip_serializing_if` consequence; section 4 entries are split by document. |
| R9 `claim --json` | Closed. New §3.4. Verified output `{"bead_id":…,"assignee":…,"lease":…}`. |
| R10 `detail` default and `actor` | Closed. Deserialization default null vs producer `{}`, and import-nullable `actor`, both stated. |
| R11 document names and catalog booleans | Closed. Five normative document names fixed; `readable`/`writable` added with rationale. |

Both round-two carve-outs are lifted: section 3.2's member list and section 6's
opening, kind examples, and `detail` default are now correct, so the section 1
completeness test and `documents[].members` array may be authored from the
current text.

## Implementation claims the artifact now makes — verified

Measured on a build of `e5141b9`, one disposable workspace flushed and restored
into a second empty workspace:

| Surface | Live | After restore | Verdict |
|---|---|---|---|
| `revision` | 3 | 3 | fixed (`bf-4hr30`) |
| `issue_data` (nested JSON incl. null) | 1, byte-identical | 1, byte-identical | fixed (`bf-xq4ds`) |
| `external_references` | 1 | 1 | fixed (`bf-53b91`) |
| `comments` | 1 | 1 | fixed |
| `labels` / `dependencies` / `events` | 2 / 1 / 2 | 2 / 1 / 2 | no regression |
| null `description` | NULL | NULL | no longer coerced to `""` |

The merge-revision claim in section 4 is also correct as written: with a live
row at revision 4 and an incoming record at revision 2, the merged row keeps
revision 4 (`revision = MAX(revision, ?15)`, `src/service/checkpoint.rs:2153`).

## New required revisions

### R12. Merge does not preserve external references, comments, or structured data

Section 4's `external_references` entry states that "Main checkpoint code
preserves the collection transactionally on restore and merge," and section 9
states that "Main now preserves revisions, structured data, external references,
and durable comments through the native checkpoint." Restore is preserving.
Merge is not: `reconcile_and_merge` issues an unconditional
`DELETE FROM issue_data|external_references|comments WHERE issue_id = ?`
(`src/service/checkpoint.rs:2181,2184,2188`) and then calls importers that
return early when the incoming issue omits the member
(`import_issue_data:2225`, `import_external_references:2276`,
`import_comments:2318`). An incoming record without those members therefore
empties them.

Reproduced twice on a build of `e5141b9`:

```text
# live target holds one ref, one comment, one data namespace, one label
refs/comments/data/labels before merge:  1 / 1 / 1 / 1
$ bead sync import-only --input <checkpoint without those members> --merge --actor reviewer
  Issues: 0 inserted, 1 updated, 0 retained, 0 conflicted
refs/comments/data/labels after merge:   0 / 0 / 0 / 1
```

The realistic fleet form of this uses no hand-built input. Released `0.1.1`
emits only `base_status, created_at, id, issue_type, manual_blocked, notes,
priority, profile, revision, schema_ref, title, updated_at` — it cannot emit
these members at all — so any `0.1.1`-produced checkpoint merged by a HEAD
binary destroys them:

```text
HEAD-side refs before merging a 0.1.1-produced checkpoint: 1
HEAD-side refs after:                                      0
```

External references are the artifact's own named mechanism for binding a
rehydrated bead to its source tracker ID (section 8), so this silently destroys
the traceability the rehydration gate exists to prove — the defect `bf-53b91`
recorded, relocated from the restore path to the merge path. Scope the
section 4 and section 9 claims to `--restore-into-empty`, and record merge as a
`known_implementation_deviations` entry until it is fixed.

### R13. The conformance test is not cutover evidence for the merge path

Section 9 states that "The committed comprehensive round-trip conformance test
is the cutover evidence for those surfaces." The test file is real, active, and
substantive, but `test_checkpoint_round_trip_fidelity_comprehensive` exercises
`flush-only` → `--restore-into-empty` only, and
`test_checkpoint_merge_never_rolls_revision_backward` asserts one integer.
Nothing in it asserts reference, comment, or data survival across `--merge`,
which is why R12 passes CI. Either narrow the evidence claim to restore, or
extend the test to cover merge with members present and absent.

### R14. Merge semantics are undocumented and internally asymmetric

Two behaviors a consumer cannot predict from the artifact, both verified:

1. Labels and dependencies merge **additively** (`import_labels`,
   `import_dependencies` insert from staging, `:1933,1921`), while data,
   references, and comments are **replaced** by the incoming record. The live
   label survived the merge above; the live reference did not.
2. After a stale merge the row carries `revision = MAX(live, incoming)` while
   every other member takes the incoming value. Measured: live
   `4 | Live merge target | live-rev-4` merged with an incoming revision-2
   record became `4 | Merge probe | checkpoint-rev-2`. The revision is
   monotonic, as section 4 claims, but it no longer identifies the content —
   a client holding revision 4 passes `--if-revision 4` against silently
   replaced state. State the per-collection merge rule, and record the
   revision/content divergence as a deviation.

## Accepted baseline

Implementation of `bead schema list|show|explain` may proceed against the whole
artifact — sections 1 through 8 and 10 as written, with no carve-outs — and must
track R12, R13, and R14. Section 9's preservation and evidence sentences are the
only excluded text; do not derive a cutover gate or a
`known_implementation_deviations` set from them.

Fleet rehydration stays blocked, now on the merge defect rather than on
`bf-4hr30`/`bf-xq4ds`/`bf-53b91`, which are correctly closed for the restore
path. `bf-3siqo` remains open and correctly described by section 4.

## Re-review conditions

Correcting R12/R13/R14 is a text change to two sentences plus a deviation
entry; it does not touch the model or the section structure. A revision
addressing them receives a new SHA-256, a new `PROVENANCE.md` entry, and an
unconditional-acceptance review scoped to those three items. Fixing the merge
defect itself is not a precondition for accepting the artifact — accurately
describing it is.
