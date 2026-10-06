# Builder 1.2.0 provenance

Owner: `beadrs-b3059276`. Sanctioned standalone iad-ci Argo workflow
`bead-rs-ci-builder-build-4m6ds`, not an ArgoCD-managed resource.

- Source: `dd61a61acc4f603ae8a9ee2a3caf2ef21c19737b`, pushed to Forgejo main.
- Image: `ronaldraygun/bead-rs-ci-builder:1.2.0`.
- Immutable digest: `sha256:71760356a17f03dc6b1e9fbe37c94f5c12b660d7f3865858ebf77ed312b556e5`.
- Observed workflow: Succeeded, started `2026-10-06T06:44:29Z`, finished
  `2026-10-06T06:51:21Z`; build-date argument `2026-10-06T06:44:27Z`.
- Base: official Ubuntu 24.04 amd64 manifest
  `sha256:f610ab94648195aa356059f5b41d6085c9d4d903c072430cdd1af7bdb646106b`.
  Docker Hub metadata, actual local pull digest and execution were verified.
- Docker build requires glibc 2.39 and checksum-verified execution of the
  unchanged historical managed pin: policy `managed-enforce-no-ack`.
  Rust 1.98.1/MSRV 1.85, Clippy/rustfmt, gh 2.101.0/crane 0.22.1 and all
  four target standard-library assertions are retained.
- Desired deployment: declarative-config `1231c6c6`, path
  `k8s/iad-ci/argo-workflows/bead-rs-ci-workflowtemplate.yml`, Application
  `argo-workflows-ns-iad-ci`. Observed live template matches the immutable
  image, tag fetching and process-scoped Git identities. Application operation
  Succeeded at revision `1231c6c677b181f4c559615dda274d68b202aed2`, finished
  `2026-10-06T06:57:10Z`; this template is Synced. Unrelated aggregate
  Application OutOfSync/Degraded status is not claimed resolved.

Why rotate: exact-source CI `bead-rs-ci-jdfdp` on builder 1.1.0 failed
`managed_policy_pin`, `plan_tag_consistency`, and `quarantine_git_publication`.
A read-only runtime probe confirmed GLIBC_2.39 was unavailable in bookworm's
2.36; an interpreter-path alias could not fix it. The unchanged pin executed
with capabilities on this Ubuntu base locally. Tag fetching and Git identity
are corrected separately in the deployment template; no test is waived.

Requests remain 1500m/3Gi, limits 3500m/6Gi. The build initially waited for
capacity, then scheduled normally; no taint, node or managed resource was
mutated. Argo lint passed. Deployment resource audit found zero critical
findings; normal pre-commit and gitleaks guards passed. Full Ubuntu package
inventory, peak RSS and cold/warm application timings were not captured.
The completed build pod was removed normally by OnPodSuccess.

This is builder provenance only. Application full conformance, exact candidate
hashes/performance, fleet replay, version publication and both managed CLI
installations are separate incomplete gates.
