# Attempt-Resolution Feature Boundary

This document records the exact git commits representing the attempt-resolution feature implementation boundary.

## Boundary Commits

### Pre-Feature State
**Tag:** `attempt-resolution-pre`  
**Commit:** `53dade07ff2b9afda87e67459a825ec7e138dafa`  
**Date:** Mon Aug 31 09:24:28 2026 -0400  
**Message:** `feat(recovery): replace heuristic starvation mutations with recommendation-only diagnostics`

This represents the last commit **before** any attempt-resolution implementation work began. At this point, only the ADR (Architecture Decision Record) documentation existed:

- `docs/adr/010-store-attempt-facts-not-learning-policy.md`
- `docs/adr/011-atomic-idempotent-attempt-resolution.md`  
- `docs/adr/012-capability-gated-attempt-contract-rollout.md`

### Feature-Complete State
**Tag:** `attempt-resolution-complete`  
**Commit:** `bcda20a3e09b414d093818c405646d1424a9e97b`  
**Date:** Mon Aug 31 13:29:45 2026 -0400  
**Message:** `feat(attempt): implement atomic idempotent attempt resolution service`

This represents the **core implementation** of the attempt-resolution service. This commit added:

- `src/service/attempt.rs` - Main attempt resolution service
- `src/model/attempt.rs` - Attempt data model
- `tests/attempt_outcome_round_trip.rs` - Test coverage
- `src/store/migrations.rs` - Database migrations for attempt tracking
- Plus supporting fixtures and specifications

## Feature Timeline

The attempt-resolution feature was implemented in a sequence of commits between the boundaries:

1. **`da453ba`** - `feat(specs): add attempt-outcome-v1 specification and fixtures`
2. **`9c0a7bd`** - `feat(checkpoint): persist attempt outcomes through checkpoint round-trips`  
3. **`2360d82`** - `fix(checkpoint): persist attempt outcomes through sharded checkpoint round-trips`
4. **`bcda20a`** - `feat(attempt): implement atomic idempotent attempt resolution service` ← **Feature complete**

Additional fixes followed:
5. **`f7c543f`** - `fix(store): migrate on open, and let the publisher tolerate a legacy workspace`
6. **`3aac0ce`** - `fix(store): apply pending migrations when opening an existing store`  
7. **`46f35b5`** - `fix(tests): correct attempt receipt diagnostics fixtures`
8. **`ee828d9`** - `feat(why): display attempt information in human-readable output`

## Key Implementation Details

The core service (`src/service/attempt.rs`) implements:

- **Atomic transactions**: Single SQLite transaction for outcome + lifecycle action
- **Idempotent replay**: Same request returns original receipt (exit 0)
- **Conflict detection**: Same attempt_id with different payload rejects (exit 4)
- **Tier progression**: Work failures increment `consecutive_failures` and advance `attempt_tier`
- **Checkpoint persistence**: Attempt outcomes survive checkpoint round-trips

## Using These Tags

To compare pre/post feature states:

```bash
# Show diff between boundaries
git diff attempt-resolution-pre..attempt-resolution-complete

# Show files added for attempt feature
git diff --name-only attempt-resolution-pre..attempt-resolution-complete | grep attempt

# View pre-feature state
git checkout attempt-resolution-pre

# View feature-complete state  
git checkout attempt-resolution-complete
```

## Verification

Feature implementation verified with 4 passing tests:
- `attempt_outcome_round_trip_monolithic` ✅
- `attempt_outcome_round_trip_sharded` ✅  
- `attempt_outcome_duplicate_detection` ✅
- `attempt_outcome_conflict_rejection` ✅

---

**Created:** 2026-09-01  
**Bead:** `beadrs-b5c54dd2`  
**Tags:** `attempt-resolution-pre`, `attempt-resolution-complete`
