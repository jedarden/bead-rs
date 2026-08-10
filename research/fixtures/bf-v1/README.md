# bf-v1 black-box fixture set

Status: round-three correction; independent review pending.

- Author: Claude (Anthropic), acting as external clean-room fixture author for
  this correction pass
- Round-three author: OpenAI Codex, adding independently invented conformance
  cases and report expectations without inspecting implementation or producer
  source
- Observation date: 2026-08-10 UTC
- Producer: the real `bf 0.4.0` binary already installed as this workspace's
  canonical bead CLI (system PATH, not a copy)
- Environment: disposable scratch workspaces outside this repository, one
  `bf init` per observation run
- Method: invented records were created only through the public CLI
  (`bf create`, `bf update`, `bf dep add`) and exported with `bf sync
  --flush-only` (via `bf doctor --repair`, whose local-fixer path invokes the
  same export, after this producer build's flush command failed locally with
  an unrelated `no such table: export_hashes` error unconnected to F012); no
  source, upstream tests, fixtures, SQL, or internal documentation was
  inspected.

`observed-valid.jsonl` is the verbatim checkpoint from the disposable
workspace, byte-identical to what the producer wrote. It supersedes the
2026-08-10 OpenAI Codex candidate, which the independent review at
`docs/reviews/f012-independent-review-2026-08-10.md` found silently omitted
the `events` field present on every real record.

Expected observations:

- Records are sorted by ID.
- `description`, `design`, `acceptance_criteria`, and `notes` are emitted as
  empty strings when unset; other optional fields are absent.
- Every record carries an `events` array (at minimum one `created` event);
  this producer does not offer a flag to omit it.
- `open`, `in_progress`, `blocked`, `closed`, and `deferred` are all accepted
  and emitted verbatim as `status` with no CLI-side validation against a
  fixed enum — this producer will store and export **any** string passed to
  `--status`. `blocked` is observed both as an accepted explicit value and as
  the materialized result of adding an unfinished `blocks` edge. `deferred`
  is accepted at the wire level but has no established bf-v1 semantic mapping
  (see the `Status deferred (unknown to bf-v1)` record and the profile's
  status table); treat it the same as any other unrecognized status: preserve
  and report, never silently coerce it to a native lifecycle state.
- `bf dep add BLOCKER --blocks BLOCKED` stores an edge on the blocked record;
  `issue_id` is the blocked issue and `depends_on_id` is the blocker. No
  `metadata` key is present on bf-v1 dependency objects (br-v1's are, and do
  carry one).
- **Dependency array order is creation order, not lexical.** The blocked
  record with two dependencies demonstrates this directly: its blockers were
  added in an order that is *not* alphabetical by ID, and the export
  preserves that exact non-alphabetical creation order rather than resorting
  it. The 2026-08-10 candidate's claim that dependencies export in
  "deterministic lexical/canonical ordering" was independently reproduced as
  false for this producer and is corrected in `bf-v1-profile.md`. (Labels
  *are* lexically sorted on export, independently confirmed by inserting
  labels out of order and observing the sorted result — that part of the
  original claim held.)
- An unfinished `blocks` edge materializes the blocked record's exported
  status as `blocked`.
- RFC 3339 instants are emitted in UTC and observed fractional (nanosecond)
  precision is retained.
- A record closed through a generic status transition (rather than the
  dedicated close subcommand's default-reason path) shows an empty
  `close_reason` and `closed_by_session":"cli"`.

`round-trip-cases.json` is independently authored normative conformance data,
not producer output. It proves nested unknown JSON, explicit null, empty arrays,
Unicode, a real newline, and exact dependency array order have expected
same-profile outputs and machine-readable reports. `loss-report-cases.json`
exercises zero-loss output, required recovery-content omissions, and the
mandatory report for dependency reordering. `invalid-cases.json` uses one
complete baseline plus one mutation per case so missing-field and type
diagnostics are not confounded; graph cases are separately complete corpora.

This corpus is not approved until a reviewer independent of this author
records a decision and verifies the manifest hashes.
