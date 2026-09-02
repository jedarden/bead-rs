# Feature Boundary Reference Pattern

This directory contains reference markers for major feature implementation boundaries. These markers help developers quickly locate and reference the exact commits where features were implemented.

## Naming Convention

Feature boundaries use the tag pattern:
- `<feature-name>-pre` - The last commit **before** implementation began
- `<feature-name>-complete` - The commit where core implementation is complete

## Current Boundaries

| Feature | Pre Tag | Complete Tag | Documentation |
|---------|---------|--------------|---------------|
| attempt-resolution | `attempt-resolution-pre` (53dade0) | `attempt-resolution-complete` (bcda20a) | [attempt-resolution-feature.md](attempt-resolution-feature.md) |

## How to Use Boundary Tags

### Compare feature implementation
```bash
# See all changes for a feature
git diff attempt-resolution-pre..attempt-resolution-complete

# See only files added/modified for the feature
git diff --name-only attempt-resolution-pre..attempt-resolution-complete | grep attempt
```

### Checkout specific states
```bash
# View pre-feature state
git checkout attempt-resolution-pre

# View feature-complete state
git checkout attempt-resolution-complete
```

### View boundary details
```bash
# Show what the pre tag points to
git show attempt-resolution-pre

# Show what the complete tag points to
git show attempt-resolution-complete

# View git notes (if added)
git notes show 53dade07
git notes show bcda20a
```

## Git Notes

Boundary commits include git notes for discoverability:
```bash
# List all notes
git notes list

# Show specific note
git notes show <commit-sha>
```

## Adding New Feature Boundaries

When implementing a major feature, establish boundary markers:

1. **Before implementation**: Tag the current commit as `<feature>-pre`
   ```bash
   git tag -a <feature>-pre -m "Pre-implementation state for <feature>"
   ```

2. **After core implementation**: Tag the completion commit as `<feature>-complete`
   ```bash
   git tag -a <feature>-complete -m "Core implementation complete for <feature>"
   ```

3. **Create boundary documentation**: Add a `<feature>-feature.md` file documenting:
   - Exact commit SHAs and dates
   - What the feature added
   - Key implementation commits
   - Usage examples
   - Verification status

4. **Add git notes** (optional but recommended):
   ```bash
   git notes add <pre-commit-sha> -m "Boundary: <feature>-pre\n\n<description>"
   git notes add <complete-commit-sha> -m "Boundary: <feature>-complete\n\n<description>"
   ```

5. **Add code comments**: Reference the boundary in relevant source files
   ```rust
   //! # Implementation Boundary
   //!
   //! This feature was implemented between:
   //! - Pre: `<feature>-pre` (<short-sha>)
   //! - Complete: `<feature>-complete` (<short-sha>)
   //!
   //! See `docs/boundaries/<feature>-feature.md` for details.
   ```

6. **Update this README**: Add the new boundary to the "Current Boundaries" table

## Benefits

- **Quick navigation**: Jump to exact feature states without hunting through log
- **Clear diffs**: Compare before/after without noise from unrelated changes
- **Documentation**: Linked references between code, docs, and git history
- **Testing**: Isolate feature-specific changes for regression testing
- **Code reviews**: Understand what a feature actually changed

## Example: Checkpoint Compatibility

The pinned binaries framework uses boundary tags to ensure compatibility testing:

```
.beads/pinned-binaries/
├── baseline (attempt-resolution-pre) - Pre-feature binary
└── enabled (attempt-resolution-complete) - Post-feature binary
```

See `.beads/pinned-binaries/README.md` for details.

---

**Created:** 2026-09-01
**Pattern:** `<feature>-pre` / `<feature>-complete`
