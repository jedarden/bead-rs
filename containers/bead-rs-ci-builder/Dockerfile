# bead-rs CI builder image.
#
# Replaces the per-run apt / gh / rustup installs that the bead-rs-ci
# WorkflowTemplate previously repeated on every run. Consumed by
# declarative-config k8s/iad-ci/argo-workflows/bead-rs-ci-workflowtemplate.yml
# as `ronaldraygun/bead-rs-ci-builder:<version>@sha256:<digest>` -- the digest
# is the pin, the tag is for humans. See README.md in this directory for the
# design, the rotation runbook, and measured cold/warm numbers.
#
# Pinning policy (what is pinned, and how durably):
#   - Base image:            by digest (multiarch index). Rotates only on a
#                            deliberate rebuild.
#   - rustup-init:           versioned archive URL on static.rust-lang.org
#                            (frozen forever) + published SHA-256.
#   - Rust toolchains:       exact patch releases (channel manifests freeze).
#   - gh:                    exact release .deb + published SHA-256. The
#                            apt repository does not retain every version.
#   - crane:                 exact release + published SHA-256.
#   - Ubuntu packages:       pinned to the noble distribution, not to
#                            point-release versions. Distro repositories rotate
#                            .debs on security updates, so an exact-version
#                            pin here would break every rebuild within
#                            months. The versions actually installed at
#                            build time are recorded in PROVENANCE-<version>.md,
#                            and the pushed image digest is what consumers
#                            pin, so rebuild drift cannot silently reach CI.
# Historical managed pins require GLIBC_2.39. Bookworm's 2.36 cannot run
# them even after their recorded Nix interpreter path is supplied. Noble
# provides 2.39, below lab's observed 2.40 and codinghome's 2.42. Actual
# candidate execution on both fleet hosts remains a separate delivery gate.
FROM ubuntu:24.04@sha256:f610ab94648195aa356059f5b41d6085c9d4d903c072430cdd1af7bdb646106b

ARG DEBIAN_FRONTEND=noninteractive

# System dependencies. rusqlite is built with the "bundled" feature (vendors
# and compiles its own SQLite), so no libsqlite3-dev is needed -- the same
# set the per-run apt install used, plus:
#   binutils    readelf, for the pinned-binary interpreter check
#   xz-utils    rustup toolchain tarballs
#   gcc-aarch64-linux-gnu
#               aarch64-unknown-linux-gnu cross linker (was a second apt
#               round-trip in the release section of every run)
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        git \
        jq \
        python3 \
        build-essential \
        pkg-config \
        libssl-dev \
        binutils \
        xz-utils \
        gcc-aarch64-linux-gnu \
    && rm -rf /var/lib/apt/lists/*

# gh CLI: the 1.0.0 build failed because the apt repository no longer
# supplied this exact version. Use the immutable official release asset,
# verified against its published digest, rather than weakening the pin.
ARG GH_VERSION=2.101.0
ARG GH_DEB_SHA256=f876a3b87bf67c94f773d17becca4dc7340b056dab901473a9260ee2a73e237b
RUN curl -fsSL "https://github.com/cli/cli/releases/download/v${GH_VERSION}/gh_${GH_VERSION}_linux_amd64.deb" \
        -o /tmp/gh.deb \
    && echo "${GH_DEB_SHA256}  /tmp/gh.deb" | sha256sum -c - \
    && dpkg -i /tmp/gh.deb \
    && rm /tmp/gh.deb

# Rust: rustup (pinned build tool) + exact toolchains.
#
#   stable lane -> 1.98.1 as the DEFAULT toolchain under its own name. The
#       workflow only ever invokes bare `cargo`/`rustc` for this lane, so the
#       default-toolchain identity is all it needs; pinning the version here
#       is what stops fmt/clippy from drifting with rustup releases.
#   MSRV lane   -> installed under the name `1.85`, exactly the name the
#       workflow resolves (`cargo +1.85`). rustup keeps the requested name as
#       the toolchain identifier, and the resolution happens once, at image
#       build time -- frozen afterwards.
#   Targets     -> the four the release stage builds, so `rustup target add`
#       in the workflow stays as a no-op assertion rather than a download.
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:${PATH}

ARG RUSTUP_VERSION=1.29.1
ARG RUSTUP_INIT_SHA256=dda7234360b7f578ca8b0ddcb80145646fa61a67c1720a5abc7051b35c9fcb71
ARG RUST_STABLE=1.98.1
ARG RUST_MSRV=1.85
RUN curl -fsSL "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/x86_64-unknown-linux-gnu/rustup-init" \
        -o /tmp/rustup-init \
    && echo "${RUSTUP_INIT_SHA256}  /tmp/rustup-init" | sha256sum -c - \
    && chmod +x /tmp/rustup-init \
    # --component is repeatable; one flag carrying two names makes rustup-init
    # abort with an unexpected-argument error, which is how the first 1.0.0
    # build (2026-09-26) died before it could push anything.
    && /tmp/rustup-init -y --no-modify-path \
        --default-toolchain "${RUST_STABLE}" \
        --profile minimal \
        --component clippy --component rustfmt \
    && rustup toolchain install "${RUST_MSRV}" --profile minimal \
    && rustup target add --toolchain "${RUST_STABLE}" \
        x86_64-unknown-linux-gnu \
        aarch64-unknown-linux-gnu \
        x86_64-apple-darwin \
        aarch64-apple-darwin \
    && rm /tmp/rustup-init

# crane: pushes/pulls the cargo artifact cache (see ci-cache.sh). Pinned
# release tarball + published checksums.txt entry.
ARG CRANE_VERSION=v0.22.1
ARG CRANE_SHA256=0ab7a1d6932a213aed964ce97666c3077fe691c8606413674a8b3e0b9ec4cda0
RUN curl -fsSL "https://github.com/google/go-containerregistry/releases/download/${CRANE_VERSION}/go-containerregistry_Linux_x86_64.tar.gz" \
        -o /tmp/crane.tar.gz \
    && echo "${CRANE_SHA256}  /tmp/crane.tar.gz" | sha256sum -c - \
    && tar -xzf /tmp/crane.tar.gz -C /usr/local/bin crane \
    && rm /tmp/crane.tar.gz

# Build identity, supplied by the build workflow from the checked-out
# revision. Baked onto disk (not only as an image label) because ci-cache.sh
# hashes it into the cargo-cache key at run time: a new image build therefore
# invalidates the cache without anyone editing the workflow template.
ARG BUILDER_VERSION
ARG BUILD_DATE
ARG SOURCE_REVISION
RUN mkdir -p /usr/local/share/bead-rs-ci-builder \
    && printf 'version=%s\ndate=%s\nsource_revision=%s\n' \
        "${BUILDER_VERSION}" "${BUILD_DATE}" "${SOURCE_REVISION}" \
        > /usr/local/share/bead-rs-ci-builder/IDENTITY

# Execute the unchanged historical managed pin on the chosen libc, not only
# a guessed version number. The production workflow supplies its recorded
# interpreter path; direct invocation here tests the actual runtime ABI.
COPY pinned-binaries/bead-managed-secret-policy-cd4986c /usr/local/share/bead-rs-ci-builder/historical-managed-pin
RUN printf '%s  %s\n' \
        6a5969ed9c1c5215d3f0f93505aa0603d7375281f6fb3f64ea6101ece149a50f \
        /usr/local/share/bead-rs-ci-builder/historical-managed-pin | sha256sum -c - \
    && /lib64/ld-linux-x86-64.so.2 /usr/local/share/bead-rs-ci-builder/historical-managed-pin capabilities \
        > /tmp/historical-pin-capabilities.json \
    && jq -e '.secret_scan.compiled_policy == "managed-enforce-no-ack"' /tmp/historical-pin-capabilities.json \
    && test "$(getconf GNU_LIBC_VERSION)" = 'glibc 2.39'

# Build-time verification: refuse to produce the image unless every pin
# actually resolved to what this Dockerfile records. A wrong version string
# fails the build here, not a CI run three layers downstream.
RUN set -eux \
    && case "$(rustc --version)" in "rustc ${RUST_STABLE} "*) ;; *) echo "stable rustc mismatch: $(rustc --version)" >&2; exit 1 ;; esac \
    && case "$(rustup run ${RUST_MSRV} rustc --version)" in "rustc 1.85."*) ;; *) echo "MSRV rustc mismatch: $(rustup run ${RUST_MSRV} rustc --version)" >&2; exit 1 ;; esac \
    && cargo clippy --version \
    && cargo fmt --version \
    && jq --version \
    && python3 --version \
    && case "$(gh --version)" in "gh version 2.101.0 "*) ;; *) echo "gh mismatch: $(gh --version)" >&2; exit 1 ;; esac \
    && case "$(crane version)" in *0.22.1*) ;; *) echo "crane mismatch: $(crane version)" >&2; exit 1 ;; esac \
    && aarch64-linux-gnu-gcc --version | head -n1 \
    && readelf --version | head -n1 \
    && rustup target list --installed --toolchain "${RUST_STABLE}" \
        | grep -xF x86_64-unknown-linux-gnu \
    && rustup target list --installed --toolchain "${RUST_STABLE}" \
        | grep -xF aarch64-unknown-linux-gnu \
    && rustup target list --installed --toolchain "${RUST_STABLE}" \
        | grep -xF x86_64-apple-darwin \
    && rustup target list --installed --toolchain "${RUST_STABLE}" \
        | grep -xF aarch64-apple-darwin

WORKDIR /workspace
