# F012 independent review request

Status: awaiting a reviewer independent of the artifact author.

Artifacts are the two profile specifications and both profile directories under
`research/fixtures/`. The reviewer must not be OpenAI Codex acting in the
2026-08-10 authoring session and must not edit an artifact they approve.

The reviewer must recompute manifest hashes; validate provenance; check field,
null, status, timestamp, ordering, loss-report, and dependency-direction rules;
and record ambiguities as findings rather than silently repairing the artifacts.
Approval must name the reviewer, date, reviewed commit, hashes, findings, and
disposition. Until then F012 remains blocked.
