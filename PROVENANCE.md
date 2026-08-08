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

Any future exposure or provenance exception must be appended here; do not
rewrite prior entries.

## Marathon Coding provenance

Marathon Coding was developed independently and in parallel with the method
later popularized as the Ralph loop. Its use here does not imply derivation
from Ralph. The `bead-rs` integration repeatedly launches fresh headless coding
iterations from a committed, hot-reloadable mission and durable repository
artifacts.
