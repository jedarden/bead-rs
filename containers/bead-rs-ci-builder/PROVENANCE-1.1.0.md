# Builder 1.1.0 provenance

Release owner: `beadrs-b3059276`. Built by the sanctioned one-off Argo
Workflow `bead-rs-ci-builder-build-nrbqr` in iad-ci/argo-workflows.

- Source: `024eccca83beefce0704450d2adb548a8bd0c80e` (pushed Forgejo main).
- Image: `ronaldraygun/bead-rs-ci-builder:1.1.0`.
- Immutable digest: `sha256:b27e0e4c74598d388b1bf5fe82b8b0266fde282e497c776d8631574ca1bd15c1`.
- Live workflow result: `Succeeded`, started `2026-10-06T03:51:13Z`,
  finished `2026-10-06T03:55:03Z`.
- Build-date argument baked into IDENTITY: `2026-10-06T03:55:00Z`;
  the workflow timestamps above are the actual observed run interval.
- Exact checkout and VERSION cross-check passed. Docker build assertions
  require Rust 1.98.1, MSRV 1.85, Clippy/rustfmt, gh 2.101.0, crane 0.22.1,
  both Linux and both Darwin target standard libraries, plus jq/Python.

The earlier `jqn6t` (1.0.0) build failed with apt exit 100 because the exact
gh package was unavailable. Source `e226774a` replaced that mutable apt lookup
with the official immutable .deb, SHA-256
`f876a3b87bf67c94f773d17becca4dc7340b056dab901473a9260ee2a73e237b`.
Build `zpqft` (1.0.1) subsequently succeeded with digest
`sha256:42f27cdfc137b6e30bdd63eb601d326571f5222486cf4d5cf039c52231993ccb`.
1.1.0 adds the evidence tools required by the controlled promotion flow.

The builder workflow requests 1500m/3Gi and limits 3500m/6Gi, retaining the
existing bounded build reservation; current capacity and resource-limit
checks passed before submission. Peak RSS, actual Debian package inventory,
and cold/warm application CI timings have not yet been captured. Do not
treat build success as application conformance, fleet replay, published CLI
release, or two-host deployment evidence. The application CI sampler records
those run-level metrics when the candidate is verified.
