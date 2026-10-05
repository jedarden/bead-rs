# Unknown-field checkpoint corpus v1

This is an independently authored, deterministic corpus for the native
checkpoint contracts. It exercises an unknown JSON member at each required
object level: issue, event, dependency, external reference, structured-data
namespace, resource-key probe, provenance receipt, and `current.json`.

The values are deliberately small but use different JSON types so a consumer
cannot pass by preserving only strings or objects. `expected.json` is the
machine-readable inventory of the semantic known fields and exact unknown
values. The checkpoint records are ordered as required by
`research/specs/checkpoint-set-v1.md`: issues, events, then receipts.

## Files

- `checkpoint.jsonl` is the five-record monolithic checkpoint corpus: two
  issues, two events, and one provenance receipt.
- `current.json` selects `checkpoint.jsonl` and contains the pointer-level
  unknown member.
- `expected.json` records the semantic keys/values, JSON pointers, and hashes
  expected by later restore/export tests.
- `resource-key.json` is the resource-key object probe. The native resource
  contract represents `resource_keys` in an issue as sorted strings, so an
  object-level unknown-field case cannot be embedded in a valid native issue.
  This companion preserves that required probe without changing the native
  checkpoint shape.
- `verify.sh` checks JSON validity, record ordering/counts, exact unknown
  values, pointer linkage, and all deterministic hashes.

The corpus uses no live workspace IDs, timestamps, generated IDs, or producer
output. Its UUID, instants, keys, and values are fixed fixture data. The
`source_root_sha256` receipt value is the digest of the four records before
the receipt; `receipt_sha256` is the digest of the sorted compact receipt
object with its digest member removed; and `current.json` authenticates the
complete JSONL bytes.

## Governing requirements and provenance

The fixture is authored from the sanitized requirements in:

- `research/specs/checkpoint-set-v1.md` (checkpoint record and pointer shape)
- `research/specs/native-field-guide-v1.md` (issue, event, and receipt members;
  unknown-member preservation)
- `research/specs/extended-bead-payload-v1.md` (structured-data envelope)
- `research/specs/native-resource-locks-v1.md` (resource-key representation)
- `research/specs/clean-room-protocol.md` (independent fixture boundary)

No source code, fixture, SQL, or documentation from another bead
implementation was consulted or copied. Run the fixture-only check with:

```text
research/fixtures/unknown-fields-v1/verify.sh
```

Later restore/export tests may use `current.json` as the source generation
and `expected.json` as the preservation oracle. They should compare unknown
values exactly, including null, array order, numeric type, and nested object
members, while treating the listed known fields as semantic state.
