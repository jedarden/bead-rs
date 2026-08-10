# br-v1 black-box fixture set

Status: round-three correction; independent review pending.

- Author: Claude (Anthropic), acting as external clean-room fixture author for
  this correction pass
- Round-three author: OpenAI Codex, adding independently invented conformance
  cases and report expectations without inspecting implementation or producer
  source
- Observation date: 2026-08-10 UTC
- Producer: `br 0.1.28`, obtained as the official
  `br-v0.1.28-linux_amd64.tar.gz` asset from the upstream project's GitHub
  Releases page and verified against its published `.sha256` checksum before
  execution — not built from source, and not the same machine's separately
  present source checkout of the producer project, which this authoring pass
  did not open, build, or read from
- Environment: disposable scratch workspaces outside this repository, one
  `br init --prefix brx` per observation run
- Method: invented records were created only through the public CLI (`br
  create`, `br dep add`, `br close`) and exported with `br sync
  --flush-only`; no source, upstream tests, fixtures, SQL, or internal
  documentation was inspected.

`observed-valid.jsonl` is the verbatim checkpoint from the disposable
workspace. It supersedes the 2026-08-10 OpenAI Codex candidate, which the
independent review at `docs/reviews/f012-independent-review-2026-08-10.md`
found to be wrong about `blocked`: the original claimed a non-derived
`blocked` status "has no proven br-v1 representation and must fail," but the
real producer accepts and exports it directly.

Expected observations:

- Records are sorted by ID.
- Unset optional scalar and array fields are absent.
- `open`, `deferred`, `in_progress`, `blocked`, and `closed` are all accepted
  and emitted verbatim as `status`, with no CLI-side validation against the
  set `--help` documents (which lists only `open, deferred, in_progress,
  closed` for `create`'s `-s/--status` flag — that list is incomplete; the
  producer will accept and store any string). `blocked` is observed both as
  a directly created value (`Status explicit blocked`) and, separately, in
  the dependency scenario below where an unfinished blocker leaves the
  blocked record's own stored status at `open` rather than materializing it
  to `blocked` — those are two different things and this producer keeps them
  different.
- A closed record carries a `close_reason` field (here `"done"`, the value
  passed to `br close -r`) — this field was absent from the field matrix and
  from the original candidate's closed-record fixture entry; it's added to
  both here.
- Two dependencies on one record (`Dependency blocked`, blocked by both
  `Dependency blocker` and `Second dependency blocker`) are stored on the
  blocked record. Each item's `issue_id` is the blocked issue, `depends_on_id`
  is the blocker, and kind is `blocks`; the array is exported in ascending
  order by `depends_on_id`, independently confirmed by inserting two
  blockers in deliberately reverse-alphabetical order in a separate
  disposable run and observing the export re-sort them — unlike bf-v1, whose
  dependency array is creation-ordered, not sorted.
- An unfinished blocker does not rewrite the blocked record's stored `status`
  from `open`.
- RFC 3339 instants are emitted in UTC and observed fractional precision is
  retained.

`round-trip-cases.json` is independently authored normative conformance data,
not producer output. It proves nested unknown JSON, explicit null, absence,
empty arrays, Unicode, and a real newline have exact expected same-profile
outputs and machine-readable reports. `loss-report-cases.json` exercises
zero-loss output and required recovery-content omissions. `invalid-cases.json`
uses one complete baseline plus one mutation per case so missing-field and type
diagnostics are not confounded; graph cases are separately complete corpora.

This corpus is not approved until a reviewer independent of this author
records a decision and verifies the manifest hashes.
