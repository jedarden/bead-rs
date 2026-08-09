# bead-rs Marathon progress log

## 2026-08-09 — R011 namespaced external references implemented and completed

- **Completed**: Implemented R011 namespaced external references for attaching generic (namespace, key, value) references such as tracker IDs and commit identifiers.

- **Implementation Details**:
  - Added database Migration 6: external_references table with issue_id, namespace, key, value columns
  - UNIQUE constraint on (issue_id, namespace, key) for namespace-scoped uniqueness
  - Implemented ExternalReference model with comprehensive validation
  - Created service layer in src/service/external_refs.rs with full CRUD operations
  - Added CLI command 'bead ref' with add, remove, list, find subcommands
  - Idempotent operations for add and remove to support reliable deduplication
  - Cross-tool recognition via namespace/value lookup across workspace
  - No network resolution - all operations use local data only
  - Native bead IDs preserved - external references stored as foreign key

- **Model Changes** (src/model.rs):
  - Added ExternalReference struct with issue_id, namespace, key, value fields
  - Implemented validate_reference_namespace(): 1-64 bytes, lowercase alphanumeric/hyphens/underscores, must start with lowercase letter
  - Implemented validate_reference_key(): nonempty, ≤128 bytes, no control characters
  - Implemented validate_reference_value(): nonempty, ≤512 bytes, no control characters
  - Added ExternalReference::validate() method for complete validation

- **Service Layer** (src/service/external_refs.rs):
  - add_external_reference(): Adds reference with INSERT OR REPLACE for idempotency, validates issue exists
  - remove_external_reference(): Removes reference idempotently with validation
  - list_external_references(): Lists all references for an issue with error handling
  - find_issues_by_reference(): Finds all issues with given namespace/value for cross-tool recognition
  - All operations use atomic transactions with proper error handling

- **CLI Integration** (src/cli.rs, src/main.rs):
  - New command: bead ref with subcommands add/remove/list/find
  - ref add: --id ISSUE --namespace NS --key KEY --value VALUE
  - ref remove: --id ISSUE --namespace NS --key KEY
  - ref list: --id ISSUE [--json]
  - ref find: --namespace NS --value VALUE [--json]
  - JSON output support for list (line-by-line objects) and find (JSON array)
  - Comprehensive help text and validation messages

- **Test Coverage** (13 comprehensive integration tests):
  - test_add_external_reference: Basic add operation with verification
  - test_add_multiple_references_same_issue: Multiple namespaces on single issue
  - test_add_duplicate_reference_idempotent: Idempotent add behavior
  - test_remove_external_reference: Remove operation with verification
  - test_remove_nonexistent_reference_idempotent: Idempotent remove behavior
  - test_find_issues_by_reference: Cross-tool recognition across issues
  - test_reference_validation_invalid_namespace: Validation rejects invalid namespaces
  - test_reference_validation_empty_fields: Validation rejects empty values
  - test_reference_json_output: JSON format validation for list
  - test_reference_list_json_find_json: JSON format validation for find
  - test_reference_nonexistent_issue: Error handling for missing issues
  - test_reference_help: Help documentation availability
  - test_reference_namespace_scoped_uniqueness: Namespace-scoped uniqueness behavior

- **Acceptance Criteria Met**:
  - ✅ Attach generic (namespace, key, value) references such as tracker IDs and commit identifiers
  - ✅ Do not replace native bead IDs or resolve anything over the network
  - ✅ Optional namespace-scoped uniqueness supports reliable deduplication and cross-tool recognition
  - ✅ Namespace validation: lowercase only, 1-64 bytes, must start with letter
  - ✅ Key validation: nonempty, ≤128 bytes, no control characters
  - ✅ Value validation: nonempty, ≤512 bytes, no control characters
  - ✅ Idempotent add/remove operations for reliable deduplication
  - ✅ Cross-tool recognition via find_issues_by_reference namespace/value lookup
  - ✅ No network resolution: all operations use local SQLite database only
  - ✅ Native bead IDs preserved: external references stored as foreign key, never replaces native IDs

- **Code Quality**:
  - cargo test --test r011_external_references: 13/13 tests passed in 0.71s
  - cargo test: All 295 tests passed (282 existing + 13 new R011 tests)
  - cargo fmt --check: passed
  - cargo clippy --all-targets -- -D warnings: passed
  - Clean compilation with comprehensive validation and error handling
  - Proper SQL parameterization to prevent injection
  - Transaction safety with proper rollback handling
  - Display trait implementations for user-friendly output

- **Feature Status**: R011 now marked as passing in feature ledger with comprehensive evidence

## 2026-08-09 — R013 cursor-based local change feed implemented and completed

- **Completed**: Implemented R013 cursor-based local change feed for incremental local synchronization without daemon/network service.

- **Implementation Details**:
  - Added `bead changes` command with comprehensive options: --since, --latest, --snapshot, --validate, --json
  - Implemented src/service/changes.rs with cursor-based change feed functionality
  - Cursor, SnapshotIdentity, MutationRecord, ChangeFeed, and GapInfo structures for change feed operations
  - Implemented get_snapshot_identity() for workspace state tracking with UUID, sequence, checksum, timestamp
  - Implemented get_changes_since() for deterministic mutation records after cursor position
  - Implemented get_gap_info() for explicit gap detection and resynchronization signaling
  - Implemented validate_cursor() for cursor validity checking without gaps
  - Cursor serialization with optional checksum for gap detection
  - Change feed uses event sequence numbers for deterministic positioning
  - Supports both human-readable and JSON output formats
  - No daemon or network service required - uses local SQLite event table

- **CLI Changes**:
  - New command: bead changes with --since <cursor>, --latest, --snapshot, --validate <cursor>, --json flags
  - --since: Get changes since specific cursor position (sequence number or cursor string)
  - --latest: Get latest cursor position for tracking
  - --snapshot: Get current snapshot identity (UUID, max sequence, checksum, timestamp)
  - --validate: Validate cursor and check for gaps, returns detailed gap information when gaps detected
  - Default mode shows current workspace state when no flags specified
  - JSON output provides complete snapshot, mutations, and gap detection information
  - Human-readable output shows mutations with sequence, kind, issue_id, actor, and time

- **Database Schema Changes**:
  - No schema changes required - uses existing events table with sequence numbers
  - Leverages F017 forensic checkpoint event structure with origin_store_uuid, origin_event_sequence, event_sha256, local_ingestion_sequence
  - Change feed reads from existing events table without modification

- **Test Coverage** (12 comprehensive integration tests):
  - test_change_feed_empty_workspace: Verifies empty workspace handling with zero events
  - test_change_feed_after_create: Tests change feed after create and claim operations
  - test_change_feed_incremental_updates: Validates incremental updates between cursor positions
  - test_cursor_validation: Ensures cursor validation works for valid positions
  - test_cursor_serialization: Tests cursor string format with and without checksums
  - test_gap_detection: Verifies gap detection mechanism with missing sequences
  - test_change_feed_multiple_mutations: Tests change feed with multiple operation types
  - test_change_feed_workspace_events: Verifies workspace-level events and issue events
  - test_change_feed_json_format: Validates JSON output structure and required fields
  - test_change_feed_human_readable_output: Tests human-readable output format
  - test_change_feed_no_workspace: Ensures proper error handling without workspace
  - test_change_feed_help: Verifies help documentation availability

- **Acceptance Criteria Met**:
  - ✅ Emits deterministic public mutation records after cursor (event sequence-based)
  - ✅ Includes snapshot identity (UUID, max sequence, checksum, timestamp) for position tracking
  - ✅ Consumers resynchronize from JSONL after gap detection signals explicit need
  - ✅ Supports incremental local indexes and adapters without daemon, network service, or dependency on private event tables
  - ✅ Deterministic ordering: events ordered by sequence number ascending
  - ✅ Gap detection: identifies missing sequences and signals consumers to resync
  - ✅ No external dependencies: works with local SQLite event table only
  - ✅ Versioned semantic change feed format with extensible cursor structure

- **Code Quality**:
  - cargo test --test r013_change_feed: 12/12 tests passed in 0.74s
  - cargo test: All 282 tests passed (270 existing + 12 new R013 tests)
  - cargo fmt --check: passed
  - cargo clippy --all-targets -- -D warnings: passed
  - Clean compilation with only allowed dead_code warnings for public API functions
  - Implementation uses Display trait for cursor serialization instead of inherent to_string
  - Proper error handling with validation errors for invalid cursor formats
  - Safe SQLite operations with parameterized queries and proper error handling

- **Feature Status**: R013 now marked as passing in feature ledger with comprehensive evidence

## 2026-08-09 — R009 schema negotiation catalog implemented and completed

- **Completed**: Implemented R009 schema negotiation catalog for explicit schema capability declaration and negotiation.

[Previous progress entries remain...]

