# Feature-enabled build evidence — declared SHA 861cdcb

Status: **VERIFIED — pin of record intact, capability proven, provenance documented**.

Consolidated acceptance evidence for the attempt-resolution feature-enabled
build of bead-rs, assembled 2026-09-03 by beadrs-eabcf33d (child 4 of 4) onto
umbrella bead beadrs-4bb8bf78. The pin of record is verified live in this
report; every hash below was recomputed at assembly time, not copied.

Chain context: children 1–3 of the split of beadrs-4bb8bf78 did not produce
recorded outputs — beadrs-f173164a (C1, build) was labeled `over-budget` by
the system actor at 2026-09-03T14:34:06Z without executing, beadrs-c775cff1
(C2, hash evidence) remains Open with no notes, and beadrs-20831dd5 (C3, pin
confirmation) was closed by the `system` actor at 2026-09-03T14:34:06Z with
reason "verification is the gate's job (Phase 19.4)" and no recorded evidence.
No fresh archive-script artifact was therefore produced by this cycle. This
report consolidates the acceptance evidence from the durable in-repo record —
the pin, its metadata, and the verification documents — re-verified live at
assembly time. Verifying the existing pin (hash, size, capability probe,
inventory row) is corroboration, not a redo of the unexecuted build child.

## 1. Declared build SHA

`861cdcbfebeb70a9ebc6a2e33ee98cef97274fec` (short `861cdcb`), commit
`feat(tests): add binary variant integration test suite for capability
detection`, 2026-09-02.

`pinned-binaries/COMMITS.md`, section "Integration Test Binary — Declared
Feature-Enabled Build SHA", declares this the single canonical feature-enabled
build SHA (chosen 2026-09-03 by the beadrs-90f9a509 candidate audit, recorded
by beadrs-12dd0849). Its five supporting reasons: twin of e115609 (two-way
verified), e115609 rejected as unresolvable, resolvable from any fresh clone
of origin, earliest resolvable commit carrying the complete attempt-resolution
feature, and compiled source still current at HEAD (`git diff --stat
861cdcb..HEAD -- src/` empty).

Live resolvability at assembly time:

```console
$ git cat-file -t 861cdcbfebeb70a9ebc6a2e33ee98cef97274fec
commit
$ git branch -r --contains 861cdcbfebeb70a9ebc6a2e33ee98cef97274fec
  origin/HEAD -> origin/main
  origin/main
$ git log --oneline -1 861cdcbfebeb70a9ebc6a2e33ee98cef97274fec
861cdcb feat(tests): add binary variant integration test suite for capability detection
```

The built-from provenance SHA recorded in the pin's metadata,
`e1156098b01264bb998797047115521261443c13`, is expected to be unresolvable —
it names a commit of the force-pushed-away lineage, documented in
`pinned-binaries/COMMITS.md` ("SHA lineage and provenance") and
`pinned-binaries/README.md` ("Pin inventory"). This is documented reality, not
a defect:

```console
$ git cat-file -t e1156098b01264bb998797047115521261443c13
fatal: git cat-file: could not get object info
```

## 2. Artifact of record and its sha256

The artifact of record for the declared build SHA is the pinned binary
`pinned-binaries/bead-attempt-resolution-e115609`, bound to `861cdcb` through
the `restored_lineage_twin_sha` field of its metadata file. Recomputed live
(two runs, identical):

```console
$ sha256sum pinned-binaries/bead-attempt-resolution-e115609
68fe8d534721be4ba4147312364d8f0b216b62f3093e85e7c91f0a0db695a645  pinned-binaries/bead-attempt-resolution-e115609
$ sha256sum pinned-binaries/bead-attempt-resolution-e115609
68fe8d534721be4ba4147312364d8f0b216b62f3093e85e7c91f0a0db695a645  pinned-binaries/bead-attempt-resolution-e115609
```

The hash is **recorded, not rebuild-verifiable**: a rebuild through the
sanctioned archive path produces the same code but different bytes, because
`build.rs` re-embeds the build timestamp. `scripts/build-from-archive.sh
861cdcbfebeb70a9ebc6a2e33ee98cef97274fec --features attempt-resolution` is the
sanctioned rebuild path (`BUILD_PROCEDURE.md`, "Build Rule"); hash comparison
against metadata remains the pin-verification path until SOURCE_DATE_EPOCH
determinism lands (beadrs-dc295092 / beadrs-baba38b8).

Pre-feature baseline, for context (from
`pinned-binaries/bead-pre-feature.metadata.json`):

| binary | sha256 | bytes |
| --- | --- | ---: |
| `bead-pre-feature` (0.2.4, no attempt-resolution) | `7e0e73defebb75fc987ddf8b6fb959f47c73ccbbcd7e066e2af302a6a43db6b5` | 6,788,016 |
| `bead-attempt-resolution-e115609` (feature-enabled) | `68fe8d534721be4ba4147312364d8f0b216b62f3093e85e7c91f0a0db695a645` | 7,305,144 |

The distinctness verdict on this pair belongs to beadrs-0737200a (see §5).

## 3. Pin of record and metadata cross-check

Pin of record: **`bead-attempt-resolution-e115609`** — one of exactly four
pins declared by `pinned-binaries/README.md` ("Pin inventory"); no fifth
binary was added.

The metadata line binding it to the declared build SHA
(`pinned-binaries/bead-attempt-resolution-e115609.metadata.json`):

```json
  "restored_lineage_twin_sha": "861cdcbfebeb70a9ebc6a2e33ee98cef97274fec",
```

Full metadata cross-check, recomputed live — file hash equals metadata
`binary_sha256`, and byte size equals metadata `binary_size_bytes`:

| property | metadata declares | live on disk | match |
| --- | --- | --- | :---: |
| `binary_sha256` | `68fe8d534721be4ba4147312364d8f0b216b62f3093e85e7c91f0a0db695a645` | `68fe8d534721be4ba4147312364d8f0b216b62f3093e85e7c91f0a0db695a645` | yes |
| `binary_size_bytes` | `7305144` | `7305144` | yes |
| `git_commit_sha` (built-from provenance) | `e1156098b01264bb998797047115521261443c13` | unresolvable object (expected, §1) | documented |
| `restored_lineage_twin_sha` (rebuild target) | `861cdcbfebeb70a9ebc6a2e33ee98cef97274fec` | resolves as `commit` on `origin/main` | yes |
| embedded version | `bead 0.2.6 (e115609-dirty 2026-09-02T07:23:55Z)` | `bead 0.2.6 (e115609-dirty 2026-09-02T07:23:55Z)` | yes |

```console
$ stat -c '%s' pinned-binaries/bead-attempt-resolution-e115609
7305144
$ ./pinned-binaries/bead-attempt-resolution-e115609 --version
bead 0.2.6 (e115609-dirty 2026-09-02T07:23:55Z)
```

Inventory accuracy: `pinned-binaries/README.md` carries the pin's table row
(`| bead-attempt-resolution-e115609 | e115609 |
bead-attempt-resolution-e115609.metadata.json |`), its per-pin section with
build procedure and verification steps, and its uniqueness-table row listing
the same sha256 at 7.0M. The inventory is maintained against `ls
pinned-binaries/`; this report adds no binary.

## 4. Capability probe

Attempt-resolution capability, probed live against the pin of record:

```console
$ ./pinned-binaries/bead-attempt-resolution-e115609 capabilities | jq '.attempt_outcome.supported'
true
```

Full `attempt_outcome` block:

```json
{
  "supported": true,
  "outcomes": [
    "verified_success",
    "work_failure",
    "infrastructure_failure",
    "cancelled",
    "indeterminate"
  ],
  "actions": [
    "close",
    "release",
    "quarantine",
    "block",
    "none"
  ],
  "replay_detection": true,
  "revision_guard": true,
  "fencing_token": true,
  "evidence_refs": true,
  "resolve_receipt_schema": "urn:bead-rs:schema:resolve-receipt:native-v1",
  "resolve_request_schema": "urn:bead-rs:schema:resolve-request:native-v1"
}
```

The `resolve` subcommand is advertised:

```console
$ ./pinned-binaries/bead-attempt-resolution-e115609 --help | grep -i resolve
  resolve            Record an execution attempt outcome atomically
```

Negative control — the pre-feature baseline lacks the capability entirely:

```console
$ ./pinned-binaries/bead-pre-feature capabilities | jq 'has("attempt_outcome")'
false
```

## 5. Distinctness and build-doc pointer — beadrs-0737200a

Binary-distinctness verification and the reproducible build documentation are
owned by **beadrs-0737200a** (upstream blocker of the umbrella,
in_progress), which verified live on 2026-09-03 that all four pin hashes match
their metadata files exactly and are mutually distinct, that the pre-feature
vs feature-enabled pair differs, and that the rebuild path through
`scripts/build-from-archive.sh` runs end-to-end (exit 0, feature-enabled
artifact, expected hash drift per the embedded-timestamp caveat). Its doc fix
landed as commit 6f0feec (pushed to origin/main): `docs/attempts-binary-build.md`
now routes every build through the archive script.

Build documentation of record (all at HEAD):

- `BUILD_PROCEDURE.md` — Build Rule: the archive script is the only sanctioned build path
- `pinned-binaries/README.md` — pin inventory, per-pin build procedure and verification
- `pinned-binaries/COMMITS.md` — declared build SHA, SHA lineage and provenance
- `pinned-binaries/BINARY_VERIFICATION.md` — hash validation and functional capability testing
- `docs/attempts-binary-build.md` — the chain's build guide (fixed by 6f0feec)

Closed doc siblings delivering earlier build-instruction rounds, corroborated
not repeated: beadrs-cb16a73a (build instructions to a pinned README),
beadrs-d1a168b9 (pin + build instructions), beadrs-8ccf249c (build process +
distinctness, delivered `BINARY_VERIFICATION.md`), beadrs-d23c15c5
(distinctness + docs completion). Closed archive-build reproduction
verifications cited by C3's brief: beadrs-65cd875e, beadrs-f5691af4.

## 6. What this report does not claim

- No fresh archive-script build was performed by this consolidation cycle —
  children 1–3 did not execute (see the chain-context note above), and
  re-running the build is child 1's acceptance, not this report's.
- No distinctness verdict is issued here; that acceptance belongs to
  beadrs-0737200a, which recorded it live on 2026-09-03.
- The recorded sha256 remains hash-only evidence until SOURCE_DATE_EPOCH
  determinism lands; rebuild-then-compare is expected to differ by design.

## 7. Addendum (2026-09-03, later the same day) — the build was subsequently executed

The chain-context note and §6 bullet 1 above are **superseded** by events
recorded later on 2026-09-03 (~15:11–15:29Z, after this report was assembled
and committed as 5fa9803): children 1 and 2 were re-dispatched and closed with
first-hand evidence.

- **C1 `beadrs-f173164a` (build) — closed 2026-09-03T15:24:52Z with executed-build
  evidence**, built live by `claude-code-glm-4.7-glm-roam-17` via
  `scripts/build-from-archive.sh` (git-archive extraction in scratch; the
  shared checkout's HEAD/index/stash untouched). Full unedited log:
  `/home/coding/scratch/beadrs-f173164a-evidence.txt`.
- **C2 `beadrs-c775cff1` (SHA + hash evidence) — closed 2026-09-03T15:29:06Z**,
  recorded by `claude-code-glm-vista`, which independently re-resolved the
  declared SHA and double-hashed the artifact, matching C1 exactly.

Fresh-build artifact of record for the declared SHA (from
`/home/coding/scratch/beadrs-f173164a-pin/bead-861cdcb.metadata.json`):

| property | value |
| --- | --- |
| `git_commit_sha` | `861cdcbfebeb70a9ebc6a2e33ee98cef97274fec` (the declared build SHA, §1) |
| `binary_sha256` | `42b4335444d36bf7b7e6e3a21af229c47457fcb94962761eb185b05999e95f90` |
| `binary_size_bytes` | 7,305,184 |
| `build_features` / profile | `attempt-resolution` / release, `--locked` |
| `build_command` | `cargo build --release --locked --features attempt-resolution` |
| embedded version | `bead 0.2.6 (unknown 2026-09-03T15:12:33Z)` |

The embedded commit reads `unknown` because a git-archive extraction carries no
`.git`; `build.rs` documents that as the honest value for exported trees, and
the authoritative source commit is the `git_commit_sha` field above.

Independent re-verification at addendum time (recomputed live by
`claude-code-glm-4.7-glm-roam-19` on the `beadrs-4bb8bf78` re-dispatch, not
copied from either child's notes):

```console
$ sha256sum /home/coding/scratch/beadrs-f173164a-pin/bead-861cdcb
42b4335444d36bf7b7e6e3a21af229c47457fcb94962761eb185b05999e95f90  /home/coding/scratch/beadrs-f173164a-pin/bead-861cdcb
$ /home/coding/scratch/beadrs-f173164a-pin/bead-861cdcb capabilities | jq '.attempt_outcome.supported'
true
$ ./pinned-binaries/bead-pre-feature capabilities | jq 'has("attempt_outcome")'
false
```

The capability probe returns `supported: true` with all five outcomes
(`verified_success`, `work_failure`, `infrastructure_failure`, `cancelled`,
`indeterminate`), and the negative control still holds.

Distinctness: the fresh build's hash is distinct from all four inventory pins —
`68fe8d53…` (e115609 pin of record), `9a8455f2…` (f25ab5c), `d0da42bb…`
(pre-attempt-resolution), `7e0e73de…` (pre-feature) — as §2 predicts for a
rebuild (`42b4335… ≠ 68fe8d5…`). The pin inventory is unchanged: still exactly
four pins, no fifth binary added; the fresh artifact lives outside
`pinned-binaries/` at the scratch path its metadata records.

§6 bullet 3 stands unchanged: the fresh sha256 is likewise hash-only evidence
until SOURCE_DATE_EPOCH determinism lands. What this addendum adds is that the
declared-SHA archive build path is now demonstrated end-to-end with a recorded
artifact, hash, and capability probe — the acceptance C1 and C2 existed to
deliver.
