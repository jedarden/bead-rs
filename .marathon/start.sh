#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
skill_dir="${MARATHON_SKILL_DIR:-/home/coding/claude-config/skills/marathon-coding}"
session_name="${MARATHON_SESSION:-bead-rs-cleanroom}"
loop_delay="${MARATHON_DELAY:-5}"
log_dir="$script_dir/logs"

if [[ "${BEAD_RS_CLEANROOM:-}" != "1" ]]; then
    printf '%s\n' 'Refusing to start: set BEAD_RS_CLEANROOM=1 after verifying pod isolation.' >&2
    exit 1
fi

if [[ -z "${CLAUDE_CONFIG_DIR:-}" ]]; then
    printf '%s\n' 'Refusing to start: CLAUDE_CONFIG_DIR must point to a fresh clean-room directory.' >&2
    exit 1
fi

if [[ ! -x "$skill_dir/marathon.sh" ]]; then
    printf 'Marathon launcher is unavailable or not executable: %s\n' "$skill_dir/marathon.sh" >&2
    exit 1
fi

if [[ "$(git -C "$repo_root" remote get-url origin)" != "https://git.ardenone.com/jedarden/bead-rs.git" ]]; then
    printf '%s\n' 'Refusing to start: origin is not the authoritative bead-rs Forgejo repository.' >&2
    exit 1
fi

mkdir -p "$log_dir" "$CLAUDE_CONFIG_DIR"

args=(
    --prompt "$script_dir/instruction.md"
    --session "$session_name"
    --delay "$loop_delay"
    --log-dir "$log_dir"
    --config-dir "$CLAUDE_CONFIG_DIR"
)

if [[ -n "${MARATHON_MODEL:-}" ]]; then
    args+=(--model "$MARATHON_MODEL")
fi

"$skill_dir/marathon.sh" "${args[@]}"

nohup "$script_dir/watch-completion.sh" "$session_name" "$script_dir/COMPLETE" \
    >"$log_dir/completion-watch.log" 2>&1 &

printf 'Completion watcher started for tmux session %s.\n' "$session_name"

