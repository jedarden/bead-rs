# ADR-025: Surface advisory findings at write time and report partial scan coverage

**Status**: Proposed

**Date**: 2026-10-03

**Decision-makers**: bead-rs maintainers, pending independent review

**Narrows**: ADR-014's advisory tier and its doctor reporting.

## Context

ADR-014 gave the advisory tier one job: report what the blocking tier will
not reject. On 2026-10-03 it was not doing that job.

- Across 82 fleet workspaces it held 37,547 findings, more than 99% from the
  entropy rule. A real token and a commit hash carried the same rule
  identifier.
- The rule's per-character Shannon threshold of 4.3 bits cannot be reached by
  a hexadecimal string, whose maximum is 4, and is a coin flip for a random
  string under about 32 bytes. It therefore reported long hashes and paths
  while missing hexadecimal keys and short application keys.
- Nothing is printed when a mutation with advisory findings succeeds. The
  findings appear only in `bead doctor`, which no agent runs after a write.
- In 8 of 82 workspaces the doctor check returned an I/O error instead of a
  verdict, because one retained generation's root file was missing. Live
  rows were not scanned at all. One cause is the tombstoned previous root
  after a mode transition (`beadrs-9e6f9b9e`); the check fails the same way
  for any unreadable generation.

## Decision

1. **Advisory candidates are selected by shape, not by entropy.** The
   unlabelled-string rule reports a token-alphabet run that passes the
   ADR-023 randomness qualifier and is not hash-shaped: not a hexadecimal
   digest length, a UUID, a generation identifier, or a bead identifier. The
   Shannon threshold is removed.
2. **A successful mutation says when it admitted advisory findings.** One
   redacted line on standard error gives the count, the rule identifiers, and
   the doctor command. Machine output gains an additive count. Exit status
   does not change.
3. **Doctor scans each source independently.** Live rows, the current
   generation, and the previous generation each get a coverage entry:
   `scanned`, `absent`, or `unreadable`. An unreadable source makes the check
   an error and still returns every finding from the sources that were
   scanned. A legitimately missing previous generation is `absent` and is
   not an error.
4. **Volume is a release criterion.** Ruleset 4's advisory count over the
   replayed fleet stores must be at most one tenth of ruleset 3's.

## Rationale

A report that nobody reads and that cannot be triaged protects nothing. The
tier's value is in the few findings that are real, so the selection rule has
to exclude the shapes that are certainly not credentials. Hash lengths and
identifiers are exactly those shapes, and they are the bulk of the 37,547.

The write-time line puts the finding in front of the agent that just caused
it, while the text is still in its context and before the checkpoint is
pushed. It stays advisory: the exit status is unchanged, so no caller's
control flow moves.

Partial coverage follows the plan's rule that unknown state fails closed
without discarding what is known. An empty verdict reads as "no findings" to
a fleet sweep. Eight workspaces were silently unaudited that way.

## Consequences

### Benefits

- The advisory list becomes short enough to read.
- Hexadecimal and short keys that carry a label move to the blocking tier
  under ADR-023; unlabelled token-shaped strings are still reported.
- An agent learns of an admitted finding in the same invocation.
- A fleet audit can tell "clean" from "not scanned".

### Drawbacks

- An unlabelled hexadecimal key is no longer reported by any tier. It was
  reported before only by coincidence of length, and it cannot be told from
  a hash.
- One more line on standard error. A consumer that treats any standard-error
  output as failure must be checked against `needle-cli-contract-v1`.
- `coverage` is a new member of the check details.

### Alternatives Considered

- **Raise or lower the entropy threshold**: rejected. No threshold separates
  hexadecimal keys from hashes; the measure is the wrong one.
- **Print every advisory finding at write time**: rejected. One summary line
  is enough to send the agent to doctor, and it bounds output on a 4 MiB
  field.
- **Make advisory findings fail the command**: rejected. That is the blocking
  tier; ADR-014 and ADR-023 decide its membership.
- **Report a missing previous root as an error always**: rejected. After a
  mode transition its absence is the specified state.

## Implementation

BR-T38 makes doctor scan each source independently and report coverage; it
restores accepted `secret-rejection-v1` section 5 behavior, depends on
`beadrs-9e6f9b9e`, and does not wait for review. After the specification is
accepted, BR-T43 replaces the entropy rule and adds the write-time line.
Sections 5 and 6 of `research/specs/secret-ruleset-v4.md` are normative.

## Related

- ADR-007; ADR-014; ADR-023
- R040, BR-T38, and BR-T43 in `docs/plan/plan.md`
- `beadrs-9e6f9b9e`

## Supersedes

None.
