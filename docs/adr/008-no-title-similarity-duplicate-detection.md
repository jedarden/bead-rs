# ADR-008: Do not detect duplicate beads by title similarity

**Status**: Rejected

**Date**: 2026-08-16

**Decision-makers**: bead-rs maintainers

## Context

Duplicate beads cause duplicated work. The failure is documented downstream
with a concrete incident: two workers produced two byte-identical commits at
the same second for the same bead, and the surrounding guidance instructs
authors to check by hand whether an existing bead already covers a fix before
creating another.

`create` today offers no help here. Verified 2026-08-16: creating a second bead
with a byte-identical title to an existing open bead succeeds silently, with no
warning.

The obvious-looking fix is for `create` to compare the proposed title against
open beads and warn on a near match. This ADR records why that was considered
and rejected, because it is the first idea most people have and it will
otherwise be proposed repeatedly.

## Decision

Rejected. `bead-rs` will not infer duplication from title text — not as a hard
error, and not as a soft warning.

Duplicate prevention is owned by **R032**, which binds an external reference
inside the insert transaction and returns the existing bead's identity when
that reference is already bound.

## Rationale

**It contradicts an accepted decision.** ADR-001 settled this project's stance
on title inference in a directly analogous case, and settled it absolutely: "Do
not infer the relationship from issue titles under any circumstance." That ADR
chose a *declared* structural edge over a title heuristic for diagnosing
inverted verification gates. The reasoning transfers without modification:
titles are prose written for humans, they are not a stable machine-readable
declaration, and building behavior on them makes correctness depend on
phrasing. Adopting title similarity here would fork the project's position on
the same question depending on which command you are in.

**A soft warning is not a meaningfully weaker commitment.** It still requires
choosing a similarity function and a threshold; it still fires on legitimate
sibling beads, which are extremely common in this domain ("Add X to module A" /
"Add X to module B", or a deliberate investigate/implement pair for one defect,
which the downstream guidance explicitly calls correct). A warning that fires
mostly on legitimate work is trained away within days, at which point it costs
maintenance and delivers nothing.

**The real mechanism is structural, and R032 addresses it structurally.** The
documented incident is a duplicate *claim* on one unit of work, arising from
dispatchers materializing work from the same external identifier concurrently.
Where a stable external identifier exists, R032 makes duplication impossible
inside the transaction — a guarantee no text heuristic can offer. Where no such
identifier exists, the honest answer is that the store cannot know two prose
titles mean the same thing, and pretending otherwise with a threshold is worse
than declining.

## Consequences

### Benefits

- Preserves a single, consistent project-wide stance on title inference.
- Avoids a false-positive warning on sibling beads, which are normal here.
- Keeps duplicate prevention on the path that can actually guarantee it.

### Drawbacks

- The human/agent-authored case — two people independently writing beads for
  the same work with no shared external reference — remains unprevented by the
  tool, and stays a workflow-discipline problem for the layer above.
- Adopters arriving from tools that do fuzzy duplicate detection will
  experience its absence as a missing feature.

### Alternatives Considered

- **Exact-title match warning only**: rejected. Narrower, but still title
  inference, and trivially defeated by one differing character — the appearance
  of a guarantee without one.
- **`doctor` check listing similar open titles**: rejected for the same reason.
  Moving a heuristic from `create` to `doctor` does not make it less of a
  heuristic; it only makes it easier to ignore.

## Implementation

None. No code change. This ADR exists to record the rejection and its reasoning
so the idea is not re-litigated; anyone wanting duplicate prevention should be
directed to R032.

## Related

- **ADR-001** — declared verification edges over title heuristics; the
  controlling precedent
- **R032** — idempotent create by unique reference; where duplicate prevention
  belongs
- **R011** — external references, which R032 binds
