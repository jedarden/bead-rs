# F012 independent review

Date: 2026-08-10
Reviewer: Claude (Anthropic), operating independently from the OpenAI Codex
2026-08-10 authoring session. This reviewer did not author
`research/specs/br-v1-profile.md`, `research/specs/bf-v1-profile.md`, or
either fixture corpus, and made no edits to those artifacts before or during
this review.
Reviewed commit: `aee1c0f845e81cf701aef67fa05dcaeac7351892` (branch
`f012-fixture-setup`, one commit ahead of `main` at `c0cd7dc`)
Decision: **not yet accepted as the F012 implementation baseline.** Hash
integrity and every independently testable bf-v1 mechanical rule are
confirmed correct against the real `bf 0.4.0` producer. One completeness
defect and several conformance-fixture gaps must be closed, and br-v1's
central claim needs an independent producer observation this review could
not obtain, before F012 unblocks.

## Method

1. Recomputed SHA-256 for all six manifest-listed files and compared against
   `research/fixtures/{br-v1,bf-v1}/manifest.json`.
2. Read both profile specifications and both fixture corpora line by line
   against the field/null/status/timestamp/ordering/loss-report/dependency-
   direction rules named in the review request.
3. Where a real producer binary was available, reproduced the claimed
   behavior black-box: fresh disposable workspace, invented records, public
   CLI only, capturing stdout and the resulting JSONL file — the same method
   `research/specs/clean-room-protocol.md` prescribes for the specification
   role. This machine has the real `bf 0.4.0` binary installed as the
   system's canonical bead CLI, which is exactly the producer version bf-v1
   claims, so bf-v1 could be cross-checked directly. This machine's `br`
   command is a documented deprecated shim that execs `bf` (`br --version`
   reports `bf 0.4.0`, not a standalone `br 0.1.28`), so br-v1 could not be
   cross-checked the same way. A checkout of the `br` producer project exists on
   this machine, including a `v0.1.28` tag; this review deliberately did not
   clone, build, or run it, and did not open any file in it. Standing up a fresh independent producer observation
   is specification-role work with its own authorship and attestation, not
   something a review pass should backfill by improvising against the one
   named-prohibited source repository, even at the compiled-binary layer.
   That gap is recorded as a finding below rather than filled in.

## Hash verification

All six manifest hashes recomputed and matched exactly; no manifest, file,
or hash was altered by this review.

| Profile | File | Verified |
| --- | --- | --- |
| br-v1 | README.md | `sha256:c83dcbc0...fab803` match |
| br-v1 | invalid-cases.json | `sha256:36f74aa9...c9b9099` match |
| br-v1 | observed-valid.jsonl | `sha256:3b1a6af7...f6e1f2876` match |
| bf-v1 | README.md | `sha256:6146afed...f5ebf258c` match |
| bf-v1 | invalid-cases.json | `sha256:8615d1fc...d726e6734` match |
| bf-v1 | observed-valid.jsonl | `sha256:2ba100cf...6fd3b3da9` match |

## Provenance

`PROVENANCE.md`, `docs/traceability/external-dependencies.md`, both profile
headers, and both fixture READMEs/manifests agree on author (OpenAI Codex),
date (2026-08-10), producer versions (`br 0.1.28`, `bf 0.4.0`), and method
(black-box public CLI operations on invented records in disposable
`agent-sandbox` workspaces). No internal inconsistency found across these
six documents.

The negative claim — "no upstream source, tests, fixtures, SQL, or internal
documentation was inspected" — is a self-attestation by the same session
that produced the artifacts, and nothing in this repository can independently
confirm it (no session transcript or sandbox audit log is referenced). This
review found no artifact content that contradicts the attestation, and where
bf-v1's claims were independently reproducible, they reproduced exactly
against the real producer (see below), which is circumstantial support for
it. Naming this limitation plainly, rather than treating hash/format checks
as if they certified authorship, is itself part of what this review is
required to do.

## Confirmed correct (reproduced against real `bf 0.4.0`)

Every one of these was independently reproduced in a fresh disposable
workspace on this machine, not inferred from reading the fixture:

- **Dependency direction and edge schema.** `bf dep add BLOCKER --blocks
  BLOCKED` stores the edge on the blocked record with `issue_id` = blocked,
  `depends_on_id` = blocker, `type":"blocks"`, and optional `created_at`,
  `created_by":"cli"`, `thread_id":""` — matching `bfx-1uw`'s dependency
  object field-for-field, including the *absence* of a `metadata` key (which
  br-v1's dependency objects carry and bf-v1's profile correctly says they
  don't).
- **Blocked-status materialization.** Adding an unfinished `blocks` edge to
  an `open` record changes its exported `status` to `blocked` with no
  explicit status update — matching the bf-v1 README's claim and `bfx-1uw`.
- **Empty-string-vs-absent semantics.** `description`, `design`,
  `acceptance_criteria`, `notes` are always emitted, empty string when unset;
  `assignee` and `labels` are omitted entirely when unset. Matches the field
  matrix and every fixture record.
- **Export ordering.** Issues are sorted by ID in the exported JSONL.
  Confirmed on a 12-record workspace including a case that inserted labels
  out of order (`zeta, alpha, mu`) — the CLI's immediate `create --json` echo
  preserved insertion order, but the exported JSONL sorted them
  lexicographically (`alpha, mu, zeta`). The fixture's claim is specifically
  about export order, and that's the order that's actually sorted; the
  distinction matters and the fixture gets it right.
- **Timestamp format.** RFC 3339, UTC `Z`, nanosecond fractional precision,
  exactly as claimed and as shown in every fixture record.
- **`close_reason` / `closed_by_session`.** A closed record produced via a
  generic status transition (not the dedicated `bf close` subcommand, whose
  documented default reason is `"Completed"`, not empty) shows
  `close_reason":""` and `closed_by_session":"cli"` in the exported JSONL —
  consistent with `bfx-6aa`, though the fixture doesn't say which exact
  command produced the closed record, so this is consistent-with rather than
  a byte-exact reproduction.

This is meaningful positive evidence: every mechanically checkable bf-v1
claim this review could exercise came back correct, including a subtle
distinction (create-echo order vs. export order) that would have been easy
to get backwards.

## Findings

1. **`events` field is undocumented and silently absent from the bf-v1
   fixture, despite being present in every real `bf sync --flush-only`
   record.** A fresh disposable `bf 0.4.0` workspace was built and flushed
   the same way the README describes; every exported record — regardless of
   status, regardless of whether it was ever updated — carried an `events`
   array. `research/fixtures/bf-v1/observed-valid.jsonl` has no `events` key
   anywhere, and `bf-v1-profile.md`'s field matrix doesn't mention `events`
   as required, optional, or extension. Two consequences: the README's
   "verbatim checkpoint from the disposable workspace" claim doesn't hold as
   stated — either a different export path was used or the field was
   stripped, and either way that's undisclosed — and an implementer working
   strictly from this profile has no way to know the field exists, so the
   profile's own loss-reporting obligation ("native-only comments/data/
   conditions" must be reported, never silently dropped) can't actually be
   discharged for `events`. This is the one finding in this review that is a
   confirmed defect rather than a gap or an open question.
2. **Neither `invalid-cases.json` exercises explicit JSON `null`.** Both
   profiles give "Null and absence" its own section and both "Loss
   reporting" sections list explicit nulls as mandatory to report, but
   neither fixture has a single case with a literal `null` value for an
   optional field. The rule is specified; it's untested.
3. **Neither fixture exercises multi-dependency ordering.** Both profiles
   assert a canonical dependency-array order ("by blocked ID, blocker ID,
   then kind"). Every fixture record has at most one dependency, so no
   record can actually demonstrate that ordering rule.
4. **br-v1's most distinctive rule has no negative-case coverage.** The
   profile states a non-derived native `blocked` value "has no proven br-v1
   representation and must fail or produce an explicit lossy-conversion
   report" — this is the single rule that most separates br-v1 from bf-v1
   (which does support an explicit `blocked` status). No case in
   `br-v1/invalid-cases.json` sends an explicit `blocked` value through.
5. **bf-v1's "no `deferred` mapping" rule has no negative-case coverage**,
   symmetrically to #4 — no case sends a `deferred` value through bf-v1.
   (`invalid-cases.json` is otherwise near-identical text between the two
   profiles, substituting only the ID prefix; the two rules that actually
   distinguish the formats are exactly the two that go untested.)
6. **br-v1's central status-mapping claim is unverified by this review.**
   The claim that native `blocked` is never itself an exported/stored status
   for br — only derived, leaving the stored value at `open` — is the
   profile's most load-bearing, br-specific assertion, and it's exactly the
   claim #4 shows has no fixture coverage either. This review had no way to
   independently reproduce it: the only `br` on this machine is the
   documented shim to `bf`, and this review deliberately did not stand up
   the `br` producer project present on this machine to check it (see
   Method). This is the highest-priority open risk in the candidate as it
   stands — it's the one place a fabrication or misremembering would be both
   easiest to introduce and hardest to catch, and nothing in this repository
   currently closes it.

## Required disposition

F012 should not move to `passes: true` on the strength of this candidate
alone. Before acceptance:

- Finding 1 must be resolved: either regenerate `bf-v1/observed-valid.jsonl`
  as a genuinely verbatim checkpoint (events included), or add `events` to
  the field matrix with an explicit required/optional/extension
  classification and loss-report obligation, and explain in the README why
  the shipped fixture omits it.
- Findings 2–5 should be closed by adding the missing cases
  (explicit-null, multi-dependency ordering, br-v1 non-derived-`blocked`
  rejection/report, bf-v1 `deferred` handling) to the relevant
  `invalid-cases.json` before conformance work treats these fixtures as
  complete.
- Finding 6 needs a dedicated, separately attested independent-observation
  session against a real `br 0.1.28` binary — following the same method and
  producing its own manifest and `PROVENANCE.md` entry — before the br-v1
  half of F012 can be accepted as an implementation baseline. Absent that,
  br-v1 should carry an explicit "unverified" marker rather than being
  implemented as if confirmed.

bf-v1's testable rules are in good shape and should not need re-authoring —
only the `events` gap and the fixture completeness items above.
