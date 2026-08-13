# Native field guide contract v1

Status: corrected proposed normative specification; awaiting independent
re-review.

Original author and correction author: OpenAI Codex interactive session,
2026-08-12.

Original independent review: Claude (Anthropic), rejection recorded in
`docs/reviews/adr-002-field-guide-independent-review-2026-08-12.md`.

Artifact identity: `urn:bead-rs:schema:field-guide:native-v1`.

This specification defines the public, agent-facing documents and behavior of
a native bead. It distinguishes the issue projection returned by interactive
CLI reads from the richer issue and event records published in a checkpoint.
It never describes or authorizes access to the private SQLite layout.

Implementation may begin only after a reviewer who authored neither this
original artifact nor this correction records an acceptance decision against
the corrected file's exact SHA-256.

## 1. Required schema interface and typed guide document

The interface is:

```text
bead schema list --format json
bead schema show SCHEMA_REF --format json
bead schema explain SCHEMA_REF --format json|markdown
```

Unsupported identifiers are usage failures (exit 2). Missing workspaces or
documents are not-found failures (exit 3). State conflicts are exit 4;
malformed or integrity-invalid documents are exit 5.

`schema list` returns catalog entries containing `schema_ref`,
`document_kind`, `readable`, `writable`, and the supported `validate`,
`consume`, and `emit` operations. The readable/writable booleans intentionally
match the capability catalog rather than defining a divergent schema catalog.
`schema show` returns the exact immutable JSON Schema Draft 2020-12 document
whose `$id` equals `SCHEMA_REF`.

`schema explain urn:bead-rs:schema:issue:native-v1 --format json` returns this
typed document:

```json
{
  "schema_ref": "urn:bead-rs:schema:field-guide:native-v1",
  "guide_version": 1,
  "describes_schema_refs": [
    "urn:bead-rs:schema:issue:native-v1",
    "urn:bead-rs:schema:event:native-v1",
    "urn:bead-rs:schema:provenance-receipt:native-v1"
  ],
  "documents": [],
  "fields": [],
  "additional_properties": {},
  "lifecycle": {},
  "derived_state": {},
  "events": {},
  "operations": [],
  "rehydration": {},
  "known_implementation_deviations": []
}
```

The field-guide schema requires exactly those top-level members.
`schema_ref` is the artifact identity; `guide_version` is integer `1`;
`describes_schema_refs` is a unique, sorted array of absolute schema URIs.
`documents`, `fields`, `operations`, and `known_implementation_deviations` are
arrays; the other named sections are objects. Each `fields` entry requires:

```json
{
  "document": "checkpoint_issue",
  "name": "priority",
  "json_type": "integer",
  "nullable": false,
  "presence": "required",
  "has_default": true,
  "default": 2,
  "ownership": "caller",
  "operations": ["create"],
  "invariants": ["integer from 0 through 4"],
  "example": 2,
  "common_mistake": "Treating P3 as the native default."
}
```

`document`, `name`, `json_type`, `nullable`, `presence`, `has_default`, `default`,
`ownership`, `operations`, `invariants`, `example`, and `common_mistake` are
required. `has_default` distinguishes no default (`false`, with `default` null
as a placeholder) from an actual null default (`true`, `default` null).
`default` and `example` may otherwise contain any JSON value. Field entries are
uniquely keyed by `(document, name)`. Markdown output is a deterministic
rendering of this same typed value: fixed section order, fields in document
order, operations lexicographically sorted, LF line endings, and no generated
timestamp.

The normative document names are `cli_issue`, `checkpoint_issue`,
`claim_result`, `checkpoint_event`, and `checkpoint_provenance_receipt`.
Each `documents` entry is an object with required string `name`, `schema_ref`,
`document_kind`, `transport`, and `member_source`, plus required sorted string
array `members`. `additional_properties` requires boolean `allowed`, string
`ownership`, and string-array `rules`. `lifecycle` requires string arrays
`base_values` and `allowed_transitions`. `derived_state` requires objects
`status`, `ready`, `blocked_by`, and `blocking`, each with string `ownership`
and string-array `rules`. `events` requires string `envelope_member`, string
`schema_ref_member`, and string arrays `identity` and `ordering`.

Each `operations` entry requires string `name`, string `ownership_effect`,
integer `success_exit`, integer array `failure_exits`, string array
`affected_fields`, and string-array `rules`. `rehydration` requires string
`source_mode`, string array `allowed_writes`, string array `forbidden_writes`,
and string-array `verification`. Each `known_implementation_deviations` entry
requires string `id`, `severity`, `behavior`, and `required_disposition`.
Unknown members in the guide document itself are rejected. Arrays described as
sorted contain unique values. URI-valued strings are absolute URIs.

Completeness tests compare `fields` with every declared member of the CLI
issue projection, claim result, checkpoint issue, checkpoint event, and
checkpoint provenance-receipt documents. They fail on a missing, duplicate,
or stale field or lifecycle value. They separately compare
`additional_properties` with the checkpoint issue schema's
`additionalProperties`; unknown members are not a fictitious field named
`extensions`.

## 2. Ownership vocabulary and JSON presence

- **caller**: supplied through a public mutating command.
- **system**: generated or maintained by bead-rs; callers never synthesize it.
- **derived**: computed from durable issue or graph state and never imported as
  authoritative state.
- **preserved**: retained byte-semantically as opaque JSON without implying
  that bead-rs understands it.

Member absence, explicit JSON `null`, an empty string, and an empty collection
are distinct representations. Producers and consumers preserve that
distinction where the document schema permits it. A projection may deliberately
materialize a default, but that projection does not rewrite checkpoint
presence. Unknown extension values retain their exact JSON type and presence.

All timestamps below are RFC 3339 UTC strings with nanosecond precision, for
example `2026-08-12T22:51:52.301697398Z`.

## 3. The two public issue documents

### 3.1 Interactive CLI issue projection

`bead list --json` emits one compact JSON object per line. `bead show --json`
emits a JSON array containing exactly one such object. The issue object has
exactly these members in v0.1:

`id`, `title`, `description`, `priority`, `status`, `assignee`,
`dependencies`, `created_at`, `updated_at`, `labels`, and `revision`.

It does not expose `notes`, `manual_blocked`, `issue_type`, `closed_at`,
`close_reason`, `source_repo`, `profile`, `schema_ref`, structured `data`, or
unknown checkpoint members. Omission from this projection is not data loss in
the live store or checkpoint.

`status` is the agent-facing projection of native state. Its intended mapping
is `blocked` when `manual_blocked` is true on non-closed work, otherwise the
string form of `base_status`: `open`, `in_progress`, `deferred`, or `closed`.
Readiness is stricter than `status == open`, because assignment and graph
blockers also apply. The current v0.1 implementation projects `base_status`
without applying the manual-blocked overlay; this is a known producer defect,
not permission for consumers to treat `blocked` as a stored base value.

The projected dependency objects contain `blocker` and `kind`; `labels` and
`dependencies` are arrays. `description` is materialized as `""` when the
checkpoint member is absent, `assignee` is emitted as explicit null when
unassigned, and `revision` is materialized as `1` only when an older in-memory
value lacks it.

### 3.2 Native checkpoint issue record

Every checkpoint issue is carried in this envelope:

```json
{"record_type":"issue","issue":{"schema_ref":"urn:bead-rs:schema:issue:native-v1"}}
```

The `issue` object declares these named members: `id`, `title`, `revision`,
`description`, `notes`, `priority`, `base_status`, `manual_blocked`,
`assignee`, `issue_type`, `created_at`, `updated_at`, `closed_at`,
`close_reason`, `source_repo`, `profile`, `schema_ref`, `data`, `labels`, and
`dependencies`, `comments`, and `external_references`. Those four collection
projections reach the serialized object through the flattened representation,
but are known public projections, not unknown extensions. It may also contain
unknown additional properties governed by section 5. It never stores `status`,
`ready`, or `blocked_by`.

Checkpoint issue records use `schema_ref`; event records currently use
`$schema`. This inconsistency with `schema-identification-v1.md` is explicit
v0.1 behavior. Schema implementation must document it and must not silently
rename either member; resolving it requires a separately versioned schema
decision.

### 3.3 Projection mapping

The normative projection copies `id`, `title`, `priority`, `assignee`, `created_at`,
`updated_at`, and `revision`; it materializes absent `description` as `""`;
maps `base_status` plus `manual_blocked` to `status`; and joins the public label
and dependency projections. All other checkpoint issue members are omitted.
Current v0.1 code does not apply the manual-blocked overlay and instead maps
`base_status` alone; that deviation is the one recorded in section 3.1.

### 3.4 Claim result projection

`bead claim --json` emits a JSON object distinct from an issue projection. It
identifies the selected issue as `bead_id`, includes `assignee`, and may include
a `lease` object with `issue_id`, `assignee`, `fencing_token`, and `expires_at`.
An empty queue returns an object without a nonempty `bead_id`. This guide does
not rename `bead_id` to `id`.

## 4. Field semantics

Each subsection supplies the required example and common mistake as well as
type, presence, default, ownership, operations, and invariants.

### `id`

Checkpoint and CLI: required non-null string, no default, system-owned by
`create` and preserved by native restore. Example: `"bead-18409c0e"`.
Length is 1–255 UTF-8 bytes; leading/trailing whitespace, `/`, `\\`, NUL, and
control characters other than tab, LF, and CR are forbidden. It is immutable.
Mistake: manufacturing an ID or inferring chronology from its spelling.

### `title`

Checkpoint and CLI: required non-null string, no default, caller-owned by
`create` only. Example: `"Verify restore invariants"`. Length is 1–4096 UTF-8
bytes. Mistake: attempting `update --title`, which is a usage error.

### `revision`

Checkpoint and CLI: integer, system-owned, initial/default value `1`, increased
by semantic mutations. Example: `4`. `update`, `release`, `close`, and `reopen`
accept a previously read value through `--if-revision`; `claim` does not.
Mistake: choosing the next revision or treating it as time. Known defect:
released v0.1.1 reset revisions to 1. Main commit `02d4d62` preserves revision
through export, restore, and merge insertion. On merge update, an incoming
revision newer than the live token is retained; otherwise replacement advances
the live token by one. Thus a stale checkpoint cannot roll the token backward
and a holder of the pre-merge token cannot mutate replaced content.

### `description`

Checkpoint: optional nullable string with no creation default; absent when
`create` omits `--description`. CLI: required projection string, materialized
as `""` from an absent checkpoint member. Caller-owned by `create` only,
maximum 4 MiB.
Example: `"Rehearse flush and restore."`. Mistake: treating absent checkpoint
description, explicit null, and projected empty text as interchangeable.

### `notes`

Checkpoint only: optional nullable string with live default `""`,
caller-owned by `update --notes` only, maximum 4 MiB. Example:
`"Reproduction captured in the reconciliation report."`. Notes are checkpoint
content, not a secret store. Mistake: expecting `create --notes` or assuming
notes are private.

### `priority`

Checkpoint and CLI: required non-null integer, caller-owned by `create` only;
default and example `2`. Values are P0 urgent, P1 critical, P2 high (native
default), P3 normal, and P4 aspirational/backlog. Lower is more urgent.
Mistake: calling P2 normal or reversing the ordering.

### `base_status`

Checkpoint only: required non-null enum, system-mediated by `claim`, `update`,
`release`, `close`, and `reopen`; default and example `"open"`. Values are
`open`, `in_progress`, `deferred`, and `closed`. Mistake: storing `blocked` or
`ready` as a base value.

### `status`

CLI only: required non-null derived string, no caller-set default; example
`"open"`. It is the projection described in section 3.1. The post-fix target
may emit `"blocked"` for manual blocking, but v0.1 cannot currently produce
that value. Mistake: assuming `status == open` proves readiness or that a
`status` member exists in a native checkpoint issue.

### `manual_blocked`

Checkpoint only: optional nullable boolean, effective default and example
`false`, caller-owned through `update --status blocked|open`; `close` and
`reopen` clear it. True prevents readiness. Mistake: encoding graph blocking in
this flag or assuming false proves readiness.

### `assignee`

Checkpoint: optional nullable string with no default; when unset the member is
absent, and an imported explicit null reserializes as absence. CLI: required
nullable member with default/example null. Caller-owned by `create --assignee`,
`claim --assignee`, `update --assignee`, `update --clear-assignee`, and
`release`; nonempty when present. Claim assigns and enters `in_progress`.
Release applies to `in_progress` work; an open assigned bead uses `update
--clear-assignee`. Mistake: treating assignment as authorization.

### `issue_type`

Checkpoint only: optional nullable string, effective default and example
`"task"`, caller-owned by `create` only. It is a free nonempty string with no
enumerated value validation. Mistake: attempting to update it or treating the
examples in CLI help as an exhaustive enum.

### `created_at`

Checkpoint and CLI: required non-null timestamp, system-owned by `create`,
immutable, and preserved by native restore. Example:
`"2026-08-12T22:51:52.301697398Z"`. Mistake: synthesizing a source tracker's
time as a native creation instant during rehydration.

### `updated_at`

Checkpoint and CLI: required non-null timestamp, system-owned, advanced by
semantic mutation, and preserved by native restore. Example:
`"2026-08-12T22:53:00.000000000Z"`. Mistake: using it as an optimistic
concurrency token; use `revision`.

### `closed_at`

Checkpoint only: optional nullable timestamp with no default; absent on active
work, example `"2026-08-12T23:00:00.000000000Z"` when closed. System-owned by
`close` and cleared to absence by `reopen`. Normative invariant: present exactly
when `base_status` is `closed`. Main commit `2ce61ce` rejects generic update to
closed and makes doctor detect inconsistent existing rows. Import validation
rejects the invariant in both directions and diagnostic activation remains a
no-op when any invalid record is reported. Doctor does not guess repairs for
legacy rows; operators must explicitly remediate them before cutover. Released
v0.1.1 remains vulnerable. Mistake: assuming detection repaired legacy rows.

### `close_reason`

Checkpoint only: optional nullable string with no default; absent on active
work, example `"Completed and verified"` when closed. Caller-supplied by `close
--reason` and cleared to absence by `reopen`. It is nonempty for closed issues
and has no length bound. Mistake: omitting `--reason`.

### `source_repo`

Checkpoint only: optional nullable string with no default; absent normally,
example `"forgejo:jedarden/project"` when preserved. It has no public writer in
v0.1 and is unreachable through normal creation. It is never network-resolved.
Mistake: editing checkpoint JSON to inject provenance; use external references
and the separate reconciliation report.

### `profile`

Checkpoint only: optional nullable string with effective default/example
`"native-v1"`, system-owned by native checkpoint operations. Version 0.1
accepts no external checkpoint profile. Mistake: relabeling foreign records as
native.

### `schema_ref`

Checkpoint only: required non-null absolute URI, system-owned and immutable;
default/example `"urn:bead-rs:schema:issue:native-v1"`. Every native-v1 issue
record carries it. It identifies the public document schema, not SQLite.
Mistake: silently replacing an unknown reference.

### `data`

Checkpoint only: optional nullable JSON object with no default; absent when no
data exists, example `{"example":{"schema_ref":"urn:example:v1","value":{}}}`
when present. Caller-owned through `bead data set|get|list|remove`. Each
namespace has an immutable schema reference and arbitrary JSON value. Mistake:
editing the aggregate through issue update. Released v0.1.1 lost this table;
main commit `1943551` preserves each namespace, schema reference, and exact JSON
value through export and activation.

### `labels`

CLI projection and checkpoint graph projection: required array where emitted,
default/example `[]`, caller-owned by idempotent `label add|remove` and by
`create --label`. Values are case-sensitive strings. Mistake: treating a label
as lifecycle state.

### `dependencies`

CLI projection and checkpoint graph projection: required array where emitted,
default/example `[]`, caller-owned by idempotent `dep add|remove`. A CLI entry
example is `{"blocker":"bead-a","kind":"blocks"}`. Checkpoint edges retain
blocked ID, blocker ID, kind, and optional condition. Mistake: reversing the
blocked-first direction.

### `comments`

Checkpoint known projection: optional array, absent when empty. Each entry
contains `id`, `author`, `body`, nullable `reply_to_id`, nullable
`resolution_state`, and `created_at`; issue ownership comes from the enclosing
record. Checkpoint order is creation time then ID. Interactive reads expose
only the projection selected by `--comments`. Mistake: treating comments as
unknown extensions or assuming an interactive omission means no durable
comments exist.

### `external_references`

Checkpoint known projection: optional array, absent when empty. Each entry has
required non-null strings `namespace`, `key`, and `value`; the enclosing issue
provides `issue_id`. Caller-owned through `ref add|remove`; example
`{"namespace":"source","key":"issue-id","value":"bf-123"}`. Main
checkpoint code preserves the collection transactionally on restore. Merge
uses replace-when-present and preserve-when-absent semantics for external
references, comments, and structured data. This lets checkpoints from older
producers omit unsupported projections without deleting live target state.
Labels and dependencies merge additively. Scalar issue content follows the
newer `updated_at`, with revision behavior defined above.
Mistake: confusing these tracker/commit bindings with structured-data
`schema_ref` values.

## 5. Unknown additional properties

The checkpoint issue object may contain unknown JSON members as
`additionalProperties`; there is no serialized member named `extensions`.
Each unknown member is preserved with exact name, JSON value, and
null-versus-absence semantics through native round trips. A name colliding with
a defined member is not an extension. Example: `"vendor.example/trace": null`.
Mistake: normalizing or dropping an unrecognized member.

For defined optional `Issue` members, explicit JSON null deserializes to
`Option::None` and reserializes as absence because the producer skips `None`.
Exact null-versus-absence preservation therefore applies to unknown extension
members, not to defined optional members.

## 6. Events

Checkpoints may contain zero or more durable event envelopes. A create-only
workspace has none. When present, their shape is:

```json
{"record_type":"event","event":{"$schema":"urn:bead-rs:schema:event:native-v1","origin_store_uuid":"workspace-uuid","origin_event_sequence":1,"issue_id":"bead-18409c0e","kind":"updated","actor":"system","time":"2026-08-12T22:51:52.301697398Z","detail":{}}}
```

Event members are: required non-null `$schema`, `origin_store_uuid`,
`origin_event_sequence`, `kind`, `time`, and `detail`; plus nullable
`issue_id` and import-tolerated nullable `actor`. Native producers emit a
non-null actor, but the import representation does not enforce it. `$schema`
has the event identity; the sequence is a positive,
monotonically contiguous integer within the origin store; `time` uses the
timestamp format in section 2. `detail` is a JSON value: deserialization
defaults an absent member to null, while current producers emit `{}`. The
complete v0.1 kind set is `updated`, `claimed`, `released`, `reopened`,
`closed`, and `assignment_cleared`. Create, dependency, and label mutations do
not append events; this is an explicit audit-coverage gap. Events are
system-owned audit facts appended by the operations that support them. They are
ordered by `(origin_store_uuid, origin_event_sequence)`, never rewritten as
issue state, and never synthesized by a rehydrating agent. Native restore
preserves their origin identity. Mistake: deriving authoritative current state
solely by replaying a partial event projection or manufacturing events in
checkpoint JSON.

The typed guide contains one `fields` entry for each event member:

- `$schema`: required non-null URI string, system-owned, example
  `"urn:bead-rs:schema:event:native-v1"`; mistake: replacing it with the issue
  record's `schema_ref` spelling.
- `origin_store_uuid`: required non-null string, system-owned, example
  `"workspace-uuid"`; mistake: substituting the destination store identity on
  restore.
- `origin_event_sequence`: required non-null positive integer, system-owned,
  initial example `1`; mistake: renumbering origin events after restore.
- `issue_id`: nullable string, system-owned, default/example null for a
  workspace event; mistake: assuming every event belongs to an issue.
- `kind`: required nonempty string, system-owned, example `"updated"`;
  mistake: treating an unknown kind as permission to discard the event.
- `actor`: producer-required but import-nullable string, system-owned from
  operation context, example `"system"`; mistake: treating it as proof of
  authentication or assuming import rejects null.
- `time`: required non-null timestamp, system-owned, example
  `"2026-08-12T22:51:52.301697398Z"`; mistake: using it as event identity.
- `detail`: required in producer output but import-defaulted JSON value;
  deserialization default null and producer example `{}`; mistake:
  interpreting arbitrary detail keys as durable issue fields or conflating the
  producer example with the import default.

## 6.1 Provenance receipts and other advertised schemas

The third checkpoint envelope is
`{"record_type":"provenance_receipt","provenance_receipt":{...}}`, governed
by `urn:bead-rs:schema:provenance-receipt:native-v1`. Its members are
`$schema`, `receipt_id`, `kind`, `source_store_uuid`, `target_store_uuid`,
`source_root_sha256`, `actor`, `created_at`, `counts`, `result`, nullable
`summary_event_identity`, and `receipt_sha256`. It records system-owned restore
or merge provenance and is not issue state.

The capability catalog also advertises checkpoint pointer, checkpoint
manifest, capabilities, issue, event, provenance-receipt, and field-guide
schema identities. `schema show` supports every identity advertised with
`validate: true`. `schema explain` returns the native field guide for issue,
event, and provenance-receipt identities described here; for other advertised
identities it returns a concise typed explanation of that document rather than
treating the identifier as unsupported. Only an identifier absent from the
catalog is an exit-2 unsupported-schema error.

## 7. Dependencies, readiness, and lifecycle

`bead dep add BLOCKED BLOCKER --kind blocks` means BLOCKER must close before
BLOCKED can be ready. `blocks` edges reject self-edges and cycles;
`relates_to` is informational. `--condition` attaches a bounded declarative
predicate. An active blocker is unfinished exactly when its base status is not
`closed`; a deferred blocker still blocks. `blocked_by` and `blocking` are
derived graph arrays exposed by human/diagnostic views, never stored lifecycle
fields.

Allowed transitions are: `open` to `in_progress`, `deferred`, or `closed`;
`in_progress` to `open`, `deferred`, or `closed`; `deferred` to `open` or
`closed`; and `closed` to `open` only through `reopen`. Same-state behavior is
command-specific and may be idempotent.

An issue is ready exactly when base status is `open`, it is not manually
blocked, it has no assignee, and it has no active unfinished `blocks` blocker.
`list --ready` inspects this frontier without reservation. Closing the last
blocker can expose a dependent; reopening it can remove the dependent again.

`claim` performs atomic selection, assignment, and transition to
`in_progress`. Default `fifo-v1` orders candidates by priority ascending,
creation time ascending, then ID ascending. R019 policies (`aging-v1`,
`impact-v1`, `rotation-v1`, and `balanced-v1`) may alter ranking but never the
readiness predicate. Claim has no revision guard; its atomic transaction and
optional lease fencing provide concurrency safety.

`release` semantically applies to `in_progress -> open` and clears assignment;
other non-idempotent states conflict. `close` requires a reason, clears manual
blocking, sets close metadata, and may expose dependents. `reopen` clears close
metadata and manual blocking and returns closed work to open.

## 8. Leases, change feeds, comments, references, and recurrence

A leased claim returns a fencing token and expiry. After expiry, that assignee
cannot `update`, `release`, or `close` until renewal or a new claim establishes
a current lease. A stale or mismatched token is an exit-4 conflict. Tokens are
coordination guards, not issue revisions.

`bead changes` exposes a cursor-based event feed bound to a snapshot/store
identity. Native restore can establish a different identity and invalidates
old cursors; consumers restart from a fresh cursor rather than guessing an
offset.

Comments are publicly readable through `list --comments` and `show --comments
none|unresolved|all`, but v0.1 has no public command to create them. They do not
alter readiness. The current flags are known to be inert in v0.1; consumers
must not claim comment-body preservation from interactive output alone.

External references are namespaced key/value links owned by `ref
add|remove|list|find`. They neither replace native IDs nor resolve over a
network and are the public mechanism for source tracker identifiers.

Recurrence templates are public definitions. They mint occurrences only when
an external caller invokes `recurrence materialize`; bead-rs has no autonomous
scheduler. Materialized occurrences are ordinary native issues with series
metadata governed by the recurrence interface.

## 9. Minimal examples and rehydration boundary

```text
bead create --title "Verify restore rehearsal" --priority 2
bead ref add --id TARGET_ID --namespace source --key issue-id --value SOURCE_ID
bead dep add BLOCKED_ID BLOCKER_ID --kind blocks
bead list --ready --json --limit 20
```

The source tracker and repository remain read-only. An agent initializes a
fresh native workspace and reconstructs selected work only through public
`bead` commands. It never writes SQLite, manufactures checkpoint JSONL, copies
system-owned fields/events, or claims that a source lifecycle value is native.

A separate reconciliation report gives every source ID a target native ID or
exactly one disposition: `omitted`, `merged`, or `unresolved`, with rationale.
External references bind native beads to source IDs. Rehearsal occurs in a
disposable workspace and compares counts, field/lifecycle intent, dependency
direction, and ready-frontier intent; then runs `bead doctor`, flushes a native
checkpoint, restores into a fresh empty workspace, and archives source input.
The report is evidence, never checkpoint input.

Main now preserves revisions, structured data, external references, and durable
comments through native restore and merge. The committed comprehensive
conformance test covers restore fidelity, merge replacement when projections
are present, preservation when they are absent, and revision-token
monotonicity. The `bf-3siqo` generic-update path and doctor detection were fixed
at `2ce61ce`; import validation is bidirectional and diagnostic activation is
all-or-nothing. Existing inconsistent rows are detected but not guessed at by
repair, so cutover validation must reject or explicitly remediate them before
restore. Full release gates remain independent of review of this specification.

## 10. Independent review protocol

The reviewer must have authored neither the original specification nor the
correction under review, and must not be the schema implementation author.
They verify clean-room provenance and all field/document/operation coverage,
then create a dated findings document under `docs/reviews/` and append (never
rewrite) a disposition to `PROVENANCE.md`. Both record the exact Git commit,
SHA-256 algorithm, full file digest, reviewer identity, and one outcome:

1. accepted;
2. accepted with required revisions (implementation may use only the explicitly
   accepted baseline and must track the revisions);
3. rejected with concrete corrections; or
4. rejected as non-reviewable.

The status header becomes `accepted normative specification` only after an
unconditional acceptance against that exact hash. Every public issue member,
event member, lifecycle value, derived state, owner, operation, invariant,
example, and common mistake must be represented in the typed source and both
renderings. Rejection keeps schema implementation and fleet rehydration
blocked; a corrected artifact receives a new hash and review. Incompatible
semantics after release require a new schema identity.
