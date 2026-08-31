# Attempt Outcome Contract v1

Status: proposed normative specification.

Original author: bead-rs maintainers, 2026-08-31.

Artifact identity: `urn:bead-rs:spec:attempt-outcome:v1`.

This specification defines the portable attempt-resolution contract for bead-rs.
It establishes the schema, semantics, idempotency, and checkpoint behavior for
recording execution attempt outcomes atomically with lifecycle transitions.

Implementation may begin only after independent review records an acceptance
decision against this specification's exact SHA-256.

## Compliance with ADRs

This specification implements the requirements of:

- **ADR-010**: Store Attempt Facts, Not Learning or Orchestration Policy
- **ADR-011**: Resolve an Attempt and Its Lifecycle Transition Atomically
- **ADR-012**: Roll Out Attempt Resolution Through Versioned Capabilities

It also satisfies plan section 4 (Portable Attempt-Outcome Contract) and
section 5 (Capability and Compatibility Contract).

## 1. Scope and vocabulary

An **attempt** is a discrete unit of work performed by an actor against a specific
issue. An **outcome** is the caller-selected classification of that work's result.
An **action** is the requested lifecycle transition to apply atomically with the
outcome.

The **resolution** operation commits one outcome receipt and one lifecycle action
in one SQLite transaction. It guarantees exactly-once semantics for a given
canonical request hash.

This contract specifies only portable facts required for concurrency-safe
attempt resolution. It does NOT govern:
- prompts, transcripts, or model reasoning
- evidence quality or verification semantics
- learning policy, lessons, or experiments
- orchestrator scheduling or routing
- external side-effect execution

Those concerns remain the responsibility of the caller (NEEDLE or another
consumer). bead-rs validates schema and concurrency but does not interpret
evidence or policy.

## 2. Attempt identity

### 2.1 Attempt ID

The `attempt_id` is a caller-provided opaque identifier. It MUST:

- Be a non-empty UTF-8 string
- Not exceed 255 bytes
- Contain no control characters, NUL, or leading/trailing whitespace
- Be unique within a workspace for a given issue

Recommended format: `urn:namespace:specific-value` where the caller controls
the namespace. Example: `urn:needle:attempt:a1b2c3d4`.

The `attempt_id` is unique-per-workspace across all issues. Reusing the same
`attempt_id` for different issues constitutes a conflict and MUST be rejected.

### 2.2 Canonical request hash

The **canonical request hash** is a SHA-256 digest computed over the complete
resolution request in deterministic order. It includes:

1. `attempt_id` (UTF-8 bytes)
2. `issue_id` (UTF-8 bytes)
3. `outcome` (UTF-8 bytes)
4. `action` (UTF-8 bytes, or "none" if omitted)
5. `reason` (UTF-8 bytes, or "" if omitted)
6. Expected revision integer (or 0 if omitted)
7. Fencing token (UTF-8 bytes, or "" if omitted)
8. Sorted evidence references (each as "NAMESPACE:VALUE")
9. Bounded metadata fields in canonical JSON order

Hash computation:
```
input = attempt_id + "\x00" +
        issue_id + "\x00" +
        outcome + "\x00" +
        action + "\x00" +
        reason + "\x00" +
        itoa(revision) + "\x00" +
        fencing_token + "\x00" +
        joined(evidence_refs, "\x00")
canonical_hash = sha256(input)
```

The first successful resolution with a given `attempt_id` commits this hash as
the canonical form. Subsequent resolutions with the same `attempt_id` MUST:
- Return the original receipt if the hash matches (idempotent replay)
- Conflict if the hash differs (semantic divergence)

## 3. Outcome classification

Outcomes are versioned vocabulary selected by the caller. v1 defines:

| Outcome | attempt_tier effect | Meaning |
|---------|-------------------|---------|
| `verified_success` | No change | Caller asserts work completed successfully |
| `work_failure` | Increment consecutive failures | Bead-scoped failure: invalid assumptions, repeatable test failure |
| `infrastructure_failure` | No change | Worker crash, provider outage, rate limit, network loss |
| `cancelled` | No change | Explicit cancellation or interruption by operator |
| `indeterminate` | No change | Unable to determine outcome; requires manual review |

Outcome strings MUST match exactly. Unknown outcomes are usage errors (exit 2).

Only `work_failure` increments the issue's `consecutive_failures` counter and
advances its `attempt_tier` according to section 6.

## 4. Lifecycle actions

Actions determine the issue's resulting base lifecycle state. v1 defines:

| Action | Valid from | Resulting state | Effect |
|--------|------------|----------------|--------|
| `close` | `open`, `in_progress`, `deferred` | `closed` | Set closed_at, store close_reason |
| `release` | `in_progress` | `open` | Clear assignee, retain state |
| `quarantine` | any non-closed | unchanged state | Set attempt_tier=3, set retry_after |
| `block` | any non-closed | unchanged state | Set manual_blocked=true |
| `none` | any | unchanged | No lifecycle transition |

Actions MUST be legal lifecycle transitions. Invalid transitions are conflicts
(exit 4). The validation occurs BEFORE the outcome is committed.

`quarantine` sets `attempt_tier` to 3 and optionally sets `retry_after` to a
claim sequence number. A quarantined issue is not eligible for automatic claim.

## 5. Outcome-action compatibility

Not all outcome-action combinations are meaningful. Validations:

1. `verified_success + close` - Valid, normal completion
2. `verified_success + none` - Valid, partial progress
3. `verified_success + release` - Valid, yielding the claim
4. `work_failure + close` - Valid, abandoning after failure
5. `work_failure + quarantine` - Valid, escalating failure
6. `work_failure + release` - Valid, yielding after failure
7. `infrastructure_failure + none` - Valid, preserving claim
8. `infrastructure_failure + release` - Valid, yielding without penalty
9. `cancelled + close` - Valid, abandonment
10. `cancelled + release` - Valid, yielding cancellation
11. `indeterminate + block` - Valid, requiring manual review
12. `indeterminate + release` - Valid, yielding for re-attempt

Combination validation is closed: unknown pairs are rejected before mutation.

## 6. Failure epoch and tier semantics

### 6.1 Attempt tiers

The `attempt_tier` field is 0-3:

| Tier | Name | Eligibility |
|------|------|-------------|
| 0 | unproven | Fully eligible |
| 1 | retryable | Elapsed after comparable unproven work |
| 2 | struggling | Eligible in retry lane if configured |
| 3 | quarantined | Ineligible for automatic claim |

The tier is persisted per issue in the `issues` table.

### 6.2 Failure epoch boundaries

A **failure epoch** is a readiness revision boundary. The epoch advances when:

- Issue description changes materially
- Acceptance criteria or structured task data change
- Dependencies are added or removed
- Other semantic mutation that makes prior failures irrelevant

Cosmetic changes (notes, labels) do NOT advance the epoch.

When the epoch advances, `consecutive_failures` MAY reset to 0 while lifetime
attempt history is retained in audit events.

### 6.3 Tier progression

On `work_failure` outcome:
```
consecutive_failures = consecutive_failures + 1
if consecutive_failures == 1:
    attempt_tier = 1  # retryable
elif consecutive_failures == 2:
    attempt_tier = 2  # struggling
    retry_after_claim_sequence = current_claim_sequence + RETRY_DELAY
elif consecutive_failures >= 3:
    attempt_tier = 3  # quarantined
```

`RETRY_DELAY` is versioned workspace configuration, default 10.

Other outcomes do NOT modify `attempt_tier` or `consecutive_failures`.

## 7. Revision and fencing conflicts

### 7.1 Expected revision

The resolution may optionally supply `--if-revision N`. This is an optimistic
concurrency guard:

- If the issue's current `updated_at` revision equals N, proceed
- If the issue's current revision differs, conflict (exit 4)
- If omitted, no revision check is performed

The revision guard protects against silent overwrites of concurrent mutations.

### 7.2 Fencing token

When R002 (leases) is active, the resolution may optionally supply a fencing
token. Validation:

- Token must match the active lease's fencing_token
- Mismatch is a conflict (exit 4)
- Omitted token is accepted only if no active lease exists

Fencing ensures only one process holds the claim during resolution.

## 8. Idempotent replay

### 8.1 Identical replay

A replay with the same `attempt_id` and same canonical request hash MUST:

1. Return the original receipt ID and resulting state
2. NOT create a new attempt-outcome record
3. NOT append a new audit event
4. NOT advance the revision or event sequence
5. Exit 0 (success)

This is **exact idempotency**: the operation is safe to retry after an unknown
response.

### 8.2 Conflicting replay

A replay with the same `attempt_id` but different canonical hash MUST:

1. Fail with conflict (exit 4)
2. NOT mutate any state
3. Return a clear error naming the conflict type

The error message MUST include:
- The existing attempt's outcome
- The existing canonical hash (abbreviated)
- The new canonical hash (abbreviated)

This prevents silent divergence of attempt semantics.

## 9. Evidence references

### 9.1 Form and bounds

Evidence references are opaque identifiers pointing to external artifacts:
test results, coverage reports, build logs, or other side-effect receipts.

Form: `NAMESPACE:VALUE` where:
- `NAMESPACE` is `[a-z][a-z0-9-]*` (1-32 chars)
- `VALUE` is UTF-8, 1-255 bytes, no control characters

Example: `s3:build-logs/a1b2c3d4.tar.gz`

Zero or more evidence refs MAY be supplied. They are stored unchanged and
never validated or fetched by bead-rs.

### 9.2 Privacy and opacity

Evidence refs are stored as opaque strings. bead-rs:

- MUST NOT dereference or validate external refs
- MUST NOT infer evidence type from namespace
- MUST NOT store evidence content in the database
- MUST preserve refs in checkpoint round-trips

The caller maintains evidence availability and interpretation.

## 10. Bounded metadata

### 10.1 Actor identity

`actor` is a UTF-8 string identifying the caller. Maximum 255 bytes.
Examples: `needle-worker-alpha`, `user@hostname`.

### 10.2 Harness and tool hints

Optional bounded fields for telemetry and diagnostics:
- `model`: Model identifier, max 64 bytes
- `harness`: Execution harness name, max 64 bytes
- `harness_version`: Harness version, max 32 bytes

These are stored but do NOT affect scheduling or resolution semantics.
They are telemetry only.

## 11. Checkpoint representation

### 11.1 Attempt-outcome record

The checkpoint contains a dedicated record type for attempt outcomes:

```json
{
  "record_type": "attempt_outcome",
  "attempt_outcome": {
    "schema_ref": "urn:bead-rs:schema:attempt-outcome:native-v1",
    "attempt_id": "urn:needle:attempt:a1b2c3d4",
    "issue_id": "bead-0123456789abcdef",
    "outcome": "verified_success",
    "action": "close",
    "reason": "All tests passing",
    "canonical_request_hash": "a1b2c3...",
    "resulting_issue_revision": 42,
    "resulting_state": "closed",
    "resulting_attempt_tier": 0,
    "receipt_id": "ao-1234567890abcdef",
    "actor": "needle-worker-alpha",
    "created_at": "2026-08-31T12:34:56.789123456Z",
    "evidence_refs": [
      "s3:build-logs/a1b2.tar.gz",
      "coverage:report-xyz.html"
    ],
    "model": "claude-opus-5",
    "harness": "needle",
    "harness_version": "1.2.3"
  }
}
```

### 11.2 Audit event

A corresponding audit event is appended:

```json
{
  "event_type": "attempt_resolved",
  "issue_id": "bead-0123456789abcdef",
  "attempt_id": "urn:needle:attempt:a1b2c3d4",
  "outcome": "verified_success",
  "action": "close",
  "receipt_id": "ao-1234567890abcdef",
  "prior_attempt_tier": 0,
  "resulting_attempt_tier": 0,
  "resulting_state": "closed",
  "actor": "needle-worker-alpha"
}
```

The event identity and hash are stored for forensic replay.

### 11.3 Unknown field preservation

During checkpoint round-trip:
- Unknown fields in attempt-outcome records are preserved
- New versions MAY add fields; old readers preserve them as unknown
- Unknown evidence refs are preserved without validation

Preservation follows the same rules as checkpoint-set-v1.

## 12. Service API

### 12.1 Resolve operation

The service exposes:

```rust
pub fn resolve_attempt(
    &mut self,
    request: ResolveRequest,
) -> Result<ResolveReceipt, ResolveError>
```

`ResolveRequest` fields:
- `attempt_id`: String (required)
- `issue_id`: String (required)
- `outcome`: String (required)
- `action`: Option<String> (default "none")
- `reason`: Option<String> (default "")
- `if_revision`: Option<i64>
- `fencing_token`: Option<String>
- `evidence_refs`: Vec<String>
- `actor`: String (required)
- `model`: Option<String>
- `harness`: Option<String>
- `harness_version`: Option<String>

`ResolveReceipt` fields:
- `receipt_id`: String
- `canonical_request_hash`: String
- `issue_id`: String
- `attempt_id`: String
- `resulting_issue_revision`: i64
- `resulting_state`: String
- `resulting_attempt_tier`: i64
- `created_at`: String (RFC 3339)
- `is_replay`: bool

`ResolveError` variants:
- `UsageError` - Invalid request (exit 2)
- `NotFoundError` - Issue doesn't exist (exit 3)
- `ConflictError` - Revision/fencing/semantic mismatch (exit 4)
- `IntegrityError` - Database corruption (exit 5)
- `TransientError` - Lock contention (exit 6)

### 12.2 Transaction boundary

The operation runs in one `BEGIN IMMEDIATE` transaction that:

1. Validates ownership and concurrency guards
2. Computes canonical hash
3. Checks for existing attempt (replay/conflict)
4. On first resolution:
   a. Inserts attempt-outcome record
   b. Updates attempt_tier if required
   c. Applies lifecycle action
   d. Appends audit event
   e. Returns receipt
5. On identical replay:
   a. Returns existing receipt
   b. Advances no sequence
6. On conflicting replay:
   a. Returns conflict error
   b. Mutates nothing

After commit succeeds, automatic checkpoint publication (R026) may run.
Publication failure is reported but does NOT roll back the committed mutation.

## 13. CLI contract

### 13.1 Command shape

```bash
bead resolve ISSUE_ID \
  --attempt-id ATTEMPT_ID \
  --outcome OUTCOME \
  [--action ACTION] \
  [--reason TEXT] \
  [--if-revision REVISION] \
  [--fencing-token TOKEN] \
  [--evidence-ref REF]... \
  [--actor ACTOR] \
  [--model MODEL] \
  [--harness HARNESS] \
  [--harness-version VERSION] \
  [--format json]
```

Required arguments: `ISSUE_ID`, `--attempt-id`, `--outcome`.
Default `--action` is "none". Default `--reason` is "".

### 13.2 JSON output

On success, stdout is a JSON object:

```json
{
  "receipt_id": "ao-1234567890abcdef",
  "attempt_id": "urn:needle:attempt:a1b2c3d4",
  "issue_id": "bead-0123456789abcdef",
  "outcome": "verified_success",
  "action": "close",
  "canonical_request_hash": "a1b2c3...",
  "resulting_issue_revision": 42,
  "resulting_state": "closed",
  "resulting_attempt_tier": 0,
  "created_at": "2026-08-31T12:34:56.789123456Z",
  "is_replay": false
}
```

On replay, `is_replay` is `true` and the receipt is the original.

### 13.3 Exit codes

| Exit | Meaning |
|------|---------|
| 0 | Success (including idempotent replay) |
| 2 | Usage or validation error |
| 3 | Issue not found |
| 4 | Conflict (revision, fencing, outcome divergence) |
| 5 | Integrity failure |
| 6 | Database busy or transient I/O error |
| 1 | Uncategorized internal error |

## 14. Capability negotiation

### 14.1 Capabilities document

The `bead capabilities --format json` document gains:

```json
{
  "attempt_outcome": {
    "schema_version": "v1",
    "schema_ref": "urn:bead-rs:schema:attempt-outcome:native-v1",
    "supported": true,
    "outcomes": [
      "verified_success",
      "work_failure",
      "infrastructure_failure",
      "cancelled",
      "indeterminate"
    ],
    "actions": [
      "close",
      "release",
      "quarantine",
      "block",
      "none"
    ],
    "atomic_resolve": true,
    "idempotent_replay": true,
    "revision_guard": true,
    "fencing_guard": true,
    "evidence_refs": true,
    "checkpoint_representation": "attempt-outcome-record"
  }
}
```

### 14.2 Capability testing

Consumers MUST check capability before using resolution:

```bash
bead capabilities --format json | jq -e '.attempt_outcome.supported'
```

If unsupported, the consumer MUST use a legacy reconciliation sequence and
MUST NOT claim atomic semantics.

## 15. Schema references

### 15.1 Public schemas

This contract references these immutable schemas:

- `urn:bead-rs:schema:attempt-outcome:native-v1` - Attempt outcome record
- `urn:bead-rs:schema:resolve-receipt:native-v1` - Resolution receipt
- `urn:bead-rs:schema:resolve-request:native-v1` - Resolution request

Schemas are available via:
```bash
bead schema show urn:bead-rs:schema:attempt-outcome:native-v1 --format json
```

### 15.2 Schema evolution

Future versions MUST:
- Use new schema URNs (e.g., `...:v2`)
- Advertise both versions in capabilities
- Maintain v1 checkpoint compatibility

Old clients reading v2 checkpoints MUST preserve unknown fields.

## 16. Conformance requirements

### 16.1 Required tests

Implementations MUST include tests for:

1. **Idempotent replay**: Same request twice returns same receipt
2. **Conflicting replay**: Divergent request conflicts
3. **Outcome-actions**: All valid combinations succeed
4. **Invalid actions**: Illegal transitions conflict
5. **Revision guard**: Stale revision conflicts
6. **Fencing guard**: Token mismatch conflicts
7. **Tier progression**: work_failure advances tiers correctly
8. **Infrastructure failure**: No tier penalty
9. **Quarantine**: Sets tier=3, sets retry_after
10. **Evidence refs**: Preserved in checkpoint
11. **Concurrent resolution**: No duplicate outcomes
12. **Crash recovery**: Transaction atomicity
13. **Checkpoint round-trip**: Records survive export/import
14. **Unknown fields**: Preserved across versions
15. **Capability negotiation**: Correct advertisement

### 16.2 Fixture format

Fixtures live under `research/fixtures/attempt-outcome-v1/`:
- `request.json` - ResolveRequest examples
- `receipt.json` - Expected ResolveReceipt
- `checkpoint-record.jsonl` - Attempt-outcome checkpoint lines
- `audit-event.json` - Expected audit event
- `capabilities.json` - Capability document fragment

Fixtures are normative. Implementations MUST pass all fixture tests.

## 17. Security and privacy

### 17.1 No secret storage

The attempt-outcome record MUST NOT contain:
- API keys or credentials
- Secret environment values
- Private prompt content
- Full model responses

Evidence refs MUST point to external storage, not embed secrets.

### 17.2 Opaque telemetry

The `model`, `harness`, and `harness_version` fields are telemetry only.
They MUST NOT affect resolution semantics or scheduling decisions.

### 17.3 Audit completeness

All outcomes are audited. The audit trail must be sufficient to:
- Reconstruct attempt history
- Identify who resolved each attempt
- Correlate outcomes with tier changes
- Prove atomicity of each resolution

## 18. Migration path

### 18.1 Schema changes

A migration adds:
- `attempt_outcomes` table
- `attempt_outcome_id` column to `events` (nullable FK)
- `attempt_tier` column to `issues` (already exists)
- `consecutive_failures` column to `issues` (if not present)

### 18.2 Backward compatibility

Old clients continue working:
- Old checkpoints without attempt-outcome records are valid
- Old lifecycle commands are unchanged
- Existing claims operate without attempt_id

New features are strictly additive.

## 19. Related specifications

This specification integrates with:
- `checkpoint-set-v1` - Checkpoint representation
- `native-field-guide-v1` - Issue and event schemas
- `bulk-manifests-v1` - Bulk transaction syntax
- `verified-restore-v1` - Recovery semantics

## Appendix A: Hash computation example

```
Input components:
  attempt_id = "urn:needle:attempt:abc123"
  issue_id = "bead-0123456789abcdef"
  outcome = "verified_success"
  action = "close"
  reason = "All tests passed"
  revision = 42
  fencing_token = ""
  evidence_refs = ["s3:logs/abc.tar.gz", "coverage:xyz.html"]

Joined input:
  "urn:needle:attempt:abc123\x00bead-0123456789abcdef\x00verified_success\x00close\x00All tests passed\x0042\x00\x00s3:logs/abc.tar.gz\x00coverage:xyz.html"

SHA-256: "3a1b2c3d4e5f6..." (64 hex characters)
```

## Appendix B: Error messages

### B.1 Conflict errors

```
Conflict: Attempt urn:needle:attempt:abc123 already resolved with outcome 'work_failure' (hash a1b2c...) but new request has outcome 'verified_success' (hash d4e5f...)

Use a new attempt_id for this attempt or investigate the prior outcome.
```

### B.2 Revision conflict

```
Conflict: Expected revision 42 but issue is currently at revision 43

Re-fetch the issue and retry with the current revision.
```

### B.3 Fencing conflict

```
Conflict: Fencing token mismatch

This issue is leased to another process. Wait for the lease to expire or use the correct fencing token.
```

## Appendix C: Capability document example

See section 14.1 for the complete `attempt_outcome` capabilities fragment.

## Appendix D: Checkpoint example

See section 11.1 for the complete attempt-outcome checkpoint record format.
