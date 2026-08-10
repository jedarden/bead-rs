# F012 independent review request (round 4)

Date: 2026-08-10 UTC

Disposition requested: independent review; **not approval by the author**.
F012 remains `passes: false`.

## Narrow scope

Review the sole authoring change requested by the round-three review: the
bf-v1 `blocked` status table now states unconditional reverse representation
for explicit or target-materialized `blocked`. Confirm that it agrees with the
unchanged prose, observed fixture, and round-three black-box reproduction, and
does not permit coercion of explicit blocked merely because it has no unfinished
dependency.

No fixture, manifest, loss-report contract, br-v1 artifact, producer identity,
or other bf-v1 semantic rule changed. The reviewer must not use `src/profile`,
its tests, or producer source as specification evidence.

## Exact artifacts

| Artifact | SHA-256 |
| --- | --- |
| Corrected `research/specs/bf-v1-profile.md` | `e321eea25ffb72f3afff6465ed1dfd4bc3121cf274323d8d7eef7e727de2af00` |
| Unchanged `research/fixtures/bf-v1/manifest.json` | `8ce83d452ef0ee185a33ac800d713479cc27195507a5f12c340d6a33e3a99e45` |
| Unchanged governance-exception request | `29a3f38b2a495937ef7b4006a17988646d5270ec706cc0af084666b9e893dc16` |

The prior bf-v1 profile hash was
`8be589541031788b2a895ec758227b8033232a0310ce377cfb5b645f65ae38fe`.
The reviewer should verify that the only normative change from that artifact is
the status-table wording above (plus candidate status/authorship metadata).

Round three independently approved the binary governance exception while
binding it to the prior profile hash. Because this correction necessarily
changes that hash, the round-four reviewer must explicitly state whether the
existing approval carries forward to this corrected hash. No new producer fact
or executable identity is asserted, so fresh provenance discovery is not
requested unless the reviewer finds the change broader than represented or an
official attested artifact has appeared.

Record reviewer identity, independence, recomputed hashes, scope verification,
exception carry-forward disposition, and an explicit accept/reject decision.
Artifact acceptance does not approve the separate implementation or make F012
pass.
