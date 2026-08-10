# br-v1 compatibility profile

Status: authored normative candidate; independent review pending.

- Profile identifier: `br-v1`
- Observed producer: `br 0.1.28`
- Author: OpenAI Codex (external clean-room specification/fixture role)
- Authored: 2026-08-10
- Reviewer: unassigned; must be independent of this author

## Provenance and scope

This profile is derived from invented records exercised through the public
compiled CLI in an isolated `agent-sandbox` workspace. No source, upstream
tests, upstream fixtures, SQLite schema, or internal documentation was used.
The supporting corpus is `research/fixtures/br-v1/observed-valid.jsonl`.

It specifies JSONL interchange, not the producer's live database or every CLI
response. Behavior not covered here is unsupported until separately observed,
sanitized, versioned, and reviewed.

## Transport and field matrix

Each nonblank line is one UTF-8 JSON object terminated by LF.

| Field | Presence | Type and mapping |
| --- | --- | --- |
| `id` | required | nonempty string; native stable ID |
| `title` | required | string |
| `status` | required | lifecycle mapping below |
| `priority` | required | integer 0..4, unchanged |
| `issue_type` | required | string, unchanged |
| `created_at` | required | RFC 3339 instant |
| `updated_at` | required | RFC 3339 instant |
| `description` | optional | string; absent maps to native absence |
| `assignee`, `owner` | optional | string; absent maps to native absence |
| `labels` | optional | array of strings; absent maps to empty native set |
| `dependencies` | optional | dependency objects defined below |
| `closed_at`, `due_at`, `defer_until` | optional | RFC 3339 instant |
| `estimated_minutes` | optional | nonnegative integer |
| `external_ref`, `source_repo`, `created_by` | optional | preserved string |
| `compaction_level`, `original_size` | optional | preserved nonnegative integer |
| `schema_ref` | not observed | native export omission is reported as profile loss |
| other fields | extension | preserve original JSON value on same-profile round trip |

## Null and absence

Observed writers omit unset optional fields. Import treats absent optional
strings as absent, absent labels/dependencies as empty collections, and explicit
JSON null as an extension value requiring a loss/warning entry if the native
field cannot represent null distinctly. Empty strings and empty arrays remain
explicit values and must not be conflated with null. Numeric zero is a value.

## Status mapping

| br-v1 | Native base status | Reverse export |
| --- | --- | --- |
| `open` | `open` | `open` |
| `in_progress` | `in_progress` | `in_progress` |
| `deferred` | `deferred` | `deferred` |
| `closed` | `finished` | `closed` |

Dependency-derived blocked readiness does not change an observed stored `open`
status. Native `blocked` therefore exports as `open` plus dependency edges when
the block is derived; a non-derived native `blocked` value has no proven br-v1
representation and must fail or produce an explicit lossy-conversion report.
Unknown external statuses are retained as extensions and reported, never
silently mapped.

## Dependencies

Each item uses `issue_id` for the blocked issue, `depends_on_id` for the blocker,
and `type` for the kind. The observed kind is `blocks`. Optional observed edge
metadata includes `created_at`, `created_by`, `metadata`, and `thread_id` and is
preserved. The canonical direction is therefore `(issue_id, depends_on_id,
type) = (blocked, blocker, kind)`. Import validates references and cycles before
activation; it does not reverse edges based on CLI argument order.

## Timestamps and ordering

Timestamps are RFC 3339. Observed output uses UTC `Z` and nanosecond precision;
readers accept valid offsets, preserve the represented instant and available
precision, and reject invalid values. Export sorts issues by ID. Labels are a
set and export in deterministic lexical order. Dependency order is canonical by
blocked ID, blocker ID, then kind.

## Loss reporting

Every conversion reports: missing native `schema_ref`; unknown fields or status
values; explicit nulls that native fields cannot distinguish; native-only
comments/data/conditions; unsupported non-derived `blocked`; and any timestamp
precision or dependency metadata loss. Silent dropping is forbidden.

## Conformance

The fixture README defines expected observations. `invalid-cases.json` defines
negative and forward-compatibility cases. Approval requires an independent
reviewer to verify provenance, completeness, hashes, mappings, and edge
direction before implementation uses this candidate as a compatibility claim.
