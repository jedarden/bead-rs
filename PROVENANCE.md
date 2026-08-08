# Provenance record

## Origin

`bead-rs` was initialized on 2026-08-07 as a new repository with an independent
Git root. It is intended to be a clean-room implementation based on sanitized
functional specifications and independently created conformance fixtures.

## Permitted implementation inputs

- Specifications under `research/specs/`.
- Fixtures under `research/fixtures/` that record independently constructed
  inputs and observable outputs.
- Published platform and file-format standards.
- Public documentation for third-party Rust dependencies.
- NEEDLE's consumer-side interface requirements, which define what NEEDLE
  needs from any bead store.

## Prohibited implementation inputs

- Source, tests, fixtures, SQL, comments, or internal documentation from
  `beads_rust` or `bead-forge`.
- The existing `bead-forge` implementation or its implementation plans.
- Code-level diffs or translations of another bead implementation.

## Decision log

| Date | Decision | Basis |
| --- | --- | --- |
| 2026-08-07 | Use an independent repository and Git history | Separate provenance from existing implementations |
| 2026-08-07 | License under Apache-2.0 | Permissive terms, explicit patent grant, NOTICE support |
| 2026-08-07 | Use `bead` as the binary name | Distinct invocation; compatibility shims remain opt-in |
| 2026-08-07 | Interoperate through versioned CLI/JSONL profiles | Avoid source reuse and cross-tool SQLite mutation |
| 2026-08-07 | Bootstrap with Marathon Coding | Independent headless iteration harness avoids a runtime dependency on another bead implementation |
| 2026-08-07 | Specify an independent normalized SQLite schema | Satisfy public storage semantics without reproducing another implementation's schema or SQL |
| 2026-08-07 | Record sanitized `bf 0.4.0` process-boundary observations | Isolated black-box specification activity; implementation details deliberately excluded |
| 2026-08-07 | Use JSONL as the portable backup and recovery boundary | Keep SQLite focused on private ACID live state and avoid a second native backup format |
| 2026-08-07 | Identify each bead's public schema with `schema_ref` | Improve explicit cross-tool validation without exposing the SQLite schema |
| 2026-08-07 | Keep comments complete in backup but optional in retrieval | Preserve recoverability while allowing agents to control conversation context |
| 2026-08-07 | Use declarative conditional dependencies and schema-bound data | Extend coordination without scripts, SQL exposure, or executable plugins |
| 2026-08-08 | Adopt versioned intelligent claim scheduling | Combine deterministic impact, aging, rotation, failure-aware retry, and bounded context without changing `fifo-v1` silently |
| 2026-08-08 | Standardize native priority on P0-P4 | Match the observed ecosystem range and avoid lossy P5 compatibility mappings |

Any future exposure or provenance exception must be appended here; do not
rewrite prior entries.

## Marathon Coding provenance

Marathon Coding was developed independently and in parallel with the method
later popularized as the Ralph loop. Its use here does not imply derivation
from Ralph. The `bead-rs` integration repeatedly launches fresh headless coding
iterations from a committed, hot-reloadable mission and durable repository
artifacts.
