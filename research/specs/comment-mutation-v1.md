# Comment mutation v1 specification

Status: normative.

Implements plan section 12 R010 (plan line 3284). This specification is the
command contract for the first public comment mutation operations — creation,
replying, and resolution changes — together with their authorization and
validation rules, audit events, help and man pages, and stable machine
results. It must exist and be reviewable before implementation evidence is
accepted; plan prose alone is not implementation authority. Until these
commands land, comments are portable and readable but cannot be created,
edited, resolved, or deleted through the native 0.1 CLI (plan-vs-built audit
2026-10-10: no `comment` command exists; `bead comment --help` reports an
unrecognized subcommand).

Every fork in the design space is decided inline below. Nothing in this
document defers a choice to implementers.

## Scope and safety rule

R010 adds exactly four commands, one new top-level command noun, three new
audit event kinds (one of which already exists internally), and no storage
changes. Three boundaries are absolute:

- **Portability is unchanged.** Import, export, backup, restore, and the
  `list`/`show` projections continue to carry imported comments verbatim and
  never drop or rewrite them because mutation did or did not occur (plan R010,
  carried forward unchanged).
- **Bodies are immutable.** Version 1 adds no way to edit or delete a comment
  body, author, creation time, or issue binding, and specifies none. The only
  sanctioned rewriter of comment content is `bead redact`, which is a
  recovery operation, not an editing path.
- **Every committed comment write carries its audit event in the same
  transaction.** A committed comment row without its event must be
  unconstructible, exactly as the plan's mutation rule requires for every
  checkpoint-carried write (plan line 367).

No schema migration is part of this work: the `comments` and `events` tables
gain no columns, no table is added, and no schema URN changes. The additive
surface is the CLI command tree, three event kind strings, the capabilities
command list, and the field guide's comment field semantics.

## Definitions

- A **comment** is one row of the existing `comments` table: `id`, `issue_id`,
  `author`, `body`, nullable `reply_to_id`, nullable `resolution_state`, and
  `created_at` (plan line 1755: random ID, issue ID, author, immutable body,
  reply-to ID, resolution state, creation time).
- The **body** is the comment's `body` string: scanned free text, stored
  verbatim, never rewritten by a comment command.
- The **author** is the comment's `author` string: the identity that created
  it, recorded once at creation and never changed afterward.
- The **actor** is the identity a single mutation is attributed to, resolved
  from `--actor`, `BEAD_ACTOR`, or the `system` default (see *Author
  attribution*). The actor of a creation becomes its author; the actor of a
  resolution change is recorded only in the audit event.
- A **native comment** is one created by these commands. An **imported
  comment** is one delivered by checkpoint import, interchange, or restore,
  whose fields are preserved exactly as delivered, including a NULL
  `resolution_state` and non-RFC-3339 `created_at` spellings.
- The **reply target** of a reply is the pre-existing comment named by
  `--reply-to`. A **thread** is the tree obtained by following `reply_to_id`
  links upward. Thread depth is unbounded.
- The **resolution state** is `unresolved` or `resolved`. NULL is the
  import-compatibility spelling of `unresolved`: every reader, projection,
  and transition treats NULL exactly as `unresolved`. Native-created comments
  always store the explicit string.
- A **semantic transition** is a resolution command whose target actually
  changes state. A **no-op transition** names a comment already in the target
  state; it succeeds, changes nothing, and emits no event.
- The **canonical comment order** is ascending lexicographic order over the
  stored `created_at` strings, tie-broken by ascending `id`. It is a total
  order over the stored strings without interpreting their format, which is
  what the schema field guide already documents ("ordered by creation time
  then ID") and what imported mixed timestamp formats require.
- The **workspace sequence** is `MAX(sequence)` over the `events` table.
- The **parent issue** of a comment is the issue named by its `issue_id`.

## Commands and syntax

One new top-level noun `comment` with four subcommands:

```text
bead comment create  <ISSUE_ID> --body <TEXT> [--actor <WHO>] [--json] [--dry-run]
bead comment create  <ISSUE_ID> --body-file <PATH> [--actor <WHO>] [--json] [--dry-run]
bead comment reply   <ISSUE_ID> --reply-to <COMMENT_ID> (--body <TEXT>|--body-file <PATH>)
                     [--actor <WHO>] [--json] [--dry-run]
bead comment resolve <ISSUE_ID> <COMMENT_ID> [--actor <WHO>] [--json] [--dry-run]
bead comment unresolve <ISSUE_ID> <COMMENT_ID> [--actor <WHO>] [--json] [--dry-run]
```

| Element | Decision |
| --- | --- |
| Positional `<ISSUE_ID>` | Required selector on all four commands, first positional, matching `bead close <ID>` style. |
| Positional `<COMMENT_ID>` | Second positional on `resolve`/`unresolve`. Both positionals are required so the caller names the issue and the comment explicitly; the implementation verifies the comment belongs to that issue. |
| `create` vs `reply` | `create` takes no `--reply-to`. `reply` requires `--reply-to`. There is no alias and no overlap: a creation is never a reply and a reply is never a root comment. |
| Body transport | Exactly one of `--body TEXT` and `--body-file PATH` is required, mirroring `data set`'s `--value`/`--value-file` transport (structured-data-input-v1). A path is relative to the invocation directory; `--body-file -` reads the body from standard input through EOF. The bytes read are scanned and stored unmodified; nothing echoes them. Supplying both or neither is a usage error. |
| `--actor <WHO>` | Optional on all four commands. See *Author attribution*. |
| `--json` | Optional on all four commands; selects the result envelope (see *Result envelope and output*). Without it, the commands print one line: the affected comment ID. |
| `--dry-run` | Optional on all four commands. Performs selector lookup, body transport, validation, and the full secret scan without committing rows, events, or acknowledgments, then always prints the JSON envelope (matching existing mutation dry-runs, which print JSON regardless of text mode). |
| Near-miss guidance | `bead comment add` is rejected with the standard unrecognized-subcommand error plus an R037-style remedy line: use `bead comment create`. Exit 2. |
| Name spacing | `bead comment resolve` is distinct from the existing top-level `bead resolve` (attempt outcomes). No command or flag name collides anywhere in the tree. |

## Immutability of comment bodies

- No command in this specification or in the version 1 CLI edits a comment
  body, author, `created_at`, `issue_id`, or `reply_to_id`. There is no
  `comment edit`, `comment delete`, or `comment move`, and none is reserved.
- Bodies selected with `--body` are stored exactly as supplied; bodies read
  from `--body-file` are stored exactly as read. Emptiness is checked after
  trimming for validation only; the stored bytes are never trimmed, escaped,
  newline-normalized, or otherwise rewritten.
- Imported bodies remain byte-identical through import, export, backup,
  restore, and projection, as they are today.
- Resolution state is the only mutable comment column, and only the two
  resolution commands mutate it. A comment has no `updated_at`; the time of a
  resolution change lives in its audit event, and no resolution timestamp
  column is added.
- `bead redact` remains the only sanctioned rewriter of comment content. It
  may rewrite `comments.author` and `comments.body` (the redaction field
  selector already admits exactly those two fields) and does so with its own
  receipts, tombstones, and audit trail. Comment commands treat redacted
  comments like any other: they remain addressable, and their placeholder
  bodies are preserved.

## Author attribution

The resolved actor of a comment mutation is decided by, in order:

1. `--actor <WHO>` when the flag is present and, after trimming, nonempty;
2. the `BEAD_ACTOR` environment variable when it is set and, after trimming,
   nonempty;
3. the literal `system`.

This is the plan's BR-T12 attribution ladder (plan line 817: mutating
commands accept `--actor` and `BEAD_ACTOR` and record it in the audit event
instead of the `system` default). Comment commands adopt it at birth, which
is additive and conflicts with nothing; BR-T12's later generalization to the
remaining mutating commands neither changes nor depends on this document.

- The actor string is stored verbatim: no case folding, no normalization.
  It is capped at 255 bytes, the same cap applied to labels and identifiers.
- On `create` and `reply`, the resolved actor becomes the comment's `author`
  and the audit event's `actor` column.
- On `resolve` and `unresolve`, the resolved actor is recorded in the audit
  event's `actor` column only. The comment's stored `author` is never
  touched by a resolution command.
- The actor participates in the secret scan as a scanned field on every
  command (see *Secret-scan interaction*), because it is stored durable text.

## Validation rules

All rules run before any row is written; a failed validation leaves the
database, the event stream, and the checkpoint untouched. The issue selector
is resolved first; failures map to the exit codes in *Exit codes*.

| # | Rule | Failure |
| --- | --- | --- |
| V1 | A bead-rs workspace exists and is initialized. | exit 3 |
| V2 | The issue selector names an existing issue. | exit 3, `Issue <id> not found` |
| V3 | The body is supplied by exactly one transport (`--body` or `--body-file`). | exit 2 |
| V4 | The body, after trimming, is nonempty. `Comment body cannot be empty`, same wording rule as the close path's reason check. | exit 2 |
| V5 | The body is at most 4194304 bytes, the same cap the model applies to long text fields. | exit 2 |
| V6 | Body file input is valid UTF-8. Unreadable files (missing, permission) are transport failures, not validation failures. | invalid UTF-8: exit 4; transport I/O: exit 1 |
| V7 | The actor, after resolution, is nonempty and at most 255 bytes. | exit 2 |
| V8 | The reply target names an existing comment **that belongs to the issue selector**. A dangling or cross-issue reply target is a validation failure of the request, not a missing selector. | exit 4 |
| V9 | The resolution target names an existing comment that belongs to the issue selector. | exit 4 |
| V10 | A generated comment ID does not collide with any existing `comments.id`. | retried; exhaustion: exit 5 |

Additional decided semantics:

- **Authorization is the shared write gate.** The local CLI has no identity
  provider, so authorization means: the shared write gate admits the write.
  All four commands acquire the main-database write lock, recheck the secret
  maintenance hold, and refuse during recovery quarantine, exactly like every
  other service-layer mutation. Refusals from those shared gates surface
  their existing error types and exit codes; this specification introduces no
  new gate and no new gate code.
- **Lifecycle is not consulted.** Comments may be created, replied to,
  resolved, and unresolved on issues in any base status, including `closed`.
  Claim and lease state is never consulted: commenting on a claimed bead is
  the conversational norm this feature exists to support.
- **Reply depth is unbounded** and a reply may target a resolved comment;
  resolution is per-comment state, not a thread lock. Projection levels still
  apply per comment, so `--comments unresolved` shows such a reply while
  omitting its resolved parent.
- **Generation of comment IDs** follows the existing internal primitive:
  `comment-` plus 8 lowercase hex characters from 4 random bytes, checked
  against `comments.id` (which is globally unique across issues) and retried
  up to 5 times before failing as an integrity error.
- **Native creation stores `resolution_state = 'unresolved'` explicitly.**
  Imported NULL remains legal and is read as `unresolved` everywhere; the
  first resolution transition writes the explicit string.

## Audit events and event-schema impact

Three event kinds carry all comment mutations. `comment_added` already
exists in the codebase as the internal comment path's kind and is reused
verbatim, so no kind is renamed and no historical row changes meaning.

| Kind | Emitted by | `detail` JSON |
| --- | --- | --- |
| `comment_added` | `comment create` | `{"comment_id":"<id>"}` |
| `comment_added` | `comment reply` | `{"comment_id":"<id>","reply_to_id":"<parent id>"}` |
| `comment_resolved` | `comment resolve`, semantic transitions only | `{"comment_id":"<id>"}` |
| `comment_unresolved` | `comment unresolve`, semantic transitions only | `{"comment_id":"<id>"}` |

Rules for every comment event:

- **Columns.** `issue_id` is the parent issue (satisfying the foreign key and
  keeping comment events in the issue's event stream); `actor` is the
  resolved actor; `time` is the RFC 3339 UTC commit instant from the shared
  timestamp helper; `detail` is the compact JSON string above; `sequence` is
  the autoincrement value, which is the ordering authority for events that
  share a second.
- **No-op transitions emit nothing.** A resolution command that finds the
  target already in the requested state emits no event, writes no row, and
  still exits 0 with `semantic_change: false` in its envelope.
- **Same-transaction rule.** The comment row write and its event append
  commit atomically through the shared `atomic_write` transaction helper,
  which also performs the maintenance-hold recheck under the acquired write
  lock.
- **Bodies never enter event detail.** The body's durable, scanned,
  redactable home is the `comments` row. Duplicating it into `detail` would
  copy scanned content into a surface the redaction field selector does not
  cover (`comments` redaction admits exactly `author` and `body`). Detail
  keys are therefore identifiers only, and event detail stays free of comment
  bodies, resolution metadata beyond the ID, and issue titles.
- **Detail is additively extensible.** Consumers, including the forensic
  replay machinery, MUST ignore unknown detail keys. Adding a key later is
  not a breaking change.

**Event-schema impact: none.** No column is added to `events` or `comments`,
no table is created, and no schema URN in the capabilities catalog changes.
Checkpoint validators treat event kinds as opaque strings — canonical
ordering, continuity, and graph checks do not consult them — so the new
kinds flow through publication, staging, import, and restore unchanged.
Binaries older than this feature replay the new kinds as ordinary event rows
and preserve their comments through the existing `checkpoint_issue.comments`
field; they simply expose no way to produce new ones, which is exactly the
pre-R010 portability guarantee preserved in reverse.

## Revision and fencing interaction

- Comment commands take **neither `--if-revision` nor `--fencing-token`**, and
  implementations MUST NOT accept them. There is nothing to fence: the
  commands never read or write an issue-row field.
- **The parent issue's `revision` and `updated_at` are not bumped.** This
  matches the established child-table precedent — dependency and label
  writes do not bump the parent revision, and the existing internal
  comment-append primitive does not either. The issue revision remains the
  optimistic-concurrency token for issue-row fields exactly as it is today.
- **Consequence for issue-row OCC:** a concurrent `update --if-revision N`
  on the parent is neither invalidated nor blocked by comment activity, and
  comment activity cannot be used to circumvent an issue-row revision guard.
  The two mutation surfaces are independent by construction.
- **Concurrency between comment commands:** `create` and `reply` are
  append-only with server-generated unique IDs, so concurrent creations both
  commit. Resolution commands are last-writer-wins on one row's
  `resolution_state`, and every real transition appends its event, so the
  full resolution history is reconstructible from the event stream. The
  no-op rule makes retries safe: a repeated resolve after a lost response
  exits 0 and emits nothing.
- **Change detection** is by event, not revision: observers watch for
  `comment_added`, `comment_resolved`, and `comment_unresolved` events, or
  compare the `workspace_sequence` returned in the result envelope.

## Result envelope and output

Text mode (no `--json`) prints exactly one line on stdout: the affected
comment ID — the new ID for `create`/`reply`, the target ID for
`resolve`/`unresolve` — matching the ID-only stdout contract of `create`,
`close`, and `reopen`. Diagnostics always go to stderr and never corrupt the
machine-readable stream.

`--json` prints one pretty-printed JSON object:

```json
{
  "operation": "created | replied | resolved | unresolved",
  "id": "<comment id>",
  "issue_id": "<issue id>",
  "reply_to_id": "<parent id> or null",
  "author": "<resolved actor>",
  "resolution_state": "unresolved | resolved",
  "previous_resolution_state": "unresolved | resolved | null",
  "semantic_change": true,
  "workspace_sequence": 12345,
  "secret_scan": {}
}
```

- `operation` is one of the four literals, past tense, matching event-kind
  vocabulary.
- `reply_to_id` is present on `created` and `replied` envelopes (null for
  `created`) and absent from resolution envelopes.
- `previous_resolution_state` is present only on resolution envelopes; it is
  null when the stored state was the import-compatibility NULL.
- `workspace_sequence` is `MAX(events.sequence)` after commit in real runs
  and the current maximum in dry-runs.
- `semantic_change` is false exactly for no-op transitions and dry-runs of
  no-ops.
- `secret_scan` is present exactly when the scanner's advisory summary
  exists, inserted by the same `decorate_mutation` step every other mutation
  result uses. Its absence means nothing was found; its presence never means
  the write was blocked — blocked writes exit 4 with no envelope on stdout.
- **The body is never in the envelope**, in success or failure output, in
  text or JSON mode. Retrieval of bodies goes through `list`/`show` with
  `--comments unresolved|all`, whose projection defaults are unchanged. This
  also keeps the NEEDLE v1 contract intact: its issue JSON minimum is
  untouched and its consumers still receive no comment bodies by default.

Dry-runs always print the JSON envelope with the same shape, computed without
committing rows, events, or acknowledgments, and report scan findings on
stderr through the same dry-run diagnostic path as every other mutation.

## Secret-scan interaction

Comment bodies are the largest new free-text write surface in the CLI, so the
scan integration is specified at both layers and classified exhaustively.

**CLI layer (ADR-014 canonical request).** `Command::Comment` arms MUST be
added to the exhaustive `canonical_request` match. The command-inventory test
exists precisely so that "a new mutation cannot silently inherit an unscanned
string field"; an unclassified comment command fails the suite by
construction. The canonical fields are:

| Command | Scanned canonical fields |
| --- | --- |
| `comment create` | selector `issue:<id>` (or `issue:input` for a non-conforming selector), required `body`, `author` (resolved actor) |
| `comment reply` | selector as above, required `body`, `author`, `reply_to` (target comment ID) |
| `comment resolve` / `unresolve` | selector as above, `comment` (target comment ID); no free-text fields exist, and none may be added without extending this table and the inventory |

**Service layer (write boundary).** The service functions route through
`guard_pairs`, exactly as the existing internal comment primitive does:
creation guards `("id", issue_id)`, `("body", body)`, `("author", actor)`;
reply additionally guards `("reply_to_id", target)`; resolution guards the
two identifier pairs. The guard runs inside the same transactional gate that
rechecks the maintenance hold and quarantine state.

**Modes and outcomes.** The workspace-level scan mode applies unchanged:
`enforce` (the compiled default) rejects the write with a validation error
carrying the stable machine reason `secret_detected` (exit 4), leaving no
rows and no events; `advisory` writes anyway, prints the standard advisory
line to stderr, and records the acknowledgment audit; `off` does not scan.
There is no per-invocation bypass flag; admitting an exact finding by
fingerprint follows secret-write-boundary-v1 and is unavailable in
managed-policy builds. Comment commands introduce no new mode, flag, policy
file, or acknowledgment shape.

## Exit codes

Comment commands reuse the existing error ladder without adding a code. The
mapping is total over the decided failure modes:

| Exit | Meaning | Comment-command instances |
| --- | --- | --- |
| 0 | Success | Committed creation or reply; committed semantic transition; no-op transition; completed dry-run |
| 1 | Internal failure, including the post-commit publication split | Body-file/stdin read failure (transport I/O); checkpoint publication failure after a committed mutation, pinned to 1 by the plan 6.2.1 split-outcome rule |
| 2 | CLI usage or argument validation | Missing, both, or neither of `--body`/`--body-file`; empty or whitespace-only body; body over the cap; empty or oversize actor; unknown flag; `comment add` near-miss |
| 3 | Workspace or selector not found | No bead-rs workspace; issue selector does not exist |
| 4 | Conflict or validation | Reply target missing or cross-issue; resolution target missing or cross-issue; invalid UTF-8 body input; blocking secret-scan finding (`secret_detected`); refusals from the shared maintenance-hold or quarantine gates |
| 5 | Integrity | Comment ID generation exhausted its retry budget; store integrity failure raised by the write gate |
| 6 | Database busy | Write-lock contention reported by SQLite busy handling |

No comment failure reports exit 4 for a missing issue selector or exit 3 for
a bad reply target; the selector/referenced-record split above is the rule.

## Help, man pages, and capability advertisement

- **clap help.** The `comment` noun and each subcommand carry `about` and
  `long_about` with EXAMPLES blocks in the established house style (see the
  `restore` long-about): what the command writes, what it refuses to write,
  the body transports, and the actor ladder. `bead comment --help` lists all
  four subcommands; the plan-audit observation that it reports an
  unrecognized subcommand is the defect this item closes.
- **Man pages.** Generated from the clap tree by the existing generator,
  which names pages by full command path: `bead-comment.1`,
  `bead-comment-create.1`, `bead-comment-reply.1`, `bead-comment-resolve.1`,
  `bead-comment-unresolve.1`. No hand-written page is added, so the
  leaf-name collision failure mode the generator documents cannot recur.
- **Capabilities.** `capabilities.commands` gains `"comment"` in alphabetical
  position, so capability-aware consumers can detect the feature before
  calling it. The schemas catalog and profile declarations are unchanged;
  `needle-v1` continues to deliver no comment bodies in its envelope.
- **Field guide.** The `checkpoint_issue`/`comments` field semantics entry
  changes `operations` from the empty list to the four public operations,
  with invariants otherwise unchanged. Because the field guide is
  digest-pinned, this update requires regenerating its snapshot digests and
  the literal pins in the same change as the command work, per the
  established field-guide bump procedure.
- **Consumer contract.** `needle-cli-contract-v1` is untouched: comment
  commands are additive and appear in none of its required tables, and no
  required command changes output.

## Conformance scenarios

Each scenario is stated given/when/then; "commits" means both the row write
and its event land atomically, and "touches nothing" means no row, no event,
no acknowledgment, and no checkpoint change.

1. **Create.** Given a workspace with issue `X`, when
   `bead comment create X --body "hello" --actor worker-1`, then a comment
   row with author `worker-1`, explicit `resolution_state` `unresolved`, an
   RFC 3339 UTC `created_at`, and a `comment-`-prefixed ID commits with one
   `comment_added` event; stdout is the ID; exit 0.
2. **Reply.** Given comment `C` on issue `X`, when
   `bead comment reply X --reply-to C --body "point" --actor worker-2`, then
   a comment row with `reply_to_id = C` commits with a `comment_added` event
   whose detail carries both IDs; exit 0.
3. **Resolution transitions.** Given unresolved `C` on `X`, when
   `bead comment resolve X C --actor reviewer`, then `resolution_state`
   becomes `resolved` and one `comment_resolved` event commits; repeating
   the command exits 0, emits no event, and reports
   `semantic_change: false`; `bead comment unresolve X C` then emits
   `comment_unresolved` once.
4. **Import compatibility.** Given an imported comment with NULL resolution
   state, when `bead comment unresolve` targets it, then the command is a
   no-op (NULL reads as unresolved); when `bead comment resolve` targets it,
   it is a semantic transition that writes the explicit string and
   `previous_resolution_state` in the envelope is null.
5. **Bad reply target.** When a reply names a nonexistent comment, or a
   comment that belongs to a different issue, then exit 4 and nothing is
   touched.
6. **Missing selectors.** When the issue selector does not exist, then
   exit 3 with `Issue <id> not found` and nothing is touched.
7. **Body transport.** When neither or both of `--body`/`--body-file` are
   given, exit 2. When `--body-file -` receives a body on stdin, it is
   stored and scanned identically to the literal form. When the file is
   unreadable, exit 1; when it is not valid UTF-8, exit 4; when it is empty
   or whitespace, exit 2. No output ever echoes the body.
8. **Scan outcomes.** In `enforce` mode, a body carrying a blocking finding
   exits 4 with `secret_detected` and leaves no rows or events. In
   `advisory` mode the same write commits, the advisory line goes to stderr,
   and the acknowledgment audit records the finding. In `off` mode no scan
   runs.
9. **Projection defaults.** After any comment mutation,
   `bead show X` and `bead list` still default to `--comments none`, bodies
   appear only under `--comments unresolved|all` in canonical comment order,
   and the mutation receipt carries no body.
10. **Checkpoint round trip.** After a mutation, `sync --flush-only`
    publishes the comment row through `checkpoint_issue.comments` and the
    event through the ordinary event stream; import into a fresh workspace
    restores both verbatim; a pre-feature binary replays the new event kinds
    as opaque rows.
11. **Revision independence.** Given a parent at revision N, any sequence of
    comment mutations leaves the parent at revision N with `updated_at`
    unchanged, and a concurrent `update --if-revision N` still succeeds.
12. **Shared gates.** While a secret maintenance hold is active, every
    comment command is refused by the shared gate with its existing error
    and exit code; during recovery quarantine likewise; nothing in this
    specification bypasses either.

## Relationship to existing commands

- **`list`/`show`.** Unchanged, including the `--comments` projection
  semantics, defaults, and canonical order. The mutation commands produce
  exactly the rows those projections already know how to render.
- **`resolve` (attempts).** Distinct command tree; no interaction. The
  near-miss remedy lines distinguish them.
- **`create`/`close`/`reopen`/`update`.** Issue-row mutation paths are
  untouched: no comment command bumps their revisions, and their
  `--if-revision` guards see no comment traffic.
- **`sync`/`restore`/`import-only`.** The event stream and
  `checkpoint_issue.comments` already carry comments wholesale; no sync code
  changes. The plan's requirement that import/export never drop or rewrite
  comments is preserved in both directions.
- **`redact`.** Already covers `comments.author` and `comments.body`; it
  remains the only content rewriter. Comment commands impose no new
  redaction obligation.
- **`doctor`.** The existing `comments` diagnostic scope already detects
  orphaned comments and dangling `reply_to` references; native commands make
  both uncreatable, and no new doctor scope is added.
- **`data set`.** The `--value`/`--value-file` pair is the transport
  precedent the body flags follow; comment bodies are plain text, not JSON,
  so no parse step exists between scan and store.
