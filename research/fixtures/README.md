# Independent fixtures

Fixtures in this directory must be invented for `bead-rs` or captured as
black-box observations under `research/specs/clean-room-protocol.md`.

Every fixture set must include provenance metadata, its governing requirement,
and expected observable behavior. Do not copy fixtures from another bead
implementation or from a real workspace.

The recovery quarantine scenario manifest is
`recovery-finding-quarantine-v1.json`. It deliberately contains only state,
transition, diagnostic-field, and scenario identifiers. Candidate values are
assembled at runtime by executable conformance tests and must never be added
to the manifest.
