//! Runs the end-to-end installer/release-artifact suite
//! (`tests/install_release_e2e.sh`) so `cargo test` covers the documented
//! install workflow.
//!
//! The suite is hermetic: it drives the real `install.sh` against a local
//! fake release over `file://` URLs with a stubbed `uname`, so no network
//! and no compiled bead binary are required. It skips itself when the shell
//! tools the documented workflow depends on (curl/wget, sha256 tooling) are
//! unavailable. Set `BEAD_INSTALL_E2E_LIVE=1` in the environment to also
//! check the real latest GitHub release (network required); that live check
//! is skipped by default so `cargo test` stays hermetic.

use std::path::Path;
use std::process::{Command, Stdio};

#[test]
fn documented_install_workflow_e2e_suite_passes() {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/install_release_e2e.sh");
    assert!(
        Path::new(script).exists(),
        "installer e2e suite not found at {script}"
    );

    // stdin is closed so the suite (and the installer it drives) can never
    // block on an interactive prompt; the live network check is opt-in.
    let output = Command::new("bash")
        .arg(script)
        .env_remove("BEAD_INSTALL_E2E_LIVE")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run tests/install_release_e2e.sh with bash");

    println!(
        "--- install_release_e2e.sh stdout ---\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    println!(
        "--- install_release_e2e.sh stderr ---\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        output.status.success(),
        "installer e2e suite failed with status {:?}; see output above",
        output.status.code()
    );
}
