# Secret Rejection Contract v1

Status: proposed normative specification.

Original author: bead-rs maintainers, 2026-09-03.

Artifact identity: `urn:bead-rs:spec:secret-rejection:v1`.

Implementation may begin only after independent review records an acceptance
decision against this specification's exact SHA-256.

## 1. Scope

This contract implements ADR-014 and plan R038 for operator-supplied text in
create, update, close, release, reopen, claim metadata, labels, dependencies,
comments, external references, structured data, attempt resolution, recurrence,
and bulk manifests. Recovery input is reported but not rejected because the
bytes already exist.

The scanner is offline, deterministic, and compiled into the binary. Workspace
configuration cannot add, remove, or alter rules.

## 2. Findings

A finding contains only:

- ruleset version, rule ID, provider/category, and blocking/advisory tier;
- semantic record selector and field path;
- byte start/end within that field;
- SHA-256 finding fingerprint; and
- placeholder/checksum disposition.

`Debug`, `Display`, serialization, errors, events, tracing, and dry-run output
MUST NOT contain matched bytes. The internal match buffer MUST be zeroized or
overwritten before release.

The v1 fingerprint is SHA-256 over a domain separator, ruleset version, rule
ID, record selector, field path, byte range, and matched bytes. It is rendered
as lowercase hexadecimal. It identifies one exact finding without exposing it.

## 3. Rule tiers

The blocking tier contains only provider-formatted credential patterns and
private-key armor with independently demonstrated near-zero false positives.
Placeholder-shaped values and a format with a failed embedded checksum are not
blocking. Statistical/entropy rules are advisory only.

The ruleset is closed and versioned in the binary. Network validation is
forbidden.

## 4. Mutation behavior

Every operator-text mutation scans its complete canonical request before
opening the semantic write transaction. One blocking finding rejects the whole
request with exit 2 and reason code `secret_detected`. SQLite, events,
checkpoint state, and files remain byte-for-byte unchanged.

An exact fingerprint acknowledgment may admit one finding. The audit event
records fingerprint, rule ID, actor, and field selector but not matched bytes.
There is no blanket invocation bypass. Workspace mode is `enforce`, `advisory`,
or `off`, defaults to `enforce`, and is advertised by capabilities and doctor.

## 5. Recovery and diagnostics

`doctor --scope secrets --format json` scans live semantic rows and both
retained checkpoint generations. It returns findings in deterministic selector
order and never returns matched content. Import, restore, archaeology, and
reconcile report findings without preventing recovery.

## 6. Compatibility and verification

Capabilities advertise contract identity, ruleset version, effective mode,
blocking/advisory support, and exact-fingerprint acknowledgment. Older clients
ignore additive fields. Required tests cover every mutation surface,
placeholders, checksummed formats, malformed configuration, acknowledgments,
output redaction, recovery reporting, deterministic fingerprints, Unicode byte
ranges, hostile 4 MiB fields, and bounded scan overhead.
