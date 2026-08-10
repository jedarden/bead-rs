# bf-v1 compatibility profile

Status: authored normative candidate; independent review pending.

- Profile identifier: `bf-v1`
- Observed producer: `bf 0.4.0`
- Author: OpenAI Codex (external clean-room specification/fixture role)
- Authored: 2026-08-10
- Reviewer: unassigned; must be independent of this author

## Provenance and scope

This profile is derived from invented records exercised through the public
compiled CLI in an isolated `agent-sandbox` workspace. No source, upstream
tests, upstream fixtures, SQLite schema, or internal documentation was used.
The supporting corpus is `research/fixtures/bf-v1/observed-valid.jsonl`.

It specifies JSONL interchange, not the producer's live database or every CLI
response. Unobserved behavior remains unsupported pending a separate sanitized
observation and review.

## Transport and field matrix

Each nonblank line is one UTF-8 JSON object terminated by LF.

| Field | Presence | Type and mapping |
| --- | --- | --- |
| `id` | required | nonempty string; native stable ID |
| `title` | required | string |
| `description`, `design`, `acceptance_criteria`, `notes` | required by observed writer | string; empty string means present but empty |
| `status` | required | lifecycle mapping below |
| `priority` | required | integer 0..4, unchanged |
| `issue_type` | required | string, unchanged |
| `created_at`, `updated_at` | required | RFC 3339 instant |
| `assignee` | optional | string; absence means unassigned |
| `labels` | optional | array of strings; absence means empty set |
| `dependencies` | optional | dependency objects defined below |
| `closed_at` | optional | RFC 3339 instant |
| `close_reason`, `closed_by_session`, `source_repo` | optional | preserved string |
| `compaction_level` | optional | preserved nonnegative integer |
| `schema_ref` | not observed | native export omission is reported as profile loss |
| other fields | extension | preserve original JSON value on same-profile round trip |

## Null and absence

The observed writer emits empty strings for the four content fields above and
omits other unset optional fields. Import preserves the distinction among empty
string, explicit null, and absence. Explicit null is retained as an extension
or reported when native storage cannot represent it. Absent labels and
dependencies mean empty collections; explicit empty arrays remain explicit.
Numeric zero is a value.

## Status mapping

| bf-v1 | Native base status | Reverse export |
| --- | --- | --- |
| `open` | `open` | `open` |
| `in_progress` | `in_progress` | `in_progress` |
| `blocked` | `blocked` | `blocked` while required blocker is unfinished |
| `closed` | `finished` | `closed` |

`blocked` is observed both as an accepted explicit value and as the materialized
result of adding an unfinished `blocks` edge. Unknown values are preserved and
reported rather than guessed. No `deferred` mapping has been established for
this version.

## Dependencies and CLI direction

The public form is `bf dep add BLOCKER --blocks BLOCKED`. The exported item is
stored on the blocked record: `issue_id` is BLOCKED, `depends_on_id` is BLOCKER,
and `type` is the dependency kind. The observed default kind is `blocks`.
Optional edge fields `created_at`, `created_by`, and `thread_id` are preserved.
Canonical direction is `(blocked, blocker, kind)`. Import validates references
and cycles before activation and never infers direction solely from position.

## Timestamps and ordering

Timestamps are RFC 3339. Observed output uses UTC `Z` with nanosecond precision;
valid offsets are accepted, represented instants and available precision are
preserved, and invalid values are rejected. Export sorts issues by ID. Labels
and dependencies use deterministic lexical/canonical ordering.

## Loss reporting

Every conversion reports: missing native `schema_ref`; unknown fields/statuses;
explicit nulls native fields cannot distinguish; unsupported native deferred
state; native-only comments/data/conditions; empty-versus-absent coercions; and
any timestamp precision or edge metadata loss. Silent dropping is forbidden.

## Conformance

The fixture README defines expected observations. `invalid-cases.json` defines
negative and forward-compatibility cases. Approval requires an independent
reviewer to verify provenance, completeness, hashes, mappings, and dependency
direction before implementation uses this candidate as a compatibility claim.
