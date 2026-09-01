# Old-Format Checkpoint Fixtures (Pre-Attempt-Resolution)

This directory contains checkpoint fixtures representing the format **before** the attempt-resolution feature was added.

## Feature Boundary

- **Pre-feature tag**: `attempt-resolution-pre` (commit `53dade0`)
- **Feature-complete tag**: `attempt-resolution-complete` (commit `bcda20a`)
- **Date**: August 31, 2026

## Missing Features

The old format lacks the following attempt-resolution capabilities:

1. **No `attempt_outcome` record type** - Checkpoint JSONL contains only `issue` records
2. **No `attempt_outcome_count` field** - Current.json metadata lacks attempt outcome count
3. **No `attempt_outcomes` table** - SQLite schema is v13 (no attempt tracking)
4. **No attempt resolution service** - `src/service/attempt.rs` does not exist
5. **No attempt tier tracking** - Issues lack `attempt_tier` and `consecutive_failures` fields
6. **No attempt receipt IDs** - No `receipt_id` field linking outcomes to receipts

## Schema Differences

### Old Format (Pre-Attempt-Resolution)

```json
{
  "active_root": { ... },
  "added_paths": [ ... ],
  "created_at": "2026-08-30T...",
  "deleted_paths": [ ... ],
  "event_count": 0,
  "generation_id": "...",
  "issue_count": 3,
  "mode": "monolithic",
  "receipt_count": 0,
  "replaced_paths": [ ... ],
  "schema_version": 1,
  "snapshot_sequence": 1,
  "store_uuid": "...",
  "total_record_count": 3
}
```

**Note**: No `attempt_outcome_count` field.

### New Format (With Attempt-Resolution)

```json
{
  "active_root": { ... },
  "added_paths": [ ... ],
  "created_at": "2026-08-31T...",
  "deleted_paths": [ ... ],
  "event_count": 10,
  "generation_id": "...",
  "issue_count": 3,
  "mode": "monolithic",
  "receipt_count": 2,
  "replaced_paths": [ ... ],
  "schema_version": 1,
  "snapshot_sequence": 10,
  "store_uuid": "...",
  "total_record_count": 15,
  "attempt_outcome_count": 2
}
```

**Added**: `attempt_outcome_count` field.

## Checkpoint Records

### Old Format (Only Issue Records)

```jsonl
{"record_type":"issue","issue":{...}}
{"record_type":"issue","issue":{...}}
{"record_type":"issue","issue":{...}}
```

### New Format (Issue + Attempt Outcome Records)

```jsonl
{"record_type":"issue","issue":{...}}
{"record_type":"issue","issue":{...}}
{"record_type":"issue","issue":{...}}
{"record_type":"attempt_outcome","attempt_outcome":{...}}
{"record_type":"attempt_outcome","attempt_outcome":{...}}
```

## Usage

These fixtures are used for:

1. **Compatibility testing** - Verify old binaries can read old checkpoints
2. **Migration testing** - Verify new binaries can upgrade old checkpoints
3. **Feature detection** - Test capability probing for attempt-resolution support
4. **Rollback scenarios** - Test downgrade behavior when attempt-resolution is disabled

## Verification

Old-format fixtures should:
- ✅ Load successfully with both old and new binaries
- ✅ Validate as correct JSON/JSONL
- ✅ Contain only `issue` record types
- ✅ Lack `attempt_outcome_count` in metadata
- ✅ Represent schema v13 (no `attempt_outcomes` table)

## See Also

- [New-format fixtures](../new/) - Post-attempt-resolution fixtures
- [Feature boundary documentation](../../../docs/boundaries/attempt-resolution-feature.md)
- [Attempt outcome specification](../../../research/specs/attempt-outcome-v1.md)
