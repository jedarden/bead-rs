#!/usr/bin/env bash
# Verification script for attempt-outcome-v1 specification and fixtures
# This script performs non-interactive validation checks

SPEC_FILE="research/specs/attempt-outcome-v1.md"
FIXTURE_DIR="research/fixtures/attempt-outcome-v1"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Counters
PASS=0
FAIL=0
WARN=0

# Helper functions
pass() {
    echo -e "${GREEN}✓${NC} $1"
    ((PASS++))
}

fail() {
    echo -e "${RED}✗${NC} $1"
    ((FAIL++))
}

warn() {
    echo -e "${YELLOW}⚠${NC} $1"
    ((WARN++))
}

# Check if we're in the bead-rs root
if [ ! -f "$SPEC_FILE" ]; then
    echo "Error: $SPEC_FILE not found. Run this script from bead-rs root."
    exit 1
fi

echo "=== Attempt Outcome v1 Specification Verification ==="
echo ""

# 1. Specification file exists and is non-empty
echo "1. Checking specification file..."
if [ -s "$SPEC_FILE" ]; then
    pass "Specification file exists and is non-empty"
else
    fail "Specification file is missing or empty"
fi

# 2. All fixture files exist
echo ""
echo "2. Checking fixture files..."
for fixture in \
    "$FIXTURE_DIR/request.json" \
    "$FIXTURE_DIR/receipt.json" \
    "$FIXTURE_DIR/checkpoint-record.jsonl" \
    "$FIXTURE_DIR/audit-event.json" \
    "$FIXTURE_DIR/capabilities.json" \
    "$FIXTURE_DIR/README.md"
do
    if [ -f "$fixture" ]; then
        pass "Fixture exists: $(basename "$fixture")"
    else
        fail "Fixture missing: $(basename "$fixture")"
    fi
done

# 3. JSON schemas are valid
echo ""
echo "3. Validating JSON schemas..."
if command -v jq &> /dev/null; then
    for schema in "$FIXTURE_DIR/request.json" "$FIXTURE_DIR/receipt.json"; do
        if jq empty "$schema" 2>/dev/null; then
            pass "Valid JSON: $(basename "$schema")"
        else
            fail "Invalid JSON: $(basename "$schema")"
        fi
    done
else
    warn "jq not available, skipping JSON validation"
fi

# 4. Specification contains required sections
echo ""
echo "4. Checking specification sections..."
required_sections=(
    "Scope and vocabulary"
    "Attempt identity"
    "Canonical request hash"
    "Outcome classification"
    "Lifecycle actions"
    "Outcome-action compatibility"
    "Failure epoch and tier semantics"
    "Revision and fencing conflicts"
    "Idempotent replay"
    "Evidence references"
    "Bounded metadata"
    "Checkpoint representation"
    "Service API"
    "CLI contract"
    "Capability negotiation"
    "Schema references"
    "Conformance requirements"
    "Security and privacy"
)

for section in "${required_sections[@]}"; do
    if grep -qi "##.*$section" "$SPEC_FILE"; then
        pass "Section exists: $section"
    else
        fail "Section missing: $section"
    fi
done

# 5. Specification mentions all ADRs
echo ""
echo "5. Checking ADR references..."
for adr in "ADR-010" "ADR-011" "ADR-012"; do
    if grep -q "$adr" "$SPEC_FILE"; then
        pass "References $adr"
    else
        fail "Missing reference to $adr"
    fi
done

# 6. Fixture README exists
echo ""
echo "6. Checking fixture documentation..."
if [ -f "$FIXTURE_DIR/README.md" ]; then
    pass "Fixture README exists"
else
    fail "Fixture README missing"
fi

# 7. Checkpoint record format
echo ""
echo "7. Checking checkpoint record format..."
if [ -f "$FIXTURE_DIR/checkpoint-record.jsonl" ]; then
    if grep -q '"record_type":"attempt_outcome"' "$FIXTURE_DIR/checkpoint-record.jsonl"; then
        pass "Checkpoint record has correct record_type"
    else
        fail "Checkpoint record missing record_type"
    fi

    if grep -q '"attempt_outcome"' "$FIXTURE_DIR/checkpoint-record.jsonl"; then
        pass "Checkpoint record has attempt_outcome field"
    else
        fail "Checkpoint record missing attempt_outcome field"
    fi
fi

# 8. Audit event format
echo ""
echo "8. Checking audit event format..."
if [ -f "$FIXTURE_DIR/audit-event.json" ]; then
    if jq -e '.event_type == "attempt_resolved"' "$FIXTURE_DIR/audit-event.json" &>/dev/null; then
        pass "Audit event has correct event_type"
    else
        fail "Audit event has incorrect event_type"
    fi
fi

# 9. Capabilities fragment
echo ""
echo "9. Checking capabilities fragment..."
if [ -f "$FIXTURE_DIR/capabilities.json" ]; then
    if jq -e '.attempt_outcome.supported == true' "$FIXTURE_DIR/capabilities.json" &>/dev/null; then
        pass "Capabilities shows attempt_outcome.supported"
    else
        fail "Capabilities missing attempt_outcome.supported"
    fi
fi

# 10. Compute and display checksums
echo ""
echo "10. Computing checksums..."
echo "Specification checksum:"
sha256sum "$SPEC_FILE" | awk '{print "  " $1 "  " $2}'
echo ""
echo "Fixture checksums:"
for fixture in "$FIXTURE_DIR"/*.json "$FIXTURE_DIR"/*.jsonl "$FIXTURE_DIR"/*.md; do
    if [ -f "$fixture" ]; then
        sha256sum "$fixture" | awk '{print "  " $1 "  " $2}'
    fi
done
pass "Checksums computed"

# Summary
echo ""
echo "=== Verification Summary ==="
echo -e "${GREEN}Passed:${NC} $PASS"
echo -e "${RED}Failed:${NC} $FAIL"
echo -e "${YELLOW}Warnings:${NC} $WARN"
echo ""

if [ $FAIL -eq 0 ]; then
    echo "✓ All critical checks passed"
    exit 0
else
    echo "✗ Some checks failed - review required"
    exit 1
fi
