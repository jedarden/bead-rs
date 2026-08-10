# External profile loss report v1

Status: round-three normative candidate; independent review pending.

This contract applies to every `br-v1` and `bf-v1` import, export, and
migration. It makes profile transformations independently testable. One compact
JSON object followed by LF is emitted even when there is no loss.

## Shape

```json
{"schema_ref":"urn:bead-rs:schema:profile-loss-report:v1","profile":"br-v1","direction":"same-profile-round-trip","input_records":1,"output_records":1,"counts":{"preserved":1,"transformed":0,"omitted":0},"entries":[{"classification":"preserved","scope":"record","field":"*","reason":"record_preserved","count":1}]}
```

`profile` is the external profile being accounted for. `direction` is one of
`import`, `export`, `migration`, or `same-profile-round-trip`. Record counts are
nonnegative integers. `counts` has exactly the three displayed keys. `entries`
is ordered by `classification` (`preserved`, `transformed`, `omitted`), then
`scope`, `field`, and `reason`, all by UTF-8 byte order.

Each entry has the following required members and the conditional `fields`
member described below:

- `classification`: `preserved`, `transformed`, or `omitted`;
- `scope`: `record`, `record_kind`, or `field`;
- `field`: a top-level issue-payload field name, `*` for a record envelope, or
  one of the synthetic record kinds `issue`, `event`, or
  `provenance_receipt`;
- `reason`: a stable lower-snake-case identifier; and
- `count`: the number of input occurrences represented by the entry.

An entry MAY aggregate ordinary fields with identical classification and reason
by using `field:"*"` only when it also has a `fields` array listing every exact
field name in UTF-8 byte order; then `count` equals the length of `fields`.
Otherwise `fields` is absent. Counts are the sums of corresponding entry
counts. Every input record and every input issue-payload top-level field
occurrence is classified exactly once. Non-issue records are classified as a
whole by record kind rather than recursively classifying their payload. A whole
issue represented by a `scope:record` entry classifies the record envelope;
its fields are still classified separately. Producers MAY aggregate identical
entries by summing `count` but MUST NOT combine distinct fields or reasons.
Zero-count entries are omitted. A zero-record conversion therefore has empty
`entries` and all three counts zero.

## Required reasons

Known fields copied without semantic change use `field_preserved`. Unknown
fields retained with their exact JSON value use `extension_preserved`.
Preserved issue envelopes use `record_preserved`. The following reasons are
mandatory when applicable: `status_mapped`, `unknown_status_preserved`,
`explicit_null_preserved`, `empty_absence_coerced`, `timestamp_precision_lost`,
`dependency_order_changed`, `edge_metadata_omitted`, `schema_ref_omitted`,
`event_omitted`, `provenance_receipt_omitted`, `extension_omitted`,
`comment_omitted`, and `structured_data_omitted`.

An unknown field is preserved only if its JSON value is recursively identical,
including object members, array order, numbers, strings, booleans, and null.
Explicit null and absence are distinct. Same-profile round trips MUST preserve
both and classify explicit null as `explicit_null_preserved`. Empty arrays and
empty strings are values, not absence.

## Conflicts

If an extension key collides with a known field after mapping, conversion fails
with a profile transformation conflict before producing an output artifact.
No successful loss report is emitted because no conversion occurred; the
machine diagnostic uses reason `known_extension_collision` and identifies the
field without including user content.

## Required accounting

Every report explicitly classifies occurrences of `schema_ref`, `events`,
provenance receipts, unknown extensions, `comments`, and structured `data`
whenever present. Absence creates no occurrence and therefore no entry. A
target incapable of representing one of these uses the matching omitted
reason. Reordering a bf-v1 dependency array is a transformation, never silent
canonicalization.
