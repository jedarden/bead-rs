#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
CHECKPOINT="$ROOT/checkpoint.jsonl"
POINTER="$ROOT/current.json"
EXPECTED="$ROOT/expected.json"
RESOURCE_KEY="$ROOT/resource-key.json"

command -v jq >/dev/null || { echo "verify.sh: jq is required" >&2; exit 1; }
command -v sha256sum >/dev/null || { echo "verify.sh: sha256sum is required" >&2; exit 1; }

[[ "$(wc -l < "$CHECKPOINT")" -eq 5 ]] || {
  echo "verify.sh: checkpoint must contain exactly five LF-terminated records" >&2
  exit 1
}

while IFS= read -r line; do
  jq -e . >/dev/null <<<"$line"
done < "$CHECKPOINT"
jq -e . "$POINTER" >/dev/null
jq -e . "$EXPECTED" >/dev/null
jq -e . "$RESOURCE_KEY" >/dev/null
jq -e '
  (.unknown_members | keys | sort) == [
    "/event/1/x-fixture-event",
    "/event/2/x-fixture-event",
    "/issue/bead-unknown-a/data/fixture.namespace/x-fixture-structured-data",
    "/issue/bead-unknown-a/dependencies/0/x-fixture-dependency",
    "/issue/bead-unknown-a/external_references/0/x-fixture-external-reference",
    "/issue/bead-unknown-a/x-fixture-issue",
    "/issue/bead-unknown-b/x-fixture-issue",
    "/pointer/current.json/x-fixture-pointer",
    "/provenance_receipt/receipt-unknown-fields-v1/x-fixture-receipt",
    "/resource_key/build-cache~1unknown-fields/x-fixture-resource-key"
  ] and
  ([.unknown_members[] | tojson] | unique | length) == 10
' "$EXPECTED" >/dev/null

checkpoint_sha=$(sha256sum "$CHECKPOINT" | awk '{print $1}')
native_root="objects/$checkpoint_sha.jsonl"
[[ -f "$ROOT/$native_root" ]] || {
  echo "verify.sh: native restore root is missing: $native_root" >&2
  exit 1
}
cmp -s "$CHECKPOINT" "$ROOT/$native_root" || {
  echo "verify.sh: native restore root differs from checkpoint.jsonl" >&2
  exit 1
}

jq -e --slurpfile expected "$EXPECTED" '
  [.[].record_type] == ["issue", "issue", "event", "event", "provenance_receipt"] and
  ([.[] | select(.record_type == "issue")] | length) == 2 and
  ([.[] | select(.record_type == "event")] | length) == 2 and
  ([.[] | select(.record_type == "provenance_receipt")] | length) == 1 and
  .[0].issue.id == $expected[0].known_semantics.issue_a.id and
  .[0].issue.title == $expected[0].known_semantics.issue_a.title and
  .[0].issue.dependencies[0].blocker == $expected[0].known_semantics.issue_a.dependency.blocker and
  .[0].issue.external_references[0].value == $expected[0].known_semantics.issue_a.external_reference.value and
  .[0].issue.data["fixture.namespace"].value == $expected[0].known_semantics.issue_a.structured_data.value and
  .[0].issue.resource_keys == $expected[0].known_semantics.issue_a.resource_keys and
  .[1].issue.id == $expected[0].known_semantics.issue_b.id and
  .[2].event.origin_event_sequence == $expected[0].known_semantics.event_a.origin_event_sequence and
  .[3].event.origin_event_sequence == $expected[0].known_semantics.event_b.origin_event_sequence and
  .[4].provenance_receipt.receipt_id == $expected[0].known_semantics.receipt.receipt_id
' <(jq -s . "$CHECKPOINT") >/dev/null

jq -e --slurpfile expected "$EXPECTED" '
  .[0].issue.id == "bead-unknown-a" and
  .[0].issue["x-fixture-issue"] == $expected[0].unknown_members["/issue/bead-unknown-a/x-fixture-issue"] and
  .[0].issue.dependencies[0]["x-fixture-dependency"] == $expected[0].unknown_members["/issue/bead-unknown-a/dependencies/0/x-fixture-dependency"] and
  .[0].issue.external_references[0]["x-fixture-external-reference"] == $expected[0].unknown_members["/issue/bead-unknown-a/external_references/0/x-fixture-external-reference"] and
  .[0].issue.data["fixture.namespace"]["x-fixture-structured-data"] == $expected[0].unknown_members["/issue/bead-unknown-a/data/fixture.namespace/x-fixture-structured-data"] and
  .[1].issue["x-fixture-issue"] == $expected[0].unknown_members["/issue/bead-unknown-b/x-fixture-issue"]
' <(jq -s . "$CHECKPOINT") >/dev/null

jq -e --slurpfile expected "$EXPECTED" '
  .[2].event["x-fixture-event"] == $expected[0].unknown_members["/event/1/x-fixture-event"] and
  .[3].event["x-fixture-event"] == $expected[0].unknown_members["/event/2/x-fixture-event"] and
  .[4].provenance_receipt["x-fixture-receipt"] == $expected[0].unknown_members["/provenance_receipt/receipt-unknown-fields-v1/x-fixture-receipt"]
' <(jq -s . "$CHECKPOINT") >/dev/null

jq -e --slurpfile expected "$EXPECTED" '.resource_key["x-fixture-resource-key"] == $expected[0].unknown_members["/resource_key/build-cache~1unknown-fields/x-fixture-resource-key"]' "$RESOURCE_KEY" >/dev/null
jq -e --slurpfile expected "$EXPECTED" '."x-fixture-pointer" == $expected[0].unknown_members["/pointer/current.json/x-fixture-pointer"]' "$POINTER" >/dev/null

pointer_sha=$(sha256sum "$POINTER" | awk '{print $1}')
source_sha=$(sed '$d' "$CHECKPOINT" | sha256sum | awk '{print $1}')
receipt_sha=$(jq -cS 'select(.record_type == "provenance_receipt") | .provenance_receipt | del(.receipt_sha256)' "$CHECKPOINT" | sha256sum | awk '{print $1}')

[[ "$checkpoint_sha" == "$(jq -r '.checksums["checkpoint.jsonl"]' "$EXPECTED")" ]]
[[ "$pointer_sha" == "$(jq -r '.checksums["current.json"]' "$EXPECTED")" ]]
[[ "$source_sha" == "$(jq -r '.checksums.source_records_without_receipt' "$EXPECTED")" ]]
[[ "$receipt_sha" == "$(jq -r '.checksums.receipt_without_digest' "$EXPECTED")" ]]
[[ "$checkpoint_sha" == "$(jq -r '.active_root.sha256' "$POINTER")" ]]
[[ "$(jq -r '.generation_id' "$POINTER")" == gen-* ]]
[[ "$(jq -r '.mode' "$POINTER")" == monolithic ]]
[[ "$(jq -r '.active_root.path' "$POINTER")" == "$native_root" ]]
[[ "$(jq -c '.added_paths' "$POINTER")" == "$(jq -cn --arg path "$native_root" '[$path]')" ]]
[[ "$(jq -r '.total_record_count' "$POINTER")" == 5 ]]
[[ "$(jq -r '.issue_count' "$POINTER")" == 2 ]]
[[ "$(jq -r '.event_count' "$POINTER")" == 2 ]]
[[ "$(jq -r '.receipt_count' "$POINTER")" == 1 ]]
receipt_source_sha=$(jq -r '.provenance_receipt.source_root_sha256' <(jq -s '.[4]' "$CHECKPOINT"))
receipt_declared_sha=$(jq -r '.provenance_receipt.receipt_sha256' <(jq -s '.[4]' "$CHECKPOINT"))
[[ "$source_sha" == "$receipt_source_sha" ]]
[[ "$receipt_sha" == "$receipt_declared_sha" ]]

echo "unknown-fields-v1: PASS"
