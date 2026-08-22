# ADR-004: Raise the MSRV to Rust 1.85 and Migrate to Edition 2024

**Status**: Accepted

**Date**: 2026-08-15

**Decision-makers**: bead-rs release owner

## Context

The bootstrap plan fixed Rust 1.75 as the minimum supported Rust version and
edition 2021 as the language edition. Both were correct choices for a
clean-room bootstrap in early delivery: a floor old enough that toolchain
availability could never block a worker, verified once at F001.

Measured on 2026-08-15, the situation around that floor has three properties
worth recording:

1. **Every real build already uses current stable.** The development host runs
   stable 1.97.1, and the `bead-rs-ci` WorkflowTemplate installs
   `--default-toolchain stable`, so CI compiles, lints, and tests on the
   latest stable release at every run. Nothing in the delivery path actually
   exercises 1.75.
2. **The declared floor is true but untested.** `cargo +1.75 check
   --all-targets` passes against the current lockfile (verified 2026-08-15,
   warnings only). No CI lane checks this; the plan's risk register names a
   "Rust 1.75 lane" that has never existed. The declaration is honest today by
   luck, not by verification — any dependency update could silently break it.
3. **The pinned dependencies have drifted far behind current releases**:
   rusqlite `=0.31.0` against 0.40.2 upstream, clap `=4.5.4` against 4.6.6,
   clap_mangen `=0.2.20` against 0.3.3, time `=0.3.36` against 0.3.55,
   tempfile `=3.12.0` against 3.27.0, assert_cmd `=2.0.16` against 2.2.2.
   Some of those updates will eventually raise MSRV requirements of their own.

The consumers of the installed binary are the fleet machines of the
surrounding environment, all on current stable; crates.io publication remains
out of scope. Holding the floor at 1.75 therefore protects nobody, while it
blocks edition 2024 (which requires 1.85) and leaves the MSRV claim
unverifiable in practice.

An MSRV change alters plan section 8 ("subject to Rust 1.75 verification"),
the section 10 risk register, `AGENTS.md`, and the README, so under the
plan's change-governance rule it requires this record and a coherent plan
revision.

## Decision

- Raise the declared MSRV to **Rust 1.85** (`rust-version = "1.85"`).
- Migrate the crate to **edition 2024** in the same change.
- Update every forward-looking MSRV statement — `Cargo.toml`, `README.md`,
  `AGENTS.md`, plan section 8 and the section 10 risk row — in the same
  commit that flips the manifest. `PROVENANCE.md` entries citing Rust 1.75
  are historical evidence of bootstrap-era verification and are not edited.
- Add a **pinned MSRV verification lane** to `bead-rs-ci` (a
  `cargo +1.85 check --all-targets` step) so the declared floor is tested on
  every run instead of asserted. The lane must pin the same version the
  manifest declares; drift between them is a CI failure, not a doc nit.
- Scope the **dependency refresh separately**. Unpinning and updating the
  `=`-pinned dependencies — in particular rusqlite 0.31→0.40 (new API surface
  and a much newer bundled SQLite, which is behavior-adjacent for a store
  whose journal and synchronous semantics are contractual) and the
  clap/clap_mangen updates (help-snapshot and byte-reproducible man-page
  regeneration) — happens after the toolchain change lands, with the
  conformance, concurrency, and section 3.5.10 benchmark lanes rerun and the
  recorded benchmark budget rechecked.

### Dependency refresh (2026-08-22)

The post-migration refresh uses non-exact manifest ranges and commits the
resulting `Cargo.lock` resolution. This keeps routine compatible patch releases
available while making every build in this revision reproducible and reviewable.

| Dependency | Manifest range | Resolved version | Pinning decision |
| --- | --- | --- | --- |
| `rusqlite` | `0.40.2` | `0.40.2` | caret range within the 0.40 API line; the bundled SQLite change is covered by the store/concurrency lanes |
| `clap` | `4.6.6` | `4.6.6` | caret range within the 4.6 API line; help output is regenerated and tested |
| `clap_mangen` | `0.3.3` | `0.3.3` | caret range within the 0.3 API line; generated man pages are regenerated and byte-checked |
| `time` | `>=0.3.45, <0.3.46` | `0.3.45` | bounded non-exact range; 0.3.45 is the latest 0.3 release compatible with the Rust 1.85 lane, while 0.3.46+ requires Rust 1.88 |
| `tempfile` | `3.27.0` | `3.27.0` | caret range within the 3.x API line |
| `assert_cmd` | `2.2.2` | `2.2.2` | caret range within the 2.x API line |
| `uuid` | `1.24.1` | `1.24.1` | caret range within the 1.x API line |
| `fs2` | `0.4.3` | `0.4.3` | caret range within the 0.4 API line; this is the current release |

The exact versions above are lockfile resolutions, not exact manifest pins.
Any future update must rerun the help/man-page, conformance, concurrency,
benchmark-budget, and `cargo +1.85 check --all-targets` gates together.

The dependency-refresh recheck preserved the section-3.5.10 smoke budget: 40
release-harness reports (100 and 1,000 beads × five dataset families × four
workloads), each with the existing 5-second warmup and 1-second measured
interval. All reports had zero busy failures and zero claim conflicts; 34 were
`completed` and six were the contractually valid `resource_limited` result.
SQLite 3.53.2 was reported consistently. No budget re-record was required.

A future MSRV advance requires a plan revision citing a new ADR or an
explicit revision of this one; the floor never moves silently.

## Rationale

1.85 rather than current stable: the floor's job is to name the oldest
toolchain the project promises to compile on, not to chase releases. 1.85 is
the lowest version that unlocks edition 2024, is comfortably below every
consumer, and gives the CI lane a stable pin that does not churn every six
weeks. Tracking stable directly was rejected as churn without benefit; a
rolling "stable minus N" policy was rejected because a moving floor cannot be
pinned in a CI lane or cited in evidence; keeping 1.75 was rejected because
it is untested, blocks the current edition, and protects no consumer.

With `#![forbid(unsafe_code)]` across the crate, the edition 2024 changes
with real risk surface (unsafe-block semantics) do not apply; the migration
is expected to be mechanical via `cargo fix --edition` followed by the
standard fmt, clippy, test, and man-page reproducibility gates.

## Consequences

- The MSRV claim becomes enforced instead of aspirational; a dependency
  update that raises the effective floor fails CI loudly.
- Help output or man pages that change under newer clap behavior are caught
  by the existing snapshot and byte-reproducibility tests and must be
  regenerated in the same commit that causes the change.
- The plan's "Rust 1.75 lane" wording is corrected to name the real,
  ADR-governed lane.
- Bootstrap-era 1.75 evidence in `PROVENANCE.md` and historical review
  documents remains valid as history and is deliberately left unedited.
- The dependency refresh inherits a verified toolchain baseline instead of
  mixing toolchain and dependency variables in one change.

## Related

- Plan section 8 (Rust architecture, dependency verification) and the
  section 10 risk register row "MSRV or dependency drift"
- `docs/notes/ideas-ledger.md`, 2026-08-15 run-4 disposition (assessment
  request recorded there)
- Tracking beads: see genesis `Genesis: ADR-004 Rust Toolchain Modernization`
