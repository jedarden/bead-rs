# F012 round-four independent artifact review

Date: 2026-08-10 UTC

Reviewer: OpenAI Codex, the round-three reviewer, independently reviewing a
later correction authored by a different session; independent of the original
fixture author, all correction authors, and the F012 implementation author

Reviewed commit: `487ab0e5ce8c46e6668f59ea6abe8b8ddbbe0dbd`
(`main`, equal to `origin/main` at review start)

Disposition: **accepted**. The complete F012 `br-v1`/`bf-v1` specification and
fixture artifact baseline is accepted for use as the independent compatibility
baseline. This acceptance does not approve implementation code, establish
implementation conformance, or make F012 pass.

## Independence and method

I reviewed the round-four request, the round-three review and finding, the
corrected bf-v1 profile, its unchanged observed fixture and manifest, the
governance-exception request, provenance, and the exact Git diff from the
round-three review commit. I did not inspect `src/profile`, implementation
tests, or any producer source, tests, fixtures, database, SQL, internal
documentation, or local producer checkout. I did not modify any reviewed
specification or fixture artifact.

Commands included `git rev-parse HEAD`, `git rev-parse origin/main`, scoped
`git diff` and `git diff --word-diff`, `sha256sum`, `jq`, and `rg`. The working
tree was clean and both commit checks returned the requested commit before the
review record was written.

## Recomputed hashes

| Artifact | SHA-256 | Result |
| --- | --- | --- |
| Corrected `research/specs/bf-v1-profile.md` | `e321eea25ffb72f3afff6465ed1dfd4bc3121cf274323d8d7eef7e727de2af00` | match |
| Unchanged `research/fixtures/bf-v1/manifest.json` | `8ce83d452ef0ee185a33ac800d713479cc27195507a5f12c340d6a33e3a99e45` | match |
| Unchanged governance-exception request | `29a3f38b2a495937ef7b4006a17988646d5270ec706cc0af084666b9e893dc16` | match |

For completeness, the unchanged companion baseline also recomputed as:

| Artifact | SHA-256 |
| --- | --- |
| `research/specs/profile-loss-report-v1.md` | `f7c79e7cc581294ea08543452c6da2df0b700c3218fa2153eb1c1b8ae0dd62a3` |
| `research/specs/br-v1-profile.md` | `f17ca732e99af46dd232b83753f51177ee44f5c9c2c94c9c4e75c07d3d1d6e9e` |
| `research/fixtures/br-v1/manifest.json` | `eaf7f43ff72a22ac8ad4693106c0be76d82a06734d863511cd225f2f40f843fd` |

## Scope and finding verification

The diff from reviewed round-three commit
`bc6f91d9610f9e29e3c48eddf4c76b9bd67fa598` to the requested commit changes
only four files: the bf-v1 profile, the round-four request, and append-only
provenance/progress records. Within the reviewed profile, the only normative
change replaces:

`blocked while required blocker is unfinished`

with:

`blocked (whether explicitly stored or materialized by the target)`

The other profile edits only update candidate status and authorship metadata.
No fixture, manifest, shared loss-report contract, br-v1 artifact, producer
identity, or other bf-v1 semantic rule changed.

The corrected cell is unambiguous and consistent with the unchanged following
prose: bf-v1 accepts both explicitly stored `blocked` and dependency-derived
materialized `blocked`. It is also consistent with
`research/fixtures/bf-v1/observed-valid.jsonl`, whose `bf-bma` record has
`status:"blocked"` and no `dependencies` member, and with the fresh round-three
black-box reproduction recorded in the prior review. The table no longer
permits an adapter to coerce explicit blocked merely because no unfinished
dependency exists. No new internal inconsistency was introduced.

## Governance-exception carry-forward

The round-three approval of the narrow bf-v1 binary governance exception
**carries forward** to corrected profile hash
`e321eea25ffb72f3afff6465ed1dfd4bc3121cf274323d8d7eef7e727de2af00`.
The sole semantic edit does not assert a new producer fact: it makes the table
agree with the explicit-blocked fact already present in the unchanged fixture,
profile prose, and independently reproduced observation on which the approval
was based. The executable identity, fixture manifest, exception request, and
all governed observations are unchanged. The prior expiration condition also
carries forward: discovery of an official attested artifact requires
corroboration or regeneration against it.

## Final disposition and remaining gate

**Accepted.** The complete F012 external-artifact baseline consists of the
three unchanged companion hashes above, corrected bf-v1 profile hash
`e321eea25ffb72f3afff6465ed1dfd4bc3121cf274323d8d7eef7e727de2af00`,
unchanged bf-v1 manifest hash
`8ce83d452ef0ee185a33ac800d713479cc27195507a5f12c340d6a33e3a99e45`,
and the carried-forward narrow governance exception.

F012 remains `passes: false`. The remaining gate is independent review and
complete conformance verification of the separate implementation against this
accepted artifact baseline. This review approves no implementation code and
does not authorize a compatibility claim by itself.
