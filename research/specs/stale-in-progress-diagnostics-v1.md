# Stale in-progress diagnostics v1

## Purpose

This specification defines the R034 advisory diagnostic for ordinary claims
that may have been abandoned. It applies only to native workspace state and
never changes an issue, an assignment, a lease, an event, or a checkpoint.

## Workspace configuration

The threshold is stored in `.beads/config.json` at
`doctor.stale_in_progress`:

```json
{
  "doctor": {
    "stale_in_progress": {
      "version": 1,
      "max_age_seconds": 86400
    }
  }
}
```

Version 1 requires both fields. `max_age_seconds` is an integer from `1`
through `i64::MAX`; an event is stale only when its age is *strictly greater*
than that value. A missing section is interpreted as the version-1 default of
86,400 seconds for workspaces created before this feature. Any unknown version
or invalid value fails the diagnostic rather than silently applying different
semantics.

Fresh workspaces write this versioned section during `bead init`.

## Scope and selection

`bead doctor --scope store` and the default `bead doctor` include the stable
`stale_in_progress` check. It considers an issue only when all of the following
are true:

1. Its base status is `in_progress`.
2. It has an audit event, and its latest audit event in native event-sequence
   order is older than the configured interval.
3. Its current claim epoch is ordinary, not leased.

The latest event's timestamp supplies the reported age. Event sequence, rather
than a cross-machine wall-clock comparison, determines which event is latest.

A current leased claim is excluded even after its expiry time. R002 owns lease
expiry, fencing, and its recovery path. Historical lease rows do not exclude a
later ordinary claim: lease rows retain fencing-token history, so the diagnostic
uses the latest claim event and ends that epoch at a later release, close, or
reopen event.

## Output and remedy

When any issue qualifies, `doctor` emits a warning check named
`stale_in_progress`. Its JSON details contain the configuration version and
threshold, the stable R001/R019 reason code `stale_in_progress`, and a complete
`stale_issues` list. Every list item includes `id`, `last_event_at`,
`age_seconds`, and the exact manual remedy:

```text
bead release <id>
```

The human-readable line lists the same id, age, and command. A healthy check
reports the active configuration and an empty list.

The diagnostic is advisory. `doctor`, including `doctor --repair`, never runs
the remedy or releases an issue.
