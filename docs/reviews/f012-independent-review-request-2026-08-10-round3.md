# F012 independent review request (round 3)

Date: 2026-08-10 UTC

Disposition requested: independent review; **not approval by the author**.
F012 remains `passes: false` throughout this request.

## Independence

The reviewer must be independent of the original fixture author, the round-one
review/correction author, the F012 implementation author, the round-two
reviewer, and this round-three author. The reviewer must not use `src/profile`,
its tests, or producer source as specification evidence.

## Candidate hashes

| Artifact | SHA-256 |
| --- | --- |
| `research/specs/profile-loss-report-v1.md` | `f7c79e7cc581294ea08543452c6da2df0b700c3218fa2153eb1c1b8ae0dd62a3` |
| `research/specs/br-v1-profile.md` | `f17ca732e99af46dd232b83753f51177ee44f5c9c2c94c9c4e75c07d3d1d6e9e` |
| `research/fixtures/br-v1/manifest.json` | `eaf7f43ff72a22ac8ad4693106c0be76d82a06734d863511cd225f2f40f843fd` |
| `research/specs/bf-v1-profile.md` | `8be589541031788b2a895ec758227b8033232a0310ce377cfb5b645f65ae38fe` |
| `research/fixtures/bf-v1/manifest.json` | `8ce83d452ef0ee185a33ac800d713479cc27195507a5f12c340d6a33e3a99e45` |
| `docs/reviews/f012-bf-v1-binary-governance-exception-2026-08-10.md` | `29a3f38b2a495937ef7b4006a17988646d5270ec706cc0af084666b9e893dc16` |

The reviewer must recompute every manifest-listed digest and the table above.

## Round-two finding disposition to verify

1. Verify the loss-report schema is unambiguous, counts every input record and
   top-level field exactly once, emits zero-loss output, orders entries
   deterministically, handles collision failure, and explicitly accounts for
   events, provenance receipts, schema references, extensions, comments, and
   structured data. Verify both fixture sets contain exact expected reports.
2. Verify nested unknown JSON and explicit null survive same-profile round
   trips exactly and distinctly from absence and empty arrays.
3. Verify every missing required field is isolated from the complete baseline;
   representative wrong types, priority boundaries/out-of-range, timestamps,
   future statuses, dangling/self/cyclic dependencies, Unicode, and a literal
   newline have independently executable expected results.
4. Verify the br-v1 `blocked` table no longer conditions explicit blocked on an
   unfinished dependency and agrees with the prose and observed fixture.
5. Verify bf-v1 requires exact dependency array order and a
   `dependency_order_changed` entry if a target reorders it.
6. Either reject or independently approve the narrow bf binary governance
   exception using every criterion in that document. If an official attested
   artifact exists, reject the exception and require corroboration against it.

Also check internal consistency among the plan, shared report contract,
profiles, README prose, machine fixtures, manifests, and provenance. Record
reviewer identity, independence, commands/evidence, exact hashes, findings,
and an explicit disposition. Approval of specifications/fixtures does not
approve the existing implementation; that remains a separate review and
conformance gate.
