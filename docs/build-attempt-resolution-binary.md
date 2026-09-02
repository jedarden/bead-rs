# Building bead-rs with Attempt Resolution Feature

This document describes how to build and pin a bead-rs binary with the attempt-resolution feature enabled.

## Pinned Build Information

**Build Date:** 2026-09-02  
**Commit SHA:** `fdc2b304fa9659c65ff42201a866a18637783dc2`  
**Version:** 0.2.6  
**Binary Path:** `/home/coding/target/release/bead`  
**Binary Size:** 7.0M  
**SHA256 Hash:** `3dbb4ad36c9f219832c15c95ae6c632441495a12fd631e1281edf08a01bcb79f`  
**Feature:** `attempt-resolution`

## Build Procedure

### 1. Checkout Pinned Commit

```bash
cd /home/coding/bead-rs
git checkout fdc2b304fa9659c65ff42201a866a18637783dc2
```

### 2. Build with Feature Enabled

```bash
cargo build --release --features attempt-resolution
```

### 3. Verify Binary

```bash
# Check version
/home/coding/target/release/bead --version

# Verify hash
sha256sum /home/coding/target/release/bead
# Expected: 3dbb4ad36c9f219832c15c95ae6c632441495a12fd631e1281edf08a01bcb79f
```

### 4. Install (Optional)

```bash
# Install to ~/.cargo/bin
cargo install --path . --features attempt-resolution

# Or copy directly
cp /home/coding/target/release/bead ~/.cargo/bin/bead
```

## Feature Specification

The `attempt-resolution` feature enables atomic attempt outcome recording and lifecycle transitions as specified in:

- [ADR-011: Atomic Idempotent Attempt Resolution](../adr/011-atomic-idempotent-attempt-resolution.md)
- [ADR-012: Capability-Gated Attempt Contract Rollout](../adr/012-capability-gated-attempt-contract-rollout.md)

### Feature Components

When enabled, this feature provides:

1. **Data Models** (`tests/attempt.rs`):
   - `Outcome` enum: VerifiedSuccess, WorkFailure, InfrastructureFailure, Cancelled, Indeterminate
   - `Action` enum: Close, Release, Quarantine, Block, None
   - `ResolveRequest`: Structured attempt resolution request
   - `ResolveReceipt`: Atomic transaction receipt
   - `AttemptOutcomeRecord`: Checkpoint-compatible outcome record

2. **Validation**:
   - Outcome-action combination validation per spec
   - Evidence reference format validation (NAMESPACE:VALUE)
   - Request field validation (attempt_id, actor, etc.)

3. **Error Handling**:
   - `AttemptError` with exit codes (2=usage, 3=not found, 4=conflict, 5=integrity, 6=transient)

### Future Implementation

The data structures are defined and tested, but the CLI command and service layer integration are pending implementation. The feature flag allows incremental development and testing without breaking existing functionality.

## Reproducibility

To reproduce this exact build:

```bash
# Environment
# OS: Linux (Hetzner EX44)
# Rust: 1.85+
# Cargo: Latest

# Clone repository
git clone https://github.com/jedarden/bead-rs.git
cd bead-rs

# Checkout exact commit
git checkout fdc2b304fa9659c65ff42201a866a18637783dc2

# Build with feature
cargo build --release --features attempt-resolution

# Verify hash
sha256sum target/release/bead
# Should output: 3dbb4ad36c9f219832c15c95ae6c632441495a12fd631e1281edf08a01bcb79f
```

## Integration

This binary is intended for integration with NEEDLE workspaces that require atomic attempt resolution. Before deploying:

1. Verify capabilities via `bead capabilities --json`
2. Confirm attempt-resolution support is advertised
3. Test with sample workload before production use
4. Ensure backward compatibility with existing bead stores

## Rollback

If needed, rollback to a previous version:

```bash
# Checkout previous version
git checkout <previous-commit-sha>

# Build without feature
cargo build --release

# Install
cp target/release/bead ~/.cargo/bin/bead
```

## Maintenance

When updating to a new commit:

1. Update this document with new SHA and hash
2. Rebuild with feature enabled
3. Run full test suite
4. Verify backward compatibility
5. Update pinned binary in deployment

## References

- [ADR-010: Store Attempt Facts, Not Learning Policy](../adr/010-store-attempt-facts-not-learning-policy.md)
- [ADR-011: Atomic Idempotent Attempt Resolution](../adr/011-atomic-idempotent-attempt-resolution.md)
- [ADR-012: Capability-Gated Attempt Contract Rollout](../adr/012-capability-gated-attempt-contract-rollout.md)
- [Product Plan](../plan/plan.md)
