//! Concurrency and recovery paths across the pinned binary variants.
//!
//! The sibling suites each cover one of these surfaces against a single
//! binary: `checkpoint_publication_lock` serializes concurrent *publishers*,
//! `post_commit_publication` pins suppression and publication failure at
//! HEAD, `r002`/`r003` exercise lease and revision fencing in one process,
//! and `r036` restores sharded and monolithic generations built by the
//! cargo-built binary. This suite closes the remaining gaps, and drives
//! every scenario through the pinned executables themselves so the
//! guarantees hold for a consumer still running either pin:
//!
//! - **Concurrent replay**: several workers replaying one checkpoint into
//!   parallel targets must produce identical graphs, and several workers
//!   replaying into one target must leave exactly-once state with clean
//!   refusals for the losers;
//! - **Stale fencing**: a revision or fencing token handed out before
//!   another writer committed must be rejected, and the invalidated writer
//!   must be able to re-fence and proceed;
//! - **Checkpoint resilience**: suppression, publication failure, a
//!   corrupted pointer, and a damaged generation must all be reported
//!   honestly and be recoverable by the documented remedies;
//! - **Restore modes**: sharded and monolithic checkpoints -- including
//!   ones written while publication was suppressed -- restore through both
//!   the `restore` command and `sync import-only`.
//!
//! Deliberately self-contained: pin resolution re-implements the registry
//! lookup (as `needle_variant_dispatch_paths.rs` does) so this suite states
//! its own preconditions and stays decoupled from `capability_framework`'s
//! rework. Every contract asserted here was verified against both pins
//! before being written down.

use serial_test::serial;
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Registry role whose tree predates attempt-resolution entirely (release 0.2.4)
const PRE_FEATURE_ROLE: &str = "pre_feature";
/// Registry role carrying the attempt-resolution surface (0.2.6 pin)
const FEATURE_ENABLED_ROLE: &str = "attempt_resolution_f25ab5c";

/// How long any single spawned worker may run before the test fails rather
/// than hangs
const WORKER_TIMEOUT: Duration = Duration::from_secs(60);

/// A pinned binary resolved from `pinned-binaries/commits.json`, verified
/// against its recorded provenance before any test drives it
#[derive(Clone)]
struct Variant {
    role: &'static str,
    binary: PathBuf,
    version: String,
}

impl Variant {
    fn run<S: AsRef<OsStr>>(&self, args: &[S], workspace: &Path) -> Output {
        Command::new(&self.binary)
            .args(args)
            .current_dir(workspace)
            .output()
            .expect("pinned binary must be executable")
    }

    /// Run a command that must succeed, failing the test with its stderr
    fn run_ok<S: AsRef<OsStr>>(&self, args: &[S], workspace: &Path) -> Output {
        let out = self.run(args, workspace);
        let shown: Vec<String> = args
            .iter()
            .map(|arg| arg.as_ref().to_string_lossy().into_owned())
            .collect();
        assert!(
            out.status.success(),
            "{:?} failed on {}: {}",
            shown,
            self.role,
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }

    /// Spawn without waiting, so a test can race several workers
    fn spawn<S: AsRef<OsStr>>(&self, args: &[S], workspace: &Path) -> Child {
        Command::new(&self.binary)
            .args(args)
            .current_dir(workspace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("pinned binary must be spawnable")
    }

    fn stdout(&self, output: &Output) -> String {
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    fn stderr(&self, output: &Output) -> String {
        String::from_utf8_lossy(&output.stderr).to_string()
    }
}

fn pinned_binaries_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("pinned-binaries")
}

/// Resolve a registry role to its on-disk pin, checking the embedded version
/// and the sha256 of the bytes against the pin's metadata
fn resolve_variant(role: &'static str) -> Variant {
    let registry: serde_json::Value =
        serde_json::from_slice(&std::fs::read(pinned_binaries_dir().join("commits.json")).unwrap())
            .unwrap();
    let name = registry[role]["binary_name"].as_str().unwrap_or_else(|| {
        panic!("pin role '{role}' has no binary_name in pinned-binaries/commits.json")
    });
    let binary = pinned_binaries_dir().join(name);
    assert!(binary.is_file(), "pin role '{role}' missing on disk");

    let meta: serde_json::Value = serde_json::from_slice(
        &std::fs::read(pinned_binaries_dir().join(format!("{name}.metadata.json"))).unwrap(),
    )
    .unwrap();

    // Byte identity: a silently swapped pin must fail here, not downstream
    let digest = hex::encode(Sha256::digest(std::fs::read(&binary).unwrap()));
    assert_eq!(
        digest,
        meta["binary_sha256"].as_str().unwrap(),
        "pin '{role}' bytes do not match the recorded sha256"
    );

    let scratch = scratch_dir("version");
    let version = String::from_utf8_lossy(
        &Command::new(&binary)
            .args(["--version"])
            .current_dir(scratch.path())
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    if let Some(recorded) = meta["embedded_version_string"].as_str() {
        assert_eq!(
            version, recorded,
            "pin '{role}' reports a version its metadata does not record"
        );
    }

    Variant {
        role,
        binary,
        version,
    }
}

/// Both variants of record, provenance-checked once per process
fn variants() -> &'static [Variant; 2] {
    static PAIR: OnceLock<[Variant; 2]> = OnceLock::new();
    PAIR.get_or_init(|| {
        let pair = [
            resolve_variant(PRE_FEATURE_ROLE),
            resolve_variant(FEATURE_ENABLED_ROLE),
        ];
        assert_ne!(
            pair[0].version, pair[1].version,
            "both sides of the matrix resolved to the same build: the suite \
             would test one pin twice"
        );
        pair
    })
}

/// Disposable workspace; /var/tmp so no ancestor carries a foreign `.beads`
fn scratch_dir(tag: &str) -> TempDir {
    tempfile::Builder::new()
        .prefix(&format!("bead-conc-recovery-{tag}-"))
        .tempdir_in("/var/tmp")
        .unwrap()
}

fn init_workspace(variant: &Variant, prefix: &str) -> TempDir {
    let dir = scratch_dir(prefix);
    variant.run_ok(&["init", "--prefix", prefix], dir.path());
    dir
}

/// Create one P2 task and return its id
fn create_bead(variant: &Variant, workspace: &Path, title: &str) -> String {
    String::from_utf8_lossy(
        &variant
            .run_ok(&["create", "--title", title], workspace)
            .stdout,
    )
    .trim()
    .to_string()
}

/// `sync status --format json` parsed from stdout
fn status(variant: &Variant, workspace: &Path) -> serde_json::Value {
    serde_json::from_slice(
        &variant
            .run_ok(&["sync", "status", "--format", "json"], workspace)
            .stdout,
    )
    .unwrap()
}

/// Ids listed by `bead list`, in listing order
fn list_ids(variant: &Variant, workspace: &Path) -> Vec<String> {
    String::from_utf8_lossy(&variant.run_ok(&["list"], workspace).stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("ID: "))
        .map(str::to_string)
        .collect()
}

/// Ids the ready frontier hands out, in listing order
fn ready_ids(variant: &Variant, workspace: &Path) -> Vec<String> {
    String::from_utf8_lossy(&variant.run_ok(&["list", "--ready"], workspace).stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("ID: "))
        .map(str::to_string)
        .collect()
}

/// One field (`Status:`, `Revision:`, ...) of `bead show`
fn shown_field(variant: &Variant, workspace: &Path, id: &str, field: &str) -> String {
    String::from_utf8_lossy(&variant.run_ok(&["show", id], workspace).stdout)
        .lines()
        .find_map(|line| line.strip_prefix(field))
        .unwrap_or_else(|| panic!("{id} has no {field} line on {}", variant.role))
        .trim()
        .to_string()
}

/// Record a `checkpoint` section in `.beads/config.json`, preserving the rest
fn set_checkpoint_config(workspace: &Path, checkpoint: serde_json::Value) {
    let path = workspace.join(".beads/config.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    config["checkpoint"] = checkpoint;
    std::fs::write(&path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
}

/// A checkpoint pointer (`current.json` / `previous.json`) parsed
fn pointer(workspace: &Path, name: &str) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(workspace.join(".beads/checkpoint").join(name)).unwrap())
        .unwrap()
}

/// Wait for a child, failing the test rather than hanging forever
fn wait_with_timeout(mut child: Child, context: &str) -> Output {
    let deadline = Instant::now() + WORKER_TIMEOUT;
    loop {
        match child.try_wait().expect("child is running") {
            Some(_) => return child.wait_with_output().expect("child reaped"),
            None if Instant::now() > deadline => {
                let _ = child.kill();
                panic!(
                    "{context} did not finish within {}s",
                    WORKER_TIMEOUT.as_secs()
                );
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}

/// A three-bead source workspace with a real blocking edge: `blocker` must
/// close before `blocked` becomes ready. Returns the workspace and all ids.
struct ReplaySource {
    _dir: TempDir,
    workspace: PathBuf,
    checkpoint: PathBuf,
    ids: Vec<String>,
    /// The bead that must NOT appear on any ready frontier
    blocked: String,
}

fn replay_source(
    variant: &Variant,
    tag: &str,
    checkpoint_config: serde_json::Value,
) -> ReplaySource {
    let dir = init_workspace(variant, tag);
    let workspace = dir.path().to_path_buf();
    if !checkpoint_config.is_null() {
        set_checkpoint_config(&workspace, checkpoint_config);
    }
    let blocker = create_bead(variant, &workspace, "blocker bead");
    let middle = create_bead(variant, &workspace, "middle bead");
    let blocked = create_bead(variant, &workspace, "blocked bead");
    variant.run_ok(&["dep", "add", &blocked, &blocker], &workspace);
    variant.run_ok(&["sync", "flush-only"], &workspace);
    ReplaySource {
        checkpoint: workspace.join(".beads/checkpoint"),
        ids: vec![blocker, middle, blocked.clone()],
        blocked,
        workspace,
        _dir: dir,
    }
}

/// The replay contract every target of one checkpoint must satisfy: the full
/// graph is present, and the blocking edge survived the replay
fn assert_replayed_like_source(variant: &Variant, source: &ReplaySource, target: &Path) {
    let mut ids = list_ids(variant, target);
    ids.sort();
    let mut expected = source.ids.clone();
    expected.sort();
    assert_eq!(
        ids, expected,
        "replayed target must hold exactly the source graph on {}",
        variant.role
    );
    let ready = ready_ids(variant, target);
    assert_eq!(
        ready.len(),
        expected.len() - 1,
        "replay must preserve the blocks edge: exactly one bead stays blocked on {}",
        variant.role
    );
    assert!(
        !ready.contains(&source.blocked),
        "the blocked bead must stay off the ready frontier after replay on {}",
        variant.role
    );
}

// ---------------------------------------------------------------------------
// Stale fencing and revision tokens
// ---------------------------------------------------------------------------

/// N workers that all read revision 1 and all write `--if-revision 1` must
/// produce exactly one winner; every loser exits 4 with the conflict named,
/// the state shows exactly one applied change, and a loser re-fencing at the
/// current revision succeeds -- token invalidation is recoverable, not fatal.
#[test]
#[serial]
fn concurrent_revision_writers_admit_one_stale_token_per_revision() {
    for variant in variants() {
        let ws = init_workspace(variant, "revfence");
        let id = create_bead(variant, ws.path(), "revision race probe");

        let workers = 4;
        let mut children = Vec::new();
        for _ in 0..workers {
            children.push(variant.spawn(
                &[
                    "update".into(),
                    id.clone(),
                    "--status".into(),
                    "in_progress".into(),
                    "--if-revision".into(),
                    "1".into(),
                ],
                ws.path(),
            ));
        }
        let mut winners = 0;
        let mut fenced_off = 0;
        for (i, child) in children.into_iter().enumerate() {
            let output = wait_with_timeout(child, &format!("revision worker {i}"));
            match output.status.code() {
                Some(0) => winners += 1,
                Some(4) => {
                    fenced_off += 1;
                    let stderr = variant.stderr(&output);
                    assert!(
                        stderr.contains("Conflict: Revision mismatch")
                            && stderr.contains("expected 1"),
                        "the stale token must be rejected with both revisions named, got {stderr:?}"
                    );
                    assert!(
                        !stderr.contains("panicked"),
                        "fencing rejection must be a clean error"
                    );
                }
                other => panic!(
                    "revision worker {i} exited {other:?} on {}: {}",
                    variant.role,
                    variant.stderr(&output)
                ),
            }
        }
        assert_eq!(
            winners, 1,
            "exactly one stale token may commit per revision on {}",
            variant.role
        );
        assert_eq!(fenced_off, workers - 1, "every other token must lose");
        assert_eq!(
            shown_field(variant, ws.path(), &id, "Revision:"),
            "2",
            "exactly one of the racing changes may be applied"
        );

        // The invalidated writer re-reads and re-fences: the fresh token wins.
        variant.run_ok(
            &["update", &id, "--status", "deferred", "--if-revision", "2"],
            ws.path(),
        );
        assert_eq!(shown_field(variant, ws.path(), &id, "Revision:"), "3");
    }
}

/// A fencing token is invalidated by the next lease on the same bead: after
/// the first holder releases and a second worker claims, the first holder's
/// token -- and any forged token -- is refused for both update and release,
/// while the current holder's token is honored.
#[test]
#[serial]
fn lease_takeover_invalidates_the_previous_holder_fencing_token() {
    for variant in variants() {
        let ws = init_workspace(variant, "leasetake");
        let id = create_bead(variant, ws.path(), "lease takeover probe");

        let claim_json = |assignee: &str| {
            let output = variant.run_ok(
                &[
                    "claim",
                    "--assignee",
                    assignee,
                    "--lease-ttl",
                    "60",
                    "--json",
                ],
                ws.path(),
            );
            serde_json::from_slice(&output.stdout).unwrap()
        };

        let first: serde_json::Value = claim_json("old-worker");
        assert_eq!(first["bead_id"], id.as_str());
        let stale_token = first["lease"]["fencing_token"].as_i64().unwrap();

        // Hand the bead to a second worker: the lease that replaces the old
        // one must carry a strictly greater fencing token.
        variant.run_ok(&["release", &id], ws.path());
        let second: serde_json::Value = claim_json("new-worker");
        assert_eq!(second["bead_id"], id.as_str());
        let current_token = second["lease"]["fencing_token"].as_i64().unwrap();
        assert!(
            current_token > stale_token,
            "fencing tokens must be monotonic per bead: {current_token} after {stale_token}"
        );
        let current = current_token.to_string();
        let stale = stale_token.to_string();
        let forged = (stale_token + 97).to_string();

        for (actor, token) in [("stale holder", &stale), ("forged high token", &forged)] {
            for verb in ["update", "release"] {
                let mut args = vec![verb.to_string(), id.clone()];
                if verb == "update" {
                    args.push("--notes".into());
                    args.push("stale write".into());
                }
                args.push("--fencing-token".into());
                args.push(token.to_string());
                let output = variant.run(&args, ws.path());
                assert_eq!(
                    output.status.code(),
                    Some(4),
                    "{actor} must be fenced off with exit 4 on {}",
                    variant.role
                );
                let stderr = variant.stderr(&output);
                assert!(
                    stderr.contains("Fencing token mismatch"),
                    "{actor} rejection must name the fencing mismatch, got {stderr:?}"
                );
            }
        }

        // The current holder's token is the one valid token, for both verbs.
        variant.run_ok(
            &[
                "update",
                &id,
                "--notes",
                "current holder writes",
                "--fencing-token",
                &current,
            ],
            ws.path(),
        );
        assert_eq!(
            shown_field(variant, ws.path(), &id, "Notes:"),
            "current holder writes",
            "the current token's write must land"
        );
    }
}

// ---------------------------------------------------------------------------
// Concurrent replay
// ---------------------------------------------------------------------------

/// Multi-worker replay into parallel targets: every worker replaying the same
/// checkpoint into its own empty target must converge on the same graph --
/// same ids, same ready frontier, blocking edge included -- whether it replays
/// concurrently with the others or afterwards.
#[test]
#[serial]
fn parallel_replays_of_one_checkpoint_are_deterministic_across_targets() {
    for variant in variants() {
        let source = replay_source(variant, "detsrc", serde_json::Value::Null);
        let input = source.checkpoint.display().to_string();

        // Two targets initialized up front, replayed concurrently.
        let mut targets = Vec::new();
        let mut children = Vec::new();
        for i in 0..2 {
            let dir = init_workspace(variant, &format!("det{i}"));
            children.push((
                i,
                variant.spawn(
                    &[
                        "sync".to_string(),
                        "import-only".to_string(),
                        "--input".to_string(),
                        input.clone(),
                        "--restore-into-empty".to_string(),
                        "--actor".to_string(),
                        format!("replayer-{i}"),
                    ],
                    dir.path(),
                ),
            ));
            targets.push(dir);
        }
        for (i, child) in children {
            let output = wait_with_timeout(child, &format!("parallel replayer {i}"));
            assert!(
                output.status.success(),
                "parallel replayer {i} failed on {}: {}",
                variant.role,
                variant.stderr(&output)
            );
        }
        // A third target replays afterwards, sequentially.
        let sequential = init_workspace(variant, "detseq");
        variant.run_ok(
            &[
                "sync",
                "import-only",
                "--input",
                input.as_str(),
                "--restore-into-empty",
                "--actor",
                "replayer-seq",
            ],
            sequential.path(),
        );

        for target in targets
            .iter()
            .map(|d| d.path())
            .chain(std::iter::once(sequential.path()))
        {
            assert_replayed_like_source(variant, &source, target);
        }
    }
}

/// Multi-worker replay into one shared target is exactly-once: one winner
/// restores the graph, every other concurrent replayer is refused cleanly
/// without mutating anything, and the target ends up covering its own event
/// sequence with no duplicate or missing bead.
#[test]
#[serial]
fn concurrent_replays_into_one_target_admit_one_winner_and_exact_once_state() {
    for variant in variants() {
        let source = replay_source(variant, "racet", serde_json::Value::Null);
        let target = init_workspace(variant, "racetgt");

        let replayers = 4;
        let mut children = Vec::new();
        for i in 0..replayers {
            children.push(variant.spawn(
                &[
                    "sync".into(),
                    "import-only".into(),
                    "--input".into(),
                    source.checkpoint.display().to_string(),
                    "--restore-into-empty".into(),
                    "--actor".into(),
                    format!("replayer-{i}"),
                ],
                target.path(),
            ));
        }
        let mut winners = 0;
        let mut clean_refusals = 0;
        for (i, child) in children.into_iter().enumerate() {
            let output = wait_with_timeout(child, &format!("contending replayer {i}"));
            match output.status.code() {
                Some(0) => {
                    winners += 1;
                    assert!(
                        variant.stderr(&output).contains("Restored"),
                        "the winning replayer must report the restore, got {:?}",
                        variant.stderr(&output)
                    );
                }
                Some(1) => {
                    let stderr = variant.stderr(&output);
                    // The pre-F017 pin predates the newer explicit refusal
                    // suffix. Both messages reject the now-populated target;
                    // the exact-state assertions below prove it stayed intact.
                    assert!(
                        stderr.contains("Target database is not empty")
                            && (stderr.contains("Restore refused without mutation")
                                || stderr.contains(
                                    "Pre-F017 import requires an empty initialized target"
                                )),
                        "a losing replayer must refuse without mutating, got {stderr:?}"
                    );
                    clean_refusals += 1;
                }
                other => panic!(
                    "contending replayer {i} exited {other:?} on {}: {}",
                    variant.role,
                    variant.stderr(&output)
                ),
            }
        }
        assert_eq!(
            winners, 1,
            "exactly one replayer may win the empty target on {}",
            variant.role
        );
        assert_eq!(clean_refusals, replayers - 1);

        assert_replayed_like_source(variant, &source, target.path());
        let report = status(variant, target.path());
        assert_eq!(
            report["live_sequence"], report["covered_sequence"],
            "the restored target must cover its own replayed sequence"
        );
        assert_eq!(
            report["ready_to_commit"],
            serde_json::Value::Bool(true),
            "the replayed checkpoint must be ready to commit"
        );
    }
}

/// Replay is idempotent and hop-stable: merging the same forensic log into a
/// target that already replayed it inserts nothing, and replaying the target's
/// own checkpoint onward reproduces the same graph a third time.
#[test]
#[serial]
fn replay_is_idempotent_and_stable_across_hops() {
    for variant in variants() {
        let source = replay_source(variant, "idemsrc", serde_json::Value::Null);
        let first = init_workspace(variant, "idem1");
        let input = source.checkpoint.display().to_string();
        let import_args = [
            "sync",
            "import-only",
            "--input",
            input.as_str(),
            "--restore-into-empty",
            "--actor",
            "replayer",
        ];
        variant.run_ok(&import_args, first.path());

        // Merging the already-replayed log must be accepted and insert nothing.
        let merge = variant.run_ok(
            &[
                "sync",
                "import-only",
                "--input",
                source
                    .checkpoint
                    .join("forensic.jsonl")
                    .display()
                    .to_string()
                    .as_str(),
                "--merge",
                "--actor",
                "replayer-again",
            ],
            first.path(),
        );
        assert!(
            variant.stderr(&merge).contains("0 inserted"),
            "an idempotent merge replay must insert nothing, got {:?}",
            variant.stderr(&merge)
        );
        assert_replayed_like_source(variant, &source, first.path());

        // Second hop: the target publishes its own checkpoint, and replaying
        // it onward must reproduce the same graph.
        variant.run_ok(&["sync", "flush-only"], first.path());
        let second = init_workspace(variant, "idem2");
        variant.run_ok(
            &[
                "sync",
                "import-only",
                "--input",
                first
                    .path()
                    .join(".beads/checkpoint")
                    .display()
                    .to_string()
                    .as_str(),
                "--restore-into-empty",
                "--actor",
                "replayer-hop2",
            ],
            second.path(),
        );
        assert_replayed_like_source(variant, &source, second.path());
    }
}

/// A checkpoint flushed by either pin replays on the other: the durable
/// format is the cross-version contract, so work published by an old binary
/// is recoverable by a new one and vice versa.
#[test]
#[serial]
fn checkpoints_replay_across_variants_in_both_directions() {
    let [pre, enabled] = variants();

    // Old pin flushes, new pin restores.
    let source = replay_source(pre, "xpre", serde_json::Value::Null);
    let input = source.checkpoint.display().to_string();
    let target = init_workspace(enabled, "xpre-t");
    enabled.run_ok(
        &[
            "sync",
            "import-only",
            "--input",
            input.as_str(),
            "--restore-into-empty",
            "--actor",
            "newer-binary",
        ],
        target.path(),
    );
    assert_replayed_like_source(enabled, &source, target.path());

    // New pin flushes, old pin restores.
    let source = replay_source(enabled, "xen", serde_json::Value::Null);
    let input = source.checkpoint.display().to_string();
    let target = init_workspace(pre, "xen-t");
    pre.run_ok(
        &[
            "sync",
            "import-only",
            "--input",
            input.as_str(),
            "--restore-into-empty",
            "--actor",
            "older-binary",
        ],
        target.path(),
    );
    assert_replayed_like_source(pre, &source, target.path());
}

// ---------------------------------------------------------------------------
// Checkpoint suppression, failure, and recovery
// ---------------------------------------------------------------------------

/// With post-commit publication suppressed, mutations commit while the
/// checkpoint falls visibly behind; the named remedy (`sync flush-only`)
/// recovers full coverage, and the recovered checkpoint replays with every
/// suppressed mutation present -- suppression delays durability, never drops it.
#[test]
#[serial]
fn suppressed_publication_falls_behind_then_explicit_flush_recovers() {
    for variant in variants() {
        let ws = init_workspace(variant, "suppress");
        set_checkpoint_config(ws.path(), serde_json::json!({ "auto_flush": false }));

        let mut ids = Vec::new();
        for i in 0..3 {
            ids.push(create_bead(variant, ws.path(), &format!("suppressed {i}")));
        }

        let report = status(variant, ws.path());
        assert_eq!(
            report["dirty"],
            serde_json::Value::Bool(true),
            "suppressed publication must leave the checkpoint visibly dirty"
        );
        assert!(
            report["covered_sequence"].as_i64().unwrap()
                < report["live_sequence"].as_i64().unwrap(),
            "the checkpoint must lag the live store while suppressed: {}",
            report
        );
        assert_eq!(
            report["ready_to_commit"],
            serde_json::Value::Bool(false),
            "a lagging checkpoint must not be ready to commit"
        );

        variant.run_ok(&["sync", "flush-only"], ws.path());
        let report = status(variant, ws.path());
        assert_eq!(
            report["covered_sequence"], report["live_sequence"],
            "flush-only must recover full coverage"
        );
        assert_eq!(report["dirty"], serde_json::Value::Bool(false));
        assert_eq!(report["ready_to_commit"], serde_json::Value::Bool(true));

        // The recovered checkpoint replays with nothing lost.
        let checkpoint = ws.path().join(".beads/checkpoint");
        let target = init_workspace(variant, "suppress-t");
        let input = checkpoint.display().to_string();
        variant.run_ok(
            &[
                "sync",
                "import-only",
                "--input",
                input.as_str(),
                "--restore-into-empty",
                "--actor",
                "recovery-check",
            ],
            target.path(),
        );
        let mut restored = list_ids(variant, target.path());
        restored.sort();
        ids.sort();
        assert_eq!(
            restored, ids,
            "every mutation committed under suppression must survive the replay"
        );
    }
}

/// A publication failure after a committed mutation is reported as the split
/// it is -- exit 1, the mutation's own output preserved, the remedy named --
/// and the remedy recovers coverage without any mutation being lost or
/// rolled back.
#[test]
#[serial]
fn publication_failure_splits_without_rollback_then_recovers() {
    for variant in variants() {
        let ws = init_workspace(variant, "pubfail");
        variant.run_ok(&["sync", "flush-only"], ws.path());

        // Inject failure by replacing the checkpoint directory with a regular
        // file (ENOTDIR), which no privilege level can write through. The real
        // directory is parked aside so the pre-failure generation survives for
        // the assertions below.
        let checkpoint_dir = ws.path().join(".beads/checkpoint");
        let parked = ws.path().join(".beads/checkpoint.parked");
        std::fs::rename(&checkpoint_dir, &parked).unwrap();
        std::fs::write(&checkpoint_dir, b"not a directory").unwrap();

        let output = variant.run(
            &["create", "--title", "committed through failure"],
            ws.path(),
        );

        std::fs::remove_file(&checkpoint_dir).unwrap();
        std::fs::rename(&parked, &checkpoint_dir).unwrap();

        // The mutation committed: its own output is the new id, alone.
        assert_eq!(
            output.status.code(),
            Some(1),
            "a committed mutation whose publication failed must exit 1"
        );
        let stdout = variant.stdout(&output);
        let id = stdout.trim().to_string();
        assert!(
            !id.is_empty() && stdout.lines().count() == 1,
            "the mutation's success output must be preserved on stdout, got {stdout:?}"
        );
        let stderr = variant.stderr(&output);
        assert!(
            stderr.contains("checkpoint publication failed after the mutation committed"),
            "the split must be named on stderr, got {stderr:?}"
        );
        assert!(
            stderr.contains("sync flush-only"),
            "the remedy must be named on stderr, got {stderr:?}"
        );
        variant.run_ok(&["show", &id], ws.path());

        let report = status(variant, ws.path());
        assert!(
            report["covered_sequence"].as_i64().unwrap()
                < report["live_sequence"].as_i64().unwrap(),
            "the split must be observable as a lagging checkpoint: {}",
            report
        );

        variant.run_ok(&["sync", "flush-only"], ws.path());
        let report = status(variant, ws.path());
        assert_eq!(
            report["covered_sequence"], report["live_sequence"],
            "the named remedy must recover coverage"
        );
        assert_eq!(
            report["ready_to_commit"],
            serde_json::Value::Bool(true),
            "recovery must leave the checkpoint ready to commit"
        );
    }
}

/// A corrupted current pointer is reported honestly (`root_verified: false`,
/// no mode, no generation) rather than silently trusted, and republishing
/// from the authoritative store -- the database, which the pointer never was
/// -- rebuilds a verifying checkpoint without losing a bead.
#[test]
#[serial]
fn corrupted_pointer_is_reported_and_republish_recovers() {
    for variant in variants() {
        let ws = init_workspace(variant, "corrupt");
        let id = create_bead(variant, ws.path(), "survives pointer corruption");
        variant.run_ok(&["sync", "flush-only"], ws.path());

        std::fs::write(ws.path().join(".beads/checkpoint/current.json"), "{ broken").unwrap();

        let report = status(variant, ws.path());
        assert_eq!(
            report["root_verified"],
            serde_json::Value::Bool(false),
            "a corrupted pointer must not report as verified"
        );
        assert!(
            report["generation_id"].is_null() && report["mode"].is_null(),
            "a corrupted pointer must not advertise a generation: {}",
            report
        );

        variant.run_ok(&["sync", "flush-only"], ws.path());
        let report = status(variant, ws.path());
        assert_eq!(
            report["root_verified"],
            serde_json::Value::Bool(true),
            "republishing must rebuild a verifying checkpoint"
        );
        assert_eq!(report["covered_sequence"], report["live_sequence"]);
        assert_eq!(
            report["ready_to_commit"],
            serde_json::Value::Bool(true),
            "recovery must leave the checkpoint ready to commit"
        );
        assert!(list_ids(variant, ws.path()).contains(&id));
    }
}

/// When the current generation's root object is destroyed, restoring it is
/// refused before the target is touched, the damage is named by
/// `sync status`, and the retained previous generation still verifies and
/// restores -- losing the newest generation costs the newest state, never
/// the recovery path.
#[test]
#[serial]
fn damaged_current_generation_refuses_restore_while_previous_recovers() {
    for variant in variants() {
        let ws = init_workspace(variant, "damage");
        let first = create_bead(variant, ws.path(), "generation one bead");
        variant.run_ok(&["sync", "flush-only"], ws.path());
        let second = create_bead(variant, ws.path(), "generation two bead");
        variant.run_ok(&["sync", "flush-only"], ws.path());
        assert_ne!(first, second);

        let generation: String = pointer(ws.path(), "current.json")["generation_id"]
            .as_str()
            .unwrap()
            .to_string();
        let root: String = pointer(ws.path(), "current.json")["active_root"]["path"]
            .as_str()
            .unwrap()
            .to_string();
        std::fs::remove_file(ws.path().join(".beads/checkpoint").join(&root)).unwrap();

        let report = status(variant, ws.path());
        assert_eq!(
            report["root_verified"],
            serde_json::Value::Bool(false),
            "a missing root object must not report as verified"
        );
        assert_eq!(
            report["ready_to_commit"],
            serde_json::Value::Bool(false),
            "a checkpoint whose root is gone must not be ready to commit"
        );
        let reasons = report["not_ready_reasons"].as_array().unwrap();
        assert!(
            reasons
                .iter()
                .any(|r| r.as_str().unwrap().contains("root object missing")),
            "the damage must be named among the not-ready reasons: {reasons:?}"
        );

        // The damaged generation is refused before the target is touched.
        let checkpoint = ws.path().join(".beads/checkpoint").display().to_string();
        let target = init_workspace(variant, "damage-t");
        let refused = variant.run(
            &[
                "restore",
                "--source",
                checkpoint.as_str(),
                "--generation",
                generation.as_str(),
                "--actor",
                "recovery-operator",
                "--format",
                "json",
            ],
            target.path(),
        );
        assert_eq!(
            refused.status.code(),
            Some(5),
            "restoring an unverifiable generation must be an integrity refusal"
        );
        assert!(
            variant
                .stderr(&refused)
                .contains("Unverified restore source"),
            "the refusal must name the unverified source, got {:?}",
            variant.stderr(&refused)
        );
        assert!(
            list_ids(variant, target.path()).is_empty(),
            "a refused restore must not mutate the target"
        );

        // The retained previous generation still recovers.
        let previous: String = pointer(ws.path(), "previous.json")["generation_id"]
            .as_str()
            .unwrap()
            .to_string();
        let target = init_workspace(variant, "damage-p");
        let restored = variant.run_ok(
            &[
                "restore",
                "--source",
                checkpoint.as_str(),
                "--generation",
                previous.as_str(),
                "--actor",
                "recovery-operator",
                "--format",
                "json",
            ],
            target.path(),
        );
        let receipt: serde_json::Value = serde_json::from_slice(&restored.stdout).unwrap();
        assert_eq!(receipt["generation_id"], previous.as_str());
        assert_eq!(receipt["actor"], "recovery-operator");
        assert_eq!(receipt["issues_restored"], 1);
        let recovered = list_ids(variant, target.path());
        assert_eq!(
            recovered,
            vec![first],
            "the previous generation must recover exactly its own state"
        );
        assert!(
            !recovered.contains(&second),
            "the newest bead postdates the recovered generation"
        );
    }
}

// ---------------------------------------------------------------------------
// Sharded and monolithic restore
// ---------------------------------------------------------------------------

/// Both restore modes recover a checkpoint that was written while publication
/// was suppressed, through both restore entry points, preserving the graph
/// and the blocking edge: mode is a storage detail, recovery is identical.
#[test]
#[serial]
fn restore_recovers_sharded_and_monolithic_checkpoints_after_suppression() {
    for variant in variants() {
        for (mode, forced) in [
            ("monolithic", serde_json::json!({ "mode": "monolithic" })),
            ("sharded", serde_json::json!({ "mode": "sharded" })),
        ] {
            let mut config = forced;
            config["auto_flush"] = serde_json::Value::Bool(false);
            let source = replay_source(variant, &format!("mode-{mode}"), config);
            let published: serde_json::Value = pointer(&source.workspace, "current.json");
            assert_eq!(
                published["mode"], mode,
                "the forced mode must govern the published generation"
            );
            let generation = published["generation_id"].as_str().unwrap().to_string();

            // Entry point 1: the named-restore command.
            let checkpoint = source.checkpoint.display().to_string();
            let via_restore = init_workspace(variant, &format!("{mode}-r"));
            let output = variant.run_ok(
                &[
                    "restore",
                    "--source",
                    checkpoint.as_str(),
                    "--generation",
                    generation.as_str(),
                    "--actor",
                    "recovery-operator",
                    "--format",
                    "json",
                ],
                via_restore.path(),
            );
            let receipt: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(receipt["mode"], mode);
            assert_eq!(receipt["generation_id"], generation.as_str());
            assert_eq!(receipt["actor"], "recovery-operator");
            assert_eq!(
                receipt["issues_restored"], 3,
                "every suppressed mutation must be in the restored generation"
            );
            assert_replayed_like_source(variant, &source, via_restore.path());

            // Entry point 2: the interchange primitive over the same set.
            let via_import = init_workspace(variant, &format!("{mode}-i"));
            variant.run_ok(
                &[
                    "sync",
                    "import-only",
                    "--input",
                    checkpoint.as_str(),
                    "--restore-into-empty",
                    "--actor",
                    "recovery-operator",
                ],
                via_import.path(),
            );
            assert_replayed_like_source(variant, &source, via_import.path());
        }
    }
}
