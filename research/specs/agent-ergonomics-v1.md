# Agent-ergonomic CLI surface v1

Status: normative specification for plan R041.

This document records settled command ergonomics for native bead-rs. It is
implementation authority for the R041 children and does not reopen their
product decisions. The command spellings were observed in this project's own
agent transcripts; this document contains independently authored behavior,
not source, tests, fixtures, or prose from another bead implementation.

## 1. Scope and compatibility

R041 covers machine-readable issue output, claimant credential transport,
large and punctuation-heavy text input, read shortcuts, per-issue event
history, and positional labels. It does not change the native SQLite schema
unless an implementation needs an additive audit-detail member, and it does
not make title, description, or priority editable. Those fields remain fixed
after `create` under R037 and ADR-007.

The public process rules are unchanged:

- Successful machine-readable output is valid UTF-8 on stdout and diagnostics
  are on stderr.
- A failed mutation commits neither its semantic change nor its audit event.
- A successful mutation publishes the checkpoint under the native automatic
  publication rules.
- An empty result is a successful result, not an error.

Unless this document says otherwise, exit codes have the following meaning:

| Exit | Meaning | R041 examples |
| ---: | --- | --- |
| 0 | Success, including an empty result | `list` returns `[]`; no history exists |
| 1 | Transport or operational failure | unreadable input file; publication failure |
| 2 | Usage or request validation failure | conflicting flags; invalid text-source combination |
| 3 | Workspace or issue not found | `Issue X not found` |
| 4 | State, ownership, or concurrency conflict | stale fencing token; cycle; stale revision |
| 5 | Malformed input or integrity failure | invalid checkpoint; corrupt native state |

An error named below is exact apart from angle-bracket substitutions. Errors
not named here use the existing command's structured error and exit mapping.

## 2. Machine-readable output

### 2.1 Format selectors

On every command that renders machine-readable output, `--json` and
`--format json` are exact synonyms and produce byte-identical output for the
same workspace state. This includes `list`, `show`, `create`, `claim`,
`history`, `dep list`, `capabilities`, and machine-readable status/results on
other mutating or diagnostic commands. Existing command-specific spellings
remain valid.

`--format` accepts only `json` on this surface. An unsupported value fails
before workspace mutation with:

```text
unsupported output format '<value>'; use --format json
```

and exit 2. A command-specific conflict, such as `--json` together with a
non-JSON format, uses:

```text
conflicting output format selectors
```

and exit 2.

The query exception is intentional: `query --json DOCUMENT` continues to
select the inline query document, while `query --format json` selects JSON
rendering of query results. `query --output-json` remains supported.

### 2.2 Issue document

The R041 issue document is one JSON object with these members:

```json
{
  "id": "bead-18409c0e",
  "title": "Verify restore invariants",
  "description": "Rehearse flush and restore.",
  "notes": "A durable note.",
  "priority": 2,
  "status": "open",
  "assignee": null,
  "dependencies": [],
  "created_at": "2026-10-15T12:00:00.000000000Z",
  "updated_at": "2026-10-15T12:00:00.000000000Z",
  "labels": [],
  "revision": 1
}
```

The object has the native projection's required members and may carry
additive members. `description` and `notes` materialize absent checkpoint
text as `""`; `assignee` is explicit JSON null when absent. Notes are the
issue's durable note field, not a comment projection. Unknown checkpoint
members remain omitted from this issue document.

### 2.3 List

`bead list --json` emits exactly one JSON array containing zero or more issue
documents. The empty result is exactly `[]` followed by one newline. There is
no JSON object-per-line ambiguity.

`bead list --jsonl` emits one compact issue JSON object per line. An empty
result emits no bytes. `--jsonl` is mutually exclusive with `--json` and
`--format json` only when the latter is explicitly selected as a different
transport; the error is:

```text
--jsonl cannot be combined with --json
```

and exit 2. `--format jsonl` is not an alias; it fails with the unsupported
format error above. Text mode is unchanged.

### 2.4 Show and create

`bead show ID --json` emits exactly one issue document, never a one-element
array. A missing issue emits `Issue <ID> not found` on stderr and exits 3.

`bead create --json` emits exactly one issue document for the newly created
issue after the transaction commits. Plain `bead create` continues to print
only the new issue ID, as required by the NEEDLE contract. A failed create
emits no partial issue document.

### 2.5 Capabilities

`bead capabilities --json`, `--format json`, and the profile-specific form
advertise an additive `agent_ergonomics` object:

```json
{
  "agent_ergonomics": {
    "version": 1,
    "issue_documents": {
      "list_json": "array",
      "list_jsonl": "object-per-line",
      "show_json": "object",
      "create_json": "object",
      "history_json": "array",
      "dependency_list_json": "array",
      "count_json": "object"
    },
    "json_selectors": ["--json", "--format json"],
    "claim_credential_environment": [
      "NEEDLE_BEAD_ID",
      "NEEDLE_BEAD_FENCING_TOKEN"
    ],
    "text_sources": ["inline", "file", "stdin"],
    "shortcuts": [
      "ready", "search", "list-all", "list-search", "list-count",
      "dependency-list", "bare-id", "history", "positional-labels"
    ]
  }
}
```

The object is additive and consumers must feature-detect its `version` before
using these extensions. A profile that does not support R041 must not claim
this object.

## 3. Claim credential resolution

### 3.1 Scope and precedence

Every mutating command accepts the syntactic option `--fencing-token TOKEN`.
For a command with an issue target, the primary target is the first issue ID
whose state the command changes (`ID` for lifecycle, label, comment, or data
commands; `BLOCKED` for dependency commands). The following policy applies:

1. An explicit `--fencing-token` is always the credential source `flag` and
   wins over both environment variables, including when the environment value
   would be valid.
2. On a claimant-owned fenced mutation whose primary target equals
   `NEEDLE_BEAD_ID`, omission of the flag reads
   `NEEDLE_BEAD_FENCING_TOKEN` as source `environment`.
3. A mutation whose primary target differs from `NEEDLE_BEAD_ID` ignores both
   variables. It behaves exactly as if they were unset.
4. Label and dependency mutations are unfenced when the flag is omitted,
   even when the environment is set. If the flag is supplied, it is checked
   against the primary target exactly like a fenced mutation.
5. Commands without an issue target accept the option for a uniform parser,
   but reject a supplied token with
   `--fencing-token requires an issue target` and exit 2. They never infer a
   target from `NEEDLE_BEAD_ID`.

An empty explicit or environment credential is absent. If a claimant-owned
fenced mutation needs a credential and none is available, it emits exactly:

```text
Claim-epoch credential required; pass --fencing-token TOKEN or set NEEDLE_BEAD_FENCING_TOKEN
```

and exits 4. A supplied credential that is stale, malformed for the current
claim epoch, or belongs to another claim emits exactly:

```text
Fencing token mismatch; use the current claim credential
```

and exits 4. The transaction and audit stream remain unchanged in both cases.

### 3.2 Audit and multi-issue behavior

The audit detail for a mutation records `credential_source` as one of
`"flag"`, `"environment"`, or `"none"`. It never records the credential
value or an environment dump. `none` is used for an unfenced mutation without
the option. For `dep add` and `dep remove`, a supplied token is checked for
the `BLOCKED` issue; the environment is never an implicit token for that
command.

Revision and fencing checks occur inside the mutation transaction. A valid
credential does not bypass `--if-revision`, lifecycle checks, secret policy,
or graph validation.

## 4. Text input and notes

### 4.1 Sources and exact selection

`update` supports these issue-note sources:

```text
update ID --notes TEXT
update ID --append-notes TEXT
update ID --notes-file PATH|-
```

When an update supplies note input, exactly one of the three sources is
allowed. Supplying more than one emits:

```text
exactly one of --notes, --append-notes, or --notes-file may be supplied
```

and exits 2. An update with no note source may still change an unrelated
allowed field. `--notes-file PATH` reads UTF-8 bytes from the path relative to
the invocation directory; `--notes-file -` reads stdin through EOF. File I/O
failure emits `cannot read text input '<PATH>'` and exits 1. Invalid UTF-8
emits `text input is not valid UTF-8` and exits 2. Input is stored without
trimming or newline normalization and is never echoed on either output
channel.

`close` requires exactly one of `--reason TEXT` and `--reason-file PATH|-`:

```text
close ID --reason TEXT
close ID --reason-file PATH|-
```

Neither or both emits:

```text
exactly one of --reason or --reason-file is required
```

and exits 2. `create` may receive its optional description through either
`--description TEXT` or `--description-file PATH|-`, but not both. Both emits:

```text
--description and --description-file are mutually exclusive
```

and exits 2. The same file and UTF-8 rules apply.

### 4.2 Append semantics and comments

`--append-notes TEXT` runs inside the same mutation transaction as the issue
update. If the stored notes are empty, the new value is exactly `TEXT`; if
they are nonempty, the new value is:

```text
<old notes>\n\n<TEXT>
```

where `< TEXT >` means the supplied bytes with no surrounding spaces added
and the angle brackets are notation, not stored bytes. Thus one blank line
(`LF LF`) is the fixed separator. If the resulting field exceeds the native
4 MiB limit, the command emits `notes exceed the 4194304-byte limit` and exits
2 without a write.

Notes and native comments are intentionally different. R010 (tracked by
`beadrs-2ddcb210`) gives comments immutable bodies, authors, replies, and
resolution state. Use `--append-notes` for a single issue-owned durable note
that should atomically extend the note field; use `bead comment create` or
`bead comment reply` for conversation, attribution, or threading. Appending
does not create a comment event, and a comment does not rewrite `notes`.

### 4.3 Leading hyphens

The values of `--notes`, `--append-notes`, `--reason`, `--description`, and
`--title` consume their following argument even when it begins with `-` or
`--`. For example, `--reason - item` stores `- item` and
`--append-notes --- re-verify` stores `--- re-verify`. These forms are not
parser errors and require no `--` escape. A missing value remains a usage
error (exit 2).

## 5. Read shortcuts

### 5.1 Ready, search, all, and count

The following are aliases, not alternate query languages:

| Spelling | Exact meaning |
| --- | --- |
| `bead ready [OPTIONS]` | `bead list --ready [OPTIONS]` |
| `bead search TEXT [OPTIONS]` | `bead list --all --search TEXT [OPTIONS]` |
| `bead list --all` | Include every base status, including closed issues |
| `bead list --search TEXT` | Case-insensitive substring over title, description, and notes |
| `bead list --count [FILTERS]` | Count the rows selected by the other filters |

Search is Unicode case-insensitive substring matching over the three named
fields. It does not search labels, IDs, event details, or comments. It
combines with `--status`, `--assignee`, `--ready`, `--blocked`, `--limit`, and
the other existing list filters. `--all` is mutually exclusive with an
explicit status selector, `--ready`, and `--blocked`; the exact error is
`--all cannot be combined with --status, --ready, or --blocked` (exit 2).

Text `--count` emits one decimal integer followed by LF. JSON `--count` emits
exactly `{ "count": N }` with no issue rows. Count still honors search and
all other filters and returns zero with exit 0.

`bead search` requires a nonempty TEXT argument. Missing TEXT emits
`search text is required` and exits 2. A subcommand name always takes
precedence over the `search` alias.

### 5.2 Dependency listing

`bead dep list ID` returns both blockers of `ID` and issues that depend on
`ID`. It does not mutate the graph. Text output is deterministic, one edge per
line. JSON output is one array of objects with this shape:

```json
{
  "issue_id": "bead-18409c0e",
  "direction": "blocker",
  "kind": "blocks",
  "blocker_status": "open",
  "blocks_readiness": true
}
```

`direction` is `blocker` when the returned issue blocks the requested ID and
`dependent` when the returned issue is blocked by it. `blocker_status` is the
effective status of the endpoint serving as the blocker; it is therefore the
requested ID's status on a dependent row. `blocks_readiness` is true only for
an edge of kind `blocks` whose blocker is not complete. Non-blocking kinds
remain visible and have `blocks_readiness: false`. Edges are ordered by
direction, issue ID, and kind. A missing ID emits `Issue <ID> not found` and
exits 3.

### 5.3 Bare issue IDs

`bead ID` is shorthand for `bead show ID` only when `ID` passes the workspace's
native issue-ID validator. It preserves all show output selectors, including
`--json`. If the token is issue-shaped but absent, show returns the ordinary
not-found error (exit 3). If it is neither a registered subcommand nor an
issue-shaped token, it emits:

```text
'<TOKEN>' is not an issue ID or command
```

and exits 2. Registered subcommand names always win, even if a future command
name resembles an issue ID.

## 6. Per-issue history

`bead history ID [--json]` reads the issue's audit events in ascending commit
sequence. It is a per-issue view, not a cursor and not a replacement for the
workspace-wide `bead changes` feed. It includes events for closed issues and
returns an empty successful result when the issue has no events.

Text history is one deterministic event per line with time, actor, kind,
revision, and changed-field summary. JSON history is exactly one array of
objects with at least:

```json
{
  "sequence": 12,
  "time": "2026-10-15T12:00:00.000000000Z",
  "actor": "worker-1",
  "kind": "issue_updated",
  "revision": 4,
  "changed_fields": ["notes"]
}
```

The sequence and event order are preserved. Redacted event details never
reveal prior bytes: a historical value is replaced by the fixed marker
`[REDACTED:bead-rs]`, and its fingerprint or secret value is not added to the
history response. A missing issue emits `Issue <ID> not found` and exits 3.

## 7. Positional labels

`label add` and `label remove` accept one or more positional labels in
addition to the existing repeatable `--label` form:

```text
bead label add ID LABEL...
bead label remove ID LABEL...
bead label add ID LABEL... [--label LABEL]...
```

At least one label must be supplied by either form. With none, the exact
diagnostic is:

```text
at least one label is required via positional LABEL... or --label LABEL
```

and exit 2. Duplicate labels are de-duplicated before the one atomic
mutation; add and remove remain idempotent. A supplied `--fencing-token` is
validated for the target issue, while omission remains unfenced as specified
in section 3.

## 8. Conformance scenarios

Implementation children must name these scenarios in their tests or
conformance evidence. Each scenario uses a disposable native workspace and
asserts stdout, stderr, exit code, durable state, and audit/checkpoint effect
where relevant.

1. **AE-JSON-LIST-EMPTY-ARRAY** — empty `list --json` is exactly one `[]`
   document; nonempty list parses once as one array; `--jsonl` is an empty
   stream for the empty case.
2. **AE-JSON-SHOW-OBJECT** — `show ID --json` is one object with the issue
   fields, including `notes`; no array unwrap is required.
3. **AE-JSON-CREATE-OBJECT-AND-PLAIN-ID** — `create --json` returns one
   object while plain create returns only the ID.
4. **AE-JSON-FORMAT-ALIAS-BYTES** — `--json` and `--format json` are byte
   identical for list, show, capabilities, history, and dependency list.
5. **AE-JSONL-CONFLICT** — `--json` plus `--jsonl` exits 2 with the exact
   conflict message and changes nothing.
6. **AE-CAPABILITIES-SHAPES** — capabilities advertises version 1 and every
   document shape, selector, environment variable, source, and shortcut.
7. **AE-CREDENTIAL-ENV-SUCCESS** — a claimed target named by
   `NEEDLE_BEAD_ID` mutates with `NEEDLE_BEAD_FENCING_TOKEN`, records source
   `environment`, and commits once.
8. **AE-CREDENTIAL-FLAG-WINS** — a valid explicit flag succeeds with a stale
   environment value, and a stale explicit flag conflicts with a valid
   environment value.
9. **AE-CREDENTIAL-OTHER-ISSUE-IGNORED** — variables for X do not authorize a
   claimant-owned mutation on Y; Y follows its ordinary credential rule.
10. **AE-CREDENTIAL-UNFENCED-EDGE-SURFACES** — label and dependency mutations
    omit the token successfully, but a supplied stale token exits 4 without a
    graph or label change.
11. **AE-CREDENTIAL-AUDIT-REDACTION** — audit detail records only the source,
    never the token or environment value.
12. **AE-TEXT-APPEND-ATOMIC-SEPARATOR** — two transactional append operations
    preserve both notes with exactly `LF LF`, including concurrent writers.
13. **AE-TEXT-SOURCES-AND-STDIN** — inline, file, and stdin text are equal;
    missing/unreadable/invalid-UTF-8 inputs have exits 2/1/2 as specified.
14. **AE-TEXT-SOURCE-EXCLUSIVITY** — conflicting notes, reason, or description
    sources exit 2 and leave no mutation.
15. **AE-TEXT-LEADING-HYPHEN** — all five named value flags store a value
    beginning with `-` verbatim.
16. **AE-TEXT-COMMENT-SEPARATION** — append-notes changes only the issue note;
    native comment create/reply remains the attributed threaded path.
17. **AE-READ-ALIASES-AND-SEARCH** — ready/search aliases equal their list
    forms; search is case-insensitive over title, description, and notes and
    combines with other filters.
18. **AE-READ-ALL-COUNT** — all includes every status and count honors every
    filter in text and JSON forms.
19. **AE-DEP-LIST-BIDIRECTIONAL** — dependency list returns blockers and
    dependents with deterministic direction, kind, blocker status, and
    readiness effect.
20. **AE-BARE-ID-PRECEDENCE** — an issue-shaped bare token equals show, a
    non-issue token is a usage error, and a registered subcommand wins.
21. **AE-HISTORY-ORDER-AND-REDACTION** — history is per issue, ordered by
    sequence, distinct from changes, and exposes the fixed redaction marker
    without prior secret bytes.
22. **AE-POSITIONAL-LABELS** — positional and repeatable option labels can be
    mixed, de-duplicate, and apply idempotently in one transaction.
23. **AE-ERROR-EXIT-MATRIX** — exact diagnostics and exit codes are asserted
    for unsupported formats, missing issues, conflicting sources, missing
    credentials, stale tokens, and transport failures.
24. **AE-NEEDLE-CONSUMER-SHAPE** — the NEEDLE consumer accepts the object show
    document and the array list document without a NEEDLE source change; the
    existing list-row and show fallback behavior is recorded as evidence.

## 9. Out of scope and future decision

R041 does not accept `update --title`, `update --description`, or
`update --priority`. The umbrella's observed 44 such attempts and one direct
SQLite title write justify documenting the friction, but they do not settle
whether a future audited operation should edit those fields. That question is
tracked in the plan's Open questions with R037 and ADR-007 as the current
immutable-field authority.
