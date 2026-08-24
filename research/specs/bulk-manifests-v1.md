# Atomic bulk transaction manifests v1

R033 adds a versioned JSON manifest that materializes many bead-rs
mutations as one all-or-none transaction publishing at most one checkpoint
generation. This specification is independent of every interchange profile.

## Scope and vocabulary

A **manifest** is a JSON document held in one file. A manifest names
**operations**, each of which is exactly one existing command primitive:
`create`, `update`, `label add`, `label remove`, `dep add`, `dep remove`,
`close`. A manifest is executed by two read-only-dispatch commands:

  bead manifest dry-run --input <PATH> [--format text|json]
  bead manifest commit  --input <PATH> [--format text|json]

`dry-run` reports the full semantic delta and mutates nothing. `commit`
applies every operation in one SQLite transaction that either commits
completely or rolls back completely, and the automatic post-commit
publication chokepoint (R026) publishes at most one checkpoint generation
for the whole manifest. N commands remain N generations; one manifest is
one generation.

Version 1 is a **thin composition format**. It refuses any semantics a
single existing command does not already have: there are no conditionals,
loops, expressions, templating, variables beyond local references, batch
wildcards, query-scoped mutation, or new lifecycle/field semantics. Every
operation carries a subset of one command's arguments and fails exactly as
that command fails.

## Document format

The document is a single JSON object:

```json
{
  "manifest_version": 1,
  "operations": [ ... ]
}
```

`manifest_version` MUST be the integer `1`. Any other value, missing key,
or non-integer value is malformed input (exit 5). `operations` MUST be an
array of operation objects, possibly empty. An empty manifest is valid:
dry-run reports zero operations and commit succeeds as a no-op without
publishing a generation (no event advances).

Every operation object carries `"op"` naming its kind plus only the fields
listed for that kind below. Unknown operation kinds and unknown fields are
malformed input; parsing uses a closed schema, not field ignoring. Field
types must match exactly (for example `priority` must be an integer).

### `create`

```json
{"op": "create", "local_id": "a", "title": "...", "description": null,
 "priority": 2, "issue_type": "task", "assignee": null,
 "labels": ["docs"], "resource_keys": [], "unique_ref": null}
```

Only `op`, `local_id`, and `title` are required. Defaults and validation
are exactly `bead create`'s: `priority` defaults to 2 and MUST be 0-4;
`issue_type` defaults to `task`; `labels` and `resource_keys` default to
empty arrays; `unique_ref`, when present, uses `NAMESPACE:KEY` form and
makes the create idempotent exactly as `create --unique-ref` is — a bound
reference returns the existing bead (`outcome` `existing`, or
`existing_closed` when it is closed) instead of creating a duplicate, and
no later operation in the manifest is redirected to that existing bead
except through an explicit reference to its `local_id`, which resolves to
the existing bead's real ID.

### `update`

```json
{"op": "update", "id": "$a", "status": null, "assignee": null,
 "clear_assignee": false, "notes": null, "if_revision": null}
```

At least one of `status`, `assignee`, `clear_assignee`, `notes` MUST be
supplied; a bare `{"op": "update", "id": ...}` is malformed input. Values
and rules are exactly `bead update`'s: `status` accepts the same values
(`open`, `in_progress`, `deferred`, `blocked`) and rejects `closed` with
the use-`close` remedy; `assignee` and `clear_assignee` are mutually
exclusive; `if_revision` is the optimistic concurrency guard and fails
with the same conflict (exit 4) against the revision current at this
operation's position in the manifest. `--fencing-token` has no manifest
spelling in v1: an operation touching a leased issue fails closed exactly
as the command does without a token.

### `label add` / `label remove`

```json
{"op": "label_add", "id": "$a", "label": "docs"}
{"op": "label_remove", "id": "bead-0123abcd", "label": "docs"}
```

Exactly `bead label add|remove` semantics: idempotent, validated label
strings, `label_added`/`label_removed` events only on a real change.

### `dep add` / `dep remove`

```json
{"op": "dep_add", "blocked": "$a", "blocker": "$b", "kind": "blocks"}
{"op": "dep_remove", "blocked": "$a", "blocker": "bead-0123abcd", "kind": "blocks"}
```

`kind` defaults to `blocks` for `dep_add` and is optional for `dep_remove`
exactly as the command's `--kind` is. Self-edges and `blocks` cycles are
rejected with the command's conflicts; because operations apply in order,
a cycle check sees edges earlier operations in the same manifest added.

### `close`

```json
{"op": "close", "id": "$a", "reason": "done", "if_revision": null}
```

Exactly `bead close` semantics: `reason` required and non-empty,
idempotent re-close with the same reason, conflict on a different reason,
resource locks released, `if_revision` supported.

## Local references

A `create` operation MAY carry `local_id`, a caller-chosen name for the
bead it creates. A `local_id` MUST be 1-64 bytes of ASCII letters,
digits, `_`, `.`, or `-`, MUST NOT contain `$`, and MUST be unique within
the manifest. Because workspace prefixes are `[a-z][a-z0-9-]*`, a string
beginning with `$` can never be a real bead ID.

Any issue-naming field of any operation (`id`, `blocked`, `blocker`)
accepts either a real bead ID or a local reference written `$name`. A
local reference resolves to the real ID of the bead the referenced
`create` produced — including an existing bead returned by a `unique_ref`
hit. A reference to a `local_id` that no earlier `create` operation
defined is malformed input: references are forward-forbidden, so every
reference resolves before any operation executes. A real ID that does not
exist in the workspace is not malformed input; it fails at its operation's
position with the command's not-found error (exit 3).

Local references are the only indirection in v1. They are not variables:
nothing else can hold one, and they cannot be redefined.

## Validation order

1. **Document validation** — read the file, parse JSON, check
   `manifest_version`, closed-schema parse every operation, check
   `update` supplies a field, validate `local_id` syntax and uniqueness,
   resolve every `$name` against earlier `create` operations. Any failure
   here is malformed input (exit 5) and nothing is executed; a
   dry-run reports the same failure a commit would.
2. **Execution validation** — operations apply in array order inside one
   IMMEDIATE transaction. Each operation runs its command's own
   validation at its position: existence, transitions, revision guards,
   lease refusal, label rules, cycle detection. Later operations see
   earlier operations' uncommitted effects. A failure fails the whole
   manifest with that operation's index and kind prepended to the
   command's own error, preserving the command's exit code.

## Dry-run

`manifest dry-run` performs document validation, then executes the whole
manifest inside one IMMEDIATE transaction that is **always rolled back**.
The report is therefore exact: every conflict, guard, and cycle the commit
would hit is hit by the dry-run against the same pinned snapshot. The
transaction appends no events durably, advances no sequence, and
publishes no generation.

The JSON report is:

```json
{"manifest_version": 1, "dry_run": true, "operations": 2,
 "semantic_changes": 2, "workspace_sequence": 14,
 "results": [ ...one object per operation... ]}
```

Each result names the operation's index, kind, resolved target IDs, an
`outcome`, whether it was a `semantic_change`, and for state operations a
`changes` object of before/after field deltas over the fields `update` and
`close` can move (base status, assignee, manual blocked, close reason,
revision, label set; notes report only a `notes_changed` boolean, never
their content). For
`create` it reports the projected issue. Created IDs in a dry-run are
**provisional**: identifiers are generated per execution and a dry-run's
IDs are never commitments. Only the commit's result map carries real IDs;
callers correlate through `local_id`.

## Atomic commit and publication

`manifest commit` performs document validation, then executes every
operation in array order inside one IMMEDIATE transaction and commits
once. Any failure rolls back everything: no partial issue, label, edge,
event, lock, or revision survives. The event stream the commit leaves is
exactly the union of the events the equivalent individual commands would
append — v1 adds no manifest-level event kind.

Because the whole manifest is one command invocation committing one
transaction, the R026 chokepoint publishes at most one generation
covering the manifest's entire event span, or none when the manifest
changed nothing semantic (the covered-sequence rule suppresses the
no-op). `--no-auto-flush` and `checkpoint.auto_flush` suppress
publication exactly as for any command. A post-commit publication failure
is the standard split outcome: the manifest stays committed, exit 1, and
`sync flush-only` is the remedy.

## Result map

`manifest commit --format json` prints one object: the same shape as the
dry-run report with `dry_run` false plus `committed: true`, and every
result carrying real IDs. For `create` results the `local_id -> issue_id`
link is explicit, so automation can name beads it just materialized:

```json
{"manifest_version": 1, "committed": true, "dry_run": false,
 "operations": 2, "semantic_changes": 2, "workspace_sequence": 16,
 "results": [
   {"index": 0, "op": "create", "local_id": "a",
    "issue_id": "bead-9f2c1a07", "outcome": "created",
    "semantic_change": true, "issue": {...}},
   {"index": 1, "op": "dep_add", "blocked": "bead-9f2c1a07",
    "blocker": "bead-0123abcd", "kind": "blocks", "outcome": "added",
    "semantic_change": true}]}
```

`outcome` values are `created`, `existing`, `existing_closed` (create);
`updated`, `closed`, `no-op` (update, close — `updated` for a real update,
`closed` for a real close, `no-op` when the command's own idempotence made
the operation change nothing); `added`, `removed`, `no-op` (labels,
dependencies). Text format prints one line per operation with the same
information.

## What version 1 refuses

- Any operation kind or field not listed above, including `reopen`,
  `release`, `claim`, external references, structured data, recurrence,
  query-scoped mutation, and per-operation fencing tokens.
- Any control flow: conditionals, loops, includes of other manifests,
  templating, or computed values.
- Mutation of fields immutable after create (title, description,
  priority, issue type) — the same refusal `bead update` gives.
- Reading a manifest from anywhere but a filesystem path named by
  `--input`.
- Archaeology views and any checkpoint artifact as a manifest source.

A v2 may add operations or control flow only as a new
`manifest_version` with its own specification; version 1 readers reject
every other version outright.
