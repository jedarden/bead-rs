# Independent Review Guide: Attempt Outcome v1

## Review scope

This specification defines the portable attempt-resolution contract for bead-rs.
It MUST be independently reviewed before any implementation begins.

## Review requirements

### 1. ADR compliance verification

For each of ADR-010, ADR-011, ADR-012, verify:
- [ ] The specification addresses every requirement in the ADR
- [ ] No ADR requirement is silently omitted or weakened
- [ ] The specification stays within the ADR's boundary

### 2. Plan section 4 requirements

Verify the specification satisfies:
- [ ] 4.1: Outcomes and actions are orthogonal and complete
- [ ] 4.2: Exactly-once receipt semantics are fully specified
- [ ] 4.3: Atomic transaction requirements are explicit

### 3. Technical completeness

Check that the specification includes:
- [ ] Attempt identity format and uniqueness constraints
- [ ] Canonical hash computation algorithm with example
- [ ] Complete outcome vocabulary and tier effects
- [ ] Complete action vocabulary and lifecycle effects
- [ ] Outcome-action compatibility matrix
- [ ] Revision and fencing conflict detection
- [ ] Idempotent replay behavior (identical and diverging)
- [ ] Failure epoch and tier progression rules
- [ ] Evidence reference form and privacy guarantees
- [ ] Checkpoint representation format
- [ ] Service API signature and error types
- [ ] CLI command shape and exit codes
- [ ] Capability negotiation format
- [ ] Schema URNs and evolution rules

### 4. Safety properties

Verify the specification guarantees:
- [ ] Exactly-once semantics per attempt_id
- [ ] No silent overwrite on replay divergence
- [ ] Atomic commit of outcome + lifecycle transition
- [ ] No evidence content stored in database
- [ ] No secret leakage through telemetry fields
- [ ] Complete audit trail for every resolution

### 5. Clean-room boundary

Confirm the specification:
- [ ] Uses no terminology from upstream implementations
- [ ] References no upstream SQL schema or internal names
- [ ] Contains no prose copied from other tools
- [ ] Independently specifies all algorithms and formats

### 6. Schema and fixture validation

Verify all fixtures:
- [ ] request.json is valid JSON Schema
- [ ] receipt.json is valid JSON Schema
- [ ] checkpoint-record.jsonl matches specification format
- [ ] audit-event.json matches event schema
- [ ] capabilities.json matches capability format
- [ ] All examples are consistent with each other

### 7. Conformance requirements

Check that the specification lists:
- [ ] All required test categories
- [ ] Fixture structure and format
- [ ] Verification script requirements
- [ ] Migration path compatibility

## Review decision options

### Option A: Approve
The specification is complete, correct, and ready for implementation.
All requirements are satisfied with no reservations.

### Option B: Conditional approval
The specification is fundamentally sound but requires minor revisions.
Specific, actionable changes are documented. Re-review after corrections.

### Option C: Major revisions required
The specification has significant gaps or errors.
Substantial rework is needed before re-review.

### Option D: Reject
The specification violates a fundamental requirement or boundary.
Do not proceed with implementation.

## Review recording

If you are the independent reviewer, record your decision in:

```
docs/reviews/attempt-outcome-v1-independent-review-YYYY-MM-DD.md
```

Include:
- Your identity and date
- Review option selected (A/B/C/D)
- For each requirement above: satisfied, gap, or concern
- Specific issues if choosing B/C/D
- Recommended changes for conditional approval
- Rationale for rejection (if D)

## Review artifact verification

Before concluding review, verify:
- [ ] You have read the complete specification
- [ ] You have examined all fixture files
- [ ] You have checked against ADR-010, ADR-011, ADR-012
- [ ] You have checked against plan section 4
- [ ] You have validated all schemas are well-formed
- [ ] You have confirmed clean-room boundary is maintained

## Provenance documentation

After review completion, record:
- Reviewer identity and date
- Specification checksum at time of review
- Fixture checksums at time of review
- Review decision and rationale
- Any required revisions and their completion

This becomes the provenance record for the specification.
