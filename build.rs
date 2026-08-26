//! Build script for bead-rs.
//!
//! Embeds the commit and build timestamp that produced this binary so that
//! `bead --version` identifies a *build*, not just a release line.
//!
//! `CARGO_PKG_VERSION` cannot do that on its own. Cargo.toml carries one
//! version for the whole development window between releases, so every binary
//! built in that window reports the same string while being different code.
//! bead-rs sat at 0.1.3 from 2026-08-13 to 2026-08-25 across two changes to
//! documented behaviour (checkpoint auto-publication, and reopen clearing the
//! assignee). A binary predating both reported exactly what a binary
//! containing both reported, which is how a stale install came to look like
//! incorrect documentation rather than an out-of-date binary.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn main() {
    // Short commit SHA. Absent when building from an exported tarball or any
    // tree without git; "unknown" is honest there rather than misleading.
    let commit = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_string());

    // Uncommitted tracked changes mean this binary corresponds to no commit.
    // Untracked files are excluded: they do not alter what was compiled.
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
        .map(|s| !s.is_empty())
        .unwrap_or(false);

    // UTC build time, resolved without pulling in a date dependency.
    let built = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    // The first token stays the bare program name: NEEDLE identifies its bead
    // backend by parsing the leading word of `<tool> --version`, and treats a
    // trailing parenthesised detail block as expected.
    let version_string = format!(
        "{} ({}{} {})",
        env!("CARGO_PKG_VERSION"),
        commit,
        if dirty { "-dirty" } else { "" },
        built
    );

    println!("cargo:rustc-env=BEAD_COMMIT_SHA={commit}");
    println!("cargo:rustc-env=BEAD_BUILD_TIMESTAMP={built}");
    println!("cargo:rustc-env=BEAD_BUILD_DIRTY={dirty}");
    println!("cargo:rustc-env=BEAD_VERSION_STRING={version_string}");

    // HEAD moves on commit/checkout; the index moves on stage/unstage, which
    // is what flips the dirty marker. Without both, a rebuilt binary can carry
    // a stale marker.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
}
