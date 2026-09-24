//! Documentation check for the README's build-artifact locations
//! (beadrs-e2ce6874).
//!
//! The fleet build hosts run a cargo wrapper (`~/.local/bin/cargo`,
//! needle-d6b685b4) that forces `CARGO_TARGET_DIR=/build/<repo>` — `<repo>`
//! being the origin URL's basename — on any host with a `/build` directory,
//! and refuses a `--target-dir` outside it. A plain
//! `cargo build --release` therefore does not put binaries in the
//! tree-local `target/` there, and the README's old "after building, the
//! binaries are at `target/release/bead`" instructions were wrong on every
//! host the fleet actually builds on.
//!
//! The tree-local path is still the truth on hosts without `/build`, where
//! the wrapper leaves cargo's default in place, so the README now documents
//! both and leads with the enforced one. These assertions fire when that
//! documentation drifts back: if one fails, fix the README's "Build
//! artifacts" / "Verifying the build" sections, not this test.

use std::fs;
use std::path::Path;

fn readme() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md");
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

#[test]
fn readme_documents_the_enforced_per_repo_target_dir() {
    let md = readme();
    assert!(
        md.contains("CARGO_TARGET_DIR=/build/<repo>"),
        "README must state the enforced CARGO_TARGET_DIR=/build/<repo> rule \
         in its 'Build artifacts' section"
    );
    assert!(
        md.contains("/build/bead-rs/release/bead"),
        "README must document the bead binary at /build/bead-rs/release/bead \
         for hosts with /build"
    );
    assert!(
        md.contains("/build/bead-rs/release/generate-man-pages"),
        "README must document the man page generator at \
         /build/bead-rs/release/generate-man-pages for hosts with /build"
    );
}

#[test]
fn readme_does_not_present_tree_local_artifacts_as_the_location() {
    let md = readme();
    assert!(
        !md.contains("./target/release/"),
        "README documents ./target/release/… as a bare invocation/location; \
         that path is only valid on hosts without /build — document the \
         enforced /build/bead-rs/release/… location first and qualify the \
         tree-local one as the no-/build default"
    );
}

#[test]
fn readme_verify_snippet_runs_the_enforced_artifact_path() {
    let md = readme();
    let (_, section) = md
        .split_once("### Verifying the build")
        .expect("README keeps a '### Verifying the build' section");
    let fence = section
        .split("```")
        .nth(1)
        .expect("'Verifying the build' keeps a fenced command block");
    assert!(
        fence.contains("/build/bead-rs/release/bead --version"),
        "the verify snippet must run the enforced artifact \
         /build/bead-rs/release/bead"
    );
    assert!(
        !fence.contains("./target/"),
        "the verify snippet must not present ./target/… as an unconditional \
         location"
    );
}
