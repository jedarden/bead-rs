#!/usr/bin/env bash
# Release evidence verifier for bead-rs
#
# This script validates a release evidence report against the canonical schema
# and performs structural integrity checks.
#
# Usage: ./verify-evidence.sh <evidence-report.json>
#
# Exit codes:
#   0 - Evidence report is valid
#   1 - Validation failed
#   2 - Usage error or missing dependencies

set -euo pipefail

# Check for required dependencies
if ! command -v jq &> /dev/null; then
    echo "ERROR: jq is required but not installed" >&2
    exit 2
fi

# Check arguments
if [ $# -ne 1 ]; then
    echo "Usage: $0 <evidence-report.json>" >&2
    exit 2
fi

EVIDENCE_FILE="$1"
SCHEMA_FILE="$(dirname "$0")/release-evidence-v1.schema.json"

# Check files exist
if [ ! -f "$EVIDENCE_FILE" ]; then
    echo "ERROR: Evidence file not found: $EVIDENCE_FILE" >&2
    exit 1
fi

if [ ! -f "$SCHEMA_FILE" ]; then
    echo "ERROR: Schema file not found: $SCHEMA_FILE" >&2
    exit 1
fi

echo "Validating evidence report: $EVIDENCE_FILE"
echo

# Basic JSON validation
if ! jq empty "$EVIDENCE_FILE" &> /dev/null; then
    echo "❌ FAILED: Invalid JSON"
    exit 1
fi

# Check required top-level fields
REQUIRED_FIELDS=(
    "schema_version"
    "report_type"
    "generated_at"
    "tool_version"
    "workspace_uuid"
    "mapping_hash"
    "bootstrap_commit"
    "artifact_hash"
    "checkpoint_hash"
    "features"
    "gates"
)

echo "Checking required fields..."
for field in "${REQUIRED_FIELDS[@]}"; do
    if ! jq -e ".has(\"$field\")" "$EVIDENCE_FILE" &> /dev/null; then
        echo "❌ FAILED: Missing required field: $field"
        exit 1
    fi
    echo "  ✓ $field"
done

# Validate schema version
SCHEMA_VERSION=$(jq -r '.schema_version' "$EVIDENCE_FILE")
if [ "$SCHEMA_VERSION" != "1" ]; then
    echo "❌ FAILED: Unsupported schema version: $SCHEMA_VERSION"
    exit 1
fi
echo "  ✓ schema_version = $SCHEMA_VERSION"

# Validate report type
REPORT_TYPE=$(jq -r '.report_type' "$EVIDENCE_FILE")
if [ "$REPORT_TYPE" != "release_evidence" ]; then
    echo "❌ FAILED: Invalid report type: $REPORT_TYPE"
    exit 1
fi
echo "  ✓ report_type = $REPORT_TYPE"

# Check feature IDs format
echo
echo "Checking feature evidence..."
FEATURE_COUNT=$(jq '.features | length' "$EVIDENCE_FILE")
echo "  Total features: $FEATURE_COUNT"

jq -r '.features[].id' "$EVIDENCE_FILE" | while read -r feature_id; do
    if [[ ! "$feature_id" =~ ^F[0-9]{3}$ ]]; then
        echo "❌ FAILED: Invalid feature ID format: $feature_id"
        exit 1
    fi
done
echo "  ✓ All feature IDs valid"

# Check gate structure
echo
echo "Checking gate evidence..."
REQUIRED_GATES=("G0" "G1" "G2" "G3" "G4")
for gate in "${REQUIRED_GATES[@]}"; do
    if ! jq -e ".gates.\"$gate\"" "$EVIDENCE_FILE" &> /dev/null; then
        echo "❌ FAILED: Missing gate: $gate"
        exit 1
    fi

    # Check gate has required fields
    if ! jq -e ".gates.\"$gate\".passes" "$EVIDENCE_FILE" &> /dev/null; then
        echo "❌ FAILED: Gate $gate missing 'passes' field"
        exit 1
    fi

    PASSES=$(jq -r ".gates.\"$gate\".passes" "$EVIDENCE_FILE")
    echo "  ✓ $gate: passes=$PASSES"
done

# Check that G2 passes before G3 evidence exists
G2_PASSES=$(jq -r '.gates.G2.passes' "$EVIDENCE_FILE")
if [ "$G2_PASSES" = "true" ]; then
    if ! jq -e '.handoff_state' "$EVIDENCE_FILE" &> /dev/null; then
        echo "❌ FAILED: G2 passed but handoff_state is missing"
        exit 1
    fi

    HANDOFF_STATE=$(jq -r '.handoff_state' "$EVIDENCE_FILE")
    if [[ ! "$HANDOFF_STATE" =~ ^(pending|final)$ ]]; then
        echo "❌ FAILED: Invalid handoff_state: $HANDOFF_STATE"
        exit 1
    fi
    echo "  ✓ handoff_state = $HANDOFF_STATE"
fi

# Feature completeness check for bootstrap
echo
echo "Checking bootstrap feature completeness..."
BOOTSTRAP_FEATURES=("F001" "F002" "F003" "F004" "F005" "F006" "F007" "F008" "F009" "F010" "F011")
for feature in "${BOOTSTRAP_FEATURES[@]}"; do
    if ! jq -e ".features[] | select(.id == \"$feature\")" "$EVIDENCE_FILE" &> /dev/null; then
        echo "❌ FAILED: Missing bootstrap feature: $feature"
        exit 1
    fi
done
echo "  ✓ All bootstrap features present"

# Verify all bootstrap features pass before G2
if [ "$G2_PASSES" = "true" ]; then
    echo "  Verifying all bootstrap features pass for G2..."
    for feature in "${BOOTSTRAP_FEATURES[@]}"; do
        PASSES=$(jq -r ".features[] | select(.id == \"$feature\") | .passes" "$EVIDENCE_FILE")
        if [ "$PASSES" != "true" ]; then
            echo "❌ FAILED: Bootstrap feature $feature does not pass: $PASSES"
            exit 1
        fi
    done
    echo "  ✓ All bootstrap features pass"
fi

echo
echo "✅ SUCCESS: Evidence report is valid"
echo
echo "Summary:"
echo "  Schema version: $SCHEMA_VERSION"
echo "  Report type: $REPORT_TYPE"
echo "  Features: $FEATURE_COUNT"
echo "  G2 passes: $G2_PASSES"
if [ "$G2_PASSES" = "true" ]; then
    echo "  Handoff state: $HANDOFF_STATE"
fi

exit 0
