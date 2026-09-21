# bead-rs-ci-builder

The digest-pinned builder image that the `bead-rs-ci` Argo WorkflowTemplate
(iad-ci) runs its whole pipeline in. It replaces the per-run Debian apt
install, gh install, rustup stable + clippy + rustfmt install, MSRV
toolchain install and `gcc-aarch64-linux-gnu` install that the workflow
used to repeat on every run.

Consumed by
`declarative-config/k8s/iad-ci/argo-workflows/bead-rs-ci-workflowtemplate.yml`
as

```
ronaldraygun/bead-rs-ci-builder:<version>@sha256:<digest>
```

The tag is for humans; **the digest is the pin**. The repo is private
(`ronaldraygun/*` on Docker Hub), so the template carries
`imagePullSecrets: [docker-hub-registry]`.

## What is pinned, and how durably

| Component | Pin | Rotation trigger |
|---|---|---|
| Base image | `debian:bookworm-slim@sha256:f3034a6e…` (amd64 OCI manifest) | deliberate rebuild |
| rustup | archive `1.29.1` + published SHA-256 (frozen URL) | deliberate rebuild |
| Rust stable | exact `1.98.1`, installed as the **default** toolchain | deliberate rebuild |
| Rust MSRV | installed under the exact name `1.85` (what the workflow resolves with `cargo +1.85`) | deliberate rebuild + MSRV bump |
| Release targets | the four release triples' std, pre-installed | deliberate rebuild |
| gh | `gh=2.101.0` from GitHub's apt repo | deliberate rebuild |
| crane | `v0.22.1` + published checksum (cache transport) | deliberate rebuild |
| aarch64 cross gcc | `gcc-aarch64-linux-gnu` from bookworm (distro-pinned, not point-pinned) | deliberate rebuild |

Debian packages are pinned to the *bookworm distribution*, not to
point-release versions: `deb.debian.org` rotates `.debs` on security
updates, so an exact-version pin there would break every rebuild within
months. The versions actually installed at build time are recorded in
`PROVENANCE-1.0.0.md`, and the pushed image digest is what consumers pin,
so rebuild drift cannot silently reach CI.

The Dockerfile ends with a build-time verification block that refuses to
produce the image unless every pin resolved to what the file records
(rustc/clippy/fmt/gh/crane versions, MSRV resolution, target presence). A
wrong pin fails the build, not a CI run three layers downstream.

## Building and pushing the image

`build-image-workflow.yaml` is a **one-off** kaniko build+push Workflow.
It is deliberately NOT ArgoCD-managed and has no trigger: the builder
image changes rarely, and every build must be a conscious act that ends
with the resulting digest being pinned into the WorkflowTemplate.

```bash
REV=$(git rev-parse HEAD)   # a pushed bead-rs commit containing this directory
kubectl --kubeconfig=$HOME/.kube/iad-ci.kubeconfig create -f - <<EOF
<build-image-workflow.yaml with:
  parameters.revision  = $REV
  parameters.version   = contents of VERSION
  parameters.build-date = RFC3339 UTC timestamp>
EOF
```

The init container fetches exactly that revision from Forgejo (retrying
with backoff), cross-checks the `version` parameter against the `VERSION`
file in that tree, and the Dockerfile bakes revision/version/date into the
image's `IDENTITY` file. The workflow's `digest` output parameter is the
pin: record it in `PROVENANCE-<version>.md` and in the WorkflowTemplate
`image:` line. Kaniko pushes the semver tag from `VERSION` — never
`:latest`, never a bare SHA tag.

## The Cargo artifact cache (`ci-cache.sh`)

The make-or-break for run time is not the tool install (that is now
zero) but recompiling ~400 dependency crates on every run. The cache is
a **registry image, not a PVC and not a mutable tag**: one tag per cache
key on `ronaldraygun/bead-rs-ci-cargo-cache`, holding `$CARGO_HOME` plus
the workspace `target/` directory as of a green run.

Key composition (see `ci-cache.sh key`):

```
fmtv1-lock<sha256(Cargo.lock)[:16]>-rust<rustc version>-<dpkg arch>-img<sha256(IDENTITY)[:12]>
```

- `Cargo.lock` — the dependency set
- `rustc --version` — the (stable/default) toolchain
- architecture
- the image's baked `IDENTITY` file — the builder-image identity; a new
  image build invalidates the cache without anyone editing the template

Tags are content-addressed and never overwritten: `push` is a no-op when
the tag exists, and a key miss/pull failure/`crane` error falls back to a
**cold build and never fails the run** (`CACHE=miss …` in the log). Only
a run that has already passed `fmt`/`clippy`/`test` pushes, so a red run
can never seed a cache it did not prove.

Bounds: the layer excludes `registry/src` (re-extracted on demand from
`registry/cache`), `target/**/incremental` (pure churn under
`CARGO_INCREMENTAL=0`) and `target/package`; a push above the 6 GiB cap
is refused (`CACHE-PUSH=skipped reason=cap-exceeded`). Stale tags simply
stop matching their key and go unused; pruning them is an operator
action on the registry. No per-run PVC is created and no mutable tag is
written.

## Rotating the image

1. Edit the pins in `Dockerfile` (and `VERSION` — bump the semver).
2. Commit, push, build+push with `build-image-workflow.yaml` (above).
3. Copy the workflow's `digest` output into this directory's
   `PROVENANCE-<new-version>.md`.
4. Bump the `image:` line in
   `declarative-config/k8s/iad-ci/argo-workflows/bead-rs-ci-workflowtemplate.yml`
   to `<version>@sha256:<new digest>`; push; let ArgoCD sync
   `argo-workflows-ns-iad-ci`.
5. The first run on the new image is cold (new cache key via `IDENTITY`);
   every following run on the same key is warm.

## Measurements

Cold/warm setup time, build time, cache hit/bytes, peak CPU/RSS (incl.
the Argo `wait` executor), queue time and pod/node-hours are recorded
from the live bead-rs-ci runs that validated this change; the sampler in
the workflow logs `cpu_us`/`mem` every 5s and the container's
`memory.peak` at the end. Numbers and the run IDs they came from are in
`PROVENANCE-1.0.0.md`. Resource requests on the `ci` template
(1000m/2Gi) are unchanged from the proven fit for 2-vCPU nodes.
