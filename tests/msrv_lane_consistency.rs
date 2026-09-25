//! Consistency check for the pinned Rust MSRV verification lane
//! (beadrs-01e75a0c).
//!
//! `Cargo.toml` declares `rust-version` (the MSRV, currently 1.85; see
//! ADR-004). The promise is verified by a pinned lane in the `bead-rs-ci`
//! pipeline -- `cargo +1.85 check --all-targets` -- and ADR-004's rule is
//! that "drift between them is a CI failure, not a doc nit". Three
//! artifacts hardcode the toolchain that lane runs, and nothing held them
//! to the manifest from this side of the repository boundary:
//!
//! 1. `containers/bead-rs-ci-builder/Dockerfile` installs the MSRV
//!    toolchain under the exact name the lane resolves (`ARG RUST_MSRV`)
//!    and build-time-verifies that it really is that rustc
//!    (`"rustc <msrv>."*` case pattern).
//! 2. The `bead-rs-ci` WorkflowTemplate -- which lives in a separate
//!    repository (`declarative-config/k8s/iad-ci/argo-workflows/`) -- pins
//!    `MSRV=<version>` and runs `cargo "+${MSRV}" check --all-targets`
//!    after re-deriving `rust-version` from `Cargo.toml` and failing the
//!    run on a mismatch.
//!
//! A manifest bump (say 1.85 -> 1.86) that forgets either artifact would
//! fail CI at run time, but only after a queued run, a mutex wait and a
//! cold-or-warm compile -- and the Dockerfile half would additionally wait
//! for an image rebuild to surface. This file is the check that fires
//! where the edit actually happens.
//!
//! What is checked, in two layers:
//!
//! 1. *Builder image* (everywhere, including an exported tree): the
//!    Dockerfile's `ARG RUST_MSRV` pin and its build-time rustc assertion
//!    must equal the declared MSRV.
//! 2. *WorkflowTemplate* (only when a sibling `declarative-config` checkout
//!    exists next to this repository): its `MSRV=` pin, its MSRV lane
//!    command, and its run-time declared-vs-pinned guard must agree with
//!    the declared MSRV. The iad-ci container clones this repository alone,
//!    and a `git archive` extraction has no sibling at all, so the layer
//!    skips there -- which is exactly where the template's own run-time
//!    guard (layer 2's subject matter) takes over. On a development box,
//!    where both checkouts sit side by side, the cross-repo check runs.
//!
//! The negative controls are in-process on purpose: synthetic drifted
//! Dockerfiles and templates run through the *same* extraction and drift
//! functions the real artifacts go through, so a refactor that blinds the
//! checker fails its own fixtures instead of passing vacuously. Each
//! control must fail *for the named reason*, not for any incidental one.

use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

const CARGO_TOML: &str = "Cargo.toml";
const BUILDER_DOCKERFILE: &str = "containers/bead-rs-ci-builder/Dockerfile";
/// The WorkflowTemplate lives in the sibling `declarative-config` checkout,
/// not in this repository.
const WORKFLOW_TEMPLATE: &str =
    "../declarative-config/k8s/iad-ci/argo-workflows/bead-rs-ci-workflowtemplate.yml";

/// The MSRV toolchain pins extracted from the builder Dockerfile.
#[derive(Debug, Default)]
struct DockerfilePins {
    /// Every `ARG RUST_MSRV=<value>` assignment.
    toolchain_args: Vec<String>,
    /// Every build-time `"rustc <major>.<minor>."` assertion version.
    rustc_case_versions: Vec<String>,
}

fn dockerfile_pins(dockerfile: &str) -> DockerfilePins {
    let arg = Regex::new(r"(?m)^\s*ARG\s+RUST_MSRV=(\S+)\s*$").expect("static regex");
    let rustc_case = Regex::new(r#""rustc ([0-9]+\.[0-9]+)\.""#).expect("static regex");

    DockerfilePins {
        toolchain_args: arg
            .captures_iter(dockerfile)
            .map(|caps| caps[1].to_string())
            .collect(),
        rustc_case_versions: rustc_case
            .captures_iter(dockerfile)
            .map(|caps| caps[1].to_string())
            .collect(),
    }
}

/// The MSRV lane shape extracted from the `bead-rs-ci` WorkflowTemplate.
#[derive(Debug, Default)]
struct TemplateLane {
    /// Every `MSRV=<version>` pin.
    msrv_pins: Vec<String>,
    /// The toolchain every MSRV lane command runs: `"${MSRV}"` for the
    /// parameterized form, or the literal version for `cargo +X.Y ...`.
    lane_commands: Vec<String>,
    /// Whether the template still re-derives `rust-version` from Cargo.toml
    /// and fails the run when it differs from the pinned MSRV.
    has_run_time_guard: bool,
}

fn template_lane(template: &str) -> TemplateLane {
    let pin = Regex::new(r"(?m)^\s*MSRV=([0-9]+\.[0-9]+(?:\.[0-9]+)?)\s*$").expect("static regex");
    let parameterized =
        Regex::new(r#"cargo\s+"\+\$\{MSRV\}"\s+check\s+--all-targets"#).expect("static regex");
    let literal = Regex::new(r"cargo\s+\+([0-9]+\.[0-9]+(?:\.[0-9]+)?)\s+check\s+--all-targets")
        .expect("static regex");

    let mut lane_commands: Vec<String> = parameterized
        .find_iter(template)
        .map(|_| "\"${MSRV}\"".to_string())
        .collect();
    lane_commands.extend(
        literal
            .captures_iter(template)
            .map(|caps| caps[1].to_string()),
    );

    TemplateLane {
        msrv_pins: pin
            .captures_iter(template)
            .map(|caps| caps[1].to_string())
            .collect(),
        lane_commands,
        has_run_time_guard: template.contains("DECLARED_MSRV=")
            && template.contains("rust-version")
            && template.contains(r#""$DECLARED_MSRV" != "$MSRV""#),
    }
}

/// Every way the builder Dockerfile's MSRV pins can drift from the declared
/// MSRV. Empty means consistent.
fn dockerfile_drift(declared: &str, pins: &DockerfilePins) -> Vec<String> {
    let mut problems = Vec::new();

    match pins.toolchain_args.as_slice() {
        [] => problems.push(
            "builder Dockerfile carries no `ARG RUST_MSRV` pin; the MSRV \
             toolchain the lane resolves is no longer declared anywhere"
                .to_string(),
        ),
        [one] if one != declared => problems.push(format!(
            "builder Dockerfile installs the MSRV toolchain as {one}, but \
             Cargo.toml declares {declared}"
        )),
        [_, ..] if pins.toolchain_args.len() > 1 => problems.push(format!(
            "builder Dockerfile declares {} conflicting ARG RUST_MSRV pins: {:?}",
            pins.toolchain_args.len(),
            pins.toolchain_args
        )),
        _ => {}
    }

    match pins.rustc_case_versions.as_slice() {
        [] => problems.push(
            "builder Dockerfile build-time verification no longer asserts the \
             MSRV rustc version (`\"rustc <msrv>.\"*` case pattern moved or \
             vanished)"
                .to_string(),
        ),
        versions if versions.iter().any(|version| version != declared) => problems.push(format!(
            "builder Dockerfile build-time verification asserts rustc \
             {versions:?}, but Cargo.toml declares {declared}"
        )),
        _ => {}
    }

    problems
}

/// Every way the WorkflowTemplate's MSRV lane can drift from the declared
/// MSRV. Empty means consistent.
fn template_drift(declared: &str, lane: &TemplateLane) -> Vec<String> {
    let mut problems = Vec::new();

    match lane.msrv_pins.as_slice() {
        [] => problems.push("WorkflowTemplate carries no `MSRV=` pin".to_string()),
        [one] if one != declared => problems.push(format!(
            "WorkflowTemplate pins CI MSRV {one}, but Cargo.toml declares {declared}"
        )),
        [_, ..] if lane.msrv_pins.len() > 1 => problems.push(format!(
            "WorkflowTemplate declares {} conflicting MSRV= pins: {:?}",
            lane.msrv_pins.len(),
            lane.msrv_pins
        )),
        _ => {}
    }

    if lane.lane_commands.is_empty() {
        problems.push(format!(
            "WorkflowTemplate has no `cargo +{declared} check --all-targets` \
             MSRV lane command"
        ));
    }
    for command in &lane.lane_commands {
        // The parameterized form is checked through the pin equality above;
        // a literal toolchain name has to match the manifest on its own.
        if command != "\"${MSRV}\"" && command != declared {
            problems.push(format!(
                "WorkflowTemplate MSRV lane runs cargo +{command}, but \
                 Cargo.toml declares {declared}"
            ));
        }
    }

    if !lane.has_run_time_guard {
        problems.push(
            "WorkflowTemplate no longer re-derives Cargo.toml rust-version \
             against the pinned MSRV at run time (`DECLARED_MSRV` guard moved \
             or vanished)"
                .to_string(),
        );
    }

    problems
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The MSRV declared in Cargo.toml, extracted the same way the
/// WorkflowTemplate's run-time guard extracts it: the first
/// `rust-version = "..."` line, quoted value.
fn declared_msrv(root: &Path) -> String {
    let manifest = fs::read_to_string(root.join(CARGO_TOML)).expect("read Cargo.toml");
    let line = manifest
        .lines()
        .find(|line| line.starts_with("rust-version"))
        .expect("Cargo.toml declares a rust-version (MSRV); the manifest moved");
    let declared = line
        .split('"')
        .nth(1)
        .expect("rust-version is a quoted string")
        .to_string();
    assert!(
        declared.split('.').count() >= 2,
        "declared MSRV {declared:?} is not a `<major>.<minor>` toolchain name"
    );
    declared
}

/// The positive control: the builder image pins -- and, where a sibling
/// declarative-config checkout exists, the WorkflowTemplate lane -- must
/// agree with the MSRV Cargo.toml declares.
#[test]
fn declared_msrv_matches_builder_image_and_ci_lane() {
    let root = repo_root();
    let declared = declared_msrv(&root);

    let dockerfile =
        fs::read_to_string(root.join(BUILDER_DOCKERFILE)).expect("read builder Dockerfile");
    let mut problems = dockerfile_drift(&declared, &dockerfile_pins(&dockerfile));

    let template_path = root.join(WORKFLOW_TEMPLATE);
    match fs::read_to_string(&template_path) {
        Ok(template) => problems.extend(template_drift(&declared, &template_lane(&template))),
        Err(_) => println!(
            "skipping WorkflowTemplate layer: no sibling declarative-config \
             checkout at {} (expected for the iad-ci container, which clones \
             this repository alone, and for git-archive extractions); the \
             template's own run-time guard covers that environment, and all \
             Dockerfile checks plus every negative control still run",
            template_path.display()
        ),
    }

    assert!(
        problems.is_empty(),
        "the pinned MSRV lane drifted from Cargo.toml's rust-version \
         ({declared}):\n  - {}",
        problems.join("\n  - ")
    );
}

/// Why the MSRV toolchain line is installed in a fixture Dockerfile.
fn fixture_dockerfile(msrv_arg: Option<&str>, rustc_case: Option<&str>) -> String {
    let arg = msrv_arg
        .map(|version| format!("ARG RUST_MSRV={version}\n"))
        .unwrap_or_default();
    let case = rustc_case
        .map(|version| {
            format!(
                "    && case \"$(rustup run ${{RUST_MSRV}} rustc --version)\" in \
                 \"rustc {version}.\"*) ;; *) echo mismatch >&2; exit 1 ;; esac\n"
            )
        })
        .unwrap_or_default();
    format!("FROM debian:bookworm-slim\n{arg}RUN rustup toolchain install stable\n{case}")
}

/// The MSRV lane invocation a fixture template carries.
enum FixtureLane {
    /// `cargo "+${MSRV}" check --all-targets` -- the form the real template
    /// uses, resolved through the `MSRV=` pin.
    Parameterized,
    /// `cargo +<version> check --all-targets` with a literal toolchain.
    Literal(String),
    /// No lane command at all.
    None,
}

fn fixture_template(msrv_pin: Option<&str>, lane: FixtureLane, run_time_guard: bool) -> String {
    let pin = msrv_pin
        .map(|version| format!("MSRV={version}\n"))
        .unwrap_or_default();
    let guard = run_time_guard.then_some(
        "DECLARED_MSRV=$(grep -m1 '^rust-version[[:space:]]*=' Cargo.toml | sed \
         's/.*= *\"\\([^\"]*\\)\".*/\\1/')\n\
         if [ \"$DECLARED_MSRV\" != \"$MSRV\" ]; then\n  exit 1\nfi\n",
    );
    let command = match lane {
        FixtureLane::Parameterized => "cargo \"+${MSRV}\" check --all-targets\n".to_string(),
        FixtureLane::Literal(version) => format!("cargo +{version} check --all-targets\n"),
        FixtureLane::None => String::new(),
    };
    format!("{pin}{}{command}", guard.unwrap_or_default())
}

/// The negative controls: every drift class this test exists for must fail
/// the checker, and each control must fail *for the named reason*, not for
/// any incidental one. All controls run through the same extraction and
/// drift path as the real artifacts.
#[test]
fn checker_rejects_drifted_fixtures() {
    let declared = "1.85";

    let dockerfile_cases: Vec<(&str, String, Vec<&str>)> = vec![
        (
            "builder ARG pin bumped past the manifest",
            fixture_dockerfile(Some("1.86"), Some("1.85")),
            vec!["installs the MSRV toolchain as 1.86, but Cargo.toml declares 1.85"],
        ),
        (
            "builder ARG pin removed",
            fixture_dockerfile(None, Some("1.85")),
            vec!["carries no `ARG RUST_MSRV` pin"],
        ),
        (
            "build-time rustc assertion left stale after a manifest bump",
            fixture_dockerfile(Some("1.85"), Some("1.86")),
            vec!["build-time verification asserts rustc"],
        ),
        (
            "build-time rustc assertion removed",
            fixture_dockerfile(Some("1.85"), None),
            vec!["no longer asserts the MSRV rustc version"],
        ),
    ];
    for (name, dockerfile, wanted) in &dockerfile_cases {
        let pins = dockerfile_pins(dockerfile);
        let problems = dockerfile_drift(declared, &pins);
        for reason in wanted {
            assert!(
                problems.iter().any(|problem| problem.contains(reason)),
                "{name}: expected drift containing {reason:?}; the checker \
                 produced {problems:?}"
            );
        }
    }

    let template_cases: Vec<(&str, String, Vec<&str>)> = vec![
        (
            "template MSRV pin bumped past the manifest",
            fixture_template(Some("1.86"), FixtureLane::Parameterized, true),
            vec!["pins CI MSRV 1.86, but Cargo.toml declares 1.85"],
        ),
        (
            "template MSRV pin removed",
            fixture_template(None, FixtureLane::Parameterized, true),
            vec!["carries no `MSRV=` pin"],
        ),
        (
            "template lane command removed",
            fixture_template(Some("1.85"), FixtureLane::None, true),
            vec!["has no `cargo +1.85 check --all-targets` MSRV lane command"],
        ),
        (
            "template lane pinned to a literal toolchain the manifest disowns",
            fixture_template(Some("1.85"), FixtureLane::Literal("1.90".to_string()), true),
            vec!["MSRV lane runs cargo +1.90, but Cargo.toml declares 1.85"],
        ),
        (
            "template run-time declared-vs-pinned guard removed",
            fixture_template(Some("1.85"), FixtureLane::Parameterized, false),
            vec!["no longer re-derives Cargo.toml rust-version"],
        ),
    ];
    for (name, template, wanted) in &template_cases {
        let lane = template_lane(template);
        let problems = template_drift(declared, &lane);
        for reason in wanted {
            assert!(
                problems.iter().any(|problem| problem.contains(reason)),
                "{name}: expected drift containing {reason:?}; the checker \
                 produced {problems:?}"
            );
        }
    }
}

/// The controls that keep the negative controls honest: consistent builder
/// and template fixtures pass -- in both lane-command spellings the checker
/// understands, the parameterized form the real template uses and a literal
/// `cargo +1.85 check --all-targets`.
#[test]
fn checker_accepts_consistent_fixtures() {
    let declared = "1.85";

    let dockerfile = fixture_dockerfile(Some(declared), Some(declared));
    assert!(
        dockerfile_drift(declared, &dockerfile_pins(&dockerfile)).is_empty(),
        "the checker rejects a builder Dockerfile consistent with the manifest"
    );

    let parameterized = fixture_template(Some(declared), FixtureLane::Parameterized, true);
    assert!(
        template_drift(declared, &template_lane(&parameterized)).is_empty(),
        "the checker rejects a WorkflowTemplate whose pin and parameterized \
         lane match the manifest"
    );

    let literal = fixture_template(
        Some(declared),
        FixtureLane::Literal(declared.to_string()),
        true,
    );
    assert!(
        template_drift(declared, &template_lane(&literal)).is_empty(),
        "the checker rejects a WorkflowTemplate whose pin and literal lane \
         command match the manifest"
    );
}
