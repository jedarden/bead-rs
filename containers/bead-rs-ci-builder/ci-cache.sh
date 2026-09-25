#!/usr/bin/env bash
# Cargo artifact cache for bead-rs-ci.
#
# The cache is a registry image (one tag per cache key), not a PVC: a run
# with a matching key pulls it and unpacks $CARGO_HOME + the workspace
# target/ directory before compiling; a run with no matching tag builds cold.
# A green run at the end pushes its artifacts under the same key, so the
# first run after a Cargo.lock / toolchain / builder-image change is always
# cold and every following run on the same key is warm.
#
# Key composition (CACHE_FORMAT guards against format-level changes that
# would make old caches lie):
#   CACHE_FORMAT-v1
#   sha256(Cargo.lock)[:16]   -- dependency set
#   rustc --version           -- default (stable) toolchain
#   dpkg --print-architecture -- build architecture
#   sha256(IDENTITY)[:12]     -- builder image build (bakes toolchain paths,
#                                rustup/cargo version, and the image itself)
#
# Tags are content-addressed and never overwritten: push() is a no-op when
# the tag already exists. Stale tags simply go unused as their key stops
# matching; pruning them is an operator action on the registry.
#
# Usage: ci-cache.sh key | restore | push
#   key      print the cache key
#   restore  pull+unpack the cache if the key exists (never fails the run)
#   push     pack+push the cache if the key is absent (never fails the run)
#
# Requires: crane (in the builder image), DOCKER_CONFIG pointing at a
# directory with a registry config.json, and a checkout at $WORKSPACE_DIR
# with Cargo.lock present. Run from the workspace root.
set -euo pipefail

CACHE_REPO="ronaldraygun/bead-rs-ci-cargo-cache"
CACHE_FORMAT="v1"
IDENTITY_FILE="/usr/local/share/bead-rs-ci-builder/IDENTITY"
WORKSPACE_DIR="${WORKSPACE_DIR:-/workspace}"
# Bounded cache: refuse to push a layer above this (6 GiB uncompressed).
PUSH_CAP_BYTES=$((6 * 1024 * 1024 * 1024))
LAYER_TAR="$(mktemp /tmp/bead-rs-cache-layer.XXXXXX.tar)"
trap 'rm -f "$LAYER_TAR"' EXIT

die() { echo "ci-cache: $*" >&2; exit 1; }

cache_key() {
    [ -f "$IDENTITY_FILE" ] || die "builder IDENTITY file missing: $IDENTITY_FILE"
    [ -f "$WORKSPACE_DIR/Cargo.lock" ] || die "Cargo.lock missing under $WORKSPACE_DIR"
    local lock toolchain arch ident
    lock="$(sha256sum "$WORKSPACE_DIR/Cargo.lock" | cut -c1-16)"
    toolchain="$(rustc --version | awk '{print $2}')"
    arch="$(dpkg --print-architecture)"
    ident="$(sha256sum "$IDENTITY_FILE" | cut -c1-12)"
    printf 'fmt%s-lock%s-rust%s-%s-img%s' \
        "$CACHE_FORMAT" "$lock" "$toolchain" "$arch" "$ident"
}

manifest_bytes() {
    # Sum layer sizes from a crane manifest (jq is not in the image).
    tr '{,' '\n' <"$1" | grep -o '"size":[0-9]*' | cut -d: -f2 \
        | awk '{s+=$1} END{print s+0}'
}

cmd_restore() {
    local ref="${CACHE_REPO}:$(cache_key)"
    local t0 mem_peak bytes
    t0="$(date +%s)"
    if ! crane manifest "$ref" >/tmp/cache-manifest.json 2>/tmp/cache-crane.err; then
        echo "CACHE=miss reason=no-tag ref=${ref} seconds=$(( $(date +%s) - t0 ))"
        return 0
    fi
    bytes="$(manifest_bytes /tmp/cache-manifest.json)"
    if ! crane export "$ref" - | tar -xpf - -C /; then
        echo "CACHE=miss reason=pull-or-extract-failed bytes=${bytes} ref=${ref} seconds=$(( $(date +%s) - t0 ))"
        return 0
    fi
    echo "CACHE=hit bytes=${bytes} ref=${ref} seconds=$(( $(date +%s) - t0 ))"
}

cmd_push() {
    local ref="${CACHE_REPO}:$(cache_key)"
    local t0 bytes
    t0="$(date +%s)"
    if crane manifest "$ref" >/dev/null 2>&1; then
        echo "CACHE-PUSH=skipped reason=tag-exists ref=${ref}"
        return 0
    fi
    # Only $CARGO_HOME and the compiled workspace artifacts go in the layer.
    # target debug incremental/ is excluded to bound the layer; with the
    # default CARGO_INCREMENTAL it is pure churn and the largest single
    # contributor after deps. registry/src/ (unpacked .crate sources) is
    # excluded too: cargo re-extracts it from registry/cache/*.crate on
    # demand, to identical paths.
    rm -f "$LAYER_TAR"
    if ! tar -C / -cf "$LAYER_TAR" \
        --exclude='usr/local/cargo/registry/src' \
        --exclude='workspace/target/debug/incremental' \
        --exclude='workspace/target/release/incremental' \
        --exclude='workspace/target/package' \
        usr/local/cargo workspace/target; then
        echo "CACHE-PUSH=skipped reason=tar-failed ref=${ref} seconds=$(( $(date +%s) - t0 ))"
        return 0
    fi
    bytes="$(stat -c %s "$LAYER_TAR")"
    if [ "$bytes" -gt "$PUSH_CAP_BYTES" ]; then
        echo "CACHE-PUSH=skipped reason=cap-exceeded bytes=${bytes} cap=${PUSH_CAP_BYTES} ref=${ref}"
        return 0
    fi
    if ! crane append -f "$LAYER_TAR" -t "$ref"; then
        echo "CACHE-PUSH=failed bytes=${bytes} ref=${ref} seconds=$(( $(date +%s) - t0 ))"
        return 0
    fi
    echo "CACHE-PUSH=ok bytes=${bytes} ref=${ref} seconds=$(( $(date +%s) - t0 ))"
}

case "${1:-}" in
    key) cache_key ;;
    restore) cmd_restore ;;
    push) cmd_push ;;
    *) die "usage: ci-cache.sh {key|restore|push}" ;;
esac
