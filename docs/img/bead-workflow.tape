# bead-rs workflow screencast -- drives the real `bead` binary.
#
# Unlike a hand-drawn diagram, this cannot silently drift from the CLI: if a
# command or its output changes, re-running the tape either shows the new
# behaviour or visibly breaks.
#
# Regenerate:
#     cargo install --path .
#     vhs docs/img/bead-workflow.tape
#
# Requires vhs (github.com/charmbracelet/vhs), which itself needs ttyd and
# ffmpeg on PATH.

Output docs/img/bead-workflow.gif

Require bead

Set Shell bash
Set Theme "Github"
Set FontSize 18
# Sized to the tallest phase (~20 lines) so the frame is not mostly
# empty terminal, which is also most of the file-size win.
Set Width 1000
Set Height 500
Set Padding 24
Set Framerate 24
Set TypingSpeed 40ms

# Setup runs off-camera: a clean workspace directory and a bare prompt, so the
# recording shows bead-rs rather than this machine.
Hide
Type "export PS1='$ '"
Enter
Type "rm -rf /tmp/bead-demo && mkdir -p /tmp/bead-demo && cd /tmp/bead-demo"
Enter
Type "clear"
Enter
Show

# --- a workspace ---------------------------------------------------------
Type "bead init --prefix demo"
Enter
Sleep 1.8s

# --- three beads; create prints only the id, so it captures cleanly -------
Type "design=$(bead create --title 'Design schema'   --priority 1)"
Enter
Sleep 500ms
Type "store=$(bead create  --title 'Implement store' --priority 2)"
Enter
Sleep 500ms
Type "docs=$(bead create   --title 'Write docs'      --priority 3)"
Enter
Sleep 700ms
Type "echo $design $store $docs"
Enter
Sleep 1.8s

# --- a blocking edge, written blocked-first -------------------------------
Type "# store cannot start until design closes"
Enter
Sleep 400ms
Type "bead dep add $store $design"
Enter
Sleep 2s

# --- the ready frontier: store is absent, because it is blocked -----------
Type "clear"
Enter
Type "bead list --ready"
Enter
Sleep 3.5s

# --- atomic claim takes the highest-priority ready bead -------------------
Type "clear"
Enter
Type "bead claim --assignee worker-1"
Enter
Sleep 2.5s

# --- closing design advances the frontier ---------------------------------
Type "bead close $design --reason 'Schema agreed'"
Enter
Sleep 1.8s
Type "bead list --ready"
Enter
Sleep 3.5s

# --- the checkpoint Git tracks --------------------------------------------
Type "clear"
Enter
Type "bead sync flush-only"
Enter
Sleep 4s
