# Builder 1.3.0 provenance

Owner: `beadrs-b3059276`. Sanctioned standalone iad-ci Argo workflow
`bead-rs-ci-builder-build-24csm`, not an ArgoCD-managed resource.

- Source: `1973ad08f96b1584cb2d3fe8dd8d30a2c54a07f5`, pushed to Forgejo main.
- Image: `ronaldraygun/bead-rs-ci-builder:1.3.0`.
- Immutable digest: `sha256:79601fb9957da1057f78f8c297cead00a711e6c6b829da2299f8aa1e2c999281`.
  Workflow node output and independent authenticated registry digest agree.
- Observed workflow: Succeeded, started `2026-10-06T10:53:09Z`, finished
  `2026-10-06T10:56:24Z`; build-date argument `2026-10-06T10:53:00Z`.
- Base remains Ubuntu 24.04 amd64 manifest
  `sha256:f610ab94648195aa356059f5b41d6085c9d4d903c072430cdd1af7bdb646106b`.
  Historical managed-pin checksum/execution and glibc 2.39 assertions passed.
- Observed packages: `libc6-dev-arm64-cross` `2.39-0ubuntu8cross1` and its
  `linux-libc-dev-arm64-cross` dependency `6.8.0-25.25cross1`.
  Compiler version alone was insufficient under `--no-install-recommends`.
- Actual independent C witness compiled and linked with libc, pthreads,
  libdl and libm, and passed ELF64/AArch64/interpreter assertions. The loader
  was `/lib/ld-linux-aarch64.so.1`. Foreign-machine code was not executed.
- Rust 1.98.1/MSRV 1.85, Clippy/rustfmt, gh 2.101.0/crane 0.22.1 and all
  four target standard-library assertions remain enforced and passed.
- Desired deployment: declarative-config
  `15a1e9d2cb53c2231cee4d47ec1c1555f15cb137`, path
  `k8s/iad-ci/argo-workflows/bead-rs-ci-workflowtemplate.yml`, Application
  `argo-workflows-ns-iad-ci`. The normal push preserved concurrent work by
  replaying only this task's unpushed pin commit. Live reconciliation is
  pending at this recording; observe it before candidate submission.

Why rotate: `daa6613f` candidate `bead-rs-secret-0-3-0-fcjj8` passed complete
default/managed source checks and its optimized x86 build, then ARM bundled
SQLite failed. Actual 1.2.0 image probes reproduced missing libc development
headers and startup objects despite an installed cross compiler. New image
witnesses prevent that incomplete sysroot from passing image verification.

Requests remain 1500m/3Gi, limits 3500m/6Gi; no managed cluster object was
mutated. Strict offline Argo lint/server dry-run passed. GitOps resource audit
checked 532 files with zero critical findings; live capacity inspection and
all five normal pre-commit/commit hooks passed. Full package inventory, peak
RSS and corrected-image cold/warm application timings were not measured.
The successful build pod was removed normally by OnPodSuccess.

This is builder provenance only. New exact-source full conformance, complete
candidate payload, final host execution/cost/parity/fleet replay, publication
and managed CLI replacement on lab/codinghome remain separate open gates.
Latest evidence is retained under `docs/releases/` while the owning store's
organization-scanner quarantine refuses note updates; no hold was bypassed.
