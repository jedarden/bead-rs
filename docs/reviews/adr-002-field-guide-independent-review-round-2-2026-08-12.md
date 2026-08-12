# ADR-002 native field guide independent review — round 2

Date: 2026-08-12
Reviewer: Claude (Anthropic). Authored the round-1 review; authored neither the
original artifact nor the correction under review, and is not the schema
implementation author.
Artifact: `research/specs/native-field-guide-v1.md` at commit `5c45293`
Specification SHA-256:
`3a5a5228b1d2a7f38cb281733c4f9e0443970e5eabe3ebcd0a8829b7626e087d`
Prior round: `0009b32`, rejected as written, artifact hash
`32dea9411f49a2897ac72e3df0ebdfda35b819bc5af159794daf92b14d50fa52`
Tracking bead: `bf-57wtd`

Decision: **accepted with required revisions** (outcome 2 of the artifact's own
section 10). The explicitly accepted baseline and the excluded sections are
named below. The status header does **not** become `accepted normative
specification`; that requires an unconditional acceptance against a later hash.

## Independence and provenance

The reviewer authored neither the original nor the correction. `PROVENANCE.md`
records correction authorship and explicitly disclaims approval, and the
correction commit adds that entry. Method as in round 1: claims were checked
against `src/`, `tests/`, `man/man1/`, sibling specifications, and empirical
runs of both released `bead 0.1.1` and a build of `2ce61ce` (HEAD) in disposable
workspaces. No source, tests, fixtures, SQL, or internal documentation from any
other bead implementation was inspected. No clean-room contamination was found.

## Decision rationale

The structural defect that drove the round-1 rejection is resolved. The artifact
now names two public issue documents, gives each its own member list, defines a
projection mapping, and adds the events section that was wholly absent. Five of
seven blocking findings are fully closed and independently reproduced. The
typed guide document is now specified in enough detail to generate from.

It is not accepted unconditionally because two of the sections written
specifically to close round-1 findings contain factual errors of the same class
as the finding they close: section 3.2's checkpoint member list omits two
members the producer actually emits, and section 6 opens with a universal claim
about events that a create-only workspace falsifies. Section 1 keys the
completeness test to those member lists, so an implementer who builds the test
from the accepted text would produce a test that certifies an incomplete surface
as complete — the round-1 failure mode reproduced at smaller scale.

The residual defects are local, concrete, and mechanically fixable, which is why
this is outcome 2 rather than a second rejection.

## Disposition of round-1 findings

| Finding | Disposition |
|---|---|
| B1 `status` vs `base_status` | Substantially fixed; residual R1, R2, R4, R5, R9 |
| B2 priority level names | **Closed.** Verified against `src/cli.rs:231-236`, `docs/plan/plan.md:198-202`, and a live default of `2` |
| B3 `description`/`notes` defaults | Partially fixed; `notes` correct, `description` residual R6 |
| B4 null vs absence | Fixed as a rule (section 2); residual R8 |
| B5 owning operations | **Closed.** `title`/`description`/`priority`/`issue_type` create-only, `notes` update-only, and the full `assignee` set all verified |
| B6 `claim` revision guard | **Closed.** `if_revision` exists on `UpdateOptions`, `ReleaseOptions`, `CloseOptions`, `ReopenOptions` only (`src/cli.rs:538,595,662,726`) |
| B7 events absent | Section added; residual R3, R7, R10 |

Correction items 1, 2 and 4 are closed. Item 1's undefined terms are all now
defined and every one was verified: RFC 3339 UTC with nanosecond precision;
control characters exempting tab/LF/CR (`src/model.rs:29-34`); unfinished =
not closed, so a deferred blocker still blocks; `issue_type` a free string;
`revision` initial `1`; `close_reason` unbounded. Item 2 is correct — `extensions`
is `#[serde(flatten)]` and never serialized as a member name. Item 4 is
satisfied: every field entry carries an example and a common mistake. Item 3 is
mostly closed; see R11.

Items 5 through 11 are closed. The `derived` class is now applied;
`source_repo` is correctly stated as having no public writer; the omitted public
surface (leases and fencing, `bead changes` cursor and snapshot identity,
comments, refs, recurrence, `list --ready`, `update --status blocked|open`,
`blocked_by`/`blocking`, claim policies) is present and each claim I checked
holds; `release` is correctly scoped to `in_progress -> open`; the `closed_at`
invariant is stated with its deviation; `schema_ref` no longer invents an older
record class; the stale migration references are gone.

All four process findings are closed. Section 10 now specifies the algorithm,
the artifact locations, four outcomes, reviewer independence covering the
correction author and the schema implementer, and a status-header end state.

## Required revisions

These are the conditions of acceptance. Each is a specific factual correction,
not a redesign.

### R1. `bead show --json` returns an array, not one object per line

Section 3.1 says both `list --json` and `show --json` "emit one compact JSON
object per line." `src/main.rs:401-407` wraps the projection in a one-element
vector for NEEDLE v1 compatibility, and it is observably an array:

```text
$ bead show bead-c8aa2240 --json
[{"assignee":null,...,"status":"open","title":"nodesc",...}]
$ bead list --json
{"assignee":null,...,"status":"open","title":"nodesc",...}
```

A consumer written to the accepted text breaks on the first `show --json` call.
State the two shapes separately.

### R2. The checkpoint issue object emits `labels` and `dependencies`

Section 3.2 declares eighteen named members and says anything else is an unknown
additional property governed by section 5. Observed in a flushed checkpoint:

```json
{"issue":{"base_status":"open","created_at":"...","id":"bead-c030aea7",
  "issue_type":"task","labels":["bar","foo"],"manual_blocked":true,"notes":"",
  "priority":2,"profile":"native-v1","revision":1,
  "schema_ref":"urn:bead-rs:schema:issue:native-v1","title":"labelled",
  "updated_at":"..."},"record_type":"issue"}
```

`dependencies` appears the same way on any bead that has an edge, and the
importer reads it by name (`src/service/checkpoint.rs:2380`). They reach the
wire through the flattened extensions map, but they are known graph projections,
not unknown members — section 5 itself says a name colliding with a defined
member is not an extension, and section 4 already documents both as "checkpoint
graph projection". Section 3.2's list and the `documents[].members` array built
from it are therefore wrong by two, and the section 1 completeness test inherits
the error.

### R3. Not every checkpoint contains events, and `created` is not an event kind

Section 6 opens "Every checkpoint contains durable event envelopes." A workspace
holding only created beads flushes a checkpoint with zero event records —
verified: one `create`, one flush, one record, `record_type` `issue`. The
envelope example and the `kind` field entry both use `"created"`, which v0.1
never emits. The complete kind set is `updated`, `claimed`, `released`,
`reopened`, `closed`, `assignment_cleared` (`src/service/lifecycle.rs`,
`src/service/claim.rs:176,285,419,809`).

The same paragraph says events are "appended by semantic mutations." `create`,
`dep add`, and `label add` are semantic mutations that append nothing —
confirmed by a probe whose first event was sequence 1 from an `update`, after
two creates and a `dep add`. Either narrow the claim to the lifecycle and claim
operations that do emit, or record the gap as a deviation.

### R4. Section 3.3 contradicts section 3.1 on the `status` overlay

Section 3.1 correctly records that v0.1 projects `base_status` without the
manual-blocked overlay. Section 3.3 then states as present-tense fact that the
CLI "maps `base_status` plus `manual_blocked` to `status`." `src/main.rs:1264-1284`
matches on `base_status` alone. Mark 3.3 as the normative target and point at
the deviation, or the reader takes whichever section they read second.

### R5. The `status` field example cannot be produced by v0.1

Section 4 gives `status` the example `"blocked"`. Verified: a bead with
`manual_blocked = 1` in the database projects `"status":"open"`. Per section 1
every field example is carried into the typed guide and both renderings, so this
ships a value the producer cannot emit. Use `"open"` and describe `"blocked"` as
the post-fix target.

### R6. `description`'s creation default is absent, not empty text

Section 4 says the checkpoint creation default "is empty text." `create` without
`--description` writes SQL NULL — the `DEFAULT ''` on the column
(`src/store/migrations.rs:103`) is never exercised — and the checkpoint omits
the member entirely:

```text
sqlite> SELECT description IS NULL, quote(description), quote(notes) FROM issues;
1|NULL|''
```

`notes` genuinely defaults to `""` and its entry is correct. Section 3.1's
"materialized as `""` when the checkpoint member is absent" is also correct;
only the section 4 entry needs the fix.

### R7. The checkpoint has a third record family the guide does not describe

`CheckpointRecord` has three variants: `issue`, `event`, and
`provenance_receipt` (`src/service/checkpoint.rs:108-118`), and `capabilities`
already advertises `urn:bead-rs:schema:provenance-receipt:native-v1` alongside
`checkpoint-pointer` and `checkpoint-manifest` (`src/service/capabilities.rs:120-170`).
`describes_schema_refs` covers two of at least five advertised identities, and
section 1 defines `schema explain SCHEMA_REF` over an open identifier space
while specifying output for one identifier only. Either state that the other
identities are out of scope for the field guide and what `schema explain`
returns for them, or extend coverage. As written, "unsupported identifiers are
usage failures (exit 2)" leaves an advertised, emitted document ambiguous.

### R8. The checkpoint producer emits no explicit nulls for defined members

Section 2's rule is right and is the correct fix for B4. But every optional
member of `Issue` carries `skip_serializing_if = "Option::is_none"`
(`src/model.rs:194-263`), so an unset defined member is *absent* from the
checkpoint, never `null`. Section 4 nonetheless gives `assignee`, `closed_at`,
`close_reason`, `source_repo`, and `data` the checkpoint "default/example null".
The CLI projection does emit `"assignee": null`. Split the entries by document,
and note that null-versus-absence survives round trips for unknown extension
members (which land in the untyped map) but not for defined members, where an
incoming explicit null re-serializes as absence. Section 5's example is correct
as written.

### R9. `claim --json` is an undescribed public projection

The artifact defines "the public, agent-facing documents." `claim --json` is a
third one and NEEDLE consumes it:

```json
{"bead_id":"bead-c8aa2240","assignee":"w1","lease":{"issue_id":"...","assignee":"w1","fencing_token":1,"expires_at":"..."}}
```

It keys the bead as `bead_id`, not `id`. Name it, or state that only `list` and
`show` are in scope and that other commands' JSON is not governed here.

### R10. `detail` does not default to `{}` at the deserialization boundary

Section 6 says `detail` "is a JSON value and defaults to `{}`". `EventRecord`
uses `#[serde(default)]` on a `serde_json::Value`, whose default is `null`
(`src/service/checkpoint.rs:281-292`). Producers do emit `{}` — verified on
`updated` events. Also, `EventRecord.actor` is non-nullable but the import-side
`SerializedEvent.actor` is `Option<String>`; section 6 declares `actor` required
non-null, which the import path does not enforce.

### R11. Two loose ends in the guide document's own shape

Section 1 requires each `documents` entry to carry a `name`, and the field-entry
example uses `"document": "checkpoint_issue"`, but the two document names are
never fixed normatively — the prose gives only headings. Name them. Separately,
`schema list` entries are specified as `schema_ref`, `document_kind`,
`validate`, `consume`, `emit`, dropping the `readable`/`writable` booleans the
shipped `capabilities` catalog already carries for the same entries; say whether
that divergence is intended.

## Stale relative to HEAD

Not defects in the artifact at its reviewed hash, but they are already untrue on
`main` and must be reconciled before the next hash.

`bf-3siqo` is fixed by `2ce61ce`, which landed two minutes after the reviewed
commit. Verified on a build of HEAD:

```text
$ bead update bead-b9930f08 --status closed
bead: Conflict: Use 'close' command to transition an issue to closed   (exit 4)
$ bead doctor
ERROR schema_validity: ... Found 1 issues with inconsistent closed status metadata
```

Released `0.1.1` still accepts it, so the artifact is accurate for the released
binary and stale for `main`. Section 4's `closed_at` entry and section 9's
cutover-gate list both need re-stating, and the deviation needs a disposition
rather than deletion — the fix blocks the generic update path and teaches
`doctor` to detect the state, but it does not repair existing rows, and the
diagnostics import path skips invalid records rather than failing
(`src/service/checkpoint.rs:2584-2599`), so "cannot be reached" is not yet the
right claim. The bead itself is still `open` in the NEEDLE workspace; close it
or record why it stays open.

`bf-4hr30` and `bf-xq4ds` remain live and were reconfirmed: live revisions
`2, 3` exported as `1, 1`.

## Findings confirmed correct in this round

Recorded so a third round does not regress them. Independently reproduced
against the running binary unless noted.

Priority names, ordering, and the default of `2`. The `id` (1-255 bytes, no
leading/trailing whitespace, `/`, `\`, NUL, or control characters other than
tab/LF/CR), `title` (1-4096 bytes), and `description`/`notes` (4 MiB) limits.
The RFC 3339 nanosecond timestamp format. `issue_type` defaulting to `"task"` as
a free string; `profile` defaulting to `"native-v1"`; `schema_ref` present on
every native-v1 issue record. Ownership and owning operations for every field
listed in B5. `revision` starting at 1, advancing on semantic mutation, guarded
by `--if-revision` on update/release/close/reopen and not on claim.
`update --status blocked` setting `manual_blocked` while leaving `base_status`
open; `close` clearing it; `reopen` clearing close metadata and manual blocking.
The transition graph, including `deferred -> in_progress` rejected as an exit-4
conflict and `release` on a non-`in_progress` bead likewise. The readiness
predicate and `list --ready`. Dependency orientation, the two kinds
(`blocks`, `relates_to`), self-edge and cycle rejection on `blocks`, and
`--condition` taking JSON. `fifo-v1` ordering by priority, created_at, then id
(`src/service/claim.rs:464,654,844`) and the R019 policy names
(`src/service/policy.rs:195-211`). Lease fencing tokens and expiry. The
`bead changes` cursor with `--since/--latest/--snapshot/--validate` and snapshot
identity. Comments readable-but-not-creatable and the flags currently inert.
`ref add|remove|list|find` with `--id/--namespace/--key/--value`, matching the
section 9 example. `data set|get|list|remove`. `recurrence materialize` as the
only occurrence mint. The exit taxonomy 2/3/4/5 (`src/error.rs:70-79`), noting
that not-found is carried by the workspace variant. The section 9 rehydration
boundary and reconciliation-report requirements. The `$schema` versus
`schema_ref` inconsistency, correctly recorded rather than silently resolved.

## Accepted baseline

Implementation of `bead schema list|show|explain` may proceed against sections
1, 2, 3.1, 3.3, 4, 5, 7, 8, 9, and 10 as written, with these carve-outs, and
must track every required revision above:

- **Excluded:** section 3.2's declared member list (R2) and section 6's opening
  universality claim, event-kind examples, and `detail` default (R3, R10). Do
  not author the section 1 completeness test or the `documents[].members` array
  from the current text.
- **Corrected in place before use:** R1, R4, R5, R6, R8.
- **Resolve before the typed guide is generated:** R7, R9, R11.

Fleet rehydration remains blocked independently of this review, on `bf-4hr30`
and `bf-xq4ds`, per the artifact's own section 9.

## Re-review conditions

A revision addressing R1 through R11 and reconciling the HEAD staleness receives
a new SHA-256, a new `PROVENANCE.md` entry, and an unconditional-acceptance
review. None of the required revisions changes the artifact's model or its
section structure, so a third full round should not be necessary; a targeted
verification against the new hash is sufficient.
