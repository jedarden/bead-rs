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
  [.[].record_type] == ["issue", "issue", "event", "event", "provenance_receipt"] and
  ([.[] | select(.record_type == "issue")] | length) == 2 and
  ([.[] | select(.record_type == "event")] | length) == 2 and
  ([.[] | select(.record_type == "provenance_receipt")] | length) == 1
' <(jq -s . "$CHECKPOINT") >/dev/null

jq -e '
  .[0].issue.id == "bead-unknown-a" and
  .[0].issue["x-fixture-issue"] == {marker:"issue-extension",ordinal:1} and
  .[0].issue.dependencies[0]["x-fixture-dependency"] == true and
  .[0].issue.external_references[0]["x-fixture-external-reference"] == null and
  .[0].issue.data["fixture.namespace"]["x-fixture-structured-data"] == {nested:["keep",null,3.5]} and
  .[1].issue["x-fixture-issue"] == ["issue-extension",false,null]
' <(jq -s . "$CHECKPOINT") >/dev/null

jq -e '
  .[2].event["x-fixture-event"] == ["event-extension",2] and
  .[3].event["x-fixture-event"] == {second:true} and
  .[4].provenance_receipt["x-fixture-receipt"] == {marker:"receipt-extension",preserve:true}
' <(jq -s . "$CHECKPOINT") >/dev/null

jq -e '.resource_key["x-fixture-resource-key"] == "resource-extension"' "$RESOURCE_KEY" >/dev/null
jq -e '."x-fixture-pointer" == {marker:"pointer-extension",preserve:true}' "$POINTER" >/dev/null

checkpoint_sha=$(sha256sum "$CHECKPOINT" | awk '{print $1}')
pointer_sha=$(sha256sum "$POINTER" | awk '{print $1}')
source_sha=$(sed '$d' "$CHECKPOINT" | sha256sum | awk '{print $1}')
receipt_sha=$(jq -cS 'select(.record_type == "provenance_receipt") | .provenance_receipt | del(.receipt_sha256)' "$CHECKPOINT" | sha256sum | awk '{print $1}')

[[ "$checkpoint_sha" == "$(jq -r '.checksums["checkpoint.jsonl"]' "$EXPECTED")" ]]
[[ "$pointer_sha" == "$(jq -r '.checksums["current.json"]' "$EXPECTED")" ]]
[[ "$source_sha" == "$(jq -r '.checksums.source_records_without_receipt' "$EXPECTED")" ]]
[[ "$receipt_sha" == "$(jq -r '.checksums.receipt_without_digest' "$EXPECTED")" ]]
[[ "$checkpoint_sha" == "$(jq -r '.active_root.sha256' "$POINTER")" ]]
receipt_source_sha=$(jq -r '.provenance_receipt.source_root_sha256' <(jq -s '.[4]' "$CHECKPOINT"))
receipt_declared_sha=$(jq -r '.provenance_receipt.receipt_sha256' <(jq -s '.[4]' "$CHECKPOINT"))
[[ "$source_sha" == "$receipt_source_sha" ]]
[[ "$receipt_sha" == "$receipt_declared_sha" ]]

echo "unknown-fields-v1: PASS"
