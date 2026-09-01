# Attempt-Resolution Checkpoint Fixture Format Differences

## Overview

This document describes the differences between the old-format checkpoint fixtures (pre-attempt-resolution) and new-format checkpoint fixtures (with attempt-resolution feature).

## Directory Structure

```
tests/fixtures/attempts/
├── old/                    # Pre-attempt-resolution format
│   ├── checkpoint.jsonl  # Old-format checkpoint data
│   └── current.json      # Old-format manifest
├── new/                    # With attempt-resolution feature
│   ├── checkpoint.jsonl  # New-format checkpoint data
│   └── current.json      # New-format manifest
└── FORMAT_DIFFERENCES.md  # This file
```

## Old Format (Pre-Attempt-Resolution)

### Characteristics

- **Record types**: Only `issue` records
- **No attempt tracking**: No record of execution attempts
- **Simple manifest**: Minimal checkpoint metadata

### checkpoint.jsonl Structure

```json
{"record_type":"issue","issue":{...issue data...}}
{"record_type":"issue","issue":{...issue data...}}
{"record_type":"issue","issue":{...issue data...}}
```

### current.json Structure

```json
{
  "active_root": { "path": "checkpoint.jsonl", "sha256": "..." },
  "added_paths": ["checkpoint.jsonl"],
  "created_at": "2026-08-30T00:00:00.000000000Z",
  "deleted_paths": [],
  "event_count": 0,
  "generation_id": "gen-old-format-test-fixture",
  "issue_count": 3,
  "mode": "monolithic",
  "receipt_count": 0,
  "replaced_paths": ["current.json", "forensic.jsonl"],
  "schema_version": 1,
  "snapshot_sequence": 1,
  "store_uuid": "00000000-0000-0000-0000-000000000000",
  "total_record_count": 3
}
```

**Key Point**: No `attempt_outcome_count` field

## New Format (With Attempt-Resolution)

### Characteristics

- **Record types**: `issue` AND `attempt_outcome` records
- **Attempt tracking**: Full execution attempt history
- **Enhanced manifest**: Includes attempt outcome counts

### checkpoint.jsonl Structure

```json
{"record_type":"issue","issue":{...issue data...}}
{"record_type":"issue","issue":{...issue data...}}
{"record_type":"issue","issue":{...issue data...}}
{"record_type":"attempt_outcome","attempt_outcome":{...attempt outcome data...}}
{"record_type":"attempt_outcome","attempt_outcome":{...attempt outcome data...}}
```

### attempt_outcome Record Schema

Each `attempt_outcome` record includes:

**Required fields:**
- `$schema` (aliased as `schema_ref`): Schema reference URN
- `attempt_id`: Unique attempt identifier
- `issue_id`: Related issue ID
- `outcome`: Outcome classification (verified_success, work_failure, infrastructure_failure, cancelled, indeterminate)
- `action`: Lifecycle action (close, release, quarantine, block, none)
- `reason`: Human-readable reason
- `canonical_request_hash`: SHA-256 hash of the canonical request
- `resulting_issue_revision`: Issue revision after this attempt
- `resulting_state`: Issue state after this attempt (open, in_progress, closed, deferred)
- `resulting_attempt_tier`: Attempt tier after this attempt (0-3)
- `receipt_id`: Unique receipt identifier
- `actor`: Identity that performed the resolution
- `created_at`: RFC 3339 timestamp

**Optional fields:**
- `evidence_refs`: Array of evidence reference strings (format: `namespace:value`)
- `model`: Model identifier for telemetry
- `harness`: Harness name for telemetry
- `harness_version`: Harness version for telemetry

### current.json Structure

```json
{
  "active_root": { "path": "checkpoint.jsonl", "sha256": "..." },
  "added_paths": ["checkpoint.jsonl"],
  "created_at": "2026-08-31T15:00:00.000000000Z",
  "deleted_paths": [],
  "event_count": 10,
  "generation_id": "gen-new-format-test-fixture",
  "issue_count": 3,
  "mode": "monolithic",
  "receipt_count": 2,
  "replaced_paths": ["current.json", "forensic.jsonl"],
  "schema_version": 1,
  "snapshot_sequence": 10,
  "store_uuid": "00000000-0000-0000-0000-000000000000",
  "total_record_count": 5,
  "attempt_outcome_count": 2
}
```

**Key Addition**: `attempt_outcome_count` field (new in attempt-resolution feature)

## Record Count Relationships

The following invariant holds in new-format checkpoints:

```
total_record_count = issue_count + event_count + receipt_count + attempt_outcome_count
```

For the example new-format fixture:
```
5 = 3 (issues) + 0 (events) + 0 (receipts) + 2 (attempt_outcomes)
```

## Schema Validation

Both old and new formats validate against the same checkpoint-manifest schema (`urn:bead-rs:schema:checkpoint-manifest:native-v1`), but the new format includes:

1. Additional record type (`attempt_outcome`)
2. Additional manifest field (`attempt_outcome_count`)
3. New schema references for attempt records (`urn:bead-rs:schema:attempt-outcome:native-v1`)

## Compatibility

### Backward Compatibility

- **Old binaries** can read new-format checkpoints: Unknown record types are ignored during import
- **New binaries** can read old-format checkpoints: Missing `attempt_outcome_count` defaults to 0

### Forward Compatibility

- **New binaries** with old binaries: Attempt outcome records are lost during export, but issue records remain intact
- **Old binaries** with new binaries: Attempt outcomes are not recorded, but checkpoint structure remains valid

## Testing

The fixtures are used by:

1. `tests/attempt_outcome_round_trip.rs`: Validates attempt outcome persistence through checkpoint export/import
2. `tests/pinned_binary_capability.rs`: Tests capability detection and compatibility between pre-feature and feature-enabled binaries

### Validation Commands

```bash
# Validate JSONL syntax
while IFS= read -r line; do
    echo "$line" | jq empty
done < tests/fixtures/attempts/new/checkpoint.jsonl

# Count record types
jq -r 'select(.record_type == "issue") | .issue.id' tests/fixtures/attempts/new/checkpoint.jsonl | wc -l
jq -r 'select(.record_type == "attempt_outcome") | .attempt_outcome.attempt_id' tests/fixtures/attempts/new/checkpoint.jsonl | wc -l

# Verify manifest counts
jq -r '.attempt_outcome_count' tests/fixtures/attempts/new/current.json
```

## Implementation Reference

- **Schema definition**: `src/model/attempt.rs`
- **Schema registry**: `src/service/schema.rs`
- **Checkpoint service**: `src/service/checkpoint.rs`
- **Capability detection**: `src/service/capabilities.rs`

## Feature Boundary Commits

- **Old format baseline**: `beadrs-15fcfce1` (pre-attempt-resolution)
- **New format baseline**: `beadrs-ef91efc1` (with attempt-resolution)

See: `docs/boundaries/attempt-resolution-feature-commits.md`
