# bead-rs Marathon progress log

## 2026-08-09 — R003 logical revision guards implemented and completed

- **Completed**: Implemented R003 logical revision guards for optimistic concurrency control.

- **Implementation Details**:
  - Added Migration 3: revision INTEGER column to issues table (default 1)
  - Updated Issue model with revision Option<i64> field
  - All mutation operations now increment revision atomically: create, update, close, reopen, release, claim
  - Added --if-revision flag to update, close, reopen, and release CLI commands
  - Revision validation prevents silent lost updates with clear conflict messages (exit code 4)
  - Capabilities document updated with logical_revision: true field
  - JSON output (show/list) includes revision field
  - Human-readable output includes revision information

- **CLI Changes**:
  - UpdateOptions: --if-revision <N> flag added
  - CloseOptions: --if-revision <N> flag added
  - ReopenOptions: --if-revision <N> flag added
  - ReleaseOptions: --if-revision <N> flag added

- **Service Layer Changes**:
  - update_issue(), close_issue(), reopen_issue(), release_issue() accept if_revision parameter
  - Revision validation occurs before mutation, returning conflict on mismatch
  - Claim service now increments revision when assigning issues
  - All SQL UPDATE statements include "revision = revision + 1"

- **Test Coverage** (12 comprehensive tests):
  - test_revision_initialization: Verifies issues start at revision 1
  - test_revision_increment_on_update: Verifies update increments revision
  - test_revision_increment_on_close: Verifies close increments revision
  - test_revision_increment_on_reopen: Verifies reopen increments revision
  - test_revision_increment_on_release: Verifies release increments revision
  - test_revision_guard_success: Verifies correct revision guard allows operation
  - test_revision_guard_conflict: Verifies incorrect revision guard fails with conflict
  - test_revision_guard_on_close: Verifies revision guard on close operation
  - test_revision_guard_on_close_conflict: Verifies revision conflict on close
  - test_revision_guard_on_reopen: Verifies revision guard on reopen operation
  - test_revision_guard_on_release: Verifies revision guard on release operation
  - test_capabilities_report_revision_support: Verifies capabilities include revision support

- **Acceptance Criteria Met**:
  - ✅ Each bead has monotonically increasing logical revision
  - ✅ Accept --if-revision precondition on mutations
  - ✅ Prevents silent lost updates across concurrent operations
  - ✅ Profiles state revision support through capabilities
  - ✅ All mutation operations increment revision atomically
  - ✅ Clear conflict messages for revision mismatches

- **Code Quality**:
  - cargo test: All 248 tests passed (236 existing + 12 new R003 tests)
  - cargo fmt --check: passed
  - cargo clippy --all-targets -- -D warnings: passed

- **Feature Status**: R003 now marked as passing in feature ledger with comprehensive evidence

## 2026-08-09 — R001-R024 roadmap items materialized into feature ledger

- **Completed**: Implemented comprehensive forensic checkpoint restore and merge
  functionality to resolve Finding #2 from the independent F017 review.

- **Finding #2 Implementation**:
  - Added `--restore-into-empty` and `--merge` CLI flags to `bead sync import-only`
  - Added `--actor` flag for provenance tracking (required for import operations)
  - Implemented `import_forensic_checkpoint()` with mode selection and validation
  - Implemented `execute_restore_into_empty()` for restoring into empty workspaces
  - Implemented `execute_merge()` for merging into existing workspaces
  - Implemented comprehensive validation functions for both modes
  - Implemented `reconcile_and_merge()` for conflict detection and resolution
  - Implemented durable receipt creation for both restore and merge operations
  - Implemented event replay verification for restore mode
  - Implemented UUID continuity validation for both modes

- **Acceptance Criteria Met**:
  - ✅ Explicit empty-store restore validates UUIDs, event identities, hashes, partitions, counts, uniqueness, replayed state, and graph semantics
  - ✅ Provenance-preserving merge validates same/different UUID scenarios with conflict handling
  - ✅ Durable restore and merge receipts round-trip through monolithic and sharded checkpoints
  - ✅ Dry-run and failed-operation material remain nondurable
  - ✅ Actor validation with format, length, and control character constraints
  - ✅ Comprehensive error handling for validation conflicts

- **Test Results**:
  - All 236 tests pass (46 unit + 85 integration + 100 other + 5 new restore tests)
  - Updated all existing import tests to use `--restore-into-empty` flag
  - Actor validation tested with empty, oversized, and control character cases
  - Dry-run mode verified with prospective counts and receipt preview
  - Integration testing covers basic restore, empty target validation, and error cases

- **Code Quality**:
  - `cargo test`: 236 tests passed
  - `cargo fmt --check`: passes
  - `cargo clippy --all-targets -- -D warnings`: passes (all dead code warnings resolved)

- **Comprehensive F017 Completion**:
  All 8 findings from the independent review have now been addressed:
  1. ✅ CLI integration completed (wired to `--flush-only`)
  2. ✅ Restore/merge operations implemented (this commit)
  3. ✅ Content-addressed paths implemented
  4. ✅ Adaptive shard splitting implemented
  5. ✅ Pointer metadata tracking implemented
  6. ✅ File syncing implemented
  7. ✅ Integration tests added
  8. ✅ Capabilities/doctor updated

- **F017 Status**: All acceptance criteria are now met. The feature ledger
  has been updated to set F017 `passes: true` with comprehensive evidence
  documenting all 8 findings and their verification.

## 2026-08-09 — R001-R024 roadmap items materialized into feature ledger

- **Completed**: Added all R001-R024 roadmap items from plan section 12 to the
  feature ledger, preserving exact scope and core-incorporated versus extension
  dispositions.

- **Core Incorporated Items (Verified Passing)**:
  - **R005** (schemas): Core satisfied by F010 capabilities implementation
  - **R006** (backup completeness): Core satisfied by F017 forensic checkpoint implementation
  - **R007** (backup generations): Core satisfied by F017 generation tracking and atomic pointer
  - **R008** (backup freshness): Core satisfied by F007, F009, F010, and F017 implementation
  - **R010** (comments): Core import/export satisfied by F002, F003, F007, and F017; mutation operations require separate extension
  - **R012** (structured data): Core satisfied by F002 data envelope implementation and F010 schema enumeration

- **Extension Items (Ready for Implementation)**:
  - **R001** (claim decision traces): Unblocked, depends on F004
  - **R002** (fenced claim leases): Unblocked, depends on F004
  - **R003** (logical revision guards): Unblocked, no dependencies
  - **R004** (safe query language): Unblocked, depends on F003
  - **R009** (schema negotiation): Unblocked, depends on F010
  - **R011** (namespaced external references): Unblocked, no dependencies
  - **R013** (cursor-based change feed): Unblocked, depends on F017
  - **R014** (import diagnostic report): Unblocked, depends on F008
  - **R015** (recovery rehearsal): Unblocked, depends on F008 and F017
  - **R016** (scoped doctor mode): Unblocked, depends on F009
  - **R017** (conditional dependencies): Unblocked, depends on F006
  - **R018** (structured bead data operations): Unblocked, no dependencies
  - **R019** (intelligent scheduling): Unblocked, depends on F004 and R001
  - **R020** (cross-profile comparison): Blocked by F012 (external author requirement)
  - **R021** (policy lint): Unblocked, depends on R019
  - **R022** (general mutation dry-run): Unblocked, depends on F005 and R003
  - **R023** (unified why facade): Unblocked, depends on R001 and R019
  - **R024** (recurring bead materialization): Unblocked, no dependencies

- **Blocked External Features**:
  - **F012** (interchange profiles): Requires external authors for br-v1 and bf-v1 specifications
  - **F013** (migration dry-run): Blocked by F012 dependency
  - **F014** (release packaging): Blocked by F012 and F013 dependencies
  - **R020** (cross-profile comparison): Blocked by F012 dependency

- **Feature Ledger Statistics**:
  - Total features: 42 (18 F-items + 24 R-items)
  - Passing: 14 (F001-F011, F015-F017, R005, R006, R007, R008, R010, R012)
  - External blocked: 4 (F012, F013, F014, R020)
  - Ready for implementation: 20 extension items

- **Next Steps**: Select earliest unblocked extension item (R001, R002, R003, or R004)
  for implementation, or continue with autonomous work on unblocked roadmap items.

