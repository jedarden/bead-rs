# Analysis: Heuristic Starvation Mutations

## Current State - Problems Identified

### 1. Watchdog (watchdog.rs) - Process-Name Search Heuristic

**Location**: `src/service/watchdog.rs:96-112`

```rust
fn is_worker_alive(assignee: &str) -> bool {
    let output = Command::new("pgrep")
        .arg("-f")
        .arg(assignee)
        .output();
    
    match output {
        Ok(output) => output.status.success(),
        Err(_) => true, // Conservatively assume worker is alive
    }
}
```

**Problem**: Uses `pgrep -f` to search for processes by name. This is a **process-name search heuristic** that violates the acceptance criteria: "No lifecycle mutation is authorized by title inference, process-name search, or undeclared intent."

**Impact**: Lines 207-243 use this heuristic to decide whether to release beads.

### 2. Doctor Starvation Recovery - Direct Database Mutations

**Location**: `src/service/doctor.rs:2189-2517`

The `run_starvation_recovery` function performs direct SQL mutations:

#### Assigned-but-open beads (lines 2300-2357)
```rust
// Direct SQL UPDATE bypassing lifecycle service
let mut clear_stmt = conn
    .prepare("UPDATE issues SET assignee = NULL WHERE id = ?1")
    ...

for (id, title, assignee) in &assigned_open_beads {
    clear_stmt.execute([&id])
        .map_err(|e| Error::integrity(...))?;
    // ...
}
```

#### Stale in-progress beads (lines 2367-2466)
```rust
// Direct SQL UPDATE bypassing lifecycle service
let mut release_stmt = conn
    .prepare("UPDATE issues SET base_status = 'open', assignee = NULL, released_at = ?1, release_reason = ?2 WHERE id = ?3")
    ...

for (id, title, age_seconds) in &stale_beads {
    release_stmt.execute([&release_timestamp, &release_reason, id])
        .map_err(|e| Error::integrity(...))?;
    // ...
}
```

**Problems**:
1. Bypasses `lifecycle::release_issue()` which validates:
   - Revision preconditions (`--if-revision`)
   - Lease validation (`validate_lease_for_mutation`)
   - Fencing tokens
   - Proper audit events
   - Resource lock cleanup

2. Violates acceptance criteria: "Any recovery mutation uses legal revision/lease/fencing rules, the normal audit/change-feed transaction, and checkpoint publication"

## Correct Patterns Already Exist

### Lifecycle Service (lifecycle.rs)

```rust
pub fn release_issue(
    conn: &Connection,
    id: &str,
    if_revision: Option<i64>,
    fencing_token: Option<i64>,
) -> Result<String> {
    let mut tx = begin_lifecycle_transaction(conn)?;
    let issue = get_issue_for_update(&tx, id)?;
    
    // Validates revision
    if let Some(expected_revision) = if_revision {
        // ... validation
    }
    
    // Validates lease
    if let Some(current_assignee) = &issue.assignee {
        validate_lease_for_mutation(&tx, id, current_assignee, fencing_token)?;
    }
    
    let result = release_issue_impl(&mut tx, &issue)?;
    tx.commit()?;
    Ok(result)
}
```

### Lease Validation (leases.rs)

```rust
pub fn validate_lease_for_mutation(
    conn: &rusqlite::Connection,
    issue_id: &str,
    assignee: &str,
    expected_fencing_token: Option<i64>,
) -> Result<()> {
    // Checks if lease exists and is valid
    // Validates fencing token if provided
    // Returns error if lease expired or token mismatch
}
```

## Lease-Based Detection vs Process Search

The correct approach for detecting stale claims:

1. **For non-leased claims**: Use time-based expiry detection (already in `check_stale_in_progress`)
2. **For leased claims**: Lease expiry is authoritative - no process search needed
3. **For recovery**: Use `lifecycle::release_issue()` with proper validation

## Required Changes

1. ✅ **Remove process-name search from watchdog**
   - Delete `is_worker_alive()` function
   - Use lease expiry detection instead

2. ✅ **Replace direct mutations in doctor**
   - Use `lifecycle::release_issue()` instead of direct SQL
   - Pass `if_revision` and `fencing_token` parameters

3. ✅ **Make recovery recommendation-only**
   - Don't perform automatic mutations
   - Emit recommendations for manual application

4. ✅ **Preserve read-only diagnostics**
   - Keep `check_stale_in_progress` as-is
   - Keep `check_ready_frontier` as-is
   - Keep `run_starvation_check` as-is

## Implementation Plan

1. Remove `is_worker_alive()` from watchdog.rs
2. Replace watchdog's release logic to use lifecycle service
3. Replace doctor's starvation recovery mutations with lifecycle calls
4. Add dry-run mode to doctor starvation recovery
5. Update tests to verify new behavior
