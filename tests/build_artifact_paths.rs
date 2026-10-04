//! Documentation checks for release artifact locations.
//!
//! Fleet build hosts enforce `CARGO_TARGET_DIR=/build/<repo>` through the cargo
//! wrapper. Keep the build procedures aligned with that rule so their commands
//! do not send users looking for binaries in stale tree-local paths.

use std::fs;
use std::path::{Path, PathBuf};

const ENFORCED_BINARY: &str = "/build/bead-rs/release/bead";

fn document(relative_path: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn build_procedure_uses_the_enforced_artifact_path() {
    let document = document("BUILD_PROCEDURE.md");
    assert!(
        document.contains("CARGO_TARGET_DIR=/build/<repo>"),
        "BUILD_PROCEDURE.md must document the enforced per-repository target directory"
    );
    assert!(
        document.contains(ENFORCED_BINARY),
        "BUILD_PROCEDURE.md must use the enforced bead artifact path"
    );
    assert!(
        !document.contains("/home/coding/target/release/bead"),
        "BUILD_PROCEDURE.md must not use the obsolete shared-checkout target path"
    );
}

#[test]
fn attempts_build_procedure_does_not_present_tree_local_artifact_as_location() {
    let document = document("docs/attempts-binary-build.md");
    assert!(
        document.contains("CARGO_TARGET_DIR=/build/<repo>"),
        "attempts build documentation must state the enforced target-directory rule"
    );
    assert!(
        document.contains(ENFORCED_BINARY),
        "attempts build documentation must identify the enforced bead artifact path"
    );
    assert!(
        !document.contains("./target/release/bead")
            && !document.contains("target/release/bead"),
        "attempts build documentation must not present target/release/bead as the artifact location"
    );
}
