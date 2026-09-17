#!/usr/bin/env bash
# Regenerate the unknown-field checkpoint fixtures (monolithic + sharded).
#
# Issue extension fields -- the JSON keys that are NOT part of the native-v1
# issue schema -- are preserved end to end through the `#[serde(flatten)]`
# catch-all on `Issue.extensions`. No CLI command can produce one, so these
# fixtures cannot be built by driving the lifecycle alone. Like the
# attempts fixtures, they are well-formed by construction rather than by
# inspection:
#
#   1. a scratch workspace drives the real binary through a lifecycle rich
#      enough to populate every projected collection (labels, dependencies,
#      comments, external references, structured data, events);
#   2. the monolithic forensic generation is rewritten to add the unknown
#      fields to each issue record, and the pointer's `active_root.sha256`
#      is recomputed from the rewritten bytes -- the only doctored artifact
#      in the corpus;
#   3. the doctored checkpoint is restored into a second scratch workspace
#      and re-published with `checkpoint.mode = sharded`, so the sharded
#      fixture is unmodified binary output (content-addressed objects and
#      manifest included);
#   4. both fixtures are restored into fresh scratch workspaces and the
#      restored `issue_extensions` rows are diffed against the injected
#      payload before the fixture files are published.
#
# Usage:
#   tests/fixtures/extensions/generate.sh
#
# Environment:
#   BEAD_BIN   binary to build the fixtures with. Defaults to the workspace
#              debug build, then $PATH.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
FIXTURE_DIR="$SCRIPT_DIR"

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

echo "fixture binary: $BEAD"
"$BEAD" --version

# A scratch workspace must not sit under a directory that already holds a
# .beads/, or the CLI will pick that store up as its own.
SCRATCH_BASE="/var/tmp"
for base in /var/tmp /run/user/"$(id -u)" /tmp; do
    if [ -d "$base" ] && [ ! -e "$base/.beads" ]; then
        SCRATCH_BASE="$base"
        break
    fi
done
WORK="$(mktemp -d "$SCRATCH_BASE/bead-unknown-field-fixture-gen.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

# Keep every child tempfile (including the validator's) inside the scratch
# tree: the ambient /tmp carries a foreign .beads/, and workspace discovery
# walks ancestors.
export TMPDIR="$WORK"

echo "scratch workspace: $WORK"

W1="$WORK/w1-source"
W2="$WORK/w2-sharded"

init_workspace() {
    local dir="$1"
    mkdir -p "$dir"
    # cd BEFORE init: run from anywhere else, the CLI discovers whatever
    # workspace encloses the cwd (including this repository's own store)
    # and refuses or, worse, targets it.
    (cd "$dir" && "$BEAD" init --prefix bead --skip-foreign-workspace >"$dir/.init.log")
    # Suppress automatic publication so the explicit `sync flush-only` is
    # the single publisher whose output the fixture captures.
    python3 - "$dir" <<'PY'
import json, sys
from pathlib import Path

workspace = Path(sys.argv[1])
config_path = workspace / ".beads" / "config.json"
config = json.loads(config_path.read_text())
config.setdefault("checkpoint", {})["auto_flush"] = False
config_path.write_text(json.dumps(config))
PY
}

seed_lifecycle() {
    local dir="$1"
    cd "$dir"

    # Issue A: open, every projected collection populated.
    local a
    a="$("$BEAD" create \
        --title "Unknown-field fixture A: unknown fields coexist with every projection" \
        --description "Carries labels, a dependency edge, a comment, an external reference, structured data, and unknown extension fields." \
        --priority 1 \
        --issue-type task)"
    "$BEAD" update "$a" --notes "Fixture corpus for unknown-field checkpoint round trips."
    # Issue B: in progress with an assignee, blocked by A.
    local b
    b="$("$BEAD" create \
        --title "Unknown-field fixture B: scalar, container, and empty-key edges" \
        --description "Carries the awkward unknown-field shapes: empty object, empty array, null, and the empty-string key." \
        --priority 2)"
    # Issue C: closed, unknown fields on terminal state.
    local c
    c="$("$BEAD" create \
        --title "Unknown-field fixture C: closed issue carries unknown fields" \
        --description "Proves unknown fields survive on a closed issue with closed_at and close_reason set." \
        --priority 3)"

    "$BEAD" label add --label unknown-field "$a"
    "$BEAD" label add --label conformance "$a"

    "$BEAD" update "$b" --status in_progress --assignee fixture-worker

    "$BEAD" dep add "$b" "$a" --kind blocks
    "$BEAD" dep add "$c" "$b" --kind relates_to

    "$BEAD" ref add --id "$a" --namespace github --key issue-number --value 12345
    "$BEAD" ref add --id "$b" --namespace gitlab --key commit-hash --value abc123def

    "$BEAD" data set --id "$a" --namespace test-ns --schema-ref "urn:test:schema:1" \
        --value '{"probe":"unknown-field-fixture"}'
    "$BEAD" data set --id "$b" --namespace metrics --schema-ref "urn:metrics:schema:1" \
        --value '{"count":42}'

    "$BEAD" close "$c" --reason fixture-complete

    # Comments have no CLI writer; the lifecycle conformance suite seeds
    # them the same way -- direct insertion.
    python3 - "$dir" "$a" "$b" <<'PY'
import sqlite3, sys
from pathlib import Path

workspace, issue_a, issue_b = sys.argv[1], sys.argv[2], sys.argv[3]
db = Path(workspace) / ".beads" / "beads.db"
conn = sqlite3.connect(db)
with conn:
    conn.execute(
        "INSERT INTO comments (id, issue_id, author, body, created_at) VALUES (?, ?, ?, ?, ?)",
        ("comment-uf0001", issue_a, "fixture-generator",
         "Projection comment on the open issue", "2026-09-17T00:00:01.000000000Z"),
    )
    conn.execute(
        "INSERT INTO comments (id, issue_id, author, body, reply_to_id, resolution_state, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        ("comment-uf0002", issue_b, "fixture-generator",
         "Projection comment on the in-progress issue", "comment-uf0001", "resolved",
         "2026-09-17T00:00:02.000000000Z"),
    )
conn.close()
PY

    # Bump one revision past the CLI-written value so the fixture does not
    # carry an all-ones revision column.
    python3 - "$dir" "$a" <<'PY'
import sqlite3, sys
from pathlib import Path

workspace, issue_a = sys.argv[1], sys.argv[2]
db = Path(workspace) / ".beads" / "beads.db"
conn = sqlite3.connect(db)
with conn:
    conn.execute("UPDATE issues SET revision = revision + 1 WHERE id = ?", (issue_a,))
conn.close()
PY
}

echo "== seeding source workspace =="
init_workspace "$W1"
seed_lifecycle "$W1"

echo "== flushing monolithic generation =="
(cd "$W1" && "$BEAD" sync flush-only >.flush.log 2>&1) || {
    cat "$W1/.flush.log" >&2
    exit 1
}
grep -q "Flushed forensic checkpoint" "$W1/.flush.log" ||
    { cat "$W1/.flush.log" >&2; exit 1; }

echo "== injecting unknown fields and re-addressing the generation =="
python3 - "$W1" <<'PY'
import hashlib, json, sys
from pathlib import Path

workspace = Path(sys.argv[1])
checkpoint = workspace / ".beads" / "checkpoint"
pointer_path = checkpoint / "current.json"
pointer = json.loads(pointer_path.read_text())
generation = checkpoint / pointer["active_root"]["path"]

# The unknown-field payload, keyed by title marker. Every entry is an
# extension: none of these keys exists in the native-v1 issue schema, and
# none collides with a projected collection name.
PAYLOAD = {
    "Unknown-field fixture A": {
        "x-obsidian-fidelity": "exact",
        "vendor_metadata": {
            "origin": "weave",
            "confidence": 0.87,
            "attempts": 3,
            "tags": ["alpha", "beta"],
        },
        "annotation": "multi\nline — ünknown ✓",
        "weighting": 42,
        "ratio": 0.125,
    },
    "Unknown-field fixture B": {
        "trace_context": {
            "traceparent": "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            "spans": [{"id": "s1", "depth": 2}],
        },
        "cluster": [1, "two", [3], {"four": 4}],
        "hollow": {},
        "void": [],
        "ghost": None,
        "": "empty-string key survives round trips",
    },
    "Unknown-field fixture C": {
        "closure_metadata": {"closed_by": "fixture-generator", "success": True},
        "pinned_numbers": [0, -1, 2.5, -0.5],
        "deeply": {"nested": {"structure": {"leaf": [True, False, None]}}},
        "ünkoded_key": "vàlue-é",
    },
}

lines = generation.read_text().splitlines()
rewritten = []
injected = 0
for line in lines:
    record = json.loads(line)
    if record.get("record_type") == "issue":
        issue = record["issue"]
        for marker, fields in PAYLOAD.items():
            if issue["title"].startswith(marker):
                issue.update(fields)
                injected += len(fields)
                # Compact + sorted: byte style does not matter because the
                # pointer hash is recomputed below, but one line per record
                # does.
                rewritten.append(json.dumps(record, sort_keys=True,
                                            separators=(",", ":"),
                                            ensure_ascii=False))
                break
        else:
            rewritten.append(line)
    else:
        rewritten.append(line)

assert injected == sum(len(v) for v in PAYLOAD.values()), (
    f"expected to inject {sum(len(v) for v in PAYLOAD.values())} fields, "
    f"touched {injected} -- title markers must match the seeded lifecycle"
)

body = "\n".join(rewritten) + "\n"
generation.write_text(body, encoding="utf-8")
pointer["active_root"]["sha256"] = hashlib.sha256(body.encode("utf-8")).hexdigest()
pointer_path.write_text(json.dumps(pointer, indent=2), encoding="utf-8")
print(f"injected {injected} unknown fields; new root hash {pointer['active_root']['sha256'][:16]}…")
PY

echo "== publishing fixtures =="
mkdir -p "$FIXTURE_DIR/sharded/manifests" "$FIXTURE_DIR/sharded/objects"
rm -f "$FIXTURE_DIR/checkpoint.jsonl" "$FIXTURE_DIR/current.json"
rm -f "$FIXTURE_DIR/sharded/current.json" \
      "$FIXTURE_DIR"/sharded/manifests/*.json "$FIXTURE_DIR"/sharded/objects/*.jsonl

GENERATION="$(python3 -c "
import json, sys
pointer = json.load(open('$W1/.beads/checkpoint/current.json'))
print(pointer['active_root']['path'])
")"
cp "$W1/.beads/checkpoint/$GENERATION" "$FIXTURE_DIR/checkpoint.jsonl"
cp "$W1/.beads/checkpoint/current.json" "$FIXTURE_DIR/current.json"
# The fixture carries the generation under the fixed name checkpoint.jsonl
# (the current publisher content-addresses monolith generations under
# objects/), so the copied pointer must be repointed at the fixture layout.
# The sha256 is unchanged: only the relative path is rewritten.
python3 - "$FIXTURE_DIR" <<'PY'
import json, sys
from pathlib import Path

fixture_dir = Path(sys.argv[1])
pointer_path = fixture_dir / "current.json"
pointer = json.loads(pointer_path.read_text())
pointer["active_root"]["path"] = "checkpoint.jsonl"
pointer_path.write_text(json.dumps(pointer, indent=2), encoding="utf-8")
PY

# The sharded fixture is unmodified binary output: restore the doctored
# monolithic fixture, then republish with checkpoint.mode = sharded.
init_workspace "$W2"
(cd "$W2" && "$BEAD" sync import-only \
    --input "$FIXTURE_DIR/checkpoint.jsonl" \
    --restore-into-empty \
    --actor fixture-generator >.import.log 2>&1) || {
    cat "$W2/.import.log" >&2
    exit 1
}
python3 - "$W2" <<'PY'
import json, sys
from pathlib import Path

workspace = Path(sys.argv[1])
config_path = workspace / ".beads" / "config.json"
config = json.loads(config_path.read_text())
config.setdefault("checkpoint", {})["mode"] = "sharded"
config_path.write_text(json.dumps(config))
PY
(cd "$W2" && "$BEAD" sync flush-only >.flush.log 2>&1) || {
    cat "$W2/.flush.log" >&2
    exit 1
}
grep -q "Flushed forensic checkpoint" "$W2/.flush.log" ||
    { cat "$W2/.flush.log" >&2; exit 1; }

cp "$W2/.beads/checkpoint/current.json" "$FIXTURE_DIR/sharded/current.json"
cp "$W2"/.beads/checkpoint/manifests/*.json "$FIXTURE_DIR/sharded/manifests/"
cp "$W2"/.beads/checkpoint/objects/*.jsonl "$FIXTURE_DIR/sharded/objects/"

echo "== validating both fixtures by restoration =="
BEAD="$BEAD" python3 - "$FIXTURE_DIR" <<'PY'
import json, os, sqlite3, subprocess, sys, tempfile
from pathlib import Path

fixture_dir = Path(sys.argv[1])
bead = os.environ["BEAD"]

PAYLOAD_BY_MARKER = {
    "Unknown-field fixture A": {
        "x-obsidian-fidelity": "exact",
        "vendor_metadata": {"origin": "weave", "confidence": 0.87,
                            "attempts": 3, "tags": ["alpha", "beta"]},
        "annotation": "multi\nline — ünknown ✓",
        "weighting": 42, "ratio": 0.125,
    },
    "Unknown-field fixture B": {
        "trace_context": {"traceparent":
                          "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                          "spans": [{"id": "s1", "depth": 2}]},
        "cluster": [1, "two", [3], {"four": 4}],
        "hollow": {}, "void": [], "ghost": None,
        "": "empty-string key survives round trips",
    },
    "Unknown-field fixture C": {
        "closure_metadata": {"closed_by": "fixture-generator", "success": True},
        "pinned_numbers": [0, -1, 2.5, -0.5],
        "deeply": {"nested": {"structure": {"leaf": [True, False, None]}}},
        "ünkoded_key": "vàlue-é",
    },
}

KNOWN_PROJECTIONS = {"labels", "dependencies", "external_references",
                     "comments", "resource_keys"}

def fixture_issues(monolithic):
    if monolithic:
        source = str(fixture_dir / "checkpoint.jsonl")
    else:
        source = str(fixture_dir / "sharded" / "current.json")
    with tempfile.TemporaryDirectory() as raw:
        workspace = Path(raw)
        subprocess.run([bead, "init", "--prefix", "bead",
                        "--skip-foreign-workspace"],
                       cwd=workspace, check=True,
                       stdout=subprocess.DEVNULL)
        subprocess.run([bead, "sync", "import-only", "--input", source,
                        "--restore-into-empty", "--actor", "fixture-validator"],
                       cwd=workspace, check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        conn = sqlite3.connect(workspace / ".beads" / "beads.db")
        rows = conn.execute(
            "SELECT issue_id, key, value FROM issue_extensions").fetchall()
        titles = dict(conn.execute("SELECT id, title FROM issues").fetchall())
        conn.close()
    extensions = {}
    for issue_id, key, value in rows:
        extensions.setdefault(issue_id, {})[key] = json.loads(value)
    return titles, extensions

for monolithic in (True, False):
    layout = "monolithic" if monolithic else "sharded"
    titles, extensions = fixture_issues(monolithic)
    seen = 0
    for issue_id, title in titles.items():
        for marker, expected in PAYLOAD_BY_MARKER.items():
            if title.startswith(marker):
                actual = extensions.get(issue_id, {})
                assert actual == expected, (
                    f"{layout}: {marker}: extension drift\n"
                    f"  expected: {json.dumps(expected, sort_keys=True)}\n"
                    f"  actual:   {json.dumps(actual, sort_keys=True)}")
                seen += 1
    assert seen == len(PAYLOAD_BY_MARKER), (
        f"{layout}: matched {seen} of {len(PAYLOAD_BY_MARKER)} issues")
    for issue_id, fields in extensions.items():
        overlap = KNOWN_PROJECTIONS & set(fields)
        assert not overlap, (
            f"{layout}: {issue_id}: known projections leaked into "
            f"issue_extensions: {sorted(overlap)}")
    print(f"{layout}: unknown fields survive restore byte-faithfully")

print("fixtures valid")
PY

echo "fixtures regenerated under $FIXTURE_DIR"
