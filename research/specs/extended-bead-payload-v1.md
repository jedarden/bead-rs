# Extended bead payload specification v1

Status: draft normative post-0.1 specification.

This specification defines portable comments, structured data, conditional
dependencies, and their diagnostic requirements. These extensions preserve the
private SQLite boundary and remain representable in the JSONL recovery backup.

## Comments and optional retrieval context

Comments are durable parts of a bead, stored as normalized native records and
embedded in that bead's JSON object when included. Each comment has a stable
ID, immutable body, author, creation instant, optional `reply_to` comment ID,
and resolution state. Reply relationships must stay within one bead and must
not form cycles.

The JSONL recovery backup always includes the complete comment history,
including resolved comments. A profile that cannot represent comments reports
that loss and is not a lossless recovery profile.

Interactive and machine retrieval keeps conversation context optional:

- `list` and `show` default to `--comments none`; the `comments` field is
  absent, while `comment_count` and `unresolved_comment_count` may be returned;
- `--comments unresolved` includes only unresolved threads and their ancestors;
- `--comments all` includes the complete ordered history;
- `sync --flush-only` ignores retrieval projection flags and always exports all
  comments.

Comment order is creation instant then stable ID. Excluding comments changes
only the response projection, never the bead or its backup.

## Structured data

A bead may contain a `data` object whose keys are namespace strings and whose
values use this envelope:

```json
{
  "data": {
    "com.example.release": {
      "schema_ref": "urn:example:schema:release:v1",
      "value": {"channel": "stable", "build": 42}
    }
  }
}
```

Namespace keys are unique, nonempty, at most 255 bytes, and use lowercase
reverse-domain or URN-like naming. `schema_ref` is an absolute immutable schema
URI. `value` may be any JSON value accepted by that schema. Unknown schema
references and values survive backup/import unchanged but fail closed for
native mutation or activation unless an adapter declares preservation-only
handling.

`bead data set ID NAMESPACE --schema SCHEMA_REF --value-json JSON`, `get`,
`list`, and `remove` are atomic and revision-guarded. Data schemas validate
values only: they cannot execute code, access the network, define SQLite, or
change lifecycle behavior.

## Conditional dependencies

A dependency retains canonical direction `(blocked, blocker, kind)` and may
carry an optional `condition`. No condition means existing behavior: a
`blocks` edge is active while its blocker is unfinished.

Conditions use a versioned JSON predicate AST with only:

- boolean composition: `all`, `any`, and `not`;
- subjects: `blocked` or `blocker`;
- fields: stored scalar lifecycle fields, labels, issue type, priority,
  assignee presence, and schema-bound `data` paths;
- operators: `exists`, `equals`, `not_equals`, `in`, `not_in`, numeric
  comparisons, and set containment.

Conditions cannot read effective readiness, other dependency results, comments,
wall-clock time, environment variables, SQL, files, or network state. Values
are type checked against the governing bead/data schemas. Evaluation is pure
and deterministic over the two committed bead snapshots.

An edge is active only when its condition evaluates true and its kind's normal
blocking semantics apply. Relevant issue/data mutations re-evaluate readiness
in the same transaction. Cycle detection conservatively treats every
conditional `blocks` edge as potentially active, preventing a latent cycle
from becoming active later.

CLI creation accepts `--condition-json` or a path to a condition document and
prints a normalized preview in dry-run mode. It never accepts executable
expressions or raw SQL.

## Diagnostic scopes

The existing `doctor` command gains machine-readable, composable scopes:

```text
bead doctor --scope store|backup|schema|dependencies|comments|all --format json
```

Scopes diagnose JSONL generation completeness/freshness, schema references and
data validation, malformed predicates and latent dependency cycles, comment
thread integrity, change-feed gaps, and restore provenance. Human warnings keep
the `WARN ` prefix. JSON diagnostics use stable codes and JSON Pointers.

`doctor --repair` remains narrowly allowlisted. It never invents comment
content, rewrites structured data to satisfy a schema, changes a conditional
predicate, removes an edge, or discards an unknown schema. Those findings
require an explicit user mutation.
