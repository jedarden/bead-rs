# Recommendation: `--profile` Flag on `sync flush-only`

## Executive Summary

**Recommendation**: The `--profile` flag should **NOT** be supported on `sync flush-only` and should be **removed from documentation**.

**Status**: The implementation is correct (flag is properly rejected), but documentation contains outdated references.

---

## Investigation Findings

### 1. Current Implementation Status

**Struct Definition** (`src/cli.rs:1267-1271`):
```rust
pub struct SyncFlushOptions {
    /// Export an issue-only copy to this path instead of only updating .beads/checkpoint/
    #[arg(long)]
    pub output: Option<String>,
}
```

**Observation**: `SyncFlushOptions` has **no `profile` field**. The struct only contains an `output` field for optional export paths.

### 2. Profile Support in Sync Commands

| Command | Profile Field | Default | Purpose |
|---------|---------------|---------|---------|
| `sync flush-only` | ❌ No | N/A | Export database to checkpoint |
| `sync import-only` | ✅ Yes | `native-v1` | Import checkpoint in specific profile |
| `sync reconcile` | ❌ No | N/A | Merge checkpoint into database |
| `sync status` | ❌ No | N/A | Report checkpoint status |
| `sync diff` | ❌ No | N/A | Compare two checkpoints |
| `sync bisect` | ❌ No | N/A | Search checkpoint series |
| `sync fork` | ❌ No | N/A | Fork workspace identity |

**Key Insight**: Only `import-only` has profile support because it must handle checkpoints from different sources/versions.

### 3. Test Coverage

**Test** (`tests/cli_sync.rs:153-173`):
```rust
fn test_sync_flush_only_rejects_invalid_profile() {
    // `sync flush-only` takes no --profile: the dead profile checks were
    // removed (needle-8cb71c7c), so clap rejects the argument outright.
    Command::cargo_bin("bead")
        .unwrap()
        .args(["sync", "flush-only", "--profile", "needle-v1", "--output", ...])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument '--profile'"));
}
```

**Status**: ✅ Test validates that `--profile` is properly rejected with exit code 2.

### 4. Documentation Issues

#### Issue 1: plan.md Line 845 (Outdated)
```markdown
| `bead sync --flush-only [--profile P] [--output PATH]` | ...
```

**Problem**: Shows `[--profile P]` as if it's a supported option.

**Evidence from plan.md Line 1594**:
> "Authoritative publication and disaster-recovery restore are always `native-v1`. `sync --flush-only` rejects every other profile"

This explicitly states flush-only **rejects** other profiles, contradicting line 845.

#### Issue 2: ADR Review (Fixed Bug)
`docs/reviews/adr-002-field-guide-independent-review-2026-08-12.md:252`:
> **I6.** `sync flush-only --profile` is accepted and ignored;

**Context**: This was identified as a bug and fixed in needle-8cb71c7c. The fix removed the dead profile checks so clap now rejects the argument outright.

### 5. Implementation Analysis

**Function** (`src/main.rs:1629-1728`):
```rust
fn cmd_sync_flush_only(opts: cli::SyncFlushOptions) -> Result<()> {
    // ... workspace discovery and database opening ...

    if let Some(ref output) = opts.output {
        // Export issue-only checkpoint to path
        let result = service::flush_checkpoint(&mut store, &output_path)?;
        // ... print export success ...
    } else {
        // Publish forensic checkpoint to .beads/checkpoint/
        let report = service::forensic_checkpoint_status(&mut store, &checkpoint_base)?;
        // ... R027 checks for remote-advanced and covered-ahead ...
        // ... publish checkpoint ...
    }
}
```

**Observation**: The implementation never uses a profile parameter. It flushes from the **native database** to **checkpoint format**, which is always `native-v1`.

---

## Intended Behavior

### What `flush-only` Does

1. **Reads**: From the SQLite database (`.beads/beads.db`)
2. **Writes**: To checkpoint files (`.beads/checkpoint/`)
3. **Format**: Always `native-v1` (the authoritative database format)

### Why Profile Doesn't Make Sense

**Directionality**:
- `flush-only`: Database → Checkpoint (always native-v1)
- `import-only`: Checkpoint → Database (may be needle-v1, native-v1, etc.)

**Analogy**:
- `flush-only` is like "save as" - the source defines the format
- `import-only` is like "open with" - the source may have different formats

### Specification Confirmation

From `docs/plan/plan.md:1593-1596`:
> Authoritative publication and disaster-recovery restore are always `native-v1`. `sync --flush-only` rejects every other profile, and `--restore-into-empty` accepts only a verified native pointer/manifest or native monolith.

**This is the intended and correct behavior.**

---

## Locations Requiring Updates

### 1. Documentation Cleanup Required

**File**: `docs/plan/plan.md`

**Line 845**:
```markdown
| `bead sync --flush-only [--profile P] [--output PATH]` | ...
```

**Should be**:
```markdown
| `bead sync flush-only [--output PATH]` | ...
```

**Justification**: Remove `[--profile P]` as it's not supported and was never intended to be supported.

### 2. No Code Changes Required

- ✅ Implementation is correct (no profile field)
- ✅ Help text is correct (no profile mentioned)
- ✅ Test coverage exists (validates rejection)

---

## Acceptance Criteria Status

| Criterion | Status | Evidence |
|-----------|--------|----------|
| Document intended behavior for --profile on flush-only | ✅ Complete | flush-only should NOT support --profile (always native-v1) |
| Identify all locations where profile is referenced in relation to flush-only | ✅ Complete | plan.md:845 (documentation only) |
| Determine whether profile should be user-selectable for flush-only | ✅ Complete | NO - flush-only always exports native-v1 |
| Produce a recommendation document | ✅ Complete | This document |

---

## Recommendation

### 1. Immediate Action Required

**Update Documentation** (`docs/plan/plan.md:845`):

```markdown
- | `bead sync --flush-only [--profile P] [--output PATH]` | ...
+ | `bead sync flush-only [--output PATH]` | ...
```

### 2. No Further Action Required

- **Code**: Implementation is correct
- **Tests**: Coverage validates the correct behavior
- **Help Text**: No profile references in `--help` output

### 3. Future Consideration

If profile selection were ever needed for export (unlikely), it would require:
1. Profile adapters in `src/profile/` for export direction
2. New field in `SyncFlushOptions`
3. Updated implementation
4. Comprehensive test coverage

**Current recommendation**: Do not add profile support for flush-only. The单向 nature of the operation (database → checkpoint) means the format is inherently fixed to `native-v1`.

---

## Conclusion

The `--profile` flag on `sync flush-only` was:
1. **Never intended to be supported** (specification confirms)
2. **Properly rejected** (implementation and tests validate)
3. **Incorrectly documented** (plan.md line 845 needs update)

**Action**: Update documentation to remove `[--profile P]` from the flush-only command specification.
