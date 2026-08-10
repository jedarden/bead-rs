# br-v1 compatibility profile

Status: corrected normative candidate after 2026-08-10 independent review;
re-review pending.

- Profile identifier: `br-v1`
- Observed producer: `br 0.1.28`
- Original author: OpenAI Codex (external clean-room specification/fixture
  role, 2026-08-10)
- Correction author: Claude (Anthropic), 2026-08-10, correcting findings from
  `docs/reviews/f012-independent-review-2026-08-10.md`
- Reviewer: unassigned; must be independent of both authors above

## Provenance and scope

This profile is derived from invented records exercised through the official
`br-v0.1.28-linux_amd64.tar.gz` release asset published on the upstream
project's GitHub Releases page, downloaded and SHA-256-verified against the
matching `.sha256` asset, then run as a black box in disposable scratch
workspaces. No source, upstream tests, upstream fixtures, SQLite schema, or
internal documentation was used or consulted — including the fact that a
local source checkout of the producer project happens to exist elsewhere on
the authoring machine, which this authoring pass did not open, build, or
read from. The supporting corpus is
`research/fixtures/br-v1/observed-valid.jsonl`.

This revision corrects the independent review's central finding: the
original candidate claimed a non-derived native `blocked` status "has no
proven br-v1 representation and must fail or produce an explicit
lossy-conversion report." That claim is false. The real producer accepts
`--status blocked` on `create` (despite `--help` listing only `open,
deferred, in_progress, closed` as the documented values) and exports it as a
literal `status":"blocked"` with no error, warning, or fallback. It also adds
a previously undocumented field, `close_reason`, observed on every closed
record. See the fixture README for the reproduction method. The
dependency-derived-blocked behavior the original candidate also described —
an unfinished `blocks` edge leaves the blocked record's stored `status` at
`open` rather than rewriting it — was independently reproduced and holds.

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
| `close_reason` | optional | preserved string; present on records closed through the public CLI, absent otherwise |
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
| `blocked` | `blocked` | `blocked` while required blocker is unfinished |
| `closed` | `finished` | `closed` |

`blocked` is observed both as an accepted explicit value on `create`/`update`
and as the materialized result of adding an unfinished `blocks` edge: an
unfinished dependency does not rewrite an already-`open` record's stored
status away from `open`, but a record explicitly created or set to `blocked`
exports that value directly, with no error, warning, or coercion to `open`.
The producer performs no CLI-side validation of `status` against its
documented set — any string is accepted and exported verbatim, matching
bf-v1's behavior. Unknown external statuses are retained as extensions and
reported, never silently mapped.

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
comments/data/conditions; and any timestamp precision or dependency metadata
loss. Silent dropping is forbidden. (The original candidate also required
reporting "unsupported non-derived `blocked`"; that requirement is removed —
non-derived `blocked` is a directly supported status, not a lossy case.)

## Conformance

The fixture README defines expected observations. `invalid-cases.json` defines
negative and forward-compatibility cases. Approval requires an independent
reviewer to verify provenance, completeness, hashes, mappings, and edge
direction before implementation uses this candidate as a compatibility claim.
