# bead-rs Marathon Coding harness

This directory contains the durable control plane for the clean-room bootstrap.

| File | Purpose |
| --- | --- |
| `instruction.md` | Hot-reloaded mission supplied to every coding iteration |
| `feature_list.json` | Machine-readable release requirements and evidence |
| `progress.md` | Append-only handoff and decision log |
| `start.sh` | Validated wrapper around the central Marathon Coding launcher |
| `watch-completion.sh` | Stops the tmux loop only after full-project completion |

Runtime logs and the full-project `COMPLETE` sentinel are intentionally ignored
by Git. `BOOTSTRAP_HANDOFF` remains a tracked historical record of the earlier
bootstrap experiment; it no longer stops Marathon.

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

This Marathon session owns completion of F001-F017 and the adopted R001-R024
roadmap. It runs on `main` until every feature has verified evidence and the
full-project `.marathon/COMPLETE` sentinel is created. Native beads may be used
as an implementation aid, but NEEDLE is not the execution authority for this
completion run.
