# Fleet view v1

`bead fleet` is a read-only view of bead-rs workspaces below one filesystem root.
It never acquires a write lock, changes store files, runs repair, or publishes a
checkpoint. Failures in one workspace do not prevent reporting other workspaces.

## Interface

The subcommands are `status`, `ready`, `search TEXT`, and `doctor`. Each accepts
`--root PATH`, `--max-depth N`, `--include-copies`, and `--json`. The default root
is `$HOME` and the default maximum depth is 3. A missing home directory is an
argument error. `--max-depth` counts directory edges below the root; the root
itself is depth zero. A `--root` that does not exist or is not a directory is an
argument error. `search TEXT` requires a non-empty TEXT. The command returns one
document sorted by workspace path in JSON mode. Human output has the same
ordering. Exit codes: 0 when the view was produced (workspaces that could not be
observed are error records inside the document, not a failed run), 1 on internal
failure, 2 on argument errors.

## Discovery and grouping

Walk directories through the maximum depth without following symbolic links and
without descending into a `.beads` directory itself. A directory containing
`.beads/config.json` with a bead-rs workspace UUID is a candidate. A `.beads`
directory with a different format is skipped with a reason, including bf stores
and unrecognized stores. A candidate that cannot be opened is reported as an
error, and discovery continues.

Classification of a candidate directory `<ws>/.beads`:

- `bf_store`: `config.yaml` (and usually `issues.jsonl`) with no bead-rs
  `config.json` — the retired bead-forge layout.
- `unrecognized`: a `.beads` directory with neither a bead-rs `config.json` nor
  a bf `config.yaml`.
- `uninitialized`: a bead-rs `config.json` with no `beads.db` (the fresh-clone
  state). Skipped, not an error: it is an observation state, not a failure.
- `foreign_schema`: a `beads.db` that exists but fails the native schema-shape
  classification.
- error records: `config.json` that exists but cannot be read or parsed, a
  `beads.db` that cannot be opened (corrupt, wrong permissions), or a native
  database whose workspace UUID row cannot be read.

Group candidates by workspace UUID. The canonical member is the shallowest
path whose components below the root are neither dot directories nor
temporary-copy names. Temporary-copy names are: any dot-prefixed component; a
component containing `-verify`, `-committed`, or `.tmp`; and a component whose
final dot-separated segment is a mktemp-style suffix (one separator dot, a
non-empty base, and a mixed-case alphanumeric segment of six or more
characters). If every member looks like a copy, choose the shallowest member.
Break ties by lexical path order. All other members are copies. Report each
copy's path, UUID, and sequence relative to its canonical member, where the
canonical member is sequence 0 and copies number from 1 in lexical path order.
Copies are excluded from aggregate totals and subcommand results by default;
`--include-copies` includes them. Grouping is based on UUID, not path or store
contents. Unreadable candidates with unknown UUID remain separate errors.

## Workspace observations

Read each selected store with a bounded per-workspace timeout (15 seconds;
each workspace is observed on its own thread and one that exceeds the budget
becomes that workspace's error record). Store connections are opened with
SQLite read-only flags; no pragma that writes to the file or its sidecars is
issued. `status` reports the path, UUID, counts for each issue status, ready
count, and stale `in_progress` count using that workspace's R034 threshold.
`ready` lists ready issues with their workspace identity. `search TEXT`
searches issues in each workspace using the existing native search semantics
(case-insensitive, escaped LIKE over title, description, and notes) and
reports workspace identity with matches. `doctor` runs only the read-only
doctor checks — schema-shape classification, format-identity verification, and
`PRAGMA quick_check` — and reports each workspace's verdict. Any observation
error or timeout is attached to that workspace rather than terminating the
fleet view. Once committed checkpoint lag fields exist (beadrs-7431d728),
report that lag per workspace; until then the field is an explicit null.

The ready count uses the `list --ready` frontier filter: open, unassigned,
not manually blocked, no unfinished blocking dependency, and no conflicting
resource lock. The stale count reuses the R034 doctor semantics: an ordinary
(non-leased) `in_progress` issue whose latest event is older than the
workspace's `doctor.stale_in_progress.max_age_seconds` (versioned in
`.beads/config.json`, default 86400 seconds), where "leased" is the claim-epoch
test the doctor diagnostic applies.

## Output

JSON output is one document containing the root, selected options, canonical
workspace records, copy records, skipped records with reasons, error records,
and aggregate counts. Paths are absolute and all arrays are sorted by path.
Workspace records always carry all four status keys (`open`, `in_progress`,
`deferred`, `closed`) so consumers never index a missing key. No registry or
daemon is used, and the view is composed from live stores on every run.
