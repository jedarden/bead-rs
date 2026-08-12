#!/usr/bin/env python3
"""Generate docs/img/bead-lifecycle.gif -- the bead-rs lifecycle animation.

The animation is a documentation surface like any other, so it is generated
from a committed script rather than drawn once and left to rot. Re-run it after
any change to the lifecycle model:

    python3 docs/img/generate-lifecycle-animation.py

Requires cairosvg and Pillow. Frames are authored as SVG, rasterized with
cairosvg, and assembled with per-frame durations so a held beat costs one frame
instead of thirty.

The palette matches docs/img/how-bead-rs-works.png.
"""

import io
import os

import cairosvg
from PIL import Image

W, H = 1100, 520

BG = "#FFFFFF"
NAVY = "#16233F"
TEAL = "#0E9AA3"
AMBER = "#DA7B0D"
MUTED = "#9BA6B7"
MUTED_FILL = "#EFF2F6"
CAPTION = "#3C4A63"
CHIP_BG = "#F4F6F9"

FONT = "DejaVu Sans, Helvetica, Arial, sans-serif"
MONO = "DejaVu Sans Mono, monospace"

R = 44
# `docs` has no edges and sits below `design`, where it can visibly stay ready
# through every beat -- it is the control case in the story.
POS = {"design": (170, 145), "store": (500, 145), "docs": (170, 300)}
WORKER = (880, 288)
# The storage row only appears in the final beat, in space the DAG never uses.
DB_C, DOC_C = (600, 300), (868, 300)

# Readiness is a property of a bead, not of a region, so state is shown on the
# bead itself. An earlier draft drew a dashed box around "the ready set" and
# fell apart at the beat where that set stops being spatially adjacent.
STATE_STYLE = {
    "new": (MUTED_FILL, MUTED, NAVY, None),
    "ready": (TEAL, TEAL, "#FFFFFF", "ready"),
    "blocked": (MUTED_FILL, MUTED, MUTED, "blocked"),
    "in_progress": (TEAL, NAVY, "#FFFFFF", "in progress"),
    "closed": ("#FFFFFF", MUTED, MUTED, "closed"),
}


def smoothstep(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3 - 2 * t)


def bead(name, label, state, opacity=1.0, pulse=None):
    cx, cy = POS[name]
    fill, stroke, text_color, tag = STATE_STYLE[state]
    dash = ' stroke-dasharray="7 6"' if state == "in_progress" else ""
    out = [f'<g opacity="{opacity:.3f}">']
    if pulse is not None and state == "ready":
        out.append(
            f'<circle cx="{cx}" cy="{cy}" r="{R + 4 + 22 * pulse:.1f}" fill="none" '
            f'stroke="{TEAL}" stroke-width="3" opacity="{(1 - pulse) * 0.75:.3f}"/>'
        )
    out.append(
        f'<circle cx="{cx}" cy="{cy}" r="{R}" fill="{fill}" '
        f'stroke="{stroke}" stroke-width="3"{dash}/>'
    )
    out.append(
        f'<text x="{cx}" y="{cy+7}" text-anchor="middle" font-family="{FONT}" '
        f'font-size="19" font-weight="bold" fill="{text_color}">{label}</text>'
    )
    if state == "closed":
        # Keep the name legible -- a bare checkmark loses track of which bead
        # closed, which is the one thing this beat is about.
        bx, by = cx + R * 0.72, cy - R * 0.72
        out.append(
            f'<circle cx="{bx}" cy="{by}" r="14" fill="#FFFFFF" stroke="{MUTED}" stroke-width="2.5"/>'
            f'<path d="M{bx-6},{by} l4,5 l8,-9" fill="none" stroke="{MUTED}" '
            f'stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>'
        )
    if tag:
        color = NAVY if state == "in_progress" else stroke
        out.append(
            f'<text x="{cx}" y="{cy+R+25}" text-anchor="middle" font-family="{FONT}" '
            f'font-size="17" fill="{color}">{tag}</text>'
        )
    out.append("</g>")
    return "".join(out)


def blocks_edge(progress=1.0, satisfied=False):
    """Edge from design to store, drawn blocked-first as `bead dep add` reads."""
    if progress <= 0:
        return ""
    (x1, y1), (x2, _) = POS["design"], POS["store"]
    sx, ex = x1 + R + 6, x2 - R - 14
    color = MUTED if satisfied else NAVY
    dash = ' stroke-dasharray="8 7"' if satisfied else ""
    out = [
        f'<line x1="{sx}" y1="{y1}" x2="{sx + (ex - sx) * progress:.1f}" y2="{y1}" '
        f'stroke="{color}" stroke-width="3"{dash}/>'
    ]
    if progress > 0.97:
        out.append(f'<path d="M{ex},{y1} l-13,-8 l0,16 z" fill="{color}"/>')
        out.append(
            f'<text x="{(sx+ex)/2}" y="{y1-16}" text-anchor="middle" '
            f'font-family="{FONT}" font-size="17" fill="{color}">'
            f'{"satisfied" if satisfied else "blocks"}</text>'
        )
    return "".join(out)


def worker(opacity=0.0, active=False):
    if opacity <= 0:
        return ""
    cx, cy = WORKER
    accent = TEAL if active else MUTED
    return f"""<g opacity="{opacity:.3f}">
      <circle cx="{cx}" cy="{cy-56}" r="6" fill="none" stroke="{accent}" stroke-width="3"/>
      <line x1="{cx}" y1="{cy-50}" x2="{cx}" y2="{cy-37}" stroke="{accent}" stroke-width="3"/>
      <rect x="{cx-39}" y="{cy-37}" width="78" height="64" rx="14"
            fill="#FFFFFF" stroke="{accent}" stroke-width="3"/>
      <circle cx="{cx-14}" cy="{cy-12}" r="6" fill="{accent}"/>
      <circle cx="{cx+14}" cy="{cy-12}" r="6" fill="{accent}"/>
      <line x1="{cx-15}" y1="{cy+11}" x2="{cx+15}" y2="{cy+11}"
            stroke="{accent}" stroke-width="3" stroke-linecap="round"/>
      <text x="{cx}" y="{cy+52}" text-anchor="middle" font-family="{FONT}"
            font-size="17" fill="{NAVY}">worker-1</text>
    </g>"""


def claim_arrow(progress):
    """Routed well below `store` so it never reads as another dependency edge."""
    if progress <= 0:
        return ""
    (x1, y1) = POS["design"]
    cx, cy = WORKER
    sx, sy = x1 + R + 6, y1 + 14
    ex, ey = cx - 52, cy - 10
    ctrl = (520, 300)
    head = ""
    if progress > 0.95:
        head = (
            f'<path d="M{ex},{ey} l-15,-6 l4,17 z" fill="{TEAL}"/>'
            f'<text x="{ctrl[0]+40}" y="{ctrl[1]-6}" text-anchor="middle" '
            f'font-family="{FONT}" font-size="18" font-weight="bold" '
            f'fill="{TEAL}">claim</text>'
        )
    return (
        f'<path d="M{sx},{sy} Q{ctrl[0]},{ctrl[1]} {ex},{ey}" fill="none" '
        f'stroke="{TEAL}" stroke-width="4" stroke-linecap="round" pathLength="1" '
        f'stroke-dasharray="1" stroke-dashoffset="{1-progress:.3f}"/>' + head
    )


def storage(opacity=0.0, flushing=0.0):
    if opacity <= 0:
        return ""
    dx, dy = DB_C
    px, py = DOC_C
    arrow = ""
    if flushing > 0:
        ax1, ax2 = dx + 54, px - 44
        arrow = (
            f'<line x1="{ax1}" y1="{dy}" x2="{ax1 + (ax2-ax1)*flushing:.1f}" y2="{dy}" '
            f'stroke="{AMBER}" stroke-width="3"/>'
        )
        if flushing > 0.95:
            arrow += (
                f'<path d="M{ax2},{dy} l-13,-8 l0,16 z" fill="{AMBER}"/>'
                f'<text x="{(ax1+ax2)/2}" y="{dy-16}" text-anchor="middle" '
                f'font-family="{FONT}" font-size="17" fill="{AMBER}">flush</text>'
            )
    doc_color = AMBER if flushing > 0.95 else MUTED
    return f"""<g opacity="{opacity:.3f}">
      <ellipse cx="{dx}" cy="{dy-24}" rx="38" ry="12" fill="#FFFFFF" stroke="{NAVY}" stroke-width="3"/>
      <path d="M{dx-38},{dy-24} L{dx-38},{dy+24} A38,12 0 0 0 {dx+38},{dy+24} L{dx+38},{dy-24}"
            fill="#FFFFFF" stroke="{NAVY}" stroke-width="3"/>
      <ellipse cx="{dx}" cy="{dy}" rx="38" ry="12" fill="none" stroke="{NAVY}" stroke-width="3"/>
      <text x="{dx}" y="{dy+58}" text-anchor="middle" font-family="{FONT}"
            font-size="18" font-weight="bold" fill="{NAVY}">SQLite</text>
      <text x="{dx}" y="{dy+79}" text-anchor="middle" font-family="{FONT}"
            font-size="15" fill="{MUTED}">live, gitignored</text>
      {arrow}
      <path d="M{px-30},{py-32} L{px+13},{py-32} L{px+30},{py-15} L{px+30},{py+32} L{px-30},{py+32} Z"
            fill="#FFFFFF" stroke="{doc_color}" stroke-width="3" stroke-linejoin="round"/>
      <path d="M{px+13},{py-32} L{px+13},{py-15} L{px+30},{py-15}"
            fill="none" stroke="{doc_color}" stroke-width="3" stroke-linejoin="round"/>
      <text x="{px}" y="{py+58}" text-anchor="middle" font-family="{FONT}"
            font-size="18" font-weight="bold" fill="{doc_color}">checkpoint</text>
      <text x="{px}" y="{py+79}" text-anchor="middle" font-family="{FONT}"
            font-size="15" fill="{MUTED}">Git tracks this</text>
    </g>"""


def dots(step, total=6):
    return "".join(
        f'<circle cx="{W - 40 - (total - 1 - i) * 22}" cy="38" r="6" '
        f'fill="{NAVY if i == step else "#D5DBE4"}"/>'
        for i in range(total)
    )


def frame(states, caption, command, step, edge=0.0, satisfied=False,
          worker_op=0.0, worker_active=False, claim=0.0, store_op=0.0,
          flush=0.0, fades=None, pulse=None):
    fades = fades or {}
    body = [
        f'<rect width="{W}" height="{H}" fill="{BG}"/>',
        dots(step),
        blocks_edge(edge, satisfied),
        claim_arrow(claim),
        worker(worker_op, worker_active),
        storage(store_op, flush),
    ]
    for name in ("design", "store", "docs"):
        if states.get(name):
            body.append(bead(name, name, states[name], fades.get(name, 1.0), pulse))
    body.append(
        f'<rect x="{W/2-300}" y="410" width="600" height="42" rx="8" fill="{CHIP_BG}"/>'
        f'<text x="{W/2}" y="438" text-anchor="middle" font-family="{MONO}" '
        f'font-size="19" fill="{NAVY}">{command}</text>'
    )
    body.append(
        f'<text x="{W/2}" y="484" text-anchor="middle" font-family="{FONT}" '
        f'font-size="20" fill="{CAPTION}">{caption}</text>'
    )
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" '
        f'viewBox="0 0 {W} {H}">' + "".join(body) + "</svg>"
    )


def build():
    frames = []
    STEPS = 8

    def add(svg, ms):
        frames.append((svg, ms))

    # 1 -- creation
    cap1 = "Three beads are created. Priority orders them; nothing is claimed yet."
    cmd1 = "bead create --title &#8230; --priority N"
    for i in range(STEPS):
        t = smoothstep((i + 1) / STEPS)
        add(frame({"design": "new", "store": "new", "docs": "new"}, cap1, cmd1, 0,
                  fades={"design": t, "store": t, "docs": t}), 60)
    add(frames[-1][0], 1500)

    # 2 -- dependency
    cap2 = "A blocks edge: store cannot start until design closes."
    cmd2 = "bead dep add store design"
    for i in range(STEPS):
        t = smoothstep((i + 1) / STEPS)
        add(frame({"design": "new", "store": "new", "docs": "new"}, cap2, cmd2, 1,
                  edge=t), 60)
    add(frame({"design": "ready", "store": "blocked", "docs": "ready"}, cap2, cmd2, 1,
              edge=1.0), 1700)

    # 3 -- the ready frontier. Pulsing the ready beads is what distinguishes this
    # beat; the underlying state is unchanged from the end of beat 2.
    cap3 = "The ready frontier: open, unassigned, no unfinished blockers."
    for rep in range(2):
        for i in range(STEPS):
            add(frame({"design": "ready", "store": "blocked", "docs": "ready"},
                      cap3, "bead list --ready", 2, edge=1.0,
                      pulse=(i + 1) / STEPS), 55)
    add(frame({"design": "ready", "store": "blocked", "docs": "ready"},
              cap3, "bead list --ready", 2, edge=1.0), 1200)

    # 4 -- atomic claim
    cap4 = "Claim selects and assigns in one transaction — no two workers get the same bead."
    cmd4 = "bead claim --assignee worker-1"
    for i in range(STEPS):
        t = smoothstep((i + 1) / STEPS)
        add(frame({"design": "ready", "store": "blocked", "docs": "ready"}, cap4, cmd4, 3,
                  edge=1.0, worker_op=t, claim=t), 60)
    add(frame({"design": "in_progress", "store": "blocked", "docs": "ready"}, cap4, cmd4, 3,
              edge=1.0, worker_op=1.0, worker_active=True, claim=1.0), 2000)

    # 5 -- closing advances the frontier
    add(frame({"design": "closed", "store": "ready", "docs": "ready"},
              "Closing design satisfies the edge — store joins the frontier.",
              "bead close design --reason &#8230;", 4, edge=1.0, satisfied=True), 2200)

    # 6 -- checkpoint
    cap6 = "Nothing flushes implicitly. Flush before committing, or a clone sees stale state."
    for i in range(STEPS):
        t = smoothstep((i + 1) / STEPS)
        add(frame({"design": "closed", "store": "ready", "docs": "ready"},
                  cap6, "bead sync flush-only", 5,
                  edge=1.0, satisfied=True, store_op=1.0, flush=t), 60)
    add(frames[-1][0], 2600)
    return frames


def main():
    out_path = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                            "bead-lifecycle.gif")
    images, durations = [], []
    for svg, ms in build():
        png = cairosvg.svg2png(bytestring=svg.encode("utf-8"),
                               output_width=W, output_height=H)
        images.append(Image.open(io.BytesIO(png)).convert("RGB"))
        durations.append(ms)

    # Flat vector art quantizes to a small palette with no visible loss, and the
    # palette is most of the file-size win for a README asset.
    quantized = [im.quantize(colors=48, method=Image.MEDIANCUT,
                             dither=Image.Dither.NONE) for im in images]
    quantized[0].save(out_path, save_all=True, append_images=quantized[1:],
                      duration=durations, loop=0, optimize=True, disposal=2)

    print(f"wrote {out_path}")
    print(f"  {len(quantized)} frames, {sum(durations)/1000:.1f}s loop, "
          f"{os.path.getsize(out_path)/1024:.0f} KB")


if __name__ == "__main__":
    main()
