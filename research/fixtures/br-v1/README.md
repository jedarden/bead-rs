# br-v1 black-box fixture set

Status: authored; independent review pending.

- Author: OpenAI Codex, acting as an external clean-room fixture author
- Observation date: 2026-08-10 UTC
- Producer: compiled `br 0.1.28`
- Environment: `agent-sandbox`, disposable `/tmp/f012-observe/br` workspace
- Method: invented records were created only through the public CLI and exported
  with `br sync --flush-only`; no source, upstream tests, fixtures, SQL, or
  internal documentation were inspected.

`observed-valid.jsonl` is the verbatim checkpoint from the disposable workspace.
It covers minimal and populated records, all CLI-advertised lifecycle values,
and a blocking dependency. IDs and timestamps are synthetic and belong only to
the disposable observation workspace.

Expected observations:

- Records are sorted by ID.
- Unset optional scalar and array fields are absent.
- `open`, `deferred`, `in_progress`, and `closed` are emitted statuses.
- A dependency is stored on the blocked record. Its `issue_id` is the blocked
  issue, `depends_on_id` is the blocker, and its kind is `blocks`.
- An unfinished blocker does not rewrite the blocked record's stored `status`
  from `open`.
- RFC 3339 instants are emitted in UTC and observed fractional precision is
  retained.

This corpus is not approved until a reviewer independent of the author records
a decision and verifies the manifest hashes.
