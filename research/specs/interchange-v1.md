# Canonical interchange specification v1

Status: draft normative specification.

## Transport

An interchange file is UTF-8 JSON Lines. Each nonblank line contains exactly
one JSON object. Writers terminate records with LF. Readers reject invalid
UTF-8 and malformed JSON with the line number; they do not silently treat a
failed parse as an empty store.

## Required issue fields

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | nonempty string | Stable source identifier |
| `title` | string | Human-readable summary |
| `status` | string | Lifecycle value |
| `priority` | integer | Lower values represent higher priority |
| `created_at` | RFC 3339 string | Creation instant |
| `updated_at` | RFC 3339 string | Last semantic modification |

## Optional issue fields

`description`, `assignee`, `labels`, `dependencies`, `comments`, `closed_at`,
`close_reason`, `issue_type`, `source_repo`, and profile-specific extensions
may be present. A profile defines null-versus-absent behavior.

Unknown fields are stored with their original JSON values and re-emitted by
the same source profile unless they conflict with a later known-field update.

## Dependencies

A dependency edge is represented canonically as `(blocked, blocker, kind)`.
The blocked issue is not ready while a required blocker is unfinished. Import
adapters must state the direction used by their source format; they must not
infer direction from argument order alone.

## Status normalization

The native lifecycle distinguishes at least open, in-progress, blocked,
deferred, and finished. Profiles map their external values explicitly.
Unknown values are preserved and reported; they are never silently converted
to open or finished.

## Determinism

Canonical export sorts issues by identifier and emits a stable field order.
Arrays whose order has no semantic meaning use a documented stable ordering.
Timestamp values retain their represented instant and available precision.

## Safety

Import validates the full input before activating any state. Export writes a
temporary file, flushes it, and atomically replaces only an explicitly selected
destination. Input files are never overwritten by migration.

