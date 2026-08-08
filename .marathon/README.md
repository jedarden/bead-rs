# bead-rs Marathon Coding harness

This directory contains the durable control plane for the clean-room bootstrap.

| File | Purpose |
| --- | --- |
| `instruction.md` | Hot-reloaded mission supplied to every coding iteration |
| `feature_list.json` | Machine-readable release requirements and evidence |
| `progress.md` | Append-only handoff and decision log |
| `start.sh` | Validated wrapper around the central Marathon Coding launcher |
| `watch-completion.sh` | Stops the tmux loop after the handoff record becomes final |

Runtime logs and the full-release `COMPLETE` sentinel are intentionally ignored
by Git. `BOOTSTRAP_HANDOFF` is a tracked pending/final authority record.

## Launch

Use a fresh pod with only this repository mounted. Set a new Claude config
directory that contains no prior sessions, plugins, CASS indexes, or global
memory:

```text
export BEAD_RS_CLEANROOM=1
export CLAUDE_CONFIG_DIR=/cleanroom/claude-config
export MARATHON_SKILL_DIR=/home/coding/claude-config/skills/marathon-coding
./.marathon/start.sh
```

Optional variables:

- `MARATHON_MODEL`
- `MARATHON_SESSION` (default `bead-rs-cleanroom`)
- `MARATHON_DELAY` (default `5`)

The wrapper refuses to start without the clean-room acknowledgement and an
explicit Claude configuration directory.

This Marathon session implements only the governed bootstrap through G4. It
must stop after the tracked `BOOTSTRAP_HANDOFF` record reaches `state: final`;
the remaining 0.1 work is executed from native beads by NEEDLE workers.
