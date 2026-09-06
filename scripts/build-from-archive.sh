#!/usr/bin/env bash
#
# build-from-archive.sh — the only sanctioned way to build a pinned bead-rs
# binary from an older commit.
#
# The shared NEEDLE checkout (/home/coding/bead-rs, ADR-015) must never be
# stashed, reset, or checked out to build an older commit — doing that is how
# 17 stash entries and a reset erased another worker's uncommitted hour on
# 2026-09-01/02 (beadrs-5a0dc962). This script instead extracts the commit
# with `git archive` (read-only against the shared checkout: no index, HEAD,
# reflog or stash is touched), builds it in a scratch directory, copies the
# binary and a metadata file out, and removes the scratch directory on
# success. On failure the scratch directory is deliberately left in place for
# diagnosis (workspace disposable-checkout rule).
#
# Usage:
#   scripts/build-from-archive.sh <sha> [--features <feat,...>]
#                                 [--name <pin-name>] [--out <dir>]
#
#   <sha>            commit to build (full or short); archived as committed,
#                    so uncommitted changes in the shared checkout are ignored
#   --features       comma-separated cargo features to build with
#   --name           pin name; default `bead-<short-sha>` (pins are usually
#                     named `<role>-<shaslice>`, so pass a role when pinning)
#   --out            destination directory; default pinned-binaries/
#
# Exit codes: 0 success; nonzero usage/validation/extraction/build failure.

set -euo pipefail

progname="build-from-archive.sh"

usage() {
	local code=${1:-0}
	if [[ "$code" -ne 0 ]]; then
		awk 'NR > 1 { if (!/^#/) exit; print substr($0, 3) }' "${BASH_SOURCE[0]}" >&2
	else
		awk 'NR > 1 { if (!/^#/) exit; print substr($0, 3) }' "${BASH_SOURCE[0]}"
	fi
	exit "$code"
}

die() {
	echo "$progname: error: $*" >&2
	if [[ -n "${SCRATCH:-}" && -d "$SCRATCH" ]]; then
		echo "$progname: scratch dir left in place for diagnosis: $SCRATCH" >&2
	fi
	exit 1
}

[[ "${1:-}" == "-h" || "${1:-}" == "--help" ]] && usage 0
[[ $# -ge 1 ]] || usage 1

REPO=$(git rev-parse --show-toplevel 2>/dev/null) ||
	die "must be run inside the bead-rs git repository"
REQ_SHA=$1
FULL_SHA=$REQ_SHA
FEATURES=""
NAME=""
OUT_DIR="$REPO/pinned-binaries"
shift

while [[ $# -gt 0 ]]; do
	case "$1" in
	--features)
		[[ $# -ge 2 ]] || die "--features requires a value"
		FEATURES=$2
		shift 2
		;;
	--name)
		[[ $# -ge 2 ]] || die "--name requires a value"
		NAME=$2
		shift 2
		;;
	--out)
		[[ $# -ge 2 ]] || die "--out requires a value"
		OUT_DIR=$2
		shift 2
		;;
	-h | --help)
		usage 0
		;;
	*)
		die "unknown argument: $1 (see --help)"
		;;
	esac
done

trap 'echo "$progname: FAILED — scratch dir left in place for diagnosis: ${SCRATCH:-<none created>}" >&2' ERR

command -v cargo >/dev/null || die "cargo not found on PATH"
command -v python3 >/dev/null || die "python3 not found on PATH"

# Resolve and validate the requested commit. Read-only against the shared
# checkout, like every git call in this script.
FULL_SHA=$(git -C "$REPO" rev-parse --verify --quiet "$FULL_SHA^{commit}") ||
	die "not a commit: $REQ_SHA"
SHORT_SHA=${FULL_SHA:0:7}
COMMIT_SUBJECT=$(git -C "$REPO" show -s --format=%s "$FULL_SHA")
COMMIT_DATE=$(git -C "$REPO" show -s --format=%cI "$FULL_SHA")
[[ -n "$NAME" ]] || NAME="bead-$SHORT_SHA"

mkdir -p "$OUT_DIR"
DEST="$OUT_DIR/$NAME"
META="$OUT_DIR/$NAME.metadata.json"
if [[ -e "$DEST" || -e "$META" ]]; then
	die "refusing to overwrite an existing pin: $DEST"
fi

# Scratch extraction lives under ~/scratch per the disposable-checkout rule.
# A cold release build needs a couple of GB; refuse early rather than filling
# the disk for every agent and worker on the box (2026-08-21 ENOSPC incident).
SCRATCH_BASE="$HOME/scratch"
mkdir -p "$SCRATCH_BASE"
AVAIL_KB=$(df -k "$SCRATCH_BASE" | awk 'NR==2 {print $4}')
if (( AVAIL_KB < 5 * 1024 * 1024 )); then
	die "only ${AVAIL_KB}KiB free under $SCRATCH_BASE; a cold build needs several GB"
fi
SCRATCH=$(mktemp -d -p "$SCRATCH_BASE" "bead-archive-$SHORT_SHA-XXXXXX")

echo "$progname: extracting $SHORT_SHA into $SCRATCH"
if ! git -C "$REPO" archive "$FULL_SHA" | tar -x -C "$SCRATCH"; then
	die "archive extraction of $SHORT_SHA failed"
fi
[[ -f "$SCRATCH/Cargo.toml" ]] || die "no Cargo.toml in the archive of $SHORT_SHA"

BUILD_CMD=(cargo build --release --locked)
if [[ -n "$FEATURES" ]]; then
	BUILD_CMD+=(--features "$FEATURES")
fi
BUILD_COMMAND_LINE="${BUILD_CMD[*]}"

echo "$progname: building: $BUILD_COMMAND_LINE (CARGO_TARGET_DIR=$SCRATCH/target)"
BUILD_LOG="$SCRATCH/build.log"
if ! (cd "$SCRATCH" && CARGO_TARGET_DIR="$SCRATCH/target" "${BUILD_CMD[@]}" 2>&1 | tee "$BUILD_LOG"); then
	tail -n 40 "$BUILD_LOG" >&2 || true
	die "build of $SHORT_SHA failed (full log: $BUILD_LOG)"
fi

BIN="$SCRATCH/target/release/bead"
[[ -x "$BIN" ]] || die "build reported success but $BIN is missing"

# Run the binary once from inside the scratch dir to prove it executes; its
# embedded commit is "unknown" by design — an archive carries no .git, and
# build.rs documents that as the honest value for exported trees.
VERSION_STRING=$(cd "$SCRATCH" && "$BIN" --version) ||
	die "built binary does not run: $VERSION_STRING"
echo "$progname: built: $VERSION_STRING"

install -m 755 "$BIN" "$DEST"
BIN_SHA256=$(sha256sum "$DEST" | awk '{print $1}')
BIN_SIZE=$(stat -c %s "$DEST")
BIN_SIZE_HUMAN=$(du -h "$DEST" | awk '{print $1}')
BUILD_TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)
RUSTC_VERSION=$(rustc --version)
CARGO_PKG_VERSION=$(awk -F'"' '/^\[package\]/ { inpkg = 1; next } /^\[/ { inpkg = 0 } inpkg && /^version = / { print $2; exit }' "$SCRATCH/Cargo.toml")

BIN_NAME="$NAME" \
	DESCRIPTION="bead-rs release binary built from a git-archive extraction of $SHORT_SHA" \
	PIN_PATH="$DEST" \
	SOURCE_REPO="$REPO" \
	SOURCE="git-archive" \
	BUILD_HOST="$(uname -n)" \
	SCRATCH_PATH="$SCRATCH" \
	FULL_SHA="$FULL_SHA" \
	SHORT_SHA="$SHORT_SHA" \
	COMMIT_SUBJECT="$COMMIT_SUBJECT" \
	COMMIT_DATE="$COMMIT_DATE" \
	VERSION_STRING="$VERSION_STRING" \
	BUILD_TIMESTAMP="$BUILD_TIMESTAMP" \
	BIN_SHA256="$BIN_SHA256" \
	BIN_SIZE="$BIN_SIZE" \
	BIN_SIZE_HUMAN="$BIN_SIZE_HUMAN" \
	CARGO_PKG_VERSION="$CARGO_PKG_VERSION" \
	FEATURES="$FEATURES" \
	BUILD_COMMAND_LINE="$BUILD_COMMAND_LINE" \
	RUSTC_VERSION="$RUSTC_VERSION" \
	python3 - >"$META" <<'PY'
import json
import os

features = os.environ["FEATURES"]
data = {
    "binary_name": os.environ["BIN_NAME"],
    "description": os.environ["DESCRIPTION"],
    "pinned_timestamp": os.environ["BUILD_TIMESTAMP"],
    "git_commit_sha": os.environ["FULL_SHA"],
    "git_commit_short": os.environ["SHORT_SHA"],
    "git_commit_message": os.environ["COMMIT_SUBJECT"],
    "git_commit_date": os.environ["COMMIT_DATE"],
    "embedded_version_string": os.environ["VERSION_STRING"],
    "build_timestamp": os.environ["BUILD_TIMESTAMP"],
    "build_host": os.environ["BUILD_HOST"],
    "build_scratch_path": os.environ["SCRATCH_PATH"],
    "binary_sha256": os.environ["BIN_SHA256"],
    "binary_size_bytes": int(os.environ["BIN_SIZE"]),
    "binary_size_human": os.environ["BIN_SIZE_HUMAN"],
    "cargo_package_version": os.environ["CARGO_PKG_VERSION"],
    "build_features": features if features else "default",
    "build_profile": "release",
    "rustc_version": os.environ["RUSTC_VERSION"],
    "build_command": os.environ["BUILD_COMMAND_LINE"],
    "pinned_binary_path": os.environ["PIN_PATH"],
    "source": os.environ["SOURCE"],
    "source_extraction": "git archive from %s into a scratch dir under ~/scratch; "
    "the shared checkout's index, HEAD, reflog and stashes were not touched "
    "(scripts/build-from-archive.sh)" % os.environ["SOURCE_REPO"],
    "notes": [
        "The embedded version string reports commit 'unknown' because a git-archive "
        "extraction carries no .git; build.rs documents that as the honest value for "
        "exported trees. The authoritative source commit is git_commit_sha above.",
        "sha256 is NOT reproducible across rebuilds: build.rs re-embeds "
        "BEAD_BUILD_TIMESTAMP on every build. Verify by hash comparison against these "
        "pinned bytes; do not rebuild and expect this hash.",
    ],
}
json.dump(data, __import__("sys").stdout, indent=2)
print()
PY

echo "$progname: pinned $DEST"
echo "$progname: metadata $META"
echo "$progname: sha256 $BIN_SHA256"

rm -rf "$SCRATCH"
trap - ERR
echo "$progname: scratch dir removed: $SCRATCH"
