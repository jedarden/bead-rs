# Reconciliation report specification v1

Status: draft normative specification.

This specification defines the reviewable reconciliation report that an
agent emits after an agent-guided rehydration run (ADR-002). The report
accounts for every identifier in a source tracker and makes the run's
lossy or ambiguous decisions visible to a human reviewer.

The report is a review artifact. It is never native store input, never a
checkpoint record, and never an import source. That rule is restated
normatively in "Relationship to native store input" and is enforced by the
native checkpoint import paths.

## Normative language

The key words "must", "must not", "required", "should", and "may" are to
be interpreted as described in RFC 2119 when they appear in capital or
plain text in this specification.

## Overview

Moving work from another tracker into `bead-rs` is agent-guided
rehydration, not import. The agent reads the source repository without
modifying it, creates native beads exclusively through public `bead`
commands in the destination workspace, and then emits one report that
maps every source identifier to exactly one disposition. A reviewer uses
the report to answer three questions without re-reading the source:

1. Where did each source item go?
2. What was deliberately left behind, and why?
3. What could the agent not decide, and what does a human need to settle?

A run is complete only when its report parses, validates, and carries no
`unresolved` entries.

## Document identity and shape

A reconciliation report is a UTF-8 JSON Lines document. Its schema
identity is:

```text
urn:bead-rs:schema:reconciliation-report:v1
```

Every record carries that identity in `schema_ref`. The first non-blank
line must be the header record (`record_type` `header`); every following
non-blank line must be an entry record (`record_type` `entry`). Blank
lines are ignored. A report carries exactly one header.

This identifier is deliberately outside the native store schema family
(`urn:bead-rs:schema:issue:native-v1`,
`urn:bead-rs:schema:event:native-v1`, and the receipt identities). It is
bound by this specification and the in-repository validator, not by
`bead schema show`.

## Header record

The header identifies what was migrated, by whom, and into where.

| Field                   | Type   | Requirement                                   |
|-------------------------|--------|-----------------------------------------------|
| `record_type`           | string | required, exactly `header`                    |
| `schema_ref`            | string | required, the report schema identity          |
| `source_repository`     | string | required, nonempty; URL or path of the source repository |
| `source_commit`         | string | required, nonempty; commit identifier the export was taken at |
| `source_tracker`        | string | required, nonempty; tracker or profile name (for example `bf-v1`, `github-issues`) |
| `generated_at`          | string | required, RFC 3339 timestamp                  |
| `generator`             | string | required, nonempty; agent or tool identity that authored the report |
| `destination_workspace` | string | required, nonempty; the native workspace the run wrote to |
| `counts`                | object | required; per-disposition totals, see "Counts and closure" |

`source_repository` and `source_commit` are what make a report auditable:
a reviewer must be able to check out the exact source state the agent
saw. A run whose source moved mid-run must either pin the commit it
rehydrated from or be split into one report per commit.

Unknown additional fields are permitted in any record and must be
preserved by any tool that rewrites or merges reports; validators that
do not understand them must ignore them. Source-side context (labels,
milestones, original URLs) travels in these extension fields.

## Entry record

Each entry accounts for one source identifier.

| Field           | Type   | Requirement                                        |
|-----------------|--------|----------------------------------------------------|
| `record_type`   | string | required, exactly `entry`                          |
| `schema_ref`    | string | required, the report schema identity               |
| `source_id`     | string | required, nonempty, unique within the report       |
| `disposition`   | string | required, one of `native`, `omitted`, `merged`, `unresolved` |
| `target_bead`   | string | required exactly when disposition is `native`; otherwise must be absent |
| `merged_into`   | string | required exactly when disposition is `merged`; otherwise must be absent |
| `rationale`     | string | required, nonempty, for `omitted`, `merged`, and `unresolved`; optional for `native` |
| `source_title`  | string | optional human context copied from the source      |

## Dispositions

Every source identifier appearing in the source tracker's issue set at
`source_commit` must appear in exactly one entry, and every entry carries
exactly one disposition.

### `native`

A native bead was created for this source identifier through public
`bead` commands. `target_bead` must name that bead and must be a valid
native issue identifier (a reviewer verifies it with `bead show` in the
destination workspace). One native bead accounts for one source
identifier: two entries must not claim the same `target_bead`. A source
item absorbed into another item's bead is `merged`, not a second `native`
entry sharing the target.

### `omitted`

The source identifier was deliberately not migrated. `rationale` must
say why in terms a reviewer can act on — for example "completed and
archived upstream", "duplicate of LEGACY-101" (paired with `merged` when
another entry absorbed it), or "spam". An omission is a decision, not an
accident; a record the agent merely failed to read is `unresolved`.

### `merged`

The source identifier was absorbed into another entry's native bead.
`merged_into` must name the `source_id` of another entry in the same
report whose disposition is `native`. Chained merges are invalid: a
`merged` entry must not be the target of another merge, so every merge
resolves to a bead in one hop. `rationale` must state what made the two
items the same work.

### `unresolved`

The agent could not decide. `rationale` must state what is ambiguous and
what a reviewer must determine. These entries are the review queue. A
report containing any `unresolved` entry is not clean, and a runbook
must not close out a run against an unclean report; the reviewer either
settles each entry (the agent rewrites it as `native`, `omitted`, or
`merged`) or accepts the loss explicitly.

## Counts and closure

The header `counts` object carries exactly the fields `total`, `native`,
`omitted`, `merged`, and `unresolved`. Each must equal the tally of the
entry records, and `total` must equal the number of entries. A report
whose declared counts disagree with its entries is invalid, not merely
suspicious: the disagreement is itself a finding for review.

Entry order is not significant and should follow source order. Batching
a large migration produces one report per batch, each with its own
header; combining batches into a single report is a mechanical rewrite
that must recompute the header and keep `source_id` unique.

## Relationship to native store input

The report exists only for review. The following rules are normative:

- No `bead` command accepts a reconciliation report as input. Native
  recovery import (`bead sync import-only`) accepts only the exact
  self-describing native checkpoint formats emitted by `bead-rs`.
- The report schema identity is not a native store input identifier.
  Records carrying it are refused by the checkpoint import staging
  paths — monolithic JSONL, sharded objects, and the whole-document
  pre-classifier — with a dedicated refusal, before generic parse or
  unknown-record errors can misdescribe the situation.
- A report must not be written inside the destination workspace's
  `.beads/` tree, and never inside `.beads/checkpoint/`. Recommended
  locations are the run's documentation directory or the migration
  archive beside the preserved source artifact.
- Nothing in a report is read back into native state. Correcting a run
  means running public `bead` commands again (create, label, dep,
  close) and updating the report; it never means editing checkpoint
  data.

Rehydration reconstructs work through the public CLI precisely so that
transactions, validation, audit events, and derived readiness stay
intact. The report describes what was done; it is never the mechanism
that did it.

## Authoring workflow

The report is one step of the rehydration runbook (ADR-002
Implementation item 5):

1. Treat the source repository as read-only; pin `source_commit`.
2. Create or select a disposable destination workspace.
3. Rehearse the rehydration there using only public `bead` commands.
4. Emit the report beside the workspace, never under `.beads/`.
5. Compare counts and dependency intent against the source; run
   `doctor`; flush a native checkpoint.
6. Submit the report for human review; resolve every `unresolved`
   entry; only then repeat against the real destination or accept the
   run.

## Validation and conformance

A conforming report parses and validates under the in-repository
validator (`bead_rs::reconciliation::parse_report`). Validation enforces
every requirement marked above: header completeness and timestamp shape,
the exact schema identity, `source_id` uniqueness, the disposition-specific
presence and absence of `target_bead`, `merged_into`, and `rationale`,
one-hop merge resolution to a `native` entry, `target_bead` uniqueness,
and count agreement.

Fixture tests live in `tests/reconciliation_report.rs`. They cover a
valid all-dispositions report, one rejection case per rule, extension
preservation, and — at the binary level — the named refusal of a report
handed to `bead sync import-only` in restore-into-empty and merge modes.

## Related

- `docs/adr/002-agent-guided-rehydration-over-cross-tool-migration.md`
- `research/specs/schema-identification-v1.md`
- `research/specs/native-field-guide-v1.md`
- `docs/plan/plan.md` section 6 (agent-guided rehydration)
