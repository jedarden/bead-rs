#!/usr/bin/env bash
set -euo pipefail

session_name="${1:?tmux session name is required}"
sentinel="${2:?completion sentinel path is required}"
required_pattern="${3:-}"

while tmux has-session -t "$session_name" 2>/dev/null; do
    if [[ -f "$sentinel" ]] && { [[ -z "$required_pattern" ]] || grep -Eq "$required_pattern" "$sentinel"; }; then
        tmux kill-session -t "$session_name"
        printf 'Stopped %s after verified handoff state appeared.\n' "$session_name"
        exit 0
    fi
    sleep 10
done
