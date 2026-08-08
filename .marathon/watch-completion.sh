#!/usr/bin/env bash
set -euo pipefail

session_name="${1:?tmux session name is required}"
sentinel="${2:?completion sentinel path is required}"

while tmux has-session -t "$session_name" 2>/dev/null; do
    if [[ -f "$sentinel" ]]; then
        tmux kill-session -t "$session_name"
        printf 'Stopped %s after verified completion sentinel appeared.\n' "$session_name"
        exit 0
    fi
    sleep 10
done

