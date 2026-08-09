# Checkpoint Set v1 Specification (DRAFT - FOR INDEPENDENT REVIEW)

**Status**: DRAFT - Requires independent review before F017 implementation
**Created**: 2026-08-09
**Author**: Marathon Coding (bead-rs implementation)
**Required Reviewer**: Independent of implementation author
**Source**: Based on plan.md sections 6.1-6.3 (public design prose)

## Abstract

This specification defines the native forensic checkpoint-set format for bead-rs, including monolithic and sharded modes, immutable generation pointers, content-addressed objects, event provenance, and Git-trackable artifacts. The format preserves complete audit history and enables disaster recovery while maintaining deterministic verification.

## Format Overview

A checkpoint set is a collection of immutable content-addressed records representing the complete state of a bead workspace at a point in time. It supports two modes:

- **Monolithic mode**: Single JSONL file containing all records
- **Sharded mode**: Manifest with content-addressed object shards

Both modes are semantically equivalent and produce identical public state when restored.

## Record Types

### Issue Record

```json
{
  "record_type": "issue",
  "issue": {
    "$schema": "urn:bead-rs:schema:issue:native-v1",
    "id": "bead-0123456789abcdef",
    "title": "Example task",
    "description": "Detailed description",
    "priority": 2,
    "base_status": "open",
    "assignee": null,
    "created_at": "2026-08-09T12:00:00Z",
    "updated_at": "2026-08-09T12:00:00Z",
    "labels": ["bug", "urgent"],
    "dependencies": [
      {"blocked": "bead-aaaa", "blocker": "bead-bbbb", "kind": "blocks"}
    ]
  }
}
```

### Event Record

```json
{
  "record_type": "event",
  "event": {
    "$schema": "urn:bead-rs:schema:event:native-v1",
    "origin_store_uuid": "workspace-uuid",
    "origin_event_sequence": 1,
    "issue_id": "bead-0123456789abcdef",
    "kind": "created",
    "actor": "user",
    "time": "2026-08-09T12:00:00Z",
    "detail": {}
  }
}
```

### Provenance Receipt Record

```json
{
  "record_type": "provenance_receipt",
  "provenance_receipt": {
    "$schema": "urn:bead-rs:schema:provenance-receipt:native-v1",
    "schema_ref": "urn:bead-rs:schema:provenance-receipt:native-v1",
    "receipt_id": "receipt-uuid",
    "kind": "restore",
    "source_store_uuid": "original-uuid",
    "target_store_uuid": "current-uuid",
    "source_root_sha256": "abc123...",
    "actor": "user",
    "created_at": "2026-08-09T12:00:00Z",
    "counts": {
      "issues": 100,
      "events": 250,
      "provenance_receipts": 1
    },
    "result": "success",
    "summary_event_identity": null,
    "receipt_sha256": "def456..."
  }
}
```

## Monolithic Format

A monolithic checkpoint is a single JSONL file where each line is one complete JSON object. Records are ordered:

1. All issue records sorted by issue ID ascending
2. All event records sorted by (origin_store_uuid, origin_event_sequence) ascending  
3. All provenance receipt records sorted by receipt ID ascending

File: `.beads/checkpoint/forensic.jsonl`

### Monolithic Limits

- Maximum issue records: 50,000
- Maximum total size: 64 MiB
- Maximum single line: 8 MiB

## Sharded Format

A sharded checkpoint consists of:

1. **Current pointer**: `.beads/checkpoint/current.json`
2. **Previous pointer**: `.beads/checkpoint/previous.json`
3. **Manifest**: `.beads/checkpoint/manifests/<manifest-sha256>.json`
4. **Issue shards**: `.beads/checkpoint/objects/issue-<prefix>.jsonl`
5. **Event shards**: `.beads/checkpoint/objects/event-<range>.jsonl`
6. **Receipt shards**: `.beads/checkpoint/objects/receipt-<prefix>.jsonl`

### Current Pointer

```json
{
  "schema_version": 1,
  "generation_id": "gen-123",
  "mode": "sharded",
  "store_uuid": "workspace-uuid",
  "snapshot_sequence": 1000,
  "active_root": {
    "path": "manifests/abc123.json",
    "sha256": "abc123..."
  },
  "added_paths": ["manifests/abc123.json", "objects/issue-a.jsonl"],
  "replaced_paths": ["manifests/old.json"],
  "deleted_paths": ["objects/obsolete.jsonl"],
  "issue_count": 100,
  "event_count": 500,
  "receipt_count": 2,
  "total_record_count": 602,
  "created_at": "2026-08-09T12:00:00Z"
}
```

### Manifest Structure

```json
{
  "format": "checkpoint-set-v1",
  "schema_version": 1,
  "store_uuid": "workspace-uuid",
  "snapshot_sequence": 1000,
  "max_local_ingestion_sequence": 1000,
  "created_at": "2026-08-09T12:00:00Z",
  "profile": "native-v1",
  "partition_algorithm": "hash-prefix",
  "partition_thresholds": {
    "max_issues_per_shard": 10000,
    "max_shard_size_bytes": 52428800,
    "max_events_per_shard": 100000,
    "max_event_shard_size_bytes": 67108864
  },
  "issue_shards": [
    {
      "path": "objects/issue-0.jsonl",
      "sha256": "hash1",
      "byte_length": 1024000,
      "record_count": 50,
      "id_prefix": "0",
      "role": "issues"
    }
  ],
  "event_shards": [
    {
      "path": "objects/event-1-1000.jsonl", 
      "sha256": "hash2",
      "byte_length": 2048000,
      "record_count": 1000,
      "origin_store_uuid": "workspace-uuid",
      "sequence_range": ["1", "1000"],
      "role": "events"
    }
  ],
  "receipt_shards": [
    {
      "path": "objects/receipt-r.jsonl",
      "sha256": "hash3", 
      "byte_length": 10240,
      "record_count": 2,
      "id_prefix": "r",
      "role": "provenance_receipts"
    }
  ]
}
```

### Issue Shard Assignment

```
key = SHA-256(UTF-8 bead ID)
partition = leading hexadecimal characters of key
```

Start with 1-character prefix, split into 2-character prefixes when a shard exceeds thresholds.

### Event Sharding

Events are packed in canonical (origin_store_uuid, origin_event_sequence) order and sealed at:
- 100,000 events per shard, OR
- 64 MiB per shard

Event ranges are inclusive composite pairs: [first_origin_store_uuid, first_origin_event_sequence] through [last_origin_store_uuid, last_origin_event_sequence].

## Atomic Publication

1. Write all new objects to temporary `.tmp` files
2. Verify hashes, counts, and byte lengths
3. Sync all files and parent directory
4. Preserve old pointer as `previous.json` (if exists)
5. Atomically rename new `current.json` 
6. Apply tombstones (deletions) for unreferenced objects
7. Update checkpoint_state in SQLite transaction

A crash before pointer replacement leaves the old generation authoritative. A crash after pointer replacement leaves the new generation authoritative with safe cleanup of unreferenced files.

## Import Operations

### Empty-Store Restore

`bead sync --import-only --input checkpoint/ --restore-into-empty --actor ACTOR`

Requirements:
- Target must be newly initialized with no semantic mutations
- Checkpoint must pass full hash and count validation
- All issue/event/receipt identities must be unique
- Event sequence must be contiguous from 1
- Replayed events must produce checkpoint snapshot

Process:
1. Adopt checkpoint store UUID
2. Stage all records in single transaction
3. Replay events and verify resulting state
4. Insert immutable `restore` receipt
5. Activate committed state

### Merge Operation

`bead sync --import-only --input checkpoint/ --merge --actor ACTOR`

Same-UUID merge:
- Input event stream must share identical hash prefix with target
- Input may extend target, but gaps/rewrites reject entire import

Different-UUID merge:
- Origin identities must be new or byte-identical to existing events
- Identity/hash mismatch is divergence

Merge transaction:
- Insert IDs absent from native state
- Replace when imported updated_at is later
- Retain native state when timestamp is later
- Equal timestamps with different content: conflict (rollback)
- Never delete native issues absent from checkpoint
- Validate final graph for cycles and constraints
- Append imported events with provenance envelopes
- Insert local import-summary event
- Insert immutable `merge` receipt

### Dry Run

`bead sync --import-only --input checkpoint/ --merge --actor ACTOR --dry-run`

Performs identical validation and staging without:
- Changing SQLite rows, events, sequences
- Modifying checkpoint metadata or files  
- Writing durable receipts

Reports prospective counts with `dry_run: true` and `prospective: true` flags.

## Validation Requirements

### Monolithic Validation
- One JSON object per line
- Valid UTF-8 with LF terminators
- All record_type values valid
- Payload schemas validate correctly
- No duplicate IDs within record type
- Canonical ordering maintained
- Total counts match declared values

### Sharded Validation
- All referenced objects exist and hash correctly
- Manifest declared counts match actual shard contents
- No overlapping or duplicate ID ranges
- Event sequences are contiguous per origin
- Composite ordering is consistent
- All paths are relative and normalized
- No symlinks or directory traversal

### Restore Equivalence

Two checkpoints are semantically equivalent if their canonical public corpuses are byte-identical after removing only the newly inserted operation receipt. Compare:

1. Render source through canonical encoder
2. Render target through canonical encoder  
3. Remove new restore/merge receipt from target
4. Compare bytes and counts byte-for-byte

Operational metadata (local ingestion sequences, checkpoint_state, pointers, manifest metadata, mtimes) is excluded from this comparison.

## Conformance Scenarios

Required test fixtures must cover:

1. **Empty workspace**: Zero-byte checkpoint file
2. **Single issue**: Monolithic with one issue and created event
3. **Dependency graph**: Issues with blocks and relates_to edges
4. **Lifecycle states**: Open, in_progress, deferred, closed issues
5. **Event history**: Multiple events per issue in sequence
6. **Provenance receipts**: Restore and merge receipts
7. **Shard transition**: Monolith → sharded at threshold
8. **Incremental flush**: Multiple generations with changed paths
9. **Merge conflict**: Same UUID with divergent events
10. **Restore equivalence**: Monolith and shard produce same state

## Schema Identities

Immutable public schema references for checkpoint records:

- Issue: `urn:bead-rs:schema:issue:native-v1`
- Event: `urn:bead-rs:schema:event:native-v1` 
- Provenance receipt: `urn:bead-rs:schema:provenance-receipt:native-v1`
- Capabilities: `urn:bead-rs:schema:capabilities:native-v1`
- Migration receipt: `urn:bead-rs:schema:migration-receipt:native-v1`

## Security Considerations

- All content addressing uses SHA-256
- Path traversal validation prevents escape from checkpoint base
- Symlink rejection prevents aliasing attacks  
- Atomic pointer replacement prevents partial-state exposure
- Crash safety preserves authoritative old generation until new generation is fully verified and durable

## Appendix: Terminating Definitions

- **Authoritative**: The single source of truth for workspace state
- **Content-addressed**: File naming based on cryptographic hash of contents
- **Deterministic**: Same input produces identical byte-for-byte output
- **Forensic**: Complete audit history for investigation and recovery
- **Immutable**: Never modified after creation, only superseded
- **Monolithic**: Single-file checkpoint format
- **Sharded**: Multi-file manifest-based checkpoint format
- **Provenance**: Record of operation source, actor, and outcome

---

**INDEPENDENT REVIEW REQUIRED**: This draft specification requires review and acceptance by an independent reviewer before F017 implementation can proceed. The reviewer must confirm:

1. Specification is complete and unambiguous
2. All format identities and schemas are properly defined  
3. Conformance scenarios are sufficient
4. No implementation leakage from upstream bead systems
5. Clean-room principles have been maintained

Upon acceptance, this specification should be moved to the accepted normative location and the F017 block can be removed.