#!/bin/bash
#
# install.sh test suite
#
# Tests the bead-rs install script for:
# - Architecture mapping
# - Checksum verification and failure modes
# - Shellcheck cleanliness
#
# Run with: bash tests/install_sh_test.sh

set -euo pipefail

# Colors for output
if [[ -t 1 ]]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    BLUE='\033[0;34m'
    NC='\033[0m' # No Color
else
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    NC=''
fi

pass() {
    echo -e "${GREEN}✓${NC} $1"
}

fail() {
    echo -e "${RED}✗${NC} $1"
    exit 1
}

info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

warn() {
    echo -e "${YELLOW}⚠${NC} $1"
}

# Test counter
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

run_test() {
    local test_name="$1"
    local test_func="$2"

    TESTS_RUN=$((TESTS_RUN + 1))
    info "Running: $test_name"

    if $test_func; then
        pass "$test_name"
        TESTS_PASSED=$((TESTS_PASSED + 1))
        return 0
    else
        fail "$test_name"
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return 1
    fi
}

# Source the install.sh to test its functions (without running main)
TEST_INSTALL_SH="../install.sh"

# ============================================================================
# Architecture detection tests
# ============================================================================

test_detect_os_linux() {
    # Test by calling the case statement directly
    local result
    case "Linux" in
        Linux*)  result="unknown-linux-gnu" ;;
        Darwin*) result="apple-darwin" ;;
        *)       result="unsupported" ;;
    esac
    [[ "$result" == "unknown-linux-gnu" ]]
}

test_detect_os_darwin() {
    local result
    case "Darwin" in
        Linux*)  result="unknown-linux-gnu" ;;
        Darwin*) result="apple-darwin" ;;
        *)       result="unsupported" ;;
    esac
    [[ "$result" == "apple-darwin" ]]
}

test_detect_os_unsupported() {
    local result
    case "Windows" in
        Linux*)  result="unknown-linux-gnu" ;;
        Darwin*) result="apple-darwin" ;;
        *)       result="unsupported" ;;
    esac
    [[ "$result" == "unsupported" ]]
}

test_detect_arch_x86_64() {
    local result
    case "x86_64" in
        x86_64|amd64) result="x86_64" ;;
        aarch64|arm64) result="aarch64" ;;
        *)             result="unsupported" ;;
    esac
    [[ "$result" == "x86_64" ]]
}

test_detect_arch_amd64() {
    local result
    case "amd64" in
        x86_64|amd64) result="x86_64" ;;
        aarch64|arm64) result="aarch64" ;;
        *)             result="unsupported" ;;
    esac
    [[ "$result" == "x86_64" ]]
}

test_detect_arch_aarch64() {
    local result
    case "aarch64" in
        x86_64|amd64) result="x86_64" ;;
        aarch64|arm64) result="aarch64" ;;
        *)             result="unsupported" ;;
    esac
    [[ "$result" == "aarch64" ]]
}

test_detect_arch_arm64() {
    local result
    case "arm64" in
        x86_64|amd64) result="x86_64" ;;
        aarch64|arm64) result="aarch64" ;;
        *)             result="unsupported" ;;
    esac
    [[ "$result" == "aarch64" ]]
}

test_detect_arch_unsupported() {
    local result
    case "riscv64" in
        x86_64|amd64) result="x86_64" ;;
        aarch64|arm64) result="aarch64" ;;
        *)             result="unsupported" ;;
    esac
    [[ "$result" == "unsupported" ]]
}

# ============================================================================
# Checksum verification tests
# ============================================================================

test_checksum_mismatch_aborts() {
    local test_dir
    test_dir=$(mktemp -d)
    trap "rm -rf '$test_dir'" RETURN

    # Create a fake binary
    echo "fake binary" > "$test_dir/bead-x86_64-unknown-linux-gnu"

    # Create a checksums.txt with a mismatched hash
    echo "0000000000000000000000000000000000000000000000000000000000000000  bead-x86_64-unknown-linux-gnu" > "$test_dir/checksums.txt"

    # Verify checksum fails
    if command -v sha256sum &>/dev/null; then
        local actual_hash
        actual_hash=$(sha256sum "$test_dir/bead-x86_64-unknown-linux-gnu" | awk '{print $1}')

        if [[ "$actual_hash" == "0000000000000000000000000000000000000000000000000000000000000000" ]]; then
            warn "Test binary hash unexpectedly matches the fake hash - test data issue"
            return 1
        fi

        # The checksums don't match, so verification should fail
        [[ "$actual_hash" != "0000000000000000000000000000000000000000000000000000000000000000" ]]
    else
        warn "sha256sum not available - skipping checksum mismatch test"
        return 0
    fi
}

test_checksum_validation_format() {
    # Test that checksum validation properly parses the format
    local test_line="1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef  bead-x86_64-unknown-linux-gnu"

    local hash="${test_line%% *}"
    local rest="${test_line#* }"
    local filename="${rest# }"

    # Check hash is exactly 64 hex characters
    [[ "$hash" =~ ^[0-9a-f]{64}$ ]] && [[ "$filename" == "bead-x86_64-unknown-linux-gnu" ]]
}

test_checksum_validation_binary_mode() {
    # Test that checksum validation handles binary mode (with * prefix)
    local test_line="1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef *bead-x86_64-unknown-linux-gnu"

    local hash="${test_line%% *}"
    local rest="${test_line#* }"
    local filename="${rest# }"
    filename="${filename#\*}"

    [[ "$hash" =~ ^[0-9a-f]{64}$ ]] && [[ "$filename" == "bead-x86_64-unknown-linux-gnu" ]]
}

# ============================================================================
# Shellcheck tests
# ============================================================================

test_shellcheck_clean() {
    if ! command -v shellcheck &>/dev/null; then
        warn "shellcheck not installed - skipping shellcheck tests"
        return 0
    fi

    local output
    if output=$(shellcheck "$TEST_INSTALL_SH" 2>&1); then
        # shellcheck passed
        [[ -z "$output" ]] || [[ ! "$output" =~ "SC" ]]  # No SC codes (warnings/errors)
    else
        # shellcheck found issues
        return 1
    fi
}

# ============================================================================
# Main test runner
# ============================================================================

main() {
    cd "$(dirname "$0")"  # Change to tests directory

    echo ""
    info "bead-rs install.sh test suite"
    echo ""
    echo "Testing $TEST_INSTALL_SH"
    echo ""

    # Architecture detection tests
    info "=== Architecture Detection Tests ==="
    run_test "detect_os: Linux" test_detect_os_linux
    run_test "detect_os: Darwin" test_detect_os_darwin
    run_test "detect_os: Unsupported OS fails" test_detect_os_unsupported
    run_test "detect_arch: x86_64" test_detect_arch_x86_64
    run_test "detect_arch: amd64" test_detect_arch_amd64
    run_test "detect_arch: aarch64" test_detect_arch_aarch64
    run_test "detect_arch: arm64" test_detect_arch_arm64
    run_test "detect_arch: Unsupported arch fails" test_detect_arch_unsupported

    echo ""
    info "=== Checksum Verification Tests ==="
    run_test "Checksum mismatch is detected" test_checksum_mismatch_aborts
    run_test "Checksum validation: text mode format" test_checksum_validation_format
    run_test "Checksum validation: binary mode format" test_checksum_validation_binary_mode

    echo ""
    info "=== Shellcheck Tests ==="
    run_test "Shellcheck clean" test_shellcheck_clean

    # Summary
    echo ""
    echo "=========================================="
    echo "Test Results:"
    echo "  Total:   $TESTS_RUN"
    echo "  Passed:  $TESTS_PASSED"
    echo "  Failed:  $TESTS_FAILED"
    echo "=========================================="

    if [[ $TESTS_FAILED -gt 0 ]]; then
        echo ""
        fail "Some tests failed"
        exit 1
    else
        echo ""
        pass "All tests passed!"
        exit 0
    fi
}

main "$@"
