# F012 independent review request (round 2)

Status: awaiting a reviewer independent of both prior authors.

This is a second round. The round-1 candidate (OpenAI Codex, 2026-08-10) was
reviewed and not accepted — see
`docs/reviews/f012-independent-review-2026-08-10.md`. Claude (Anthropic), the
round-1 reviewer, then corrected both profiles and both fixture corpora in
the specification/fixture-author role to resolve that review's findings —
see the "F012 fixture correction" entry in `PROVENANCE.md`.

Artifacts are the two profile specifications and both profile directories
under `research/fixtures/`, as corrected. **The reviewer must not be OpenAI
Codex acting in the 2026-08-10 authoring session, must not be Claude acting
in the 2026-08-10 review or correction sessions, and must not edit an
artifact they approve.**

The reviewer must recompute manifest hashes; validate provenance (including
that the correction pass's claimed producer sources — the real `bf 0.4.0`
binary and the checksummed `br-v0.1.28-linux_amd64.tar.gz` GitHub release
asset — are themselves legitimate and were not fabricated); check field,
null, status, timestamp, ordering, loss-report, and dependency-direction
rules; and record ambiguities as findings rather than silently repairing the
artifacts. Approval must name the reviewer, date, reviewed commit, hashes,
findings, and disposition. Until then F012 remains blocked.

Two things worth deliberately re-checking, since they were the round-1
candidate's most consequential errors and the correction pass grounded its
fixes in producer versions the reviewer should independently reproduce
rather than take on faith:

- br-v1: is `blocked` really accepted as a literal `status` value by real
  `br 0.1.28`, or did the correction pass misread its own black-box output?
- bf-v1: is the `dependencies` array really creation-ordered rather than
  sorted, or was the correction pass's non-alphabetical insertion test
  constructed in a way that doesn't actually prove that?
