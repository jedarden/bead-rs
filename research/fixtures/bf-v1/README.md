# bf-v1 black-box fixture set

Status: authored; independent review pending.

- Author: OpenAI Codex, acting as an external clean-room fixture author
- Observation date: 2026-08-10 UTC
- Producer: compiled `bf 0.4.0`
- Environment: `agent-sandbox`, disposable `/tmp/f012-observe/bf` workspace
- Method: invented records were created only through the public CLI and exported
  with `bf sync --flush-only`; no source, upstream tests, fixtures, SQL, or
  internal documentation were inspected.

`observed-valid.jsonl` is the verbatim checkpoint from the disposable workspace.
It covers minimal and populated records, observed lifecycle values, and a
blocking dependency. IDs and timestamps are synthetic and belong only to the
disposable observation workspace.

Expected observations:

- Records are sorted by ID.
- `description`, `design`, `acceptance_criteria`, and `notes` are emitted as
  empty strings when unset; other optional fields are absent.
- `open`, `in_progress`, `blocked`, and `closed` are emitted statuses.
- `bf dep add BLOCKER --blocks BLOCKED` stores an edge on the blocked record;
  `issue_id` is the blocked issue and `depends_on_id` is the blocker.
- An unfinished `blocks` edge materializes the blocked record's exported status
  as `blocked`.
- RFC 3339 instants are emitted in UTC and observed fractional precision is
  retained.

This corpus is not approved until a reviewer independent of the author records
a decision and verifies the manifest hashes.
