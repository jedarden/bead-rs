# F012 round-two independent review

Date: 2026-08-10 UTC

Reviewer: OpenAI Codex, current review session, independent of the OpenAI
Codex 2026-08-10 original authoring session, the Claude 2026-08-10 round-one
review/correction sessions, and the Marathon implementation session

Reviewed commit: `16a06e817e68a60597e3d90e8575813597631988`
(`main`, equal to `origin/main` at review start)

Disposition: **rejected**. The corrected producer observations are materially
accurate, but the candidate is not yet a complete, internally unambiguous,
executable profile baseline. F012 must remain `passes: false`.

The requested `/home/needle/workspace/bead-rs` checkout was inspected through
the `agent-sandbox` Kubernetes API. The only running bead-rs workload was
`bead-rs-marathon`; its checkout was stale at `a58ff78` and had unrelated
uncommitted F013/profile work. It was left untouched. This review used the
clean `/home/coding/bead-rs` checkout, whose origin, branch, and exact expected
commit were verified before review.

## Scope and independence

I read `AGENTS.md`, the round-two request, `research/specs/clean-room-protocol.md`,
and `PROVENANCE.md` before reviewing the two profiles and fixture directories.
I did not read or build source, tests, fixtures, SQLite schemas, or internal
documentation from any other producer. I did not use
the existing `src/profile` code or its tests as evidence and did not modify any
reviewed specification or fixture. All producer records below were invented in
fresh directories under `/home/coding/scratch`.

## Artifact hashes

Every manifest-listed digest was recomputed and matched exactly. The table also
records the profile and manifest hashes so the complete reviewed candidate is
unambiguous.

| Artifact | SHA-256 |
| --- | --- |
| `research/specs/br-v1-profile.md` | `1a8763d4e54aa8867399eb86dd560978f000daf729faac6d094a902970c41847` |
| `research/fixtures/br-v1/manifest.json` | `0608805fde43d5dda1af9889240dc00a0805d6e797905de27a45216db1d3e99a` |
| `research/fixtures/br-v1/README.md` | `9390aaaa7038c9afe83bee0d9161563fa1f5bbc983667b92aed3ecc7be55bece` |
| `research/fixtures/br-v1/invalid-cases.json` | `befcd3f14c7c3aa89e64dc5420162c8d9a4380aba7a575e3e93be75775624e23` |
| `research/fixtures/br-v1/observed-valid.jsonl` | `d1f5d393df10011a3843097b2d4855d08ffb51df67b3b316a78653f3041b0a8b` |
| `research/specs/bf-v1-profile.md` | `e418c19bf73325c4ffde46833f347fc7851a1433442d7e6719d60fc41bf03a4e` |
| `research/fixtures/bf-v1/manifest.json` | `51f05bd794c0f5ddd50f36b3c0742341b956f0d44a7080ea2ad6143f76f6f172` |
| `research/fixtures/bf-v1/README.md` | `21328ab57dca72b4bfd68d8f7cec2811a9c0880e714d515efa5bce801ff2008a` |
| `research/fixtures/bf-v1/invalid-cases.json` | `cba20e9c5f64ec6473b72898da5d175928dace2a023328bf61c67096575da053` |
| `research/fixtures/bf-v1/observed-valid.jsonl` | `c0e7d74d40c04b04412c76781bc7b76553a2a5384e5e55375104a9a528436d0b` |

## Producer acquisition and method

### br 0.1.28

I queried GitHub's release API for the official `v0.1.28` release, downloaded
only `br-v0.1.28-linux_amd64.tar.gz`, its adjacent `.sha256` asset, and
`checksums.sha256`, and compared all three independently. Both published
checksum files and the downloaded archive gave:

`1fb9962e6d27a2a606aacba95460f1dd9f6c38e500ef85f9ab2073cc6bbf99e9`

The extracted x86-64 Linux binary reported `br 0.1.28`; its SHA-256 was
`0da57fc213165e876cd6f0d9a1ffc23ebe27408afba60119137681289d740707`.
The acquisition URL was the producer project's official GitHub release asset
for `v0.1.28`.

In a fresh `br init --prefix r2` workspace, public CLI operations created
literal `blocked`, `deferred`, and invented `future_state` statuses; labels in
`zeta,alpha,mu` input order; two blockers inserted in reverse lexical ID order;
and a closed issue with an invented close reason. `br sync --flush-only` output
was inspected as JSONL.

### bf 0.4.0

The installed `bf` reported `bf 0.4.0`; its SHA-256 was
`696019aeaaeee50ce1fc62fe2407e73892caf9818e54f434f5e22b0dad81018e`
(6,395,912-byte x86-64 ELF). Cargo's local installation registry identifies it
as the producer package at version 0.4.0, installed from a local path. I did
not open or inspect that source path.

This proves the exact executable and version exercised, but not an independent
public-release acquisition/checksum chain. The review boundary permits public
compiled release binaries; no official bf 0.4.0 release asset or publisher
checksum was identified. This limitation is a provenance finding below, not a
claim that the observed executable was fabricated.

In a fresh `bf init --prefix q3` workspace, public CLI operations created and
updated invented records to literal `blocked`, `deferred`, and `future_state`;
created labels in `zeta,alpha,mu` order; and added two blockers in deliberate
reverse lexical ID order. `bf sync --flush-only` returned the known
`export_hashes` error after writing `.beads/issues.jsonl`; that written JSONL
was inspected, and `bf doctor --repair` then reported the workspace healthy.

Representative reproducible commands (with a fresh temporary directory and
the checked binary paths) were:

```text
br init --prefix r2 --actor round2-reviewer
br create "Explicit blocked invented" --status blocked --silent
br create "Unknown status invented" --status future_state --silent
br dep add BLOCKED BLOCKER
br sync --flush-only

bf init --prefix q3
bf create --title "Explicit blocked invented" --json --no-auto-flush
bf update ID --status blocked --no-auto-flush
bf dep add BLOCKER --blocks BLOCKED --no-auto-flush
bf sync --flush-only
```

## Independently reproduced behavior

- br accepts and exports explicit `blocked`, `deferred`, and an arbitrary
  invented status verbatim. Explicit `blocked` needs no dependency.
- An unfinished br `blocks` edge leaves the blocked issue's stored/exported
  status at `open`. Two dependencies inserted blocker `r2-87k` then
  `r2-11u` exported as `r2-11u`, `r2-87k`: lexical blocker-ID order. Direction
  was `(issue_id, depends_on_id) = (blocked, blocker)` and edge objects included
  `metadata:"{}"`. Labels exported as `alpha,mu,zeta`. A close operation
  exported `closed_at` and the supplied `close_reason`.
- bf accepts and exports explicit `blocked`, `deferred`, and an arbitrary
  invented status verbatim. An unfinished `blocks` edge materializes the
  blocked record's exported status as `blocked`.
- Two bf dependencies inserted blocker `q3-5j6` then `q3-36m` exported in that
  same non-lexical creation order. Direction was `(issue_id, depends_on_id) =
  (blocked, blocker)`; edge objects omitted `metadata`. Labels exported as
  `alpha,mu,zeta`.
- bf emitted required empty `description`, `design`, `acceptance_criteria`, and
  `notes` strings, plus a `created` event on every observed record. br omitted
  unset optional strings and arrays.
- Both outputs sorted issue IDs lexically and emitted RFC 3339 UTC timestamps
  with nine fractional digits in these observations. The checked fixtures use
  the same timestamp shape.

These observations confirm the material producer facts in the corrected
profiles, including the two points explicitly called out by the round-two
request: literal br `blocked` is real, and bf dependency arrays are creation
ordered rather than lexically sorted.

## Round-one correction audit

The correction commit addresses each named round-one finding:

- bf `events` is now in the matrix and every valid fixture record;
- both corpora contain explicit-null cases;
- both valid corpora contain two-dependency ordering examples;
- br contains and specifies literal `blocked` correctly in prose and fixture;
- bf contains and specifies observable `deferred` as preserved/reported rather
  than silently mapped;
- br adds `close_reason`; and
- the bf dependency-order rule is corrected from lexical to creation order.

The correction and review roles are explicitly separated in the profiles,
fixture manifests, review request, and `PROVENANCE.md`. The later Marathon
implementation violated the sequencing gate, but it did not alter the reviewed
artifact hashes after `b8b2600`; it was excluded as evidence here.

## Findings requiring correction

1. **Loss reports are not specified or fixture-tested as required.** Each
   profile lists categories to report, but neither defines the machine-readable
   report shape, preserved/transformed/omitted counts, field identifiers,
   collision behavior, or zero-loss output. No fixture supplies an input and
   expected loss report. This does not satisfy `docs/plan/plan.md` section 6.4,
   which requires a machine-readable report even when all counts are zero and
   specific accounting for events, provenance receipts, schema references,
   extensions, comments, and structured data. It also leaves “every lossy
   transformation is reported” non-conformable.

2. **Unknown-field preservation is normative but has no conformance case.**
   Neither valid corpus nor `invalid-cases.json` includes an invented unknown
   key with a nested/non-scalar JSON value and expected same-profile round-trip
   output. The `unknown-status` cases test a different rule. The explicit-null
   cases provide prose expectations only; they do not provide a round-trip
   artifact or expected loss report demonstrating preservation distinct from
   absence.

3. **Required/optional/edge coverage remains incomplete and one negative is
   confounded.** Both `missing-id` objects also omit profile-required
   `issue_type`, so they cannot isolate the promised missing-ID diagnostic.
   There are no isolated cases for missing `title`, `status`, `priority`,
   `issue_type`, `created_at`, or `updated_at`; wrong JSON types; priority
   boundaries/out-of-range values; empty arrays versus absence; invalid or
   dangling dependency references; or cycles. The corpora contain no actual
   Unicode value, and the value named “Invented multiline description” has no
   newline. These omissions fall short of the repository's normative
   conformance scenarios for missing, null, unknown, future, Unicode, multiline,
   and dependency validation behavior.

4. **br's status table contradicts its corrected prose and fixture.** The
   `blocked` row says reverse export is `blocked` “while required blocker is
   unfinished,” but the independently reproduced explicit-blocked case exports
   `blocked` with no blocker. The prose below the table is correct; the table
   must express both explicit blocked and dependency-derived readiness without
   implying a condition that does not apply.

5. **bf's dependency-order requirement is internally weakened.** The profile
   first requires creation order, then permits an importer to preserve creation
   order “or another explicitly declared canonical order.” Re-sorting would
   contradict the observed producer profile and trigger the profile's own
   dependency-order loss obligation. The normative requirement must say whether
   exact array order is preserved and, if transformation is ever allowed, what
   loss entry is mandatory.

6. **bf producer provenance is incomplete for this gate.** The exercised
   binary is versioned and checksummed above, and all claimed behavior was
   reproducible, but the installation metadata records a local path build rather
   than a checksummed public compiled release. To meet the round-two acquisition
   requirement and clean-room binary boundary, supply an official bf 0.4.0
   compiled artifact with publisher checksum/signature, or document and approve
   a narrow governance exception with an independently verifiable build/release
   attestation. Do not inspect the producer source to fill this gap.

Timestamp offset acceptance, invalid timestamp rejection, arbitrary unknown
field round trips, cycles, and loss reports were therefore reviewed as stated
requirements only; the current fixtures do not independently demonstrate them.

## Clean-room assessment and disposition

No evidence of copied source-derived material was found in the reviewed
artifacts. The br acquisition and observation comply with the clean-room
protocol. The bf behavior observations are useful corroboration, but its local
path-install provenance does not establish the permitted public-release binary
chain. Separation of duties for this review is satisfied; the earlier Marathon
sequencing violation remains accurately disclosed in `PROVENANCE.md`.

**Disposition: rejected.** Correct findings 1-6 in a new specification/fixture
authoring pass, regenerate all affected manifests, and obtain another
independent review. Do not silently repair the current artifacts. F012 remains
`passes: false`; no implementation approval, compatibility claim, or F012
completion follows from this review.
