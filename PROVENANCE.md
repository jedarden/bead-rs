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

## F017 clean-room boundary violation (2026-08-09)

**Violation Type**: Implementation proceeded without independent specification review as required by plan.md section 6.1 and section 15.

**Sequence of Events**:
1. 2026-08-09 04:14:07 UTC - Created research/specs/checkpoint-set-v1.md as DRAFT specification authored by implementer
2. 2026-08-09 04:50:45 UTC - Implemented F017 forensic checkpoint system (36 minutes later)

**Nature of Violation**: The plan explicitly states:
- Section 6.1: "Sections 6.1-6.3 are nonnormative F017 design input until an independently reviewed `research/specs/checkpoint-set-v1.md` defines the format... Under `AGENTS.md`, no F017 implementation may be derived from these sections alone."
- Section 15: "F017 still needs an independently authored and reviewed normative `checkpoint-set-v1.md` plus conformance fixtures."

The checkpoint-set-v1.md specification was created by the same agent that implemented F017, was explicitly marked "DRAFT - Requires independent review before F017 implementation," and was implemented from without the required independent review.

**Impact**: F017 implementation code exists and is marked as passing in the feature ledger, but this violates the clean-room boundary that requires independent specification review before implementation.

**Corrective Action Required**: F017 must be considered incomplete pending truly independent specification review. The implementation should only be activated after a specification that has been independently reviewed (not by the implementer) exists and is approved.

**Evidence**: Git commits 08f094d (spec creation) and 3d4951c (F017 implementation) showing same author and 36-minute interval without independent review.
