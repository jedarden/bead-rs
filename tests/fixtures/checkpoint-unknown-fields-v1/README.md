# Unknown-field checkpoint fixture v1

This is an independently authored native-v1 checkpoint-set fixture for
conformance tests. It is intentionally small, but carries one distinct
unknown JSON member at every checkpoint object level required by the native
restore contract:

| level | sentinel | location |
| --- | --- | --- |
| issue | `future_issue_fixture_sentinel` | issue object |
| event | `future_event_fixture_sentinel` | event object |
| dependency | `future_dependency_fixture_sentinel` | dependency entry in the issue projection |
| external reference | `future_reference_fixture_sentinel` | external-reference entry |
| structured data | `future_data_fixture_sentinel` | structured-data envelope |
| resource key | `future_resource_key_fixture_sentinel` | resource-key entry |
| receipt | `future_receipt_fixture_sentinel` | provenance-receipt object |
| pointer | `future_pointer_fixture_sentinel` | `current.json` |

The sentinel names are not native-v1 fields and are deliberately distinct.
`expected.json` is the comparison companion: it identifies each location and
records the exact JSON value that must survive. The test compares these
sentinels after native `sync import-only --restore-into-empty` and a fresh
`sync flush-only` publication. It does not compare generated generation IDs,
restore receipt IDs, publication timestamps, or content hashes; those are
documented native rewrites.

The fixture uses monolithic mode so the complete restore input is easy to
review: `current.json` selects `objects/unknown-fields.jsonl`, and the
pointer's root hash and record counts are checked by the targeted test and by
the native restore verifier.

This corpus is invented for bead-rs and does not contain another
implementation's database or source material.
