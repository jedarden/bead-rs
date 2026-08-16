# ADR-007: CLI errors for immutable fields and near-miss flags must name the remedy

**Status**: Proposed

**Date**: 2026-08-16

**Decision-makers**: bead-rs maintainers

## Context

Several correct-looking invocations fail with raw argument-parser output that
describes the parse failure rather than the domain rule behind it. Verified
against the shipped CLI on 2026-08-16:

```
$ bead update <id> --title "New title"
error: unexpected argument '--title' found
  tip: to pass '--title' as a value, use '-- --title'

$ bead close <id> --body "..."
error: unexpected argument '--body' found
  tip: to pass '--body' as a value, use '-- --body'
```

Both messages are actively misleading. The `tip` proposes escaping the flag as
a positional value, which is never what the operator wanted and, if followed,
produces a second confusing failure. Neither message states the actual rule:
that title is fixed at `create` time, or that `close` takes `--reason`.

The evidence that these are load-bearing is that downstream environment
documentation carries explicit warnings about both — a note that `update`
cannot change title/description/priority/issue-type/labels, and a note that
`close` needs `--reason`, "not `--body`: `--body` is not a valid flag and the
close will fail." Rules that must be written down in someone else's runbook to
prevent recurring failures are rules the tool should be stating at the point of
failure.

`release` already demonstrates the standard this ADR generalizes. Its refusal
of an assigned open issue reads:

```
Cannot release assigned open issue - use 'update --clear-assignee' instead
```

That message names the rule and the remedy in one line, and it is why the state
it guards is recoverable without consulting documentation.

## Decision

Where a command rejects an argument that a competent operator would reasonably
expect to work, the error must name the domain rule and the remedy, in the
style `release` already uses — not surface a bare parser error.

Applies at minimum to:

- fields that are immutable after `create` (title, description, priority,
  issue-type) — state that the field is fixed at creation, and name the
  supported alternative where one exists (for example `label add|remove`);
- near-miss flag spellings on lifecycle commands, notably `close --body` →
  `--reason`.

This is a message-quality decision, not a change to which operations are
permitted. No currently-rejected invocation becomes accepted.

## Rationale

These are the errors most likely to be a user's first contact with the tool's
model — they occur while someone is learning what beads are and what may change
after creation. A parser error teaches nothing and, because of the `tip`,
teaches something false. A rule-naming error teaches the model exactly once.

Cost is low and contained: the domain rules are already settled and already
enforced, so this adds no semantics and no new failure modes. It also directly
retires two lines of downstream documentation, which is the clearest available
signal that the tool was under-explaining itself.

## Consequences

### Benefits

- Two recurring, separately-documented papercuts stop needing documentation.
- Error text becomes consistent with the standard `release` already sets.
- New adopters learn the immutability model at the moment it bites, in place.

### Drawbacks

- Custom messages for near-miss flags require deciding which misspellings are
  worth special-casing; an unbounded alias list is its own maintenance burden.
  The specification should bound this to flags that exist on sibling commands
  (`--body` exists on other tooling and on `create`-shaped mental models),
  rather than general fuzzy matching.
- Diverging from the argument parser's default rendering means the project owns
  the wording, including in future parser upgrades.

### Alternatives Considered

- **Make the fields mutable instead**: out of scope here, and a much larger
  decision — mutable priority/issue-type interacts with ordering determinism
  and audit semantics. This ADR deliberately does not prejudge it.
- **Accept `--body` as an alias for `--reason`**: rejected. Silent aliasing
  hides the model rather than teaching it, and would let a wrong mental model
  persist into automation.
- **Leave to documentation**: rejected. Documentation is the current state, and
  it is why these appear in a downstream runbook at all.

## Implementation

Post-0.1, small. Touches argument definitions and the error paths of `update`
and `close`; no schema, storage, or lifecycle change. Conformance scenarios
should assert the remedy text, not merely a nonzero exit, so the affordance
cannot silently regress to a parser default.

Tracked as post-0.1 roadmap item **R037**.

## Related

- Section 5, Command and process contract
- `release`'s assigned-open-issue refusal — the standard being generalized
- **ADR-005** — same principle applied to a diagnostic: name the exact remedy
