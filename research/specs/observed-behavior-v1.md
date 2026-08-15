# Sanitized observable behavior report v1

Status: non-normative research input.

This report records process-boundary and file-format facts observed without
consulting implementation source. It informs compatibility profiles but does
not prescribe `bead-rs` internals. Where it conflicts with a normative
consumer contract, the profile or consumer contract decides the exposed
behavior.

## Method

On 2026-08-07, a specification activity invoked the publicly installed
`bf 0.4.0` executable in a new temporary workspace. It used help output,
created synthetic issues, added labels and a blocking dependency, claimed an
issue, flushed a checkpoint, and queried SQLite metadata through documented
SQLite pragmas. No source, tests, fixtures, SQL definitions, or implementation
documentation from another bead implementation were consulted or copied.

The synthetic workspace is disposable research material and is not an
implementation fixture. Conformance fixtures for this project must be authored
independently from the normative specifications.

## Sanitized facts

- Initialization creates a `.beads/` workspace containing configuration,
  metadata, a SQLite live store, a checkpoint path, and ignore rules.
- The observed default priority is `2`, default issue type is `task`, and
  priorities are ordered from `0` (highest) through `4` (lowest).
- Create accepts repeated labels. Exported labels have stable lexical order.
- Issue records visibly contain stable IDs, text fields, lifecycle status,
  priority, type, timestamps, optional assignment, labels, dependencies, and
  source-workspace information.
- A blocking edge has the semantic direction `(blocked, blocker, blocks)`.
  Adding an unfinished blocker makes the blocked issue report `blocked`.
- The observed dependency spelling is
  `dep add BLOCKED BLOCKER --kind blocks`. The NEEDLE v1 contract
  matches the shipped CLI spelling; this is not a profile difference.
- Claim accepts an assignee and optional model/harness telemetry. A successful
  claim returns a JSON object containing the selected ID and assignee, assigns
  the issue, and reports its lifecycle as `in_progress`.
- JSON output shapes vary by command: observed list output is a stream of JSON
  objects, show output is an array, and claim output is one object. The NEEDLE
  contract already permits these record-stream variations where applicable.
- A flush writes one JSON object per issue to `.beads/issues.jsonl` and embeds
  labels and dependency objects in the owning issue record.
- The live store represents issue data, labels, dependencies, comments,
  configuration, and metadata as structured SQLite data. This fact does not
  make its schema a compatibility API.
- Dependency and label deletion behavior is tied to issue lifetime. `bead-rs`
  may satisfy that semantic requirement with its own foreign-key design.

## Deliberately excluded details

Observed table names beyond the public issue concepts, column lists, index
names, SQL text, cache structures, migration machinery, internal error prose,
and implementation-specific bookkeeping are excluded from the implementation
specification. `bead-rs` uses the independent native design in
`docs/plan/plan.md`.

## Compatibility conclusions

1. Keep the canonical dependency tuple explicit everywhere.
2. Treat command argument order and output envelope as profile adapters.
3. Treat SQLite as private native state and JSONL as the interoperability
   boundary.
4. Test semantic equivalence rather than database equivalence.
5. Never write or migrate in place over another implementation's live store.
