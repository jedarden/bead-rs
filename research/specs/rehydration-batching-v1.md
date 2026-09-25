# Rehydration batching specification v1

Status: draft normative specification.

This specification defines how an agent-guided rehydration run (ADR-002)
is segmented so that a large source repository can be migrated in
restartable batches. It adds no wire format: every batch is recorded as
an ordinary reconciliation report under
`urn:bead-rs:schema:reconciliation-report:v1`, using the extension-field
freedom that format already guarantees, and every destination mutation
goes through public `bead` commands. The report remains a review
artifact and is never native store input.

## Normative language

The key words "must", "must not", "required", "should", and "may" are to
be interpreted as described in RFC 2119 when they appear in capital or
plain text in this specification.

## Run identity

A run is identified by four values, each carried in the report header:

| Field                   | Meaning                                          |
|-------------------------|--------------------------------------------------|
| `source_repository`     | URL or path of the source repository             |
| `source_commit`         | commit the source issue set was read at          |
| `source_tracker`        | tracker or profile name of the source            |
| `destination_workspace` | native workspace the run writes to               |

All batches of one run share these four values. A run whose source
commit differs is a different run: a moved source is handled per commit,
never by resuming across commits, so a resumed session can trust that
the source identifiers it sees mean what the prior session saw.

## Batch segmentation

- The source tracker's issue set at `source_commit` is partitioned into
  batches: every source identifier in exactly one batch, no overlap, no
  identifier dropped. This mirrors the report rule that every source
  identifier appears in exactly one entry.
- `batch_id` values must be stable across sessions and derived, not
  free prose: `batch-01`, `batch-02`, and so on, zero-padded to a fixed
  width for the run, so lexical order equals processing order.
- Batch size is bounded by review capacity, not by a magic constant. A
  batch should be small enough that a reviewer can settle its
  `unresolved` entries in one sitting, and large enough that its
  checkpoint flush and report write are amortizable. A batch that
  regularly produces more than one review session's worth of
  `unresolved` entries is too large; split it.
- Segmentation must be deterministic from the sorted source-identifier
  set and the batch size, so a resumed session re-derives the same
  partition instead of trusting remembered state.
- Batches are processed in `batch_id` order. Dependency intent across a
  batch boundary is recorded when the entry is written and wired with
  `bead dep add` as soon as both endpoint beads exist; the combined
  report's review compares dependency intent against the source, as the
  rehydration runbook already requires.

## Per-batch run loop

Each batch follows the rehydration runbook (ADR-002 Implementation
item 5) and adds:

1. Create each native bead with
   `bead create --unique-ref <source_tracker>:<source_id>`, binding the
   source identity in the same transaction that creates the bead (R032).
   Recommended labels: `rehydration` and the `batch_id`, which make
   cross-session inventory a plain list scan.
2. Write the entry into the batch's report file when the decision is
   made, not when the batch ends. The report line is the decision
   ledger; deferring it re-opens decisions an interruption already
   settled.
3. Keep the report outside `.beads/`, one file per batch, named for its
   `batch_id` (for example `reconciliation-batch-01.jsonl`).
4. Close the batch: `bead sync flush-only` must be clean, the batch
   report must parse and validate under the report specification's
   rules, and the header counts must agree with the entries.

A batch report is complete when every source identifier assigned to
that batch appears in it. `unresolved` entries are permitted in a batch
report; they are only required to be settled in the combined report.

## Restarting after an interrupted session

A resumed session performs this inventory before deciding anything:

1. Confirm the run identity — same `source_repository`,
   `source_commit`, `source_tracker`, and `destination_workspace`. A
   mismatch is a new run, not a resume.
2. Read every report file already written for the run. Each entry
   present is a decided disposition: carry it forward verbatim; never
   re-derive it. Only `unresolved` entries may be rewritten, and only
   to settle them after review.
3. For each still-undecided source identifier, query the creation
   ledger before creating anything:
   `bead ref find --namespace <source_tracker> --value <source_id>`.
   A hit names the bead the interrupted session already created; the
   entry is recorded as `native` citing that bead, and the decision is
   not repeated.
4. Re-derive the batch partition from the sorted source-identifier set
   and the recorded batch size; continue the current batch.

The decision ledger and the creation ledger bracket every interruption
window: a report line written but a create lost is decided-but-pending
(step 3 finds nothing; the create is simply performed), and a create
committed but a report line lost is found by step 3. The `EXISTING` /
`EXISTING_CLOSED` result of a repeated `create --unique-ref` is the
terminal guard: even if both ledgers are stale, the store returns the
existing bead instead of duplicating it, and `EXISTING_CLOSED` stops
retrying finished work.

## Duplicate screening before creation

Duplicate prevention is structural (R032), and duplicate detection by
title similarity is rejected (ADR-008). What an agent runs before
creating a bead for a source identifier is screening: read-only queries
whose hits inform the agent's judgment. The store never infers
duplication from titles, and neither does this procedure.

In priority order:

1. **Source identifier first.**
   `bead ref find --namespace <source_tracker> --value <source_id>`.
   This is the only query that can establish identity, because it
   matches the same binding `--unique-ref` creates. A hit normally
   means the item is already migrated; record `native` or `merged`
   citing the found bead.
2. **Exact and substring title probes** when no stable identifier
   exists, through the safe query language (R004):

```text
bead query --json '{"version":"v1","predicates":[{"field":"title","operator":"equals","value":"<exact source title>"}],"sort":[],"limit":20}' --output-json
bead query --json '{"version":"v1","predicates":[{"field":"title","operator":"contains","value":"<distinctive fragment>"}],"sort":[],"limit":20}' --output-json
```

   A probe hit is evidence, not a verdict: the agent reads the hits,
   decides whether the work is the same, and records that decision and
   its reason in the report entry. Probe operators are the query
   grammar's own — `equals`, `contains`, `starts_with`, `ends_with` —
   and there is no similarity score; stacking substring probes into a
   pseudo-threshold is title inference under another name and must not
   be done.
3. **Label and reference scans.** `bead list --json` with a client-side
   scan over `labels`, and `bead ref list --id <ID>` on any candidate,
   to see what else references it. Labels are not a query predicate, so
   this is a list scan, not a store query.

Every non-`native` disposition reached through a title probe must say
so in its `rationale`, so a reviewer can weigh a text-match decision as
text-match evidence.

## Combining batch reports

Combining a run's batch reports into one report for review is a
mechanical rewrite, not a re-decision:

- exactly one header, with `counts` recomputed, `generated_at` of the
  combination, and the run identity fields unchanged;
- all entries concatenated, and `source_id` must remain unique — a
  duplicate is a segmentation bug and fails validation;
- every extension field is preserved, so `batch_id` provenance survives
  combination;
- the entry set and dispositions must be exactly what the batch reports
  contained: nothing is re-decided, renumbered, or dropped.

The combined report is the review artifact. It must carry no
`unresolved` entries before the run is closed out; the reviewer settles
them against the batch reports, which remain the audit trail of what
was decided when.

## Relationship to native store input

Unchanged by batching: no `bead` command accepts a batch report or a
combined report as input; reports never live under `.beads/`; all
destination state is created through public `bead` commands. Batching
changes how much work one report accounts for, never how the work is
performed.

## Related

- `docs/adr/002-agent-guided-rehydration-over-cross-tool-migration.md`
- `docs/adr/008-no-title-similarity-duplicate-detection.md`
- `research/specs/reconciliation-report-v1.md`
- `research/specs/bulk-manifests-v1.md` (`unique_ref` semantics and
  atomic multi-command materialization)
- `docs/plan/plan.md` R004 (safe query language), R011 (external
  references), R032 (idempotent create by unique reference)
