# ADR-002: Prefer Agent-Guided Rehydration Over Cross-Tool Schema Migration

**Status**: Accepted

**Date**: 2026-08-12

**Decision-makers**: bead-rs release owner

## Context

Historical bead tools have emitted several JSONL and SQLite shapes under the
same broad product names. Some checkpoints omit fields later treated as
required, some materialize derived state, and some encode relationships or
events differently. A profile name such as `bf-v1` cannot honestly describe
all of those artifacts. Accepting one observed corpus creates a misleading
expectation that the next repository can be converted without semantic loss.

The purpose of adopting `bead-rs` is to obtain a small, explicit native model
with transactional invariants and auditable mutations. A generic transformer
works against that purpose: it must guess which source dialect it received and
can produce structurally valid native records whose meaning is wrong.

Coding agents are already capable of reading a repository's public task
artifacts, CLI output, documentation, and Git history. What they lack is a
precise, discoverable explanation of the target model and a reviewable way to
account for each source item.

## Decision

Remove cross-tool migration commands, source-profile import adapters, and
migration receipts. Replace them with a versioned **native field guide** that
teaches an agent the meaning and authoring rules for every public bead field.

The guide is available through
`bead schema explain SCHEMA_REF --format json|markdown`. For each field it
states:

- semantic meaning and whether the field is stored, derived, or read-only;
- type, nullability, default, allowed values, and invariants;
- which CLI operations create or change it;
- interactions with lifecycle, readiness, dependencies, events, and revision;
- a minimal valid example and common interpretation mistakes.

The JSON form is a stable, versioned machine contract. The Markdown form is a
deterministic rendering of the same data for an agent's context. JSON Schema
remains the authority for structural validation; the field guide adds
operational semantics that JSON Schema cannot express clearly.

Moving work from another tracker is **agent-guided rehydration**, not import.
The agent reads the source repository without modifying it, creates native
beads exclusively through public `bead` commands, and produces a reconciliation
report mapping every source identifier to a native identifier or an explicit
`omitted`, `merged`, or `unresolved` disposition. The report records the source
repository and commit, but is not accepted as native store input.

No guide instructs an agent to write SQLite or synthesize native checkpoint
records. Native recovery import remains supported only for the exact,
self-describing native checkpoint version emitted by `bead-rs`.

## Rationale

The source tool owns the meaning of its data; `bead-rs` cannot infer that
meaning reliably from historical wire shapes. An agent can surface ambiguity,
combine repository context with task records, and ask for review instead of
silently coercing data. Restricting writes to the public CLI preserves native
transactions, audit events, revision rules, graph validation, and derived
readiness.

The field guide is useful beyond transition work: it gives every coding agent
a compact, version-matched description of the model without requiring it to
reverse-engineer help prose or implementation details.

## Consequences

### Benefits

- Eliminates an unbounded promise to recognize historical external schemas.
- Makes ambiguous or lossy decisions visible in a reconciliation report.
- Preserves native invariants by requiring all created work to use public CLI
  operations.
- Gives agents a reusable, versioned explanation of field semantics.
- Reduces clean-room and conformance surface by removing external adapters.

### Drawbacks

- Rehydration is slower and requires agent judgment and human review.
- Large repositories need batching, resumable reconciliation reports, and
  duplicate detection.
- Exact source history and source-only metadata may remain archived rather
  than becoming native state.
- The field guide must evolve with every public schema version and is part of
  the compatibility surface.

### Alternatives Considered

- **Continue adding external profiles.** Rejected because a small set of
  observed fixtures cannot bound the historical schema space or guarantee
  semantic equivalence.
- **Permit agents to generate native JSONL directly.** Rejected because it
  bypasses command-level validation and audit semantics and turns recovery
  input into another migration API.
- **Document fields only in prose.** Rejected because prose drifts, cannot be
  negotiated through capabilities, and is awkward for automated agents to
  validate against a running binary.
- **Expose only JSON Schema descriptions.** Rejected because structural schema
  keywords do not adequately explain derived fields, legal transitions, or
  the CLI operations that own each mutation.

## Implementation

1. Remove `bead migrate`, its service module, migration receipt schema, help,
   manuals, capabilities entries, and conformance lane.
2. Remove `br-v1` and `bf-v1` import/export adapters and claims. Keep
   `native-v1` recovery and the independently specified `needle-v1` subprocess
   contract.
3. Specify the native field-guide document and add immutable schema identity
   `urn:bead-rs:schema:field-guide:native-v1`.
4. Implement `schema explain` from one typed source shared by JSON and Markdown
   output; add completeness tests requiring every public issue field and
   lifecycle value to be described exactly once.
5. Document an agent-guided rehydration runbook with read-only source handling,
   CLI-only target writes, reconciliation-report format, dry rehearsal in a
   disposable workspace, `doctor`, and checkpoint flush.

Removal is a deliberate breaking change before 0.1. No compatibility alias or
hidden legacy parser remains.

## Related

- `docs/plan/plan.md` sections 5, 6, 11, and 15
- `research/specs/schema-identification-v1.md`
- `research/specs/needle-cli-contract-v1.md`

## Supersedes

The cross-tool migration and external-profile decisions previously recorded
directly in `docs/plan/plan.md`; no earlier ADR governed them.
