# Unknown-Field Checkpoint Fixtures

Checkpoint fixtures whose issue records carry **unknown JSON fields** — keys
that are not part of the native-v1 issue schema and are not one of the
projected collections. The `Issue` model preserves such keys through
serialization via the `#[serde(flatten)]` extensions catch-all
(`src/model.rs`), and the checkpoint layer persists them in the
`issue_extensions` table on import and re-projects them on export. These
fixtures pin that contract at the file level.

## Layout

```
extensions/
├── checkpoint.jsonl        # monolithic generation: 3 issues + 14 events
├── current.json            # monolithic pointer (active_root -> checkpoint.jsonl)
├── sharded/
│   ├── current.json        # sharded pointer (active_root -> manifests/<sha>.json)
│   ├── manifests/          # content-addressed checkpoint-set-v1 manifest
│   └── objects/            # content-addressed record shards
├── generate.sh             # rebuilds everything above
└── README.md
```

## The unknown-field payload

15 extension fields across the 3 issues, chosen to cover the shapes a
preservation contract can get wrong:

| Issue (title marker)     | Fields | Edge being covered |
|--------------------------|--------|--------------------|
| A — projections coexist  | `x-obsidian-fidelity` | namespaced scalar key |
|                          | `vendor_metadata` | nested object, float + int + array mixing |
|                          | `annotation` | newline + em-dash + non-ASCII text |
|                          | `weighting` | plain integer |
|                          | `ratio` | exactly representable float (0.125) |
| B — awkward shapes       | `trace_context` | two-level nesting with object array |
|                          | `cluster` | heterogeneous array (int, string, array, object) |
|                          | `hollow` | empty object |
|                          | `void` | empty array |
|                          | `ghost` | JSON null |
|                          | `""` (empty key) | empty-string key |
| C — closed issue         | `closure_metadata` | boolean values |
|                          | `pinned_numbers` | zero, negative int, negative float |
|                          | `deeply` | three-level nesting ending in `[true, false, null]` |
|                          | `ünkoded_key` | non-ASCII key and value |

Issue A also carries every projected collection (labels, a `blocks`
dependency edge, a comment, an external reference, structured data), so the
fixtures prove unknown fields coexist with projections — and that
projections never leak into `issue_extensions` as if they were unknown.

## How these fixtures are built

No CLI command produces an unknown field, so `generate.sh` builds them the
only way that stays well-formed by construction:

1. drive the real binary through a lifecycle that populates every
   projection (comments are direct-inserted, like the lifecycle conformance
   suite does);
2. flush the monolithic forensic generation, rewrite the three issue
   records to add the payload, and recompute the pointer's
   `active_root.sha256` from the rewritten bytes — the pointer `path` is
   also repointed to the fixture's fixed `checkpoint.jsonl` name;
3. restore that doctored checkpoint into a second scratch workspace and
   republish with `checkpoint.mode = sharded`, so the sharded fixture is
   unmodified binary output (content addressing intact by construction);
4. restore both fixtures into fresh scratch workspaces and diff the
   restored `issue_extensions` rows against the payload before publishing.

Content-addressed artifacts (sharded objects, manifest) cannot be
hand-edited: any byte change invalidates every hash above it. Regenerate
instead:

```bash
tests/fixtures/extensions/generate.sh          # BEAD_BIN to override the binary
cargo test --test checkpoint_unknown_field_round_trip
```

## Consumers

- `tests/checkpoint_unknown_field_round_trip.rs` — structural validation,
  restore fidelity for both layouts, flush fidelity for both layouts, the
  full export×import chain, merge insert/replace/retain semantics, and the
  known-projection boundary (`resource_keys` is validated, not preserved
  blindly).
