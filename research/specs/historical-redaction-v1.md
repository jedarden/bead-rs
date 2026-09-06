# Historical Redaction Contract v1

Status: proposed normative specification.

Original author: bead-rs maintainers, 2026-09-03.

Artifact identity: `urn:bead-rs:spec:historical-redaction:v1`.

Implementation may begin only after independent review records an acceptance
decision against this specification's exact SHA-256.

## 1. Scope and authority

This contract implements ADR-015 and plan R038. It defines one exceptional
maintenance operation for destroying sensitive bytes already accepted by an
older bead-rs store. It is not a general issue editor, event editor, deletion,
or checkpoint transformation API.

Eligible targets are scanner findings in issue title/description/notes/close
reason, event detail, comment body, structured data, external-reference value,
attempt reason/evidence metadata, recurrence text, and provenance-receipt text.

## 2. Request

`bead redact --finding FINGERPRINT --actor ACTOR --reason REASON` accepts only:

- one fingerprint previously returned by the same binary ruleset for the live
  workspace;
- a validated nonempty actor; and
- a bounded nonsecret reason that itself passes secret scanning.

The request does not accept the matched value, replacement text, arbitrary SQL,
JSON pointer, file path, generation, or byte range. `--dry-run` reports the same
selector and planned effects without changing state.

## 3. Replacement and preserved facts

The fixed serialized marker is `[REDACTED:bead-rs]`. The service replaces only
the finding's current byte range after recomputing and matching its fingerprint.
All other bytes remain unchanged.

Issue IDs, event origin identities and sequences, local ingestion order,
lifecycle state, timestamps, dependency/label/reference identities, attempt
identity, and unrelated fields MUST remain unchanged. The affected issue's
logical revision advances once when an issue materialization changes. Event
identity remains stable; its stored content hash is recomputed and linked by the
redaction receipt.

## 4. Receipt and anti-resurrection tombstone

One canonical receipt records:

- receipt ID and schema identity;
- finding fingerprint, ruleset version, and rule ID;
- record selector, field path, and prior byte range;
- prior and sanitized record hashes;
- actor, reason, and time;
- affected issue revision when applicable; and
- publication state/resulting checkpoint identity.

It never stores matched bytes. A tombstone keyed by origin record identity,
field path, prior record hash, and finding fingerprint is durable checkpoint
state. Identical replay returns the receipt without a second mutation. A stale
range/hash/fingerprint conflicts with no mutation.

## 5. Transaction and locking

The command acquires the workspace maintenance lock and checkpoint publication
lock before reading the target. One IMMEDIATE SQLite transaction revalidates
the finding, writes the fixed marker, updates affected hashes/revision, inserts
the receipt/tombstone, and appends a nonsecret `historical_redaction` event.

Failure before commit changes nothing. Failure after commit leaves a
committed-but-unpublished epoch; `bead redact --resume RECEIPT_ID` republishes
without reapplying semantic changes.

## 6. Sanitized generation set

Publication serializes the complete sanitized live store. Neither
`current.json` nor `previous.json` may reference a root containing a finding
removed in the epoch. During the first redaction publication, `previous.json`
points to the new sanitized root with explicit reset metadata rather than the
dirty predecessor. After the pointer pair and forensic view are durable, every
superseded object/manifest/shard named by the epoch is tombstoned and removed.

Temporary files use mode 0600, never appear under a Git-trackable path, contain
only sanitized output, and are removed on success and interruption recovery.

## 7. Recovery precedence

Import, merge, reconcile, and restore compare incoming content with durable
redaction tombstones. Matching pre-redaction bytes cannot overwrite sanitized
content. A full empty-target restore from a sanitized checkpoint reconstructs
receipts and tombstones before accepting other records. A deliberately selected
older checkpoint without the receipt remains historical evidence that may
contain exposed bytes and is reported as unsafe; it is never silently promoted
over a sanitized workspace.

## 8. Output and exit behavior

Success emits a receipt containing selectors, hashes, and publication state.
Exit 2 covers invalid actor/reason/fingerprint. Exit 3 covers a missing target.
Exit 4 covers stale target or semantic conflict. Exit 1 covers internal or
committed-but-unpublished failure. No output channel includes matched bytes.

## 9. Conformance

Required tests cover issue and event fields, multiple occurrences, Unicode byte
ranges, exact replay, stale fingerprints, concurrent mutation, crash before and
after commit, monolithic and sharded generation reset, tombstone application,
restore equivalence, old-input resurrection attempts, capabilities/help/schema
surfaces, gitleaks-clean artifacts, and byte-for-byte preservation of every
unaffected semantic field.
