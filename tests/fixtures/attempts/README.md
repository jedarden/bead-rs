# Attempt-Resolution Checkpoint Fixtures

This directory contains checkpoint test fixtures for the attempt-resolution feature, enabling compatibility testing between pre-feature and feature-enabled bead-rs binaries.

## Quick Start

```bash
# Validate fixtures
./tests/fixtures/attempts/validate.sh

# Run round-trip tests
cargo test --test attempt_outcome_round_trip

# Run capability detection tests
cargo test --test pinned_binary_capability
```

## Directory Structure

```
attempts/
├── old/                           # Pre-attempt-resolution format
│   ├── checkpoint.jsonl          # Old-format checkpoint (3 issues)
│   └── current.json              # Old manifest (no attempt_outcome_count)
├── new/                           # With attempt-resolution feature
│   ├── checkpoint.jsonl          # New-format checkpoint (3 issues + 2 attempt_outcomes)
│   └── current.json              # New manifest (attempt_outcome_count: 2)
├── FORMAT_DIFFERENCES.md         # Detailed format comparison
├── VALIDATION_REPORT.md          # Validation results and coverage
├── README.md                      # This file
└── validate.sh                    # Fixture validation script
```

## Format Differences

### Old Format (Pre-Feature)
- **Record types:** Issues only
- **Manifest:** No `attempt_outcome_count` field
- **Purpose:** Baseline for pre-feature binary testing

### New Format (With Feature)
- **Record types:** Issues + Attempt outcomes
- **Manifest:** Includes `attempt_outcome_count` field
- **Purpose:** Test attempt-resolution feature behavior

See `FORMAT_DIFFERENCES.md` for detailed comparison.

## Attempt Outcome Records

Each attempt outcome record includes:

**Required Fields:**
- `schema_ref`: Schema identifier (urn:bead-rs:schema:attempt-outcome:native-v1)
- `attempt_id`: Unique attempt identifier
- `issue_id`: Related issue ID
- `outcome`: Classification (verified_success, work_failure, infrastructure_failure, cancelled, indeterminate)
- `action`: Lifecycle action (close, release, quarantine, block, none)
- `reason`: Human-readable explanation
- `canonical_request_hash`: SHA-256 of canonical request
- `resulting_issue_revision`: Issue revision after attempt
- `resulting_state`: Issue state after attempt
- `resulting_attempt_tier`: Attempt tier (0-3)
- `receipt_id`: Unique receipt identifier
- `actor`: Identity that performed resolution
- `created_at`: RFC 3339 timestamp

**Optional Fields:**
- `evidence_refs`: Evidence artifacts (format: namespace:value)
- `model`: Model identifier for telemetry
- `harness`: Harness name for telemetry
- `harness_version`: Harness version for telemetry

## Test Coverage

These fixtures support:

1. **Capability Detection Tests** (`pinned_binary_capability.rs`)
   - Pre-feature binaries lack attempt_outcome support
   - Feature-enabled binaries report attempt_outcome capability
   - `bead resolve` command availability
   - `bead why` attempt information display

2. **Checkpoint Round-Trip Tests** (`attempt_outcome_round_trip.rs`)
   - Monolithic mode export/import
   - Sharded mode export/import
   - Conflicting duplicate detection
   - Malformed record rejection
   - Backward compatibility with old readers

## Validation

All fixtures have been validated for:

- ✅ JSONL syntax correctness
- ✅ Schema compliance
- ✅ Field presence and types
- ✅ Referential integrity
- ✅ Enum value validity
- ✅ Outcome-action combination correctness
- ✅ Evidence reference format

See `VALIDATION_REPORT.md` for detailed validation results.

## Compatibility

| Binary Version | Old Format | New Format |
|---------------|-----------|-----------|
| Pre-feature   | ✅ Normal | ⚠️ Ignores attempt_outcome |
| Feature-enabled| ✅ Normal | ✅ Full support |

## Maintenance

Update these fixtures when:

- Schema versions change
- New attempt outcome fields are added
- Outcome-action combinations are modified
- Test requirements expand

## Implementation Reference

- **Model:** `src/model/attempt.rs`
- **Schema:** `src/service/schema.rs`
- **Checkpoint:** `src/service/checkpoint.rs`
- **Capabilities:** `src/service/capabilities.rs`

## Feature Boundary

- **Old format baseline:** beadrs-15fcfce1 (pre-attempt-resolution)
- **New format baseline:** beadrs-ef91efc1 (with attempt-resolution)

## License

Same as parent bead-rs project.
