# ADR-015: Audited Historical Redaction Over Hand-Edited Recovery Artifacts

**Status**: Accepted

**Date**: 2026-09-03

**Decision-makers**: bead-rs maintainers

## Context

ADR-014 prevents a new detectable secret from entering the semantic store, but
older bead-rs versions already accepted arbitrary operator text. Because a
native checkpoint contains issue snapshots and the complete audit-event stream,
ordinary issue updates, reopen/reclose, or another flush cannot remove bytes
from a historical event. The retained previous generation can also keep a
removed value after the live store is corrected.

The motivating NEEDLE incident is `needle-27ec0073`: Forgejo rejected a
checkpoint-only push after its pre-receive scanner found sensitive-material
patterns in historical bead records. The rejection occurred after the bytes had
already reached SQLite, checkpoint objects, an ignored harness trace, and an
unpushed local Git object. No matched value is recorded in this ADR.

Direct SQLite or JSONL editing would make live state, event hashes, checkpoint
pointers, and restore behavior disagree. A repository-wide scanner allowlist
would conceal future leaks. Preserving the original bytes for audit would keep
the credential exposed. The system therefore needs one explicit exception to
normal historical immutability.

## Decision

bead-rs will provide a fingerprint-selected, audited historical-redaction
operation. It destroys only bytes selected by a current scanner finding,
preserves semantic record identity, records a nonsecret receipt, publishes a
sanitized checkpoint generation set, and prevents known removed content from
being reintroduced by recovery operations.

Redaction is not general editing or deletion. The caller supplies a finding
fingerprint, actor, and nonsecret reason; it never supplies or receives the
matched value. The implementation revalidates the fingerprint, replaces the
exact selected bytes with a fixed typed marker, and retains the issue/event ID,
origin sequence, lifecycle state, relationships, and unrelated bytes.

The redaction receipt records the rule, selector, field path, prior-content
fingerprint, actor, reason, time, and resulting generation identity. It never
contains removed content. Import, merge, reconcile, and restore must honor a
known receipt/tombstone over older matching content.

Recovery precedence recomputes a tombstoned finding from its stored ruleset
version, rule ID, semantic selector, field path, and byte range. It does not
depend on the current scanner rediscovering that finding, because a legitimate
ruleset release changes fingerprint identity and may retire a detector.

Checkpoint publication for a redaction epoch is exceptional: it may discard a
secret-bearing recovery generation instead of retaining it as `previous.json`.
The sanitized pointer becomes durable before dirty objects are tombstoned. A
post-commit publication failure is resumable and cannot repeat the semantic
redaction.

## Rationale

The semantic store is the only layer that can identify every materialized copy
and preserve recovery invariants while removing it. Fingerprint selection keeps
the value out of argv and diagnostics. A receipt retains accountability without
retaining the harmful evidence itself. Anti-resurrection state is necessary
because an older valid checkpoint is otherwise capable of reintroducing the
same bytes.

## Consequences

### Benefits

- Historical credentials can be removed without unsupported database surgery.
- The audit trail states that a redaction occurred without keeping the value.
- Current, previous, monolithic, and sharded checkpoint forms converge on the
  same sanitized semantics.
- Recovery cannot silently undo a known redaction.

### Drawbacks

- Event payload bytes are no longer absolutely immutable; identity and a
  receipt remain immutable instead.
- Discarding a dirty previous generation intentionally reduces local rollback
  depth for that redaction epoch.
- Implementations must coordinate SQLite, checkpoint locks, import semantics,
  and crash recovery, making this a security-sensitive maintenance operation.

### Alternatives Considered

- **Hand-edit SQLite and JSONL**: rejected because hashes, pointers, event
  identity, and restore equivalence would diverge.
- **Append a correction only**: rejected because scanners and attackers can
  still read the original bytes.
- **Broad gitleaks allowance**: rejected because it hides future credentials.
- **Delete the issue/event**: rejected because it destroys unrelated task and
  audit facts.
- **Encrypt historical values**: rejected because key management does not
  remove already published plaintext copies.

## Implementation

R038 and BR-T13 through BR-T18 in the current plan own the normative specs,
scanner, storage, transaction, CLI/checkpoint integration, conformance,
release, and NEEDLE remediation. Implementation may begin only after the exact
`historical-redaction-v1` specification receives independent review.

## Related

- [ADR-014](014-hard-reject-secret-bearing-mutations.md)
- [Checkpoint set specification](../../research/specs/checkpoint-set-v1.md)
- [Current plan R038](../plan/plan.md)

## Supersedes

None.
