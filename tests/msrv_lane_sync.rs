//! Pins the CI MSRV lane to the `rust-version` the manifest declares.
//!
//! ADR-004 requires the pinned `cargo +1.85 check --all-targets` lane to
//! compile with exactly the toolchain `Cargo.toml` declares: "The lane must
//! pin the same version the manifest declares; drift between them is a CI
//! failure, not a doc nit." The lane's invocation lives in declarative-config
//! (`k8s/iad-ci/argo-workflows/bead-rs-ci-workflowtemplate.yml`), outside this
//! repository, so these tests enforce the in-repo half of that contract: the
//! builder image (`containers/bead-rs-ci-builder/Dockerfile`) must install an
//! MSRV toolchain under the exact name the manifest declares, and its
//! build-time verification block must refuse to produce an image that does
//! not resolve that floor. The remaining direction — the workflowtemplate's
//! own MSRV literal drifting from the image — cannot fail silently: a
//! toolchain name only resolves if the image installs it, so a lane asking
//! for an uninstalled `+X` fails outright rather than checking the wrong
//! floor.
//!
//! Both pins are compiled in with `include_str!` so the check travels with
//! every checkout and runs in every `cargo test`, catching a one-sided
//! version bump here — before push — instead of at the next CI run.

/// The crate manifest, as compiled in from the checkout root.
const MANIFEST: &str = include_str!("../Cargo.toml");

/// The builder image recipe consumed by the `bead-rs-ci` WorkflowTemplate
/// as `ronaldraygun/bead-rs-ci-builder:<version>@sha256:<digest>`.
const BUILDER_DOCKERFILE: &str = include_str!("../containers/bead-rs-ci-builder/Dockerfile");

/// The declared MSRV, extracted the same line-anchored way the CI lane
/// extracts it (first `rust-version = "X"` assignment in `Cargo.toml`).
fn declared_msrv() -> String {
    MANIFEST
        .lines()
        .find_map(|line| {
            let value = line.trim().strip_prefix("rust-version")?.trim();
            let value = value.strip_prefix('=')?.trim();
            Some(value.trim_matches('"').to_owned())
        })
        .unwrap_or_else(|| {
            panic!(
                "Cargo.toml declares no `rust-version`; the MSRV lane has no \
                 manifest floor to stay synchronized with (ADR-004)"
            )
        })
}

/// The MSRV toolchain the builder image installs, from the Dockerfile's
/// `ARG RUST_MSRV` pin — the exact name the lane resolves with `cargo +<name>`.
fn image_msrv_toolchain() -> String {
    BUILDER_DOCKERFILE
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("ARG RUST_MSRV=")
                .map(str::to_owned)
        })
        .unwrap_or_else(|| {
            panic!(
                "containers/bead-rs-ci-builder/Dockerfile declares no \
                 `ARG RUST_MSRV`; the builder image installs no MSRV \
                 toolchain for the lane to resolve"
            )
        })
}

#[test]
fn builder_image_installs_the_declared_msrv_toolchain() {
    let declared = declared_msrv();
    let installed = image_msrv_toolchain();
    assert_eq!(
        declared, installed,
        "the declared MSRV and the CI lane's toolchain diverge: Cargo.toml \
         declares rust-version = \"{declared}\" while the builder image pins \
         ARG RUST_MSRV = \"{installed}\". Bump both in the same change, rerun \
         `cargo +{declared} check --all-targets`, rebuild the builder image so \
         `cargo +{declared}` resolves in CI, and record the advance in a plan \
         revision citing an ADR — the floor never moves silently (ADR-004)."
    );
}

#[test]
fn builder_image_refuses_to_build_without_the_declared_msrv() {
    let declared = declared_msrv();
    let pinned_pattern = format!("\"rustc {declared}.");
    assert!(
        BUILDER_DOCKERFILE.lines().any(|line| {
            line.contains("rustup run ${RUST_MSRV} rustc --version")
                && line.contains(&pinned_pattern)
        }),
        "the builder image's build-time verification does not pin rustc \
         {declared}: its `rustup run ${{RUST_MSRV}} rustc --version` case must \
         match {pinned_pattern}*) so an image whose MSRV toolchain does not \
         resolve the declared floor fails its own build instead of the first \
         CI run that needs it."
    );
}
