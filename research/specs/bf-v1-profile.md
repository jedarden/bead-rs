# bf-v1 compatibility profile

Status: round-four corrected normative candidate; independent review pending.

- Profile identifier: `bf-v1`
- Observed producer: `bf 0.4.0`
- Original author: OpenAI Codex (external clean-room specification/fixture
  role, 2026-08-10)
- Correction author: Claude (Anthropic), 2026-08-10, correcting findings from
  `docs/reviews/f012-independent-review-2026-08-10.md`
- Round-three author: OpenAI Codex, 2026-08-10, correcting findings 1-6 from
  `docs/reviews/f012-independent-review-round2-2026-08-10.md`
- Round-four author: OpenAI Codex, 2026-08-10, correcting the sole finding from
  `docs/reviews/f012-independent-review-round3-2026-08-10.md`
- Reviewer: unassigned; must be independent of both authors above

## Provenance and scope

This profile is derived from invented records exercised through the real
`bf 0.4.0` binary in disposable scratch workspaces. No source, upstream
tests, upstream fixtures, SQLite schema, or internal documentation was used.
The supporting corpus is `research/fixtures/bf-v1/observed-valid.jsonl`.

This revision corrects three findings from the 2026-08-10 independent
review, each reproduced directly against the real producer: the `events`
field was undocumented and silently missing from the fixture; the claimed
lexical ordering of the `dependencies` array does not hold (it is creation
order); and `deferred` is accepted and exported by the producer rather than
being an established bf-v1 mapping question. See the fixture README for the
reproduction method.

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
| `events` | required by observed writer | array of native audit-event objects (`id`, `issue_id`, `type`, `actor`, `created_at`); every observed record carries at least one `created` event; preserve as an extension on same-profile round trip and report if a target cannot store it |
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
| `blocked` | `blocked` | `blocked` (whether explicitly stored or materialized by the target) |
| `closed` | `finished` | `closed` |

`blocked` is observed both as an accepted explicit value and as the materialized
result of adding an unfinished `blocks` edge. The producer performs no
CLI-side validation of `status` against a fixed enum: any string, including
`deferred`, is accepted and exported verbatim. `deferred` is therefore
observable on the wire, but bf-v1 defines no native lifecycle mapping for it;
treat it exactly as any other unrecognized status — preserve and report,
never silently coerce it to a native lifecycle state. Unknown values in
general are preserved and reported rather than guessed.

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
export in deterministic lexical order, independently confirmed by inserting
labels out of order and observing the sorted result. **The `dependencies`
array does not**: it is emitted in the order the edges were created, not
sorted by `depends_on_id` or any other key. This was independently reproduced
by adding two blockers in a deliberately non-alphabetical order and observing
the export preserve exactly that order; the earlier candidate's claim of
"deterministic lexical/canonical ordering" for dependencies did not hold and
is corrected here. Import and same-profile export MUST preserve the exact input
array order. A conversion to a representation that cannot retain it MUST emit
a `dependency_order_changed` transformed entry; reordering without that entry
is nonconforming.

## Loss reporting

Every conversion emits the machine-readable report defined by
`profile-loss-report-v1.md`, including when all transformed and omitted counts
are zero. Classification covers every input top-level field and record and
specifically accounts for `schema_ref`, events, provenance receipts,
extensions, comments, structured data, explicit nulls, unknown statuses,
timestamp precision, dependency order, edge metadata, and empty-versus-absent
coercions. Silent dropping is forbidden. An unknown/known-field collision is a
transformation conflict and fails before output; it is never resolved by
overwriting either value.

## Conformance

The fixture README defines expected observations. `round-trip-cases.json`
defines exact same-profile preservation cases and expected reports;
`invalid-cases.json` defines isolated validation cases. Approval requires an independent
reviewer to verify provenance, completeness, hashes, mappings, and dependency
direction before implementation uses this candidate as a compatibility claim.
