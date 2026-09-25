//! Consistency check for the plan's latest-tag/package-version claims
//! (beadrs-b7ddfc67).
//!
//! Revision 16 (commit `b5a5df8`) reconciled sections 0-8 of
//! `docs/plan/plan.md` with the audited tag and bead evidence: the plan now
//! states, normatively, that the latest tag is v0.2.6 at `d9a32b3` and that it
//! matches the package version declared in `Cargo.toml`. Nothing held that
//! reconciliation to the repository, so a later plan edit could quietly
//! contradict the tags or the manifest -- claiming both "vX tagged" and "no
//! matching tag exists", or citing a package version `Cargo.toml` does not
//! declare -- and the contradiction would sit in the normative section
//! unchallenged. This file is that check.
//!
//! Scope: the plan's own rule (section 0) makes sections 0-8 the normative
//! product and transition plan and demotes everything after them to a
//! historical appendix. The appendix legitimately still says "version 0.1
//! remains incomplete" and other stale things, so the scan window is exactly
//! the revision preamble plus sections 0-8: from the top of the file to the
//! first `## ` heading that follows the `## 8.` heading (the appendix
//! restarts its numbering at `## 1.` with no separator). Claims outside the
//! window are history, not contradictions.
//!
//! What is checked, in three layers:
//!
//! 1. *Internal consistency* (everywhere, including an exported tree): every
//!    explicit latest-tag assertion in the window must agree with every
//!    other; an absolute tag-existence denial ("no matching tag exists")
//!    must not coexist with a latest-tag assertion; a tag-existence denial
//!    scoped to the asserted version ("no v0.2.6 tag") must not coexist with
//!    asserting that same version; and a claimed tag version must equal the
//!    claimed package version, because the plan itself asserts they match.
//! 2. *Manifest cross-check* (everywhere): every `package version X.Y.Z`
//!    claim in the window must equal the version declared in `Cargo.toml`.
//! 3. *Tag cross-check* (real checkouts only): the asserted latest tag must
//!    exist, point at the cited commit, and be the semver-greatest `v*` tag.
//!
//! The tag cross-check needs `.git`, which a `git archive` extraction does
//! not carry. As in `build_from_archive_checkout_untouched.rs`, an absent
//! `.git` marker is only believed when git itself confirms the directory is
//! not inside any work tree; disagreement is a failure, not a skip. Layers 1
//! and 2 still run in an exported tree, so the packaged suite keeps checking
//! plan-vs-manifest consistency and every negative control below.
//!
//! The negative controls are in-process on purpose: synthetic contradictory
//! plans run through the *same* window, claim-extraction, and contradiction
//! functions the real plan goes through, so a refactor that blinds the
//! checker fails its own fixtures instead of passing vacuously. One control
//! pins the window boundary: a contradictory historical appendix must be
//! ignored.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;

const PLAN: &str = "docs/plan/plan.md";
const CARGO_TOML: &str = "Cargo.toml";

/// Tag-existence denial phrases that contradict any latest-tag assertion.
/// Scoped denials (naming a version) live in [`extract_claims`] instead,
/// because they only contradict when the named version is the one asserted.
const ABSOLUTE_DENIALS: [&str; 3] = [
    "no matching tag exists",
    "no such tag exists",
    "no tag exists",
];

/// The plan's normative window: the revision preamble plus sections 0-8.
///
/// Starts at the top of the file (the preamble carries tag claims too) and
/// ends immediately before the first `## ` heading that follows the `## 8.`
/// heading -- which is the historical appendix restarting its numbering at
/// `## 1.`, not a section 9.
fn normative_window(plan: &str) -> Result<&str, String> {
    if !plan.lines().any(|line| line.starts_with("## 0.")) {
        return Err(
            "plan has no `## 0.` heading; the normative window's start marker moved".to_string(),
        );
    }
    let mut saw_section_eight = false;
    let mut offset = 0usize;
    for line in plan.split_inclusive('\n') {
        if line.starts_with("## 8.") {
            saw_section_eight = true;
        } else if saw_section_eight && line.starts_with("## ") {
            return Ok(&plan[..offset]);
        }
        offset += line.len();
    }
    if saw_section_eight {
        // The file ends inside or directly after section 8, so the window is
        // the whole file.
        return Ok(plan);
    }
    Err("plan has no `## 8.` heading; the normative window's end marker moved".to_string())
}

/// The latest-tag and package-version claims found in a normative window.
#[derive(Debug, Default)]
struct PlanClaims {
    /// Every explicit latest-tag assertion: (version, cited commit prefix).
    latest_tag_assertions: Vec<(String, String)>,
    /// Every `package version X.Y.Z` mention.
    package_version_claims: Vec<String>,
    /// Absolute tag-existence denials, verbatim.
    absolute_denials: Vec<String>,
    /// Version-scoped denials: (denied version, verbatim phrase).
    scoped_denials: Vec<(String, String)>,
}

fn extract_claims(window: &str) -> PlanClaims {
    let latest_is = Regex::new(r"(?i)latest tag is v(\d+\.\d+\.\d+) at `([0-9a-f]{7,40})`")
        .expect("static regex");
    let latest_suffix =
        Regex::new(r"(?i)tag v(\d+\.\d+\.\d+) at `([0-9a-f]{7,40})` as the latest tag")
            .expect("static regex");
    let package_version = Regex::new(r"(?i)package version (\d+\.\d+\.\d+)").expect("static regex");
    let no_version_tag = Regex::new(r"(?i)no v(\d+\.\d+\.\d+) tag").expect("static regex");
    let not_tagged =
        Regex::new(r"(?i)\bv?(\d+\.\d+\.\d+) (?:has|have) not been tagged").expect("static regex");

    let mut claims = PlanClaims::default();
    for caps in latest_is.captures_iter(window) {
        claims
            .latest_tag_assertions
            .push((caps[1].to_string(), caps[2].to_string()));
    }
    for caps in latest_suffix.captures_iter(window) {
        claims
            .latest_tag_assertions
            .push((caps[1].to_string(), caps[2].to_string()));
    }
    for caps in package_version.captures_iter(window) {
        claims.package_version_claims.push(caps[1].to_string());
    }
    // The denial phrases are lowercase constants, matched against a
    // lowercased window so sentence position cannot hide them.
    let lowered = window.to_lowercase();
    for phrase in ABSOLUTE_DENIALS {
        if lowered.contains(phrase) {
            claims.absolute_denials.push(phrase.to_string());
        }
    }
    for caps in no_version_tag.captures_iter(window) {
        claims
            .scoped_denials
            .push((caps[1].to_string(), caps[0].to_string()));
    }
    for caps in not_tagged.captures_iter(window) {
        claims
            .scoped_denials
            .push((caps[1].to_string(), caps[0].to_string()));
    }
    claims
}

/// The repository state the plan's claims are checked against.
struct ActualState {
    package_version: String,
    /// `Some` only in a real checkout; an exported tree carries no `.git`.
    git: Option<GitState>,
}

#[derive(Debug)]
struct GitState {
    /// Every `v*` tag as (name, full commit sha it points at).
    tags: Vec<(String, String)>,
    /// The semver-greatest `vX.Y.Z` tag: (name, sha).
    latest: Option<(String, String)>,
}

impl GitState {
    fn from_tags(tags: Vec<(String, String)>) -> GitState {
        let latest = tags
            .iter()
            .filter(|(name, _)| is_release_tag(name))
            .max_by(|a, b| version_key(&a.0).cmp(&version_key(&b.0)))
            .cloned();
        GitState { tags, latest }
    }

    fn discover(root: &Path) -> GitState {
        let listing = git(root, &["tag", "--list", "v*"]);
        let tags = listing
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| {
                let commit = git(
                    root,
                    &["rev-parse", "--verify", &format!("{name}^{{commit}}")],
                );
                (name.to_string(), commit)
            })
            .collect();
        GitState::from_tags(tags)
    }
}

/// Whether `name` looks like a plain `vX.Y.Z` release tag.
fn is_release_tag(name: &str) -> bool {
    name.strip_prefix('v').is_some_and(|rest| {
        !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == '.')
    })
}

/// Numeric semver ordering: "v0.2.10" must outrank "v0.2.6", which plain
/// string comparison gets backwards.
fn version_key(tag: &str) -> Vec<u64> {
    tag.strip_prefix('v')
        .unwrap_or(tag)
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

/// Whether two cited commit ids can name the same commit: one must be a
/// case-insensitive prefix of the other (short sha vs full sha).
fn commits_agree(a: &str, b: &str) -> bool {
    let (a, b) = (a.to_ascii_lowercase(), b.to_ascii_lowercase());
    a.starts_with(&b) || b.starts_with(&a)
}

/// Every way the plan's normative window contradicts itself or the actual
/// repository state. Empty means consistent.
fn contradictions(plan_text: &str, actual: &ActualState) -> Vec<String> {
    let mut problems = Vec::new();
    let window = match normative_window(plan_text) {
        Ok(window) => window,
        Err(err) => {
            problems.push(err);
            return problems;
        }
    };
    let claims = extract_claims(window);

    // Explicit latest-tag assertions must agree with one another: two
    // different "latest" tags in one normative window is exactly the
    // contradiction class this check exists for.
    for pair in claims.latest_tag_assertions.windows(2) {
        let (a_version, a_sha) = &pair[0];
        let (b_version, b_sha) = &pair[1];
        if a_version != b_version || !commits_agree(a_sha, b_sha) {
            problems.push(format!(
                "normative sections 0-8 assert two different latest tags: \
                 v{a_version} at `{a_sha}` and v{b_version} at `{b_sha}`"
            ));
        }
    }

    // Absolute denials contradict an explicit assertion, and in a real
    // checkout they contradict the repository itself.
    for denial in &claims.absolute_denials {
        if let Some((version, sha)) = claims.latest_tag_assertions.first() {
            problems.push(format!(
                "normative sections 0-8 both assert latest tag v{version} at \
                 `{sha}` and deny any tag exists ({denial:?})"
            ));
        }
        if let Some(git) = &actual.git {
            if !git.tags.is_empty() {
                problems.push(format!(
                    "normative sections 0-8 claim no tag exists ({denial:?}), \
                     but the repository carries {} v* tags",
                    git.tags.len()
                ));
            }
        }
    }

    // A version-scoped denial only contradicts when it names the asserted
    // version or a tag the repository actually has: "0.2.7 has not been
    // tagged yet" next to "the latest tag is v0.2.6" is true, not broken.
    for (denied, denial) in &claims.scoped_denials {
        if let Some((version, sha)) = claims.latest_tag_assertions.first() {
            if version == denied {
                problems.push(format!(
                    "normative sections 0-8 both assert latest tag v{version} \
                     at `{sha}` and deny that tag exists ({denial:?})"
                ));
            }
        }
        if let Some(git) = &actual.git {
            if git
                .tags
                .iter()
                .any(|(name, _)| name == &format!("v{denied}"))
            {
                problems.push(format!(
                    "normative sections 0-8 deny tag v{denied} exists \
                     ({denial:?}), but the repository carries it"
                ));
            }
        }
    }

    // Tag cross-check: real checkouts only.
    if let Some(git) = &actual.git {
        for (version, sha) in &claims.latest_tag_assertions {
            let tag_name = format!("v{version}");
            let Some((_, commit)) = git.tags.iter().find(|(name, _)| name == &tag_name) else {
                problems.push(format!(
                    "normative sections 0-8 cite {tag_name} as the latest \
                     tag, but no such tag exists in the repository"
                ));
                continue;
            };
            if !commit.starts_with(&sha.to_ascii_lowercase()) {
                problems.push(format!(
                    "normative sections 0-8 cite {tag_name} at `{sha}`, but \
                     the tag points at `{commit}`"
                ));
            }
            if let Some((latest_name, _)) = &git.latest {
                if latest_name != &tag_name {
                    problems.push(format!(
                        "normative sections 0-8 cite {tag_name} as the latest \
                         tag, but the repository's newest v* tag is {latest_name}"
                    ));
                }
            }
        }
    }

    // Every package-version claim must match the manifest.
    for claimed in &claims.package_version_claims {
        if claimed != &actual.package_version {
            problems.push(format!(
                "normative sections 0-8 cite package version {claimed}, but \
                 Cargo.toml declares {}",
                actual.package_version
            ));
        }
    }

    // A claimed latest tag and a claimed package version must be the same
    // version: the plan's own rule (section 1) is that the tag matches the
    // package version declared in Cargo.toml.
    if let (Some((tag_version, _)), Some(package_version)) = (
        claims.latest_tag_assertions.first(),
        claims.package_version_claims.first(),
    ) {
        if tag_version != package_version {
            problems.push(format!(
                "normative sections 0-8 cite latest tag v{tag_version} and \
                 package version {package_version}; the plan asserts they match"
            ));
        }
    }

    problems
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn declared_package_version(root: &Path) -> String {
    let manifest = fs::read_to_string(root.join(CARGO_TOML)).expect("read Cargo.toml");
    let line = manifest
        .lines()
        .find(|line| line.starts_with("version = "))
        .expect("Cargo.toml declares a package version");
    line.split('"')
        .nth(1)
        .expect("package version is a quoted string")
        .to_string()
}

enum CheckoutContext {
    /// `.git` is present, so the tag cross-check applies.
    Real,
    /// An exported tree -- a `git archive` extraction or an unpacked
    /// package. There is no tag state to query, so that layer is skipped.
    ExportedTree,
}

/// Whether the tag cross-check applies to `root`.
fn checkout_context(root: &Path) -> CheckoutContext {
    if root.join(".git").exists() {
        return CheckoutContext::Real;
    }
    let discovery = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .expect("spawn git");
    if discovery.status.success() {
        let resolved = String::from_utf8_lossy(&discovery.stdout).trim().to_owned();
        panic!(
            "{} has no .git marker but git resolved a work tree from it \
             (is-inside-work-tree = {resolved:?}); the tag probes would query \
             a repository this test did not select. Move this exported tree \
             out from inside any enclosing repository instead of skipping.",
            root.display()
        );
    }
    println!(
        "skipping tag cross-check: {} is an exported tree -- no .git marker \
         and git confirms it is not inside a work tree (expected for cargo \
         package and git-archive verification); plan-vs-manifest checks and \
         all negative controls still run",
        root.display()
    );
    CheckoutContext::ExportedTree
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed in {}: {}",
        root.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git stdout is utf-8")
        .trim()
        .to_string()
}

/// The positive control: the reconciled plan must be consistent with the
/// repository it ships in.
#[test]
fn plan_latest_tag_claims_match_repository() {
    let root = repo_root();
    let plan_text = fs::read_to_string(root.join(PLAN)).expect("read plan");
    let git_state = match checkout_context(&root) {
        CheckoutContext::Real => Some(GitState::discover(&root)),
        CheckoutContext::ExportedTree => None,
    };
    let actual = ActualState {
        package_version: declared_package_version(&root),
        git: git_state,
    };

    // The window has to be the normative sections, not the whole file: pin
    // both ends so the scan cannot silently widen onto the historical
    // appendix or collapse onto nothing.
    let window = normative_window(&plan_text).expect("plan exposes a normative window");
    assert!(
        window.contains("## 0. How to read this plan"),
        "normative window lost its start marker"
    );
    assert!(
        !window.contains("Authority and clean-room boundary"),
        "normative window swallowed the historical appendix"
    );

    // Vacuity guard: the current plan states the claim, so the checker must
    // actually find it. If a future revision phrases it differently, either
    // restore the recognised shape or teach `extract_claims` the new one --
    // a silent pass with nothing extracted is the failure mode this test
    // exists to prevent.
    let claims = extract_claims(window);
    assert!(
        !claims.latest_tag_assertions.is_empty(),
        "the normative sections state no latest-tag claim the checker \
         recognises; restore the `The latest tag is vX.Y.Z at \\`sha\\`` form \
         or teach extract_claims the new shape"
    );
    assert!(
        !claims.package_version_claims.is_empty(),
        "the normative sections state no package-version claim the checker \
         recognises; restore the `package version X.Y.Z` form or teach \
         extract_claims the new shape"
    );

    let problems = contradictions(&plan_text, &actual);
    assert!(
        problems.is_empty(),
        "docs/plan/plan.md sections 0-8 contradict the repository:\n  - {}",
        problems.join("\n  - ")
    );
}

/// A minimal plan with the skeleton the checker keys on: a preamble, the
/// `## 0.` start marker, the `## 8.` end marker, and -- when the caller
/// appends one -- the first appendix heading that closes the window.
fn fixture_plan(normative_body: &str) -> String {
    format!(
        "# bead-rs Current Product and Software Factory Plan\n\
         \n\
         Plan revision: 99\n\
         \n\
         Status: synthetic fixture for the consistency checker.\n\
         \n\
         ## 0. How to read this plan\n\
         \n\
         Sections 0-8 are the current normative product and transition plan.\n\
         \n\
         ## 8. Combined-factory success measures\n\
         \n\
         {normative_body}\n"
    )
}

/// A synthetic repository state matching the plan's public claim: package
/// 0.2.6, tags v0.2.6 at `d9a32b3...` and v0.2.4 at `bbbbbbb0...`.
fn state_with_tags(tags: &[(&str, &str)]) -> ActualState {
    ActualState {
        package_version: "0.2.6".to_string(),
        git: Some(GitState::from_tags(
            tags.iter()
                .map(|(name, commit)| (name.to_string(), commit.to_string()))
                .collect(),
        )),
    }
}

fn fixture_state() -> ActualState {
    state_with_tags(&[
        ("v0.2.6", "d9a32b300000000000000000000000000000000"),
        ("v0.2.4", "bbbbbbb000000000000000000000000000000000"),
    ])
}

/// The negative controls: every contradiction class the bead names must fail
/// the checker, and each control must fail *for the named reason*, not for
/// any incidental one. All controls run through the same window/extraction/
/// contradiction path as the real plan.
#[test]
fn checker_rejects_synthetic_contradictions() {
    let consistent = "The latest tag is v0.2.6 at `d9a32b3`; it matches the \
                      package version 0.2.6 declared in `Cargo.toml`.";

    let mut v0210_state = fixture_state();
    v0210_state
        .git
        .as_mut()
        .expect("fixture state has git tags")
        .tags
        .push((
            "v0.2.10".to_string(),
            "ccccccc000000000000000000000000000000000".to_string(),
        ));
    v0210_state
        .git
        .as_mut()
        .expect("fixture state has git tags")
        .latest = Some((
        "v0.2.10".to_string(),
        "ccccccc000000000000000000000000000000000".to_string(),
    ));

    let cases: Vec<(&str, String, ActualState, Vec<&str>)> = vec![
        (
            "asserted latest tag and absolute existence denial",
            fixture_plan(&format!("{consistent} However, no matching tag exists.")),
            fixture_state(),
            vec!["deny any tag exists"],
        ),
        (
            "asserted latest tag and version-scoped denial of that same tag",
            fixture_plan(&format!("{consistent} Yet no v0.2.6 tag exists.")),
            fixture_state(),
            vec!["deny that tag exists"],
        ),
        (
            "latest tag claim the repository does not carry",
            fixture_plan(
                "The latest tag is v0.2.7 at `aaaaaaa0`; it matches the \
                          package version 0.2.7 declared in `Cargo.toml`.",
            ),
            fixture_state(),
            vec!["no such tag exists in the repository"],
        ),
        (
            "latest tag claim at the wrong commit",
            fixture_plan(
                "The latest tag is v0.2.6 at `aaaaaaaa`; it matches the \
                          package version 0.2.6 declared in `Cargo.toml`.",
            ),
            fixture_state(),
            vec!["the tag points at"],
        ),
        (
            "package version inconsistent with Cargo.toml",
            fixture_plan(
                "The latest tag is v0.2.6 at `d9a32b3`; the package \
                          version 0.2.7 is declared in `Cargo.toml`.",
            ),
            ActualState {
                package_version: "0.2.6".to_string(),
                git: None,
            },
            vec!["but Cargo.toml declares 0.2.6"],
        ),
        (
            "tag version and package version disagree",
            fixture_plan(
                "The latest tag is v0.2.4 at `bbbbbbb0`; it matches the \
                          package version 0.2.6 declared in `Cargo.toml`.",
            ),
            fixture_state(),
            vec!["the plan asserts they match"],
        ),
        (
            "semver-greatest tag outranks the claimed one (v0.2.10 vs v0.2.6)",
            fixture_plan(consistent),
            v0210_state,
            vec!["newest v* tag is v0.2.10"],
        ),
    ];

    let mut failures = Vec::new();
    for (name, plan, state, expected) in &cases {
        let problems = contradictions(plan, state);
        for wanted in expected {
            if !problems.iter().any(|problem| problem.contains(wanted)) {
                failures.push(format!(
                    "{name}: expected a contradiction containing {wanted:?}; \
                     the checker produced {problems:?}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "negative controls did not fail as required:\n  - {}",
        failures.join("\n  - ")
    );
}

/// The controls that keep the negative controls honest: a fully consistent
/// synthetic plan passes, and a contradictory *historical appendix* -- the
/// text the plan's own section-0 rule puts outside the normative window --
/// is ignored rather than flagged.
#[test]
fn checker_accepts_consistent_plan_and_ignores_historical_appendix() {
    let state = fixture_state();
    let consistent = "The latest tag is v0.2.6 at `d9a32b3`; it matches the \
                      package version 0.2.6 declared in `Cargo.toml`.";

    let bare = fixture_plan(consistent);
    assert!(
        contradictions(&bare, &state).is_empty(),
        "the checker rejects a fully consistent normative plan"
    );

    let mut with_appendix = fixture_plan(consistent);
    with_appendix.push_str(
        "## 1. Authority and clean-room boundary\n\n\
         Historical: no matching tag exists, version 0.1 remains incomplete,\n\
         and the package version 9.9.9 is declared in `Cargo.toml`.\n",
    );
    assert!(
        contradictions(&with_appendix, &state).is_empty(),
        "the checker flagged text inside the historical appendix, which \
         section 0 puts outside the normative window"
    );
}
