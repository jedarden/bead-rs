# Conformance plan v1

Status: normative test plan.

## Lanes

1. **Native invariants:** lifecycle, dependency graph, atomicity, recovery, and
   deterministic export.
2. **Interchange:** profile import/export and unknown-field round trips.
3. **NEEDLE consumer contract:** subprocess commands and JSON parsing.
4. **Concurrency:** competing claimers, readers during writes, checkpoint
   snapshots, and interrupted mutations.
5. **Migration:** dry-run, validation failures, receipts, and non-overwrite
   guarantees.

## Required scenarios

- Empty store and no eligible work.
- One issue through every lifecycle state.
- Multiple priorities with stable selection.
- Blocking chains, diamonds, cycles, and blocker completion.
- Unicode and multiline user content.
- Missing, null, unknown, and future fields.
- Duplicate identifiers and malformed JSON at a known line.
- At least 20 simultaneous claim processes receiving no duplicate successes.
- Process termination before and after commit boundaries.
- Checkpoint import/export followed by semantic comparison.
- NEEDLE invocation in an isolated temporary HOME and workspace.

## Evidence

Each fixture records its author, creation method, governing requirement, and
SHA-256 digest. Test output records the `bead-rs` revision, toolchain, platform,
profile version, and pass/fail result.

## Compatibility declaration

A release may claim `needle-v1` compatibility only when every required NEEDLE
scenario passes against the released binary. Interchange profile claims are
reported separately and include known lossy transformations.

