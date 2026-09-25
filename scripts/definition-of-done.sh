#!/usr/bin/env bash
#
# definition-of-done.sh — the repository's own verification gate, safe to run
# from a git-archive extraction (no .git required).
#
# Runs the AGENTS.md "Verification" trio in order, stopping at the first
# failure:
#
#	cargo fmt --check
#	cargo clippy --all-targets -- -D warnings
#	cargo test
#
# Pass --fast to skip the clippy pass when a quick gate is wanted; the full
# run (no arguments) is the definition of done.
#
# Why the script owns TMPDIR: workspace discovery stops at the FIRST `.beads`
# directory on the upward walk and fails closed when that directory lacks the
# bead-rs fingerprint `.beads/config.json` (R030) — by design, so a command
# can never silently operate on an unrelated parent workspace. Integration
# tests chdir into tempdirs created from TMPDIR and run `bead init` there,
# which requires discovery to find NO `.beads` at all. A foreign `.beads`
# anywhere on TMPDIR's ancestor chain therefore fails dozens of suite entries
# with "discovery stopped at ... which is not a bead-rs workspace" even though
# the code under test is correct — the situation on the shared lab box, where
# /tmp/.beads (logs-only debris, 2026-09-07) sits above every mktemp default.
# This script points TMPDIR at a freshly created directory whose ancestor
# chain is verified free of any `.beads`, so the suite passes identically in
# a clean CI container and in a scratch extraction on the contaminated box.
# It never reads, moves, or deletes any existing `.beads` directory.
#
# Usage:
#	scripts/definition-of-done.sh [--fast]
#
# Environment:
#	BEAD_DOD_TMPBASE	base directory for the test TMPDIR
#				(default /var/tmp; must not have a `.beads`
#				anywhere on its ancestor chain)
#
# Exit codes: 0 all steps passed; 1 a step failed or the environment is
# unsuitable. On failure the test TMPDIR is left in place for diagnosis and
# its path is printed; on success it is removed.

set -euo pipefail

progname="definition-of-done.sh"

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
	if [[ -n "${TEST_TMPDIR:-}" && -d "$TEST_TMPDIR" ]]; then
		echo "$progname: test TMPDIR left in place for diagnosis: $TEST_TMPDIR" >&2
	fi
	exit 1
}

FAST=0
while [[ $# -gt 0 ]]; do
	case "$1" in
	--fast)
		FAST=1
		shift
		;;
	-h | --help)
		usage 0
		;;
	*)
		die "unknown argument: $1 (see --help)"
		;;
	esac
done

command -v cargo >/dev/null || die "cargo not found on PATH"
command -v rustfmt >/dev/null || die "rustfmt not found on PATH"

# Resolve the tree root from the script location, not `git rev-parse`: the
# definition of done must hold in a git-archive extraction, which carries no
# .git by design (see BUILD_PROCEDURE.md).
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
[[ -f "$ROOT/Cargo.toml" ]] || die "no Cargo.toml beside $ROOT — run from a checkout or extraction"

# A cold fmt+clippy+test build needs several GB; refuse early rather than
# filling the disk for every agent and worker on the box (2026-08-21 ENOSPC
# incident, same guard as build-from-archive.sh).
AVAIL_KB=$(df -k "$ROOT" | awk 'NR==2 {print $4}')
if (( AVAIL_KB < 8 * 1024 * 1024 )); then
	die "only ${AVAIL_KB}KiB free beside $ROOT; the verification build needs several GB"
fi

# Hand the suite a TMPDIR with a clean ancestor chain (see header). Created
# under BEAD_DOD_TMPBASE, which must itself be free of `.beads` ancestors —
# checked below rather than assumed, because discovery fails closed and a
# silent mis-set would present as dozens of unrelated test failures.
TMP_BASE="${BEAD_DOD_TMPBASE:-/var/tmp}"
[[ -d "$TMP_BASE" ]] || die "BEAD_DOD_TMPBASE is not a directory: $TMP_BASE"
TEST_TMPDIR=$(mktemp -d "$TMP_BASE/bead-dod-XXXXXXXXXX")
probe="$TEST_TMPDIR"
while [[ "$probe" != "/" ]]; do
	probe=$(dirname -- "$probe")
	if [[ -e "$probe/.beads" ]]; then
		die "$probe/.beads exists — discovery from test tempdirs would stop there (R030); pick a BEAD_DOD_TMPBASE with a clean ancestor chain"
	fi
done
export TMPDIR="$TEST_TMPDIR"

cleanup() {
	if [[ -d "$TEST_TMPDIR" ]]; then
		rm -rf "$TEST_TMPDIR"
	fi
}
trap cleanup EXIT

step() {
	echo "$progname: $*"
}

cd "$ROOT"

step "cargo fmt --check"
if ! cargo fmt --check; then
	die "cargo fmt --check failed"
fi

if [[ "$FAST" -ne 1 ]]; then
	step "cargo clippy --all-targets -- -D warnings"
	if ! cargo clippy --all-targets -- -D warnings; then
		die "cargo clippy failed"
	fi
else
	step "skipping clippy (--fast)"
fi

step "cargo test"
if ! cargo test; then
	die "cargo test failed"
fi

step "PASS: fmt$( [[ "$FAST" -ne 1 ]] && printf ' + clippy' ) + test"
