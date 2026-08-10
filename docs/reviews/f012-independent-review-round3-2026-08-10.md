# F012 round-three independent artifact review

Date: 2026-08-10 UTC

Reviewer: OpenAI Codex, current review session, independent of the original
fixture author, the round-one review/correction author, the F012 implementation
author, the round-two reviewer, and the round-three correction author

Reviewed commit: `7a6ad205d4714bdcae29294d65672e8a7b747a04`
(`main`, equal to `origin/main` at review start)

Disposition: **rejected**. Round-two findings 1-6 are substantively corrected,
and the narrow bf-v1 binary governance exception is independently **approved**,
but one material bf-v1 status-table contradiction remains. F012 must remain
`passes: false`.

This review approves no implementation code. I did not inspect `src/profile`,
implementation tests, or any producer source, tests, fixtures, database, SQL,
internal documentation, or local producer checkout. I modified none of the
reviewed specification or fixture artifacts.

## Scope and method

I read `AGENTS.md`, `.marathon/instruction.md`, `PROVENANCE.md`,
`research/specs/clean-room-protocol.md`, the round-two review, the round-three
request, the bf-v1 exception request, plan sections 6.4, 9, 13, and 15,
`research/specs/conformance-v1.md`, the shared loss-report contract, both
candidate profiles, and every file in the two candidate fixture directories.
I validated every JSON and JSONL document with `jq`, independently recomputed
all candidate and manifest digests, checked report arithmetic and ordering, and
compared the prose, matrices, READMEs, observations, expectations, manifests,
plan, and provenance.

Fresh black-box workspaces were created under `/home/coding/scratch`. The br
producer was the official `br-v0.1.28-linux_amd64.tar.gz` GitHub release asset,
whose archive SHA-256 `1fb9962e6d27a2a606aacba95460f1dd9f6c38e500ef85f9ab2073cc6bbf99e9`
matched both the GitHub release API digest and its adjacent publisher checksum.
It reported `br 0.1.28`. The bf producer was the exact executable governed by
the exception below.

## Artifact hashes

The round-three request table matched exactly:

| Artifact | SHA-256 |
| --- | --- |
| `research/specs/profile-loss-report-v1.md` | `f7c79e7cc581294ea08543452c6da2df0b700c3218fa2153eb1c1b8ae0dd62a3` |
| `research/specs/br-v1-profile.md` | `f17ca732e99af46dd232b83753f51177ee44f5c9c2c94c9c4e75c07d3d1d6e9e` |
| `research/fixtures/br-v1/manifest.json` | `eaf7f43ff72a22ac8ad4693106c0be76d82a06734d863511cd225f2f40f843fd` |
| `research/specs/bf-v1-profile.md` | `8be589541031788b2a895ec758227b8033232a0310ce377cfb5b645f65ae38fe` |
| `research/fixtures/bf-v1/manifest.json` | `8ce83d452ef0ee185a33ac800d713479cc27195507a5f12c340d6a33e3a99e45` |
| `docs/reviews/f012-bf-v1-binary-governance-exception-2026-08-10.md` | `29a3f38b2a495937ef7b4006a17988646d5270ec706cc0af084666b9e893dc16` |

Every manifest entry also matched:

| Artifact | SHA-256 |
| --- | --- |
| `research/fixtures/br-v1/README.md` | `bf4aedc4f43725cd5dea3f569cb154bc05c459d2924dad11209575d15025f617` |
| `research/fixtures/br-v1/invalid-cases.json` | `a55414616bb930a0ca3db2fda82521f57c2f3a746941bc16a182f2d1e0acaf8a` |
| `research/fixtures/br-v1/loss-report-cases.json` | `9e6915b210a466cb0572244de95028b7d74c5e9b5214163409681b3c4d462023` |
| `research/fixtures/br-v1/observed-valid.jsonl` | `d1f5d393df10011a3843097b2d4855d08ffb51df67b3b316a78653f3041b0a8b` |
| `research/fixtures/br-v1/round-trip-cases.json` | `37d1b21816fc5ffe59047d1673c2cc23b7f907fff89553f92fba474f6b8c6b3e` |
| `research/fixtures/bf-v1/README.md` | `5ddf0b840b63f3a271af71bbb33c96f51c23b00631733aef09dc5ebdbeed9b70` |
| `research/fixtures/bf-v1/invalid-cases.json` | `f69d2b88d6f83a756e7b207b1a43714201ea650b350d251892c9a1c51f8ea61a` |
| `research/fixtures/bf-v1/loss-report-cases.json` | `a14bb71fec3625c0f2f786b63cb5f3347f72f2ced642b369367c4ad76f40b1eb` |
| `research/fixtures/bf-v1/observed-valid.jsonl` | `c0e7d74d40c04b04412c76781bc7b76553a2a5384e5e55375104a9a528436d0b` |
| `research/fixtures/bf-v1/round-trip-cases.json` | `44fbc08ae180ba9de14dfeaed2d66645c95c1ceab5896647e92b90e284f2f87a` |

## Round-two findings 1-6

1. **Corrected.** The report schema classifies every input record envelope,
   non-issue record kind, and issue top-level field occurrence exactly once;
   defines exact preserved/transformed/omitted counts; specifies deterministic
   ordering, zero-record output, and collision failure; and names required
   accounting for events, provenance receipts, schema references, extensions,
   comments, and structured data. Both profiles have exact report fixtures,
   including zero loss and recovery-content omissions. Fixture arithmetic and
   ordering are consistent with the contract.
2. **Corrected.** Both same-profile suites preserve recursively nested unknown
   JSON exactly. Explicit null, absence, empty strings, and empty arrays are
   distinct; exact outputs and `explicit_null_preserved` reports are supplied.
3. **Corrected.** Each profile uses a complete baseline with one isolated
   mutation for every required-field omission and representative wrong types.
   P0/P4 and both out-of-range priorities, invalid and offset timestamps,
   future statuses, dangling/self/two-record-cycle graphs, Unicode, and an
   actual LF inside a string all have independently executable expectations.
4. **Corrected for br-v1.** The br table now maps explicit `blocked` to and from
   native `blocked` without a dependency condition. Fresh `br 0.1.28`
   reproduction exported an explicit blocked record with no dependencies as
   `blocked`; an open record with two unfinished dependencies remained `open`.
5. **Corrected.** bf-v1 requires exact dependency-array order and mandates a
   `dependency_order_changed` transformation entry for any reorder. The exact
   nonlexical input/output case and expected entry are executable fixtures.
6. **Governance exception approved independently**, as detailed next.

## bf-v1 binary governance exception

Disposition: **approved for the narrow request only**.

At `2026-08-10T03:18:52Z` through `2026-08-10T03:19:40Z`, I checked the public
GitHub mirror release API and pages for `jedarden/bead-forge`, the direct
`v0.4.0` release URL, crates.io's package/version API and page, and the public
Forgejo release URLs. GitHub returned an empty release list and 404 for the tag;
crates.io had no package/version page (the API denied anonymous automation and
the page returned 404); Forgejo redirected anonymous access to sign-in. General
public web searches likewise found no bead-forge 0.4.0 compiled release. No
official compiled artifact, publisher checksum, signature, SBOM, or attestation
was found. The public Forgejo API was authentication-gated, so this is an
approval of the documented provisional exception, not proof that no private or
future attestation exists.

The exercised `/home/coding/.cargo/bin/bf` independently reported `bf 0.4.0`.
It was 6,395,912 bytes, a stripped dynamically linked x86-64 Linux PIE ELF,
with SHA-256
`696019aeaaeee50ce1fc62fe2407e73892caf9818e54f434f5e22b0dad81018e`
and GNU build ID `58f50ef6ce07b6385d837ff37df3032803210b39`. Local Cargo install metadata
identified `bead-forge 0.4.0 (path+file:///home/coding/bead-forge)` and binary
`bf`; the referenced checkout was not opened or inspected.

In fresh workspace `/home/coding/scratch/f012-r3-bf.44dyd3`, public CLI calls
reproduced all required load-bearing facts:

- explicit `blocked`, `deferred`, and invented `future_round3` statuses exported
  verbatim;
- `bf dep add BLOCKER --blocks BLOCKED` stored `(blocked, blocker, blocks)`,
  materialized the target status as `blocked`, and preserved the two-edge
  creation order `r3-2r8`, then `r3-1xl`, which was nonlexical;
- every exported issue carried `events`, while dependency objects omitted
  `metadata`;
- unset content fields exported as empty strings, while optional assignee,
  labels, and dependencies were absent; the create JSON response represented
  unset assignee as null and labels as an empty array, confirming those public
  representations are distinct.

As previously documented, `bf sync --flush-only` wrote the inspected JSONL but
then exited 1 on missing `export_hashes`. That known local failure did not alter
the observed JSONL facts. Byte identity and the required observations match the
exception request. No prohibited producer material was consulted. The accepted
exception is bound exactly to profile hash
`8be589541031788b2a895ec758227b8033232a0310ce377cfb5b645f65ae38fe`,
manifest hash
`8ce83d452ef0ee185a33ac800d713479cc27195507a5f12c340d6a33e3a99e45`,
and exception hash
`29a3f38b2a495937ef7b4006a17988646d5270ec706cc0af084666b9e893dc16`.
If an official attested artifact is later found, this approval expires and the
observations require corroboration or regeneration against it.

## Remaining finding

**bf-v1's status table contradicts its prose, fixture, and observed behavior.**
The `blocked` row says reverse export is `blocked` "while required blocker is
unfinished." The next paragraph says explicit `blocked` is accepted, and both
`observed-valid.jsonl` and the fresh reproduction contain an explicit blocked
record with no dependency that exports as `blocked`. This is the same kind of
conditional-table ambiguity corrected for br-v1 in round three. The bf row
must state unconditional reverse representation for explicitly stored blocked,
while separately describing dependency-derived materialization. A conforming
adapter must not be allowed to coerce an explicit blocked record merely because
it has no unfinished dependency.

## Clean-room assessment and final disposition

The reviewed artifacts contain behavioral contracts and independently invented
conformance cases; I found no evidence of prohibited source-derived transfer.
Authorship, correction, implementation, and review roles are disclosed, and
this reviewer did not modify an artifact it reviewed.

**Final disposition: rejected.** Correct only the bf-v1 status-table row,
regenerate the bf-v1 profile hash and dependent review request/manifest metadata
if necessary, and obtain a new independent review of the changed artifact.
The binary governance exception is approved and need not be relitigated unless
its bound hashes or executable identity change or an official attested artifact
appears. F012 remains `passes: false`; implementation review and full profile
conformance remain separate gates.
