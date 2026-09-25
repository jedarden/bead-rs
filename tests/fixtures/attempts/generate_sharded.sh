#!/usr/bin/env bash
# Regenerate the sharded checkpoint fixtures under new/sharded/ and old/sharded/.
#
# Checkpoint objects are content-addressed -- object file names are the sha256 of
# their contents, and the inner manifest pins those hashes -- so these fixtures
# cannot be hand-edited. Any byte change invalidates every name above it. This
# script rebuilds them by driving real workspaces through the full lifecycle, so
# the committed fixtures are well-formed by construction rather than by
# inspection.
#
# The two fixtures are produced by two different binaries on purpose:
#
#   new/sharded  -- the current build. Carries the full post-feature shape: an
#                   attempt_outcome_shards manifest key, attempt_outcome records
#                   covering every outcome and every action value in
#                   src/model/attempt.rs, and events covering every kind in the
#                   urn:bead-rs:schema:event:native-v1 enum from
#                   src/service/schema.rs.
#
#   old/sharded  -- the pinned pre-feature binary (bead-pre-feature 0.2.4).
#                   Same lifecycle, but the attempt_outcome table does not exist
#                   yet, so the inner manifest has no attempt_outcome_shards key
#                   and the manifest carries no attempt_outcome_count. Generating
#                   this with the current build would silently produce a
#                   *new-format* manifest and destroy the property that makes the
#                   fixture a pre-feature baseline.
#
# Usage:
#   tests/fixtures/attempts/generate_sharded.sh
#
# Environment:
#   BEAD_BIN      binary for new/sharded. Defaults to the workspace debug build,
#                 then $PATH.
#   BEAD_BIN_PRE  binary for old/sharded. Defaults to pinned-binaries/bead-pre-
#                 feature. Set it to an empty string to skip the old fixture.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

PRE_FEATURE_BIN="$REPO_ROOT/pinned-binaries/bead-pre-feature"
if [ "${BEAD_BIN_PRE+x}" = x ]; then
    PRE_FEATURE_BIN="$BEAD_BIN_PRE"
fi

if [ -n "${BEAD_BIN:-}" ]; then
    BEAD="$BEAD_BIN"
elif [ -x "${CARGO_TARGET_DIR:-$REPO_ROOT/target}/debug/bead" ]; then
    BEAD="${CARGO_TARGET_DIR:-$REPO_ROOT/target}/debug/bead"
elif command -v bead >/dev/null 2>&1; then
    BEAD="$(command -v bead)"
else
    echo "error: no bead binary found; set BEAD_BIN" >&2
    exit 1
fi

echo "new fixture binary: $BEAD"
"$BEAD" --version
if [ -n "$PRE_FEATURE_BIN" ]; then
    echo "old fixture binary: $PRE_FEATURE_BIN"
    "$PRE_FEATURE_BIN" --version
    [ -x "$PRE_FEATURE_BIN" ] || { echo "error: pre-feature binary not executable" >&2; exit 1; }
fi

# A scratch workspace must not sit under a directory that already holds a
# .beads/, or the CLI will pick that store up as its own.
SCRATCH_BASE="/var/tmp"
for base in /var/tmp /run/user/"$(id -u)" /tmp; do
    if [ -d "$base" ] && [ ! -e "$base/.beads" ]; then
        SCRATCH_BASE="$base"
        break
    fi
done
WORK="$(mktemp -d "$SCRATCH_BASE/bead-fixture-gen.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

echo "scratch workspace: $WORK"

set_checkpoint_config() {
    # auto_flush off so nothing publishes mid-lifecycle; mode sharded so the
    # flush produces the sharded layout.
    python3 - "$1" <<'PY'
import json, os, sys
path = os.path.join(sys.argv[1], ".beads", "config.json")
config = json.load(open(path))
checkpoint = config.setdefault("checkpoint", {})
checkpoint["auto_flush"] = False
checkpoint["mode"] = "sharded"
json.dump(config, open(path, "w"), indent=2)
PY
}

init_workspace() {
    local dir="$1"
    local bin="$2"
    mkdir -p "$dir"
    (cd "$dir" && "$bin" init >/dev/null)
    set_checkpoint_config "$dir"
}

# Every event kind the new fixture must carry. This is the
# urn:bead-rs:schema:event:native-v1 enum from src/service/schema.rs plus
# "created", which issue creation writes but which sits outside that enum.
EXPECTED_EVENT_KINDS="created updated claimed released reopened closed assignment_cleared"

drive_lifecycle() {
    # Leaves one issue behind that has traversed every lifecycle transition.
    local dir="$1"
    local bin="$2"
    (
        cd "$dir"
        local id
        id="$("$bin" create --title 'Sharded fixture: lifecycle issue' --priority 2 | tail -1)"
        "$bin" update "$id" --notes 'first edit' >/dev/null
        "$bin" update "$id" --assignee worker-1 >/dev/null
        # assignment_cleared is only emitted for an open *and* assigned issue;
        # clearing an already-unassigned issue succeeds without writing an event.
        "$bin" update "$id" --clear-assignee >/dev/null
        "$bin" update "$id" --assignee worker-1 >/dev/null
        # release requires in_progress.
        "$bin" update "$id" --status in_progress --assignee worker-1 >/dev/null
        "$bin" release "$id" >/dev/null
        # claimed is only emitted by the frontier claim path; `update --status
        # in_progress` writes a plain "updated" event.
        "$bin" claim --assignee worker-2 >/dev/null
        "$bin" close "$id" --reason 'fixture lifecycle complete' >/dev/null
        "$bin" reopen "$id" >/dev/null
        "$bin" create --title 'Sharded fixture: secondary issue' --priority 3 >/dev/null
    )
}

insert_outcomes() {
    # One attempt_outcome per outcome value and per action value. Pairing one
    # action to each outcome naively is not enough: the pair must also be one of
    # the fifteen combinations validate_outcome_action_combo accepts in
    # src/model/attempt.rs. For instance cancelled+block is not a valid pair even
    # though both values are individually legal. These five pairs cover every
    # outcome and every action while staying inside the valid set.
    python3 - "$1" <<'PY'
import os, sqlite3, sys

PAIRS = [
    ("verified_success", "close"),
    ("work_failure", "quarantine"),
    ("infrastructure_failure", "none"),
    ("cancelled", "release"),
    ("indeterminate", "block"),
]

db = os.path.join(sys.argv[1], ".beads", "beads.db")
conn = sqlite3.connect(db)
issue_id = conn.execute(
    "SELECT id FROM issues ORDER BY created_at LIMIT 1"
).fetchone()[0]

for i, (outcome, action) in enumerate(PAIRS, start=1):
    conn.execute(
        """
        INSERT INTO attempt_outcomes (
            receipt_id, attempt_id, issue_id, outcome, action, reason,
            canonical_request_hash, prior_attempt_tier,
            resulting_attempt_tier, resulting_issue_revision, actor,
            created_at, evidence_refs_json, model, harness, harness_version
        ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
        """,
        (
            f"ao-fixt{i:03d}",
            f"urn:needle:attempt:fixture{i:03d}",
            issue_id,
            outcome,
            action,
            f"{outcome} resolved with {action}",
            f"{i:064x}",
            0,
            0 if outcome == "verified_success" else 1,
            2,
            "fixture-worker",
            "2026-09-03T00:00:00Z",
            "[]",
            "fixture-model",
            "fixture-harness",
            "0.0.0",
        ),
    )
conn.commit()
conn.close()
PY
}

collect_checkpoint() {
    local dir="$1"
    local dest="$2"
    local bin="$3"
    (cd "$dir" && "$bin" sync flush-only >/dev/null)
    rm -rf "$dest"
    mkdir -p "$dest"
    cp "$dir/.beads/checkpoint/current.json" "$dest/current.json"
    cp -r "$dir/.beads/checkpoint/manifests" "$dest/manifests"
    cp -r "$dir/.beads/checkpoint/objects" "$dest/objects"
}

assert_event_kinds() {
    local dest="$1"
    shift
    local kind
    for kind in "$@"; do
        if ! cat "$dest"/objects/*.jsonl |
             jq -e --arg k "$kind" \
                'select(.record_type=="event" and .event.kind==$k)' \
                >/dev/null; then
            echo "error: event kind '$kind' missing from $dest" >&2
            return 1
        fi
    done
}

generate() {
    # $1 scratch workspace, $2 destination, $3 binary, $4 insert outcomes,
    # remaining args: event kinds that must be present
    local ws="$1" dest="$2" bin="$3" want_outcomes="$4"
    shift 4
    init_workspace "$ws" "$bin"
    drive_lifecycle "$ws" "$bin"
    if [ "$want_outcomes" = "yes" ]; then
        insert_outcomes "$ws"
    fi
    collect_checkpoint "$ws" "$dest" "$bin"
    assert_event_kinds "$dest" "$@"
}

# ---- old fixture: genuine pre-feature shape --------------------------------
if [ -n "$PRE_FEATURE_BIN" ]; then
    OLD_DIR="$SCRIPT_DIR/old/sharded"
    rm -rf "$OLD_DIR"
    # The pre-feature binary has no attempt_outcome table, so outcome insertion
    # is skipped and the fixture keeps its genuine pre-feature shape. Only
    # "created"/"updated" are asserted: the remaining kinds are not guaranteed
    # to exist in the 0.2.4 CLI surface.
    generate "$WORK/old" "$OLD_DIR" "$PRE_FEATURE_BIN" no created
    assert_event_kinds "$OLD_DIR" created updated
    echo "wrote $OLD_DIR"
fi

# ---- new fixture: full post-feature coverage -------------------------------
NEW_DIR="$SCRIPT_DIR/new/sharded"
rm -rf "$NEW_DIR"
generate "$WORK/new" "$NEW_DIR" "$BEAD" yes $EXPECTED_EVENT_KINDS
echo "wrote $NEW_DIR"

# ---- summary ---------------------------------------------------------------
for name in old new; do
    dir="$SCRIPT_DIR/$name/sharded"
    [ -d "$dir" ] || continue
    echo
    echo "=== $name/sharded ==="
    jq '{issue_count, attempt_outcome_count, total_record_count,
         has_attempt_outcome_shards: has("attempt_outcome_shards")}' \
        "$dir"/manifests/*.json
    echo "event kinds:"
    cat "$dir"/objects/*.jsonl |
        jq -r 'select(.record_type=="event") | .event.kind' | sort | uniq -c
    echo "attempt_outcome (outcome action):"
    cat "$dir"/objects/*.jsonl |
        jq -r 'select(.record_type=="attempt_outcome") |
               "\(.attempt_outcome.outcome) \(.attempt_outcome.action)"' |
        sort
done

echo
echo "Regenerated sharded fixtures under $SCRIPT_DIR"
