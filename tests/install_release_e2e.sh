#!/bin/bash
#
# End-to-end installer and release-artifact verification.
#
# Runs the documented install workflow (README "Install" -> the one-line
# installer) against a hermetic fake release that mirrors the real GitHub
# release layout, reached through the installer's BEAD_GITHUB_API /
# BEAD_DOWNLOAD_BASE overrides:
#
#   <api>/latest.json              {"tag_name": "v0.2.6-e2e"}
#   <dl>/<tag>/bead-<arch>-<os>    arch-suffixed executable artifact
#   <dl>/<tag>/checksums.txt       "<sha256>  <asset>" per artifact
#
# Platform detection is faked through a PATH-local `uname` stub, so every
# supported platform and every failure mode can be exercised from any host.
# No network access is needed: every download still goes through the
# installer's curl code path, on file:// URLs.
#
# Covered:
#   - arch-suffixed artifact selection for all four supported platforms,
#     including the amd64/arm64 uname aliases
#   - the installed binary answers `bead --version` with its artifact's
#     platform marker, and its bytes match the release artifact exactly
#   - checksum verification on the happy path (and that --skip-checksum
#     still verifies when checksums ARE available - fail-closed default)
#   - failure cases: unsupported OS, unsupported architecture, tampered
#     artifact (a mismatch is never skippable, even with --skip-checksum),
#     missing artifact, missing checksums.txt (fail-closed), missing
#     per-asset entry, artifact that fails --version, unknown option,
#     unresolvable latest tag, unreachable release API
#   - --skip-checksum bypasses only UNAVAILABLE checksums, never a mismatch
#   - the release layout itself: one "<sha256>  bead-<arch>-<os>" entry per
#     asset, exactly the four supported assets
#
# Set BEAD_INSTALL_E2E_LIVE=1 (with network) to additionally verify the REAL
# latest GitHub release exposes all four arch-suffixed assets, that its
# checksums.txt covers each one, and that the host platform's real artifact
# passes checksum and `bead --version`.
#
# Run with: bash tests/install_release_e2e.sh

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
INSTALL_SH="$ROOT/install.sh"
TAG="v0.2.6-e2e"

PASSED=0
FAILED=0
SKIPPED=0

note()     { printf '      %s\n' "$1"; }
ok()       { printf 'ok     %s\n' "$1"; PASSED=$((PASSED + 1)); }
failed()   { printf 'FAIL   %s\n' "$1"; FAILED=$((FAILED + 1)); }
skipped()  { printf 'skip   %s\n' "$1"; SKIPPED=$((SKIPPED + 1)); }
section()  { printf '\n== %s\n' "$1"; }

expect_exit() { # <desc> <wanted> <actual>
    if [[ "$2" == "$3" ]]; then ok "$1"; else failed "$1: exit $3, wanted $2"; fi
}
expect_nonzero_exit() { # <desc> <actual>
    if [[ "$2" != "0" ]]; then ok "$1"; else failed "$1: unexpectedly exited 0"; fi
}
expect_eq() { # <desc> <wanted> <actual>
    if [[ "$2" == "$3" ]]; then ok "$1"; else failed "$1: wanted '$2', got '$3'"; fi
}
expect_contains() { # <desc> <haystack> <needle>
    if [[ "$2" == *"$3"* ]]; then ok "$1"; else failed "$1: output does not contain '$3'"; fi
}
expect_file() { # <desc> <path>
    if [[ -e "$2" ]]; then ok "$1"; else failed "$1: $2 does not exist"; fi
}
expect_no_file() { # <desc> <path>
    if [[ ! -e "$2" ]]; then ok "$1"; else failed "$1: $2 unexpectedly exists"; fi
}

hash_of() { # <file> -> sha256
    if command -v sha256sum &>/dev/null; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

# ---------------------------------------------------------------------------
# Prerequisites
# ---------------------------------------------------------------------------

if [[ ! -f "$INSTALL_SH" ]]; then
    echo "FAIL: installer not found at $INSTALL_SH" >&2
    exit 1
fi

if ! command -v curl &>/dev/null && ! command -v wget &>/dev/null; then
    echo "SKIP: neither curl nor wget is available; the documented install workflow cannot run here" >&2
    exit 0
fi

for tool in mktemp cp chmod mv; do
    command -v "$tool" &>/dev/null || { echo "FAIL: missing prerequisite: $tool" >&2; exit 1; }
done

if ! command -v sha256sum &>/dev/null && ! command -v shasum &>/dev/null; then
    echo "SKIP: neither sha256sum nor shasum is available; cannot build or verify release checksums" >&2
    exit 0
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# ---------------------------------------------------------------------------
# Fake release + platform stubs
# ---------------------------------------------------------------------------

# PATH-local uname stub so scenarios can pretend to be any platform.
UNAME_BIN="$WORK/stub-bin"
mkdir -p "$UNAME_BIN"
cat > "$UNAME_BIN/uname" <<'STUB'
#!/bin/sh
case "$1" in
    -s) printf '%s\n' "${BEAD_E2E_UNAME_S:-Linux}" ;;
    -m) printf '%s\n' "${BEAD_E2E_UNAME_M:-x86_64}" ;;
    *)   printf '%s\n' ;;
esac
STUB
chmod +x "$UNAME_BIN/uname"

make_artifact() { # <path> <asset-name> - stand-in release binary
    mkdir -p "$(dirname "$1")"
    cat > "$1" <<STUB
#!/bin/sh
# e2e stand-in artifact for $2
case "\$1" in
    --version) echo "bead 0.2.6-e2e ($2)" ;;
    *) echo "bead e2e stub" ;;
esac
STUB
    chmod +x "$1"
}

ASSETS=(
    bead-x86_64-unknown-linux-gnu
    bead-aarch64-unknown-linux-gnu
    bead-x86_64-apple-darwin
    bead-aarch64-apple-darwin
)

# Regenerate checksums.txt for a release dir ("<sha256>  <asset>" per asset)
write_checksums() { # <release-dl-dir>
    local rel="$1" asset
    (
        cd "$rel/$TAG"
        : > checksums.txt
        for asset in "${ASSETS[@]}"; do
            printf '%s  %s\n' "$(hash_of "$asset")" "$asset" >> checksums.txt
        done
    )
}

# Canonical fake release: <WORK>/dl/<tag>/{bead-*, checksums.txt}
for asset in "${ASSETS[@]}"; do
    make_artifact "$WORK/dl/$TAG/$asset" "$asset"
done
write_checksums "$WORK/dl"
mkdir -p "$WORK/api"
printf '{"tag_name": "%s", "name": "bead %s (e2e fixture)"}\n' "$TAG" "$TAG" \
    > "$WORK/api/latest.json"
# API document without a tag_name
printf '{"message": "e2e: no tag here"}\n' > "$WORK/api/no-tag.json"

CURRENT_API="file://$WORK/api/latest.json"

# run_install <release-dl-dir> <uname -s> <uname -m> <install-path> [args...]
# Runs the real install.sh with the fake platform/release and captures
# CODE / OUT (stdout) / ERR (stderr) / BOTH.
run_install() {
    local rel="$1" us="$2" um="$3" ipath="$4"
    shift 4
    local out err rc=0
    out="$(mktemp)"
    err="$(mktemp)"
    BEAD_E2E_UNAME_S="$us" \
    BEAD_E2E_UNAME_M="$um" \
    BEAD_INSTALL_PATH="$ipath" \
    BEAD_GITHUB_API="$CURRENT_API" \
    BEAD_DOWNLOAD_BASE="file://$rel" \
    PATH="$UNAME_BIN:$PATH" \
        bash "$INSTALL_SH" "$@" >"$out" 2>"$err" </dev/null || rc=$?
    CODE="$rc"
    OUT="$(cat "$out")"
    ERR="$(cat "$err")"
    BOTH="$(printf '%s\n%s\n' "$OUT" "$ERR")"
    rm -f "$out" "$err"
}

# ---------------------------------------------------------------------------
# Scenarios
# ---------------------------------------------------------------------------

s_release_layout() {
    section "Release artifact layout"
    local sumdir="$WORK/dl/$TAG"
    local lines
    lines="$(wc -l < "$sumdir/checksums.txt" | tr -d ' ')"
    expect_eq "checksums.txt has one entry per asset" "${#ASSETS[@]}" "$lines"
    local asset
    for asset in "${ASSETS[@]}"; do
        expect_file "release ships arch-suffixed asset $asset" "$sumdir/$asset"
        local n
        n="$(grep -c "  ${asset}\$" "$sumdir/checksums.txt" || true)"
        expect_eq "checksums.txt lists $asset exactly once" "1" "$n"
    done
    local bad
    bad="$(grep -cE '^[0-9a-f]{64}  bead-(x86_64|aarch64)-(unknown-linux-gnu|apple-darwin)$' \
        "$sumdir/checksums.txt" || true)"
    expect_eq "every checksums.txt line is '<sha256>  bead-<arch>-<os>'" "${#ASSETS[@]}" "$bad"
}

s_supported_platforms() {
    section "Supported platforms (happy path)"
    # uname -s : uname -m : asset the installer must select
    local combos=(
        "Linux:x86_64:bead-x86_64-unknown-linux-gnu"
        "Linux:aarch64:bead-aarch64-unknown-linux-gnu"
        "Darwin:x86_64:bead-x86_64-apple-darwin"
        "Darwin:aarch64:bead-aarch64-apple-darwin"
        "Linux:amd64:bead-x86_64-unknown-linux-gnu"
        "Darwin:arm64:bead-aarch64-apple-darwin"
    )
    local combo us um asset ipath label
    for combo in "${combos[@]}"; do
        IFS=: read -r us um asset <<<"$combo"
        label="$us/$um -> $asset"
        ipath="$WORK/scan-supported-$us-$um/bin/bead"
        run_install "$WORK/dl" "$us" "$um" "$ipath"
        expect_exit "install $label: installer succeeds" 0 "$CODE"
        expect_file "install $label: binary at BEAD_INSTALL_PATH" "$ipath"
        expect_eq "install $label: bead --version reports artifact platform" \
            "bead 0.2.6-e2e ($asset)" "$("$ipath" --version)"
        expect_eq "install $label: installed bytes match release artifact" \
            "$(hash_of "$WORK/dl/$TAG/$asset")" "$(hash_of "$ipath")"
        expect_contains "install $label: checksum verified" "$OUT" "Checksum verified"
        expect_contains "install $label: success message names the tag" "$OUT" \
            "bead $TAG installed successfully"
    done
}

s_unsupported_platforms() {
    section "Unsupported platforms (fail closed)"
    local ipath="$WORK/scan-unsupported-os/bin/bead"
    run_install "$WORK/dl" "FreeBSD" "x86_64" "$ipath"
    expect_nonzero_exit "unsupported OS (uname -s FreeBSD) aborts" "$CODE"
    expect_contains "unsupported OS: legible error" "$BOTH" "Unsupported OS: FreeBSD"
    expect_no_file "unsupported OS: nothing installed" "$ipath"

    ipath="$WORK/scan-unsupported-arch/bin/bead"
    run_install "$WORK/dl" "Linux" "riscv64" "$ipath"
    expect_nonzero_exit "unsupported arch (uname -m riscv64) aborts" "$CODE"
    expect_contains "unsupported arch: legible error" "$BOTH" "Unsupported architecture: riscv64"
    expect_no_file "unsupported arch: nothing installed" "$ipath"
}

s_checksum_mismatch() {
    section "Checksum mismatch (never skippable)"
    local rel="$WORK/rel-mismatch"
    cp -r "$WORK/dl" "$rel"
    # Tamper the artifact while keeping it a fully working binary: the bytes
    # no longer match checksums.txt, but `--version` still succeeds. A tamper
    # that also broke executability would let the installer's downstream
    # binary check mask a missing/removed checksum gate, so this scenario
    # would pass for the wrong reason.
    sed -i 's/0\.2\.6-e2e/0.2.6-pwned/' "$rel/$TAG/bead-x86_64-unknown-linux-gnu"
    grep -q '0\.2\.6-pwned' "$rel/$TAG/bead-x86_64-unknown-linux-gnu" \
        || { failed "mismatch setup: tamper did not land"; return; }

    local ipath="$WORK/scan-mismatch/bin/bead"
    run_install "$rel" "Linux" "x86_64" "$ipath"
    expect_nonzero_exit "tampered artifact aborts install" "$CODE"
    expect_contains "tampered artifact: legible error" "$BOTH" "Checksum mismatch for bead-x86_64-unknown-linux-gnu"
    expect_contains "tampered artifact: error explains expected vs got" "$BOTH" "expected:"
    expect_no_file "tampered artifact: nothing installed" "$ipath"

    # The documented security property: --skip-checksum only bypasses
    # UNAVAILABLE checksums, never an actual mismatch.
    run_install "$rel" "Linux" "x86_64" "$ipath" --skip-checksum
    expect_nonzero_exit "tampered artifact aborts even with --skip-checksum" "$CODE"
    expect_contains "mismatch error says it is never skippable" "$BOTH" "never skippable"
    expect_no_file "mismatch with --skip-checksum: nothing installed" "$ipath"
}

s_skip_checksum_still_verifies_when_available() {
    section "--skip-checksum with available checksums (fail-closed default)"
    local ipath="$WORK/scan-skip-ok/bin/bead"
    run_install "$WORK/dl" "Linux" "x86_64" "$ipath" --skip-checksum
    expect_exit "skip flag + healthy checksums: install succeeds" 0 "$CODE"
    expect_contains "skip flag + healthy checksums: still verified" "$OUT" "Checksum verified"
    expect_eq "skip flag + healthy checksums: bead --version works" \
        "bead 0.2.6-e2e (bead-x86_64-unknown-linux-gnu)" "$("$ipath" --version)"
}

s_missing_checksums_file() {
    section "checksums.txt unavailable (fail-closed)"
    local rel="$WORK/rel-no-checksums"
    cp -r "$WORK/dl" "$rel"
    rm "$rel/$TAG/checksums.txt"

    local ipath="$WORK/scan-no-checksums/bin/bead"
    run_install "$rel" "Linux" "x86_64" "$ipath"
    expect_nonzero_exit "missing checksums.txt aborts by default" "$CODE"
    expect_contains "missing checksums.txt: legible error" "$BOTH" "Could not download checksums.txt"
    expect_contains "missing checksums.txt: states security reason" "$BOTH" "aborted for security reasons"
    expect_no_file "missing checksums.txt: nothing installed" "$ipath"

    # --skip-checksum exists precisely for this case (checksums unavailable)
    run_install "$rel" "Linux" "x86_64" "$ipath" --skip-checksum
    expect_exit "missing checksums.txt + --skip-checksum: install proceeds" 0 "$CODE"
    expect_contains "skip path prints the security warning banner" "$BOTH" "SECURITY WARNING"
    expect_contains "skip path says verification was skipped" "$BOTH" "Skipping checksum verification"
    expect_contains "skip path still sanity-checks the binary via --version" "$OUT" "Verifying binary"
    expect_eq "skip path: installed bead --version works" \
        "bead 0.2.6-e2e (bead-x86_64-unknown-linux-gnu)" "$("$ipath" --version)"

    # The env-var form must behave like the flag
    rm -rf "$WORK/scan-no-checksums"
    export BEAD_SKIP_CHECKSUM=1
    run_install "$rel" "Linux" "x86_64" "$ipath"
    unset BEAD_SKIP_CHECKSUM
    expect_exit "missing checksums.txt + BEAD_SKIP_CHECKSUM=1: install proceeds" 0 "$CODE"
    expect_contains "env skip form prints the warning banner" "$BOTH" "SECURITY WARNING"
}

s_missing_checksum_entry() {
    section "Asset missing from checksums.txt"
    local rel="$WORK/rel-partial-checksums"
    cp -r "$WORK/dl" "$rel"
    printf '%s  %s\n' \
        "$(hash_of "$rel/$TAG/bead-aarch64-unknown-linux-gnu")" \
        "bead-aarch64-unknown-linux-gnu" > "$rel/$TAG/checksums.txt"

    local ipath="$WORK/scan-partial/bin/bead"
    run_install "$rel" "Linux" "x86_64" "$ipath"
    expect_nonzero_exit "no entry for the asset aborts" "$CODE"
    expect_contains "no entry: legible error" "$BOTH" "Could not find checksum for bead-x86_64-unknown-linux-gnu"
    expect_no_file "no entry: nothing installed" "$ipath"
}

s_missing_artifact() {
    section "Arch-suffixed artifact missing from the release"
    local rel="$WORK/rel-missing-asset"
    cp -r "$WORK/dl" "$rel"
    rm "$rel/$TAG/bead-x86_64-unknown-linux-gnu"

    local ipath="$WORK/scan-missing-asset/bin/bead"
    run_install "$rel" "Linux" "x86_64" "$ipath"
    expect_nonzero_exit "404 on the artifact aborts" "$CODE"
    expect_contains "404 abort names the artifact URL" "$BOTH" \
        "file://$rel/$TAG/bead-x86_64-unknown-linux-gnu"
    expect_no_file "404: nothing installed" "$ipath"
}

s_broken_artifact() {
    section "Artifact that fails --version"
    local rel="$WORK/rel-broken-binary"
    cp -r "$WORK/dl" "$rel"
    printf '#!/bin/sh\nexit 1\n' > "$rel/$TAG/bead-x86_64-unknown-linux-gnu"
    chmod +x "$rel/$TAG/bead-x86_64-unknown-linux-gnu"
    # Re-checksum so the failure is the binary check, not the checksum check
    write_checksums "$rel"

    local ipath="$WORK/scan-broken/bin/bead"
    run_install "$rel" "Linux" "x86_64" "$ipath"
    expect_nonzero_exit "artifact failing --version aborts after checksum passes" "$CODE"
    expect_contains "broken artifact: legible error" "$BOTH" "not executable or corrupted"
    expect_no_file "broken artifact: nothing installed" "$ipath"
}

s_cli_surface() {
    section "Installer CLI surface"
    local ipath="$WORK/scan-cli/bin/bead"

    run_install "$WORK/dl" "Linux" "x86_64" "$ipath" --help
    expect_exit "--help exits 0" 0 "$CODE"
    expect_contains "--help prints usage" "$OUT" "Usage: install.sh"

    run_install "$WORK/dl" "Linux" "x86_64" "$ipath" --definitely-not-a-flag
    expect_nonzero_exit "unknown option aborts" "$CODE"
    expect_contains "unknown option: legible error" "$BOTH" "Unknown option: --definitely-not-a-flag"
    expect_no_file "unknown option: nothing installed" "$ipath"
}

s_tag_resolution_failures() {
    section "Latest-tag resolution failures"
    local ipath="$WORK/scan-api/bin/bead"

    CURRENT_API="file://$WORK/api/no-tag.json"
    run_install "$WORK/dl" "Linux" "x86_64" "$ipath"
    expect_nonzero_exit "API document without tag_name aborts" "$CODE"
    expect_contains "no tag_name: legible error" "$BOTH" "Failed to determine the latest version"
    expect_no_file "no tag_name: nothing installed" "$ipath"

    CURRENT_API="file://$WORK/api/does-not-exist.json"
    run_install "$WORK/dl" "Linux" "x86_64" "$ipath"
    expect_nonzero_exit "unreachable release API aborts" "$CODE"
    expect_contains "unreachable API: legible error" "$BOTH" "Could not reach the GitHub API"
    expect_no_file "unreachable API: nothing installed" "$ipath"

    CURRENT_API="file://$WORK/api/latest.json"
}

# ---------------------------------------------------------------------------
# Optional live check against the real latest GitHub release
# ---------------------------------------------------------------------------

s_live_release() {
    section "Live latest GitHub release (BEAD_INSTALL_E2E_LIVE=1)"
    if [[ "${BEAD_INSTALL_E2E_LIVE:-}" != "1" ]]; then
        skipped "live release check (set BEAD_INSTALL_E2E_LIVE=1 to enable)"
        return
    fi

    local api="https://api.github.com/repos/jedarden/bead-rs/releases/latest"
    local doc tag
    if ! doc="$(curl -fsSL "$api" 2>/dev/null)"; then
        skipped "live: cannot reach $api"
        return
    fi
    tag="$(sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' <<<"$doc")"
    if [[ -z "$tag" ]]; then
        failed "live: could not parse tag_name from $api"
        return
    fi
    note "latest release tag: $tag"

    local base="https://github.com/jedarden/bead-rs/releases/download/$tag"
    local sums="$WORK/live-checksums.txt"
    if ! curl -fsSL -o "$sums" "$base/checksums.txt" 2>/dev/null; then
        failed "live: release $tag has no checksums.txt"
        return
    fi

    local asset
    for asset in "${ASSETS[@]}"; do
        expect_contains "live $tag: checksums.txt covers $asset" \
            "$(cat "$sums")" "$asset"
    done

    # Verify the real artifact for the host platform end-to-end
    local us um os arch want
    us="$(uname -s)"
    um="$(uname -m)"
    case "$us" in
        Linux*)  os="unknown-linux-gnu" ;;
        Darwin*) os="apple-darwin" ;;
        *)       skipped "live binary check: host OS $us unsupported"; return ;;
    esac
    case "$um" in
        x86_64|amd64) arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *)       skipped "live binary check: host arch $um unsupported"; return ;;
    esac
    want="bead-$arch-$os"

    local bin="$WORK/live-bead"
    curl -fsSL -o "$bin" "$base/$want" 2>/dev/null \
        || { failed "live: could not download $want"; return; }
    expect_eq "live $tag: $want matches its checksums.txt entry" \
        "$(grep "  ${want}\$" "$sums" | awk '{print $1}')" "$(hash_of "$bin")"
    chmod +x "$bin"
    expect_contains "live $tag: real bead --version answers" "$("$bin" --version)" "bead "
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    echo "bead-rs installer/release e2e suite"
    echo "installer: $INSTALL_SH"
    echo "work dir:  $WORK"

    s_release_layout
    s_supported_platforms
    s_unsupported_platforms
    s_checksum_mismatch
    s_skip_checksum_still_verifies_when_available
    s_missing_checksums_file
    s_missing_checksum_entry
    s_missing_artifact
    s_broken_artifact
    s_cli_surface
    s_tag_resolution_failures
    s_live_release

    printf '\n==========================================\n'
    printf 'Results: %d passed, %d failed, %d skipped\n' "$PASSED" "$FAILED" "$SKIPPED"
    printf '==========================================\n'

    if [[ "$FAILED" -gt 0 ]]; then
        echo "Last installer output for debugging:"
        printf 'exit=%s\nstdout:\n%s\nstderr:\n%s\n' "$CODE" "$OUT" "$ERR"
        exit 1
    fi
    exit 0
}

main "$@"
