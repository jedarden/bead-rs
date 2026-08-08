# bead-rs Marathon Coding harness

This directory contains the durable control plane for the clean-room bootstrap.

| File | Purpose |
| --- | --- |
| `instruction.md` | Hot-reloaded mission supplied to every coding iteration |
| `feature_list.json` | Machine-readable release requirements and evidence |
| `progress.md` | Append-only handoff and decision log |
| `start.sh` | Validated wrapper around the central Marathon Coding launcher |
| `watch-completion.sh` | Stops the tmux loop after the release sentinel appears |

Runtime logs and the `COMPLETE` sentinel are intentionally ignored by Git.

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

