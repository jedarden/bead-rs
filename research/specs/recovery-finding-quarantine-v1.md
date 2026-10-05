# Recovery finding quarantine v1

Status: accepted companion contract for `secret-write-boundary-v1`.

Artifact identity: `urn:bead-rs:spec:recovery-finding-quarantine:v1`.

This document fixes the recovery state and diagnostic vocabulary used by
`restore`, `sync import-only`, `sync reconcile`, `sync flush-only`, and
`sync commit`. It does not amend the independently accepted bytes of
`secret-write-boundary-v1`; it makes that contract's section 4 operationally
unambiguous for the later recovery implementations.

## 1. Governing contracts and boundary

The governing artifacts are:

- `research/specs/secret-write-boundary-v1.md`, SHA-256
  `3ef8948c1ad5b94c911780070e5ffee5f26856276d31ccb1b6b845c2eaae4f8e`;
- `research/specs/historical-redaction-v1.md`, SHA-256
  `5658ac80cf9594283ddf65742aff2f4b2020a0a7869a612ee2230ed99033a016`;
- `research/specs/verified-restore-v1.md`; and
- ADR-022, “Quarantine secret-bearing recovery before publication”.

This contract controls accidental creation of a new Git-trackable copy by
bead-rs. It does not sanitize a source artifact, remove bytes from already
published Git history, stop direct SQLite access, or replace credential
rotation and incident containment.

“Local recovery” means a deliberate operator invocation that verifies and
activates an existing restore/import input in the local SQLite store. “Git
publication” means any creation, replacement, staging, or commit of a
checkpoint pointer, manifest, object, monolithic root, forensic export, or
other Git-trackable checkpoint output. A local commit is not publication.

The distinction is mandatory:

| Operation | Local semantic state | Automatic Git publication |
| --- | --- | --- |
| `restore` | May activate verified historical input | Must be withheld when quarantined |
| `sync import-only` | May activate a valid import or merge atomically | Must be withheld when quarantined |
| `sync reconcile` | May activate a valid remote generation atomically | Must be withheld when quarantined |
| `sync flush-only` | Does not alter semantic recovery state | Refuses while quarantined |
| auto-flush / auto-stage | Does not alter semantic recovery state | Refuses while quarantined |
| `sync commit` | Does not alter semantic recovery state | Refuses while quarantined |

The recovery actor, reason, and any newly supplied request metadata remain
ordinary operator input: they pass the normal write gate. The exemption is
only for already existing source content admitted by a deliberate recovery
path.

## 2. Finding and coverage vocabulary

A **blocking finding** is a scanner result whose severity is blocking under
the effective ruleset. Advisory findings never open this quarantine. An
**unacknowledged blocking finding** is a blocking result whose exact
fingerprint is not admitted by the effective general-policy acknowledgment
set. Managed policy may reject all acknowledgments; it must not silently
reinterpret one.

**Newly detected** is an observation boundary, not a timestamp rule. A
finding is newly detected for a recovery attempt when the recovery scan
observes its exact fingerprint in the activated live state or in a retained
generation that the next publication could reference and the fingerprint was
not already represented by a durable active hold. The same fingerprint seen
again does not create a second hold or a second audit row. A finding in an
older retained generation is still newly detected when this workspace first
observes it; age does not make it safe.

The recovery scan covers:

1. the complete activated live semantic state;
2. `current.json` and `previous.json` when present; and
3. every manifest, object, and root reachable from the retained pointers
   that the next publisher would carry forward.

An absent optional previous generation is clean absence. A present but
malformed, unreadable, or incompletely enumerated retained generation is not
clean: the result has `coverage_incomplete: true` and remains quarantined
until a complete scan or a sanitized reset proves otherwise. Scanner failure
is therefore fail-closed for publication.

## 3. Durable state machine

The quarantine is durable workspace state, not a process-local flag. The
minimum durable hold facts are:

```text
ruleset_version       effective ruleset used for the verdict
blocking_count        count of unacknowledged blocking findings
coverage_incomplete   whether complete retained-generation coverage failed
```

The diagnostic projection additionally contains sorted `rule_ids`, per-source
coverage status, the state name, and the remedy. It contains no matched text,
decoded value, replacement text, or secret-bearing source path. A diagnostic
that names an individual finding may expose only its fingerprint, rule ID,
semantic selector, field path, raw half-open byte range, and prior-record hash,
as allowed by the governing redaction contract.

The logical states and transitions are:

| From | Event and required proof | To | Publication effect |
| --- | --- | --- | --- |
| `clean` | Recovery commits and finds no unacknowledged blocking result and complete coverage | `clean` | Normal publication may proceed |
| `clean` | Recovery commits valid local state, then finds a blocking result or incomplete coverage | `quarantined` | Publication and staging are withheld |
| `quarantined` | A repeated explicit recovery commits valid state | `quarantined` | Hold remains; no bypass by repetition or `--no-auto-flush` |
| `quarantined` | An audited redaction transaction commits | `redaction_pending` | Hold remains until sanitized publication |
| `redaction_pending` | Live state and the complete retained set revalidate clean, then sanitized publication commits | `clean` | Publish the sanitized generation exactly once |
| `quarantined` | An enabled, exact, administrator-reviewed false-positive resolution commits | `review_pending` | Hold remains until revalidation |
| `review_pending` | The reviewed fingerprint and all other coverage revalidate clean, then publication commits | `clean` | Publish the reviewed generation exactly once |
| any held state | Validation, scan, or publication precondition fails before its transaction commits | prior state | No partial semantic or hold mutation |

`redaction_pending` and `review_pending` include a committed semantic
transition whose publication is unfinished. A process restart must recover
these states from durable redaction or resolution records; it must never infer
“clean” from the absence of an in-memory task.

An exact `--acknowledge-secret` admission is not a quarantine-clear operation.
It may affect whether a general-policy recovery result is called
unacknowledged, but it cannot clear an existing hold, authorize a retained
generation, or bypass the publication revalidation. In the managed artifact,
the acknowledgment path is unavailable.

## 4. Recovery transaction and split outcomes

Source verification occurs before target initialization or activation. After
verification, recovery holds the destination publication lock (or an
equivalent serialization boundary) and performs activation, provenance/audit
records, the complete recovery scan, and insertion/update of the quarantine
hold in one atomic SQLite transaction. Import and restore are all-or-none:
there is no successful partial import that leaves only some records or only
the quarantine row.

The observable outcomes are deliberately split at the local commit boundary:

| Outcome | Local state | Checkpoint/staging state | Machine result |
| --- | --- | --- | --- |
| `rolled_back` | Unchanged, including no new hold | Unchanged | Non-success; no local recovery success |
| `local_success_quarantined` | Recovery and hold committed | Unchanged; no new Git-trackable bytes | Success; `local_recovery_succeeded=true`, `secret_quarantined=true`, `checkpoint_publication_withheld=true` |
| `local_success_publication_failed` | Recovery committed and is not quarantined | Unchanged or prior generation remains authoritative | Non-success publication diagnostic; local state is not rolled back |
| `published` | Recovery or resolution committed | Exactly one verified generation set is published/staged as applicable | Success; hold is absent |

`rolled_back` applies when source validation, activation, audit insertion,
or quarantine persistence fails before commit. It also applies when a
redaction or resolution transaction fails before commit. The pre-existing
hold and checkpoint files remain unchanged.

`local_success_quarantined` is not a publication error and must not be
reported as if local recovery failed. It is the intentional split outcome:
the operator can inspect and repair local state while the old checkpoint
remains authoritative. A later process cannot flush around it.

After a local commit, an ordinary I/O or Git failure does not roll back the
SQLite commit. The result must identify the incomplete publication and leave
the remedy resumable. A quarantine hold is checked before and after every
publisher's final scan while its publication lock is held.

## 5. Resolution paths

### 5.1 Audited redaction (enabled in v1)

The v1 enabled resolution is fingerprint-selected `bead redact`, including
the atomic all-blocking form. The caller supplies a fingerprint, actor, and
nonempty reason; never a matched value, replacement string, arbitrary byte
range, or SQL statement. Actor and reason are scanned before mutation.

The redaction transaction must:

1. resolve the finding against live state or a retained checkpoint and
   revalidate its fingerprint, selector, raw range, and prior-record hash;
2. replace the selected range with the fixed sanitized marker or the
   identity-rekey form required by `historical-redaction-v1`;
3. preserve all unrelated semantic identities and advance each affected issue
   revision exactly once;
4. write the nonsecret redaction receipt, anti-resurrection tombstone, and
   audit event in the same transaction; and
5. leave the hold active until live state and every retained generation that
   publication would reference have been rechecked.

One stale, missing, overlapping, or otherwise unresolved selected finding
fails the atomic operation. It must not clear a hold for the findings that
happened to succeed.

If a finding exists only in a retained generation and no live field remains
to edit, redaction must use the sanitized-republish path: scan the complete
live store, reset both retained pointers from that clean snapshot, record the
reset audit fact, and remove the dirty generation set from the next
publication. It must not manufacture a live receipt for bytes that are no
longer present.

Sanitized publication must not leave a dirty `previous.json`, object, shard,
manifest, forensic export, or staged addition reachable by the retained set.
Temporary files contain only sanitized output, use private permissions, and
are removed after success or interruption recovery. The source artifact and
already-published history remain outside this operation's sanitization claim.

### 5.2 Explicitly reviewed resolution (defined, disabled unless advertised)

Redaction is the only enabled false-positive remedy in v1. A future build may
advertise `reviewed_resolution` only after its exact contract is independently
reviewed and its capability is visible to callers. A worker flag, workspace
configuration value, exact-fingerprint acknowledgment, or “force” option is
not that capability.

When enabled, a reviewed resolution is a separate durable, auditable record
with all of these nonsecret facts:

```text
finding_fingerprint, ruleset_version
record_kind, origin_identity, field_path
byte_start, byte_length, prior_record_hash
reviewer, review_reference, reason, reviewed_at
```

The reviewer is administrator-authorized and distinct from the recovery actor
when the actor identity is available. The record is valid only if the exact
fingerprint and prior-record hash still match under the same ruleset, the
review reference and reason pass the write gate, and the reviewer explicitly
records that the result is a false positive. It must not contain matched
bytes, a copy of the field, or a blanket “all findings” decision.

The resolution commits as one transaction with its audit event, changes no
semantic content, and moves the workspace to `review_pending`. The publisher
still rescans live state and the complete retained set under lock. Only the
reviewed fingerprint may be treated as resolved; any other blocking finding
or incomplete coverage keeps the hold. A failed or stale review changes
nothing.

## 6. Diagnostics and non-disclosure

Recovery success with a hold is represented in text and JSON by the same
state, at minimum:

```text
local_recovery_succeeded: true
secret_quarantined: true
checkpoint_publication_withheld: true
```

Publication refusal uses the stable reason `secret_quarantined` and names the
redacted remedy (`bead doctor --scope secrets`, then fingerprint-selected or
all-blocking redaction). Status and doctor may show the count, ruleset,
sorted rule IDs, coverage state, and remedy. They must not show matched bytes,
candidate values, or an unbounded source excerpt. CLI stdout, stderr, JSON,
audit detail, checkpoint roots, manifests, objects, and tests share this
value-free requirement.

The same finding identity is used by diagnostics and redaction selection, so
the CLI and library cannot disagree about what is being resolved. Hashes and
fingerprints are identifiers, not permission to print the source value.

## 7. Conformance scenarios

The value-free scenario manifest is
`research/fixtures/recovery-finding-quarantine-v1.json`. It is an independent
scenario index, not a source artifact and not a substitute for executable
conformance. Its cases are assembled with synthetic candidates at runtime by
later implementation tests; no candidate value is stored in this repository.

The required scenarios are:

- clean recovery publication and blocking recovery split outcome;
- import, restore, and reconcile rollback when quarantine persistence fails;
- repeated recovery, restart, `--no-auto-flush`, explicit flush, auto-stage,
  and `sync commit` while held;
- retained-only finding and incomplete retained-generation coverage;
- live redaction, overlapping/all-blocking atomic redaction, and stale
  fingerprint conflict;
- sanitized retained-generation reset and anti-resurrection behavior; and
- reviewed-resolution capability refusal unless independently advertised,
  followed by exact reviewed transition coverage when it is enabled.

Every scenario asserts that no matched value appears in diagnostics, durable
state, publication output, or fixture data.
