#!/bin/bash
# Validate attempt-resolution checkpoint fixtures

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OLD_FIXTURE="$SCRIPT_DIR/old"
NEW_FIXTURE="$SCRIPT_DIR/new"

echo "=== Validating Attempt-Resolution Checkpoint Fixtures ==="
echo ""

# Function to validate JSONL syntax
validate_jsonl() {
    local file=$1
    echo "Checking JSONL syntax: $file"
    local line_num=0
    while IFS= read -r line; do
        line_num=$((line_num + 1))
        if ! echo "$line" | jq empty > /dev/null 2>&1; then
            echo "❌ Invalid JSON at line $line_num: $line"
            exit 1
        fi
    done < "$file"
    echo "✓ All $line_num lines are valid JSON"
}

# Function to count record types
count_records() {
    local file=$1
    echo "Record counts for $file:"
    local total=$(wc -l < "$file")
    local issues=$(jq -r 'select(.record_type == "issue") | .issue.id' "$file" 2>/dev/null | wc -l)
    local attempts=$(jq -r 'select(.record_type == "attempt_outcome") | .attempt_outcome.attempt_id' "$file" 2>/dev/null | wc -l)
    echo "  Total records: $total"
    echo "  Issue records: $issues"
    echo "  Attempt outcome records: $attempts"
}

# Function to validate manifest
validate_manifest() {
    local manifest=$1
    local format=$2
    echo "Validating manifest: $manifest"

    if ! jq empty "$manifest" > /dev/null 2>&1; then
        echo "❌ Invalid JSON in manifest"
        exit 1
    fi

    local schema_version=$(jq -r '.schema_version' "$manifest")
    local total_records=$(jq -r '.total_record_count' "$manifest")
    local issue_count=$(jq -r '.issue_count' "$manifest")

    echo "  ✓ Schema version: $schema_version"
    echo "  ✓ Total records: $total_records"
    echo "  ✓ Issue count: $issue_count"

    if [ "$format" = "new" ]; then
        local attempt_count=$(jq -r '.attempt_outcome_count' "$manifest" 2>/dev/null || echo "missing")
        if [ "$attempt_count" = "missing" ]; then
            echo "  ❌ attempt_outcome_count field missing"
            exit 1
        fi
        echo "  ✓ Attempt outcome count: $attempt_count"
    else
        if jq -e '.attempt_outcome_count' "$manifest" > /dev/null 2>&1; then
            echo "  ⚠️  attempt_outcome_count field present (unexpected for old format)"
        else
            echo "  ✓ No attempt_outcome_count field (expected for old format)"
        fi
    fi
}

# Function to validate attempt outcome fields
validate_attempt_outcomes() {
    local file=$1
    echo "Validating attempt_outcome records in $file"

    local required_fields=(
        "schema_ref"
        "attempt_id"
        "issue_id"
        "outcome"
        "action"
        "reason"
        "canonical_request_hash"
        "resulting_issue_revision"
        "resulting_state"
        "resulting_attempt_tier"
        "receipt_id"
        "actor"
        "created_at"
    )

    local attempt_count=$(jq -r 'select(.record_type == "attempt_outcome") | .attempt_outcome.attempt_id' "$file" 2>/dev/null | wc -l)

    if [ "$attempt_count" -eq 0 ]; then
        echo "  ✓ No attempt_outcome records (expected for old format)"
        return
    fi

    echo "  Checking $attempt_count attempt_outcome records..."

    while IFS= read -r line; do
        if echo "$line" | jq -e '.attempt_outcome' > /dev/null 2>&1; then
            for field in "${required_fields[@]}"; do
                if ! echo "$line" | jq -e ".attempt_outcome.$field" > /dev/null 2>&1; then
                    echo "  ❌ Missing required field: $field"
                    exit 1
                fi
            done
        fi
    done < "$file"

    echo "  ✓ All required fields present in all attempt_outcome records"
}

# Validate old format
echo "### Old Format (Pre-Attempt-Resolution) ###"
echo ""
validate_jsonl "$OLD_FIXTURE/checkpoint.jsonl"
echo ""
count_records "$OLD_FIXTURE/checkpoint.jsonl"
echo ""
validate_manifest "$OLD_FIXTURE/current.json" "old"
echo ""
validate_attempt_outcomes "$OLD_FIXTURE/checkpoint.jsonl"
echo ""

# Validate new format
echo "### New Format (With Attempt-Resolution) ###"
echo ""
validate_jsonl "$NEW_FIXTURE/checkpoint.jsonl"
echo ""
count_records "$NEW_FIXTURE/checkpoint.jsonl"
echo ""
validate_manifest "$NEW_FIXTURE/current.json" "new"
echo ""
validate_attempt_outcomes "$NEW_FIXTURE/checkpoint.jsonl"
echo ""

# Verify record count consistency
echo "### Record Count Consistency ###"
echo ""
old_total=$(jq -r '.total_record_count' "$OLD_FIXTURE/current.json")
new_total=$(jq -r '.total_record_count' "$NEW_FIXTURE/current.json")
new_issues=$(jq -r '.issue_count' "$NEW_FIXTURE/current.json")
new_attempts=$(jq -r '.attempt_outcome_count' "$NEW_FIXTURE/current.json")

echo "Old format total records: $old_total"
echo "New format total records: $new_total"
echo "New format issue count: $new_issues"
echo "New format attempt count: $new_attempts"

if [ "$new_total" -ne $((new_issues + new_attempts)) ]; then
    echo "❌ Record count mismatch in new format"
    echo "   Expected total: $((new_issues + new_attempts))"
    echo "   Actual total: $new_total"
    exit 1
fi

echo "✓ Record counts are consistent"
echo ""

# Check for documentation
echo "### Documentation ###"
echo ""
for doc in README.md FORMAT_DIFFERENCES.md VALIDATION_REPORT.md; do
    if [ -f "$SCRIPT_DIR/$doc" ]; then
        echo "✓ $doc exists"
    else
        echo "❌ $doc missing"
        exit 1
    fi
done

echo ""
echo "=== All Validations Passed ==="
echo ""
echo "Fixtures are ready for testing."
echo ""
echo "Run tests with:"
echo "  cargo test --test attempt_outcome_round_trip"
echo "  cargo test --test pinned_binary_capability"
