# Checkpoint archaeology v1

Status: normative.

## Purpose and boundary

Checkpoint archaeology provides read-only inspection of retained native
checkpoint generations. It is not an import, restore, merge, or replay
mechanism. It never opens a workspace database and never writes a source
artifact. Every response is a partial derived view and is permanently
non-importable.

The supported commands are:

```text
bead query --checkpoint PATH (--file QUERY | --json QUERY)
bead sync diff A B
bead sync bisect --checkpoint PATH [--checkpoint PATH ...] (--file QUERY | --query QUERY)
```

`query --checkpoint` always emits its archaeology response as JSON, including
when `--output-json` is omitted. Historical saved views cannot be listed,
loaded, saved, or deleted: those operations address mutable workspace state and
are rejected with `--checkpoint`.

## Artifact selection and verification

`PATH`, `A`, and `B` may name one of these retained checkpoint artifacts:

1. a generation pointer;
2. a sharded manifest under `manifests/*.json`; or
3. a monolithic root under `objects/*.jsonl`.

A direct manifest or root is eligible only when the checkpoint-set's
`current.json` or `previous.json` selects it as `active_root`. The loader does
not infer an unretained generation from an object filename, modification time,
or directory ordering. A checkpoint directory means its `current.json`.

Before materializing a view, the loader MUST resolve the retained pointer and
run the existing named-generation verifier. That verifier checks the pointer,
root and complete sharded closure as appropriate; content-addressed names and
hashes; declared byte and record counts; record roles and parsing; native
schemas; canonical issue/event/receipt ordering; event continuity; and graph
integrity. The loader repeats the same verification after materialization. A
verification failure serves no view.

The materialized representation is an ephemeral in-memory view. It is never
accepted by import or restore, and serving it does not create a saved view,
event, receipt, checkpoint generation, or any other workspace mutation.

## Query

`bead query --checkpoint PATH` accepts the existing R004 query document. It
uses the same grammar, predicate validation, sort ordering, limit, and
projection semantics as a live query, evaluated against the historical issue
records in the ephemeral view. It does not read a live workspace, so it works
from any current directory.

The response contains the verified generation identity, mode, source store
UUID, snapshot sequence, root path and root SHA-256, together with `results`.

## Diff

`bead sync diff A B` independently verifies and materializes each selected
generation, then reports deterministic issue and event deltas. Issue identity
is the issue ID. Event identity is the pair
`origin_store_uuid:origin_event_sequence`.

For each identity, a delta is one of:

- `added`, with only `after`;
- `removed`, with only `before`; or
- `changed`, with both `before` and `after`.

Objects are compared as parsed JSON values, so object member order and output
formatting are not changes. The result has distinct `issue_deltas` and
`event_deltas` arrays, ordered lexically by identity.

## Series search

`bead sync bisect` accepts a caller-ordered series through repeatable
`--checkpoint`. It evaluates the supplied safe query against every verified
generation and reports every generation that has at least one matching issue,
including a count and deterministic issue-ID list.

The command scans rather than assuming a predicate is monotonic through time.
That makes it safe for arbitrary predicates (for example, a title or assignee
that can appear, disappear, and reappear) while still locating the historical
generation range an operator needs to inspect.

## Non-importability

Every archaeology response MUST include:

```json
{
  "artifact_kind": "bead-rs-checkpoint-archaeology-view-v1",
  "importable": false
}
```

`sync import-only` and `restore` MUST reject a document carrying an
`artifact_kind`, `kind`, or `$schema` containing `archaeology`, or carrying
`"importable": false`, before any target initialization or mutation. Writing a
response to a file does not make it a checkpoint source.

## Required conformance scenarios

1. A historical pointer query runs without a workspace and returns only issues
   from the selected generation.
2. A pointer-selected monolithic object and a pointer-selected sharded manifest
   both resolve to and serve their retained generation.
3. A changed issue and an added event appear in the corresponding semantic diff
   arrays; equivalent JSON with reordered object members does not appear.
4. A predicate series search reports every matching generation in caller order.
5. A hash-mismatched root or object is refused before query, diff, or series
   output.
6. Query, diff, and series outputs are refused by every import and restore
   path, with no target state changed.
