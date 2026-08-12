#!/usr/bin/env python3
"""Generate docs/img/bead-lifecycle.gif -- the bead-rs lifecycle animation.

The animation is a documentation surface like any other, so it is generated
from a committed script rather than drawn once and left to rot. Re-run it after
any change to the lifecycle model:

    python3 docs/img/generate-lifecycle-animation.py

Requires cairosvg and Pillow. Output is deterministic: the same source produces
a byte-identical GIF.


Motion design
-------------
Timed against Thomas & Johnston's twelve principles, applied to carry meaning
rather than charm. The load-bearing ones here:

  Timing         Semantic. The claim fires in 0.2s while every other beat runs
                 0.4-0.6s, because atomicity is the property being taught: it
                 must read as one indivisible snap, not a process.
  Staging        Elements not involved in the current beat drop to DIM so the
                 eye is led. In beat 3 `store` dims hardest -- it is the
                 counter-example that defines the ready frontier.
  Follow-through Consequences trail their causes instead of firing together.
                 Closing `design` sends a ripple ALONG the edge, and only when
                 it lands does `store` pop to ready. That stagger is dependency
                 propagation, drawn.
  Anticipation   A wind-up before the edge lands and before the claim fires,
                 so the viewer is looking in the right place already.
  Squash/stretch Volume-preserving (sx * sy == 1), used only on impact and
                 recoil, so a rigid diagram still reads as physical.
  Arc            Beads drop in along an arc; the claim path and the ripple both
                 travel curves rather than straight lines.
  Secondary      A rotating dash ring on claimed work, a worker bob on impact,
                 checkpoint lines filling one at a time.
  Slow in/out    A vocabulary of eases rather than one smoothstep: back for
                 arrivals, elastic for state flips, cubic in-out for travel.

Frames are rendered at FPS, quantized against a single global palette (a
per-frame palette would bloat the GIF), and runs of identical frames are
collapsed into one frame with a longer duration so holds cost almost nothing.
"""

import io
import math
import os

import cairosvg
from PIL import Image, ImageChops

W, H = 1000, 500
FPS = 20

BG = "#FFFFFF"
NAVY = "#16233F"
TEAL = "#0E9AA3"
TEAL_DEEP = "#0B7C84"
AMBER = "#DA7B0D"
MUTED = "#9BA6B7"
MUTED_FILL = "#EFF2F6"
CAPTION = "#3C4A63"
CHIP_BG = "#F4F6F9"

FONT = "DejaVu Sans, Helvetica, Arial, sans-serif"
MONO = "DejaVu Sans Mono, monospace"

R = 42
DIM = 0.28  # staging: how far a non-focal element recedes

POS = {"design": (168, 150), "store": (498, 150), "docs": (168, 306)}
WORKER = (862, 296)
DB_C, DOC_C = (596, 306), (856, 306)


# --------------------------------------------------------------------------
# easing
# --------------------------------------------------------------------------
def linear(t):
    return t


def ease_out_cubic(t):
    return 1 - (1 - t) ** 3


def ease_in_cubic(t):
    return t ** 3


def ease_in_out_cubic(t):
    return 4 * t ** 3 if t < 0.5 else 1 - (-2 * t + 2) ** 3 / 2


def ease_out_back(t, s=1.9):
    return 1 + (s + 1) * (t - 1) ** 3 + s * (t - 1) ** 2


def ease_out_elastic(t):
    if t in (0.0, 1.0):
        return t
    return 2 ** (-9 * t) * math.sin((t * 10 - 0.75) * (2 * math.pi / 3)) + 1


class Track:
    """A keyframed scalar. Pose to pose, with an ease per segment."""

    def __init__(self, initial):
        self.initial = initial
        self.segments = []  # (t0, t1, v0, v1, ease)

    def to(self, t0, dur, v1, ease=ease_in_out_cubic):
        v0 = self.segments[-1][3] if self.segments else self.initial
        self.segments.append((t0, t0 + dur, v0, v1, ease))
        return self

    def hold_to(self, t):
        return self

    def at(self, t):
        v = self.initial
        for t0, t1, v0, v1, ease in self.segments:
            if t >= t1:
                v = v1
            elif t >= t0:
                return v0 + (v1 - v0) * ease((t - t0) / (t1 - t0))
            else:
                break
        return v


def pulse(t, t0, dur):
    """0 -> 1 -> 0, for one-shot emphases. Outside the window returns 0."""
    if t < t0 or t > t0 + dur:
        return 0.0
    x = (t - t0) / dur
    return math.sin(x * math.pi)


def mix(c1, c2, k):
    k = max(0.0, min(1.0, k))
    a = tuple(int(c1[i:i + 2], 16) for i in (1, 3, 5))
    b = tuple(int(c2[i:i + 2], 16) for i in (1, 3, 5))
    return "#%02X%02X%02X" % tuple(round(a[i] + (b[i] - a[i]) * k) for i in range(3))


# --------------------------------------------------------------------------
# timeline
# --------------------------------------------------------------------------
T_CREATE, T_DEP, T_READY = 0.55, 2.95, 5.75
T_CLAIM, T_CLOSE, T_FLUSH = 8.45, 11.75, 14.85
END = 18.3

BEATS = [
    (T_CREATE, "bead create --title &#8230; --priority N",
     "Three beads are created. Priority orders them; nothing is claimed yet."),
    (T_DEP, "bead dep add store design",
     "A blocks edge: store cannot start until design closes."),
    (T_READY, "bead list --ready",
     "The ready frontier: open, unassigned, no unfinished blockers."),
    (T_CLAIM, "bead claim --assignee worker-1",
     "One transaction selects and assigns — no two workers get the same bead."),
    (T_CLOSE, "bead close design --reason &#8230;",
     "Closing design satisfies the edge — store joins the frontier."),
    (T_FLUSH, "bead sync flush-only",
     "Nothing flushes implicitly. Flush before committing, or a clone sees stale state."),
]


class Scene:
    def __init__(self):
        t = {}
        # --- beads: appear, dim (staging), squash (impact), state blends -----
        for n in ("design", "store", "docs"):
            t[f"{n}.scale"] = Track(0.0)
            t[f"{n}.drop"] = Track(-26.0)   # arc: falls into place
            t[f"{n}.label"] = Track(0.0)
            t[f"{n}.dim"] = Track(1.0)
            t[f"{n}.squash"] = Track(1.0)   # >1 wide, <1 tall; volume preserved
            t[f"{n}.ready"] = Track(0.0)
            t[f"{n}.blocked"] = Track(0.0)
            t[f"{n}.working"] = Track(0.0)
            t[f"{n}.closed"] = Track(0.0)
        t["edge.draw"] = Track(0.0)
        t["edge.satisfied"] = Track(0.0)
        t["edge.label"] = Track(0.0)
        t["worker.scale"] = Track(0.0)
        t["worker.bob"] = Track(0.0)
        t["worker.active"] = Track(0.0)
        t["claim.draw"] = Track(0.0)
        t["claim.label"] = Track(0.0)
        t["ripple"] = Track(0.0)
        t["store_.on"] = Track(0.0)
        t["store_.arrow"] = Track(0.0)
        t["store_.lines"] = Track(0.0)
        t["dag.dim"] = Track(1.0)
        self.t = t
        self.build()

    def v(self, k, t):
        return self.t[k].at(t)

    def build(self):
        t = self.t

        # === 1. creation ================================================
        # Overlapping action: staggered, not simultaneous. Elastic arrival
        # gives squash-and-stretch without deforming the shape.
        for i, n in enumerate(("design", "store", "docs")):
            s = T_CREATE + i * 0.22
            t[f"{n}.scale"].to(s, 0.62, 1.0, ease_out_elastic)
            t[f"{n}.drop"].to(s, 0.5, 0.0, ease_out_cubic)
            # Follow-through: the label settles after the circle does.
            t[f"{n}.label"].to(s + 0.26, 0.3, 1.0, ease_out_cubic)

        # === 2. dependency ==============================================
        # Anticipation: design swells slightly before the edge leaves it.
        t["design.squash"].to(T_DEP, 0.18, 1.10, ease_out_cubic)
        t["design.squash"].to(T_DEP + 0.18, 0.16, 1.0, ease_in_out_cubic)
        t["docs.dim"].to(T_DEP, 0.3, DIM + 0.25)          # staging
        t["edge.draw"].to(T_DEP + 0.30, 0.55, 1.0, ease_in_out_cubic)
        t["edge.label"].to(T_DEP + 0.75, 0.25, 1.0, ease_out_cubic)
        # Follow-through: store recoils on impact, then settles blocked.
        t["store.squash"].to(T_DEP + 0.83, 0.13, 1.24, ease_out_cubic)
        t["store.squash"].to(T_DEP + 0.96, 0.42, 1.0, ease_out_elastic)
        t["store.blocked"].to(T_DEP + 0.88, 0.40, 1.0, ease_out_cubic)
        t["design.ready"].to(T_DEP + 0.95, 0.40, 1.0, ease_out_cubic)
        t["docs.ready"].to(T_DEP + 1.05, 0.40, 1.0, ease_out_cubic)
        t["docs.dim"].to(T_DEP + 1.0, 0.35, 1.0)

        # === 3. ready frontier ==========================================
        # Staging: the blocked bead recedes hardest -- it is the contrast that
        # defines the frontier. Ring pulses are emitted in render().
        t["store.dim"].to(T_READY, 0.35, DIM)
        t["store.dim"].to(T_READY + 2.35, 0.3, 1.0)

        # === 4. claim ===================================================
        t["worker.scale"].to(T_CLAIM, 0.45, 1.0, ease_out_back)
        t["docs.dim"].to(T_CLAIM + 0.1, 0.3, DIM + 0.3)
        # Anticipation: design compresses and pulls back before it is taken.
        t["design.squash"].to(T_CLAIM + 0.62, 0.22, 0.86, ease_out_cubic)
        # TIMING AS MEANING: 0.2s. Everything else here is 0.4-0.6s.
        t["claim.draw"].to(T_CLAIM + 0.86, 0.20, 1.0, ease_in_cubic)
        t["claim.label"].to(T_CLAIM + 1.02, 0.18, 1.0, ease_out_cubic)
        # Impact: overlapping reactions at both ends of the arrow.
        t["design.squash"].to(T_CLAIM + 1.04, 0.12, 1.20, ease_out_cubic)
        t["design.squash"].to(T_CLAIM + 1.16, 0.46, 1.0, ease_out_elastic)
        t["design.working"].to(T_CLAIM + 1.02, 0.28, 1.0, ease_out_cubic)
        t["worker.bob"].to(T_CLAIM + 1.06, 0.14, 1.0, ease_out_cubic)
        t["worker.bob"].to(T_CLAIM + 1.20, 0.5, 0.0, ease_out_elastic)
        t["worker.active"].to(T_CLAIM + 1.02, 0.3, 1.0)
        t["docs.dim"].to(T_CLAIM + 1.6, 0.4, 1.0)

        # === 5. close, and the consequence trailing it ==================
        # Anticipation, then the cause; the effect arrives later, by ripple.
        t["design.squash"].to(T_CLOSE, 0.2, 0.9, ease_out_cubic)
        t["design.closed"].to(T_CLOSE + 0.2, 0.35, 1.0, ease_out_cubic)
        t["design.working"].to(T_CLOSE + 0.2, 0.3, 0.0)
        t["design.ready"].to(T_CLOSE + 0.2, 0.3, 0.0)
        t["design.squash"].to(T_CLOSE + 0.2, 0.4, 1.0, ease_out_elastic)
        t["claim.draw"].to(T_CLOSE + 0.15, 0.3, 0.0, ease_in_cubic)
        t["claim.label"].to(T_CLOSE, 0.2, 0.0)
        t["edge.satisfied"].to(T_CLOSE + 0.42, 0.35, 1.0, ease_out_cubic)
        # Follow-through: propagation is drawn as travel, then arrival.
        t["ripple"].to(T_CLOSE + 0.66, 0.48, 1.0, ease_in_out_cubic)
        t["store.blocked"].to(T_CLOSE + 1.06, 0.3, 0.0, ease_out_cubic)
        t["store.ready"].to(T_CLOSE + 1.08, 0.34, 1.0, ease_out_cubic)
        t["store.squash"].to(T_CLOSE + 1.08, 0.14, 1.22, ease_out_cubic)
        t["store.squash"].to(T_CLOSE + 1.22, 0.55, 1.0, ease_out_elastic)
        # Overlap: the worker leaves on its own schedule, not on the beat.
        t["worker.active"].to(T_CLOSE + 0.5, 0.4, 0.0)
        t["worker.scale"].to(T_CLOSE + 0.85, 0.5, 0.0, ease_in_cubic)

        # === 6. flush ===================================================
        t["dag.dim"].to(T_FLUSH, 0.35, 0.42)              # staging
        t["store_.on"].to(T_FLUSH + 0.2, 0.42, 1.0, ease_out_back)
        t["store_.arrow"].to(T_FLUSH + 0.72, 0.45, 1.0, ease_in_out_cubic)
        t["store_.lines"].to(T_FLUSH + 1.15, 0.85, 1.0, linear)  # secondary


S = Scene()


# --------------------------------------------------------------------------
# drawing
# --------------------------------------------------------------------------
def bead_svg(name, t):
    cx, cy = POS[name]
    scale = S.v(f"{name}.scale", t)
    if scale <= 0.001:
        return ""
    cy += S.v(f"{name}.drop", t)
    sq = S.v(f"{name}.squash", t)
    dim = S.v(f"{name}.dim", t) * S.v("dag.dim", t)
    ready, blocked = S.v(f"{name}.ready", t), S.v(f"{name}.blocked", t)
    working, closed = S.v(f"{name}.working", t), S.v(f"{name}.closed", t)

    # Volume-preserving squash: widening always costs height.
    rx, ry = R * scale * sq, R * scale / sq

    fill, stroke, ink = MUTED_FILL, MUTED, NAVY
    if ready > 0:
        fill, stroke, ink = (mix(fill, TEAL, ready), mix(stroke, TEAL, ready),
                             mix(ink, "#FFFFFF", ready))
    if blocked > 0:
        fill = mix(fill, MUTED_FILL, blocked)
        stroke, ink = mix(stroke, MUTED, blocked), mix(ink, MUTED, blocked)
    if working > 0:
        fill, stroke, ink = (mix(fill, TEAL, working), mix(stroke, NAVY, working),
                             mix(ink, "#FFFFFF", working))
    if closed > 0:
        fill = mix(fill, "#FFFFFF", closed)
        stroke, ink = mix(stroke, MUTED, closed), mix(ink, MUTED, closed)

    out = [f'<g opacity="{dim:.3f}">']

    # Ready-frontier pulses (beat 3). Exaggerated radius, staggered per bead so
    # the two do not read as one synchronized blink.
    if name in ("design", "docs"):
        for k, delay in enumerate((0.0, 0.75)):
            off = 0.0 if name == "design" else 0.3
            p = pulse(t, T_READY + 0.42 + delay + off, 0.95)
            if p > 0.01:
                out.append(
                    f'<circle cx="{cx}" cy="{cy}" r="{R + 6 + 34 * (1 - (1 - p) ** 0.5):.1f}" '
                    f'fill="none" stroke="{TEAL}" stroke-width="3" opacity="{p * 0.55:.3f}"/>'
                )

    if working > 0.02:
        # Secondary action: work in progress is never still.
        ang = (t * 62) % 360
        out.append(
            f'<g transform="rotate({ang:.1f} {cx} {cy})" opacity="{working:.3f}">'
            f'<circle cx="{cx}" cy="{cy}" r="{rx + 9:.1f}" fill="none" stroke="{TEAL_DEEP}" '
            f'stroke-width="3" stroke-dasharray="9 8" stroke-linecap="round"/></g>'
        )

    out.append(
        f'<ellipse cx="{cx}" cy="{cy}" rx="{rx:.1f}" ry="{ry:.1f}" fill="{fill}" '
        f'stroke="{stroke}" stroke-width="3"/>'
    )
    lab = S.v(f"{name}.label", t)
    if lab > 0.01:
        out.append(
            f'<text x="{cx}" y="{cy+7}" text-anchor="middle" font-family="{FONT}" '
            f'font-size="19" font-weight="bold" fill="{ink}" opacity="{lab:.3f}">{name}</text>'
        )
    if closed > 0.02:
        # Keep the name legible: a bare tick loses which bead closed, and that
        # is the one thing this beat is about.
        bx, by = cx + rx * 0.72, cy - ry * 0.72
        g = ease_out_back(min(1.0, closed))
        out.append(
            f'<g opacity="{closed:.3f}" transform="translate({bx} {by}) scale({g:.3f})">'
            f'<circle r="14" fill="#FFFFFF" stroke="{MUTED}" stroke-width="2.5"/>'
            f'<path d="M-6,0 l4,5 l8,-9" fill="none" stroke="{MUTED}" stroke-width="3" '
            f'stroke-linecap="round" stroke-linejoin="round"/></g>'
        )

    tag, tcol = None, stroke
    if closed > 0.5:
        tag, tcol = "closed", MUTED
    elif working > 0.5:
        tag, tcol = "in progress", NAVY
    elif blocked > 0.5:
        tag, tcol = "blocked", MUTED
    elif ready > 0.5:
        tag, tcol = "ready", TEAL
    if tag:
        out.append(
            f'<text x="{cx}" y="{cy+R+26}" text-anchor="middle" font-family="{FONT}" '
            f'font-size="17" fill="{tcol}">{tag}</text>'
        )
    out.append("</g>")
    return "".join(out)


def edge_svg(t):
    draw = S.v("edge.draw", t)
    if draw <= 0.001:
        return ""
    sat, dim = S.v("edge.satisfied", t), S.v("dag.dim", t)
    (x1, y1), (x2, _) = POS["design"], POS["store"]
    sx, ex = x1 + R + 8, x2 - R - 15
    color = mix(NAVY, MUTED, sat)
    dash = ' stroke-dasharray="8 7"' if sat > 0.5 else ""
    cur = sx + (ex - sx) * draw
    out = [f'<g opacity="{dim:.3f}">',
           f'<line x1="{sx}" y1="{y1}" x2="{cur:.1f}" y2="{y1}" stroke="{color}" '
           f'stroke-width="3"{dash}/>']
    if draw > 0.985:
        head = ease_out_back(min(1.0, (draw - 0.985) / 0.015 if draw < 1 else 1.0))
        out.append(
            f'<g transform="translate({ex} {y1}) scale({head:.3f})">'
            f'<path d="M0,0 l-13,-8 l0,16 z" fill="{color}"/></g>'
        )
    lab = S.v("edge.label", t)
    if lab > 0.01:
        txt = "satisfied" if sat > 0.5 else "blocks"
        out.append(
            f'<text x="{(sx+ex)/2}" y="{y1-17}" text-anchor="middle" font-family="{FONT}" '
            f'font-size="17" fill="{color}" opacity="{lab:.3f}">{txt}</text>'
        )
    # Follow-through made literal: the unblocking travels the edge it satisfies.
    rp = S.v("ripple", t)
    if 0.001 < rp < 0.999:
        px = sx + (ex - sx) * rp
        out.append(
            f'<circle cx="{px:.1f}" cy="{y1}" r="17" fill="{TEAL}" opacity="0.16"/>'
            f'<circle cx="{px:.1f}" cy="{y1}" r="7.5" fill="{TEAL}"/>'
        )
    out.append("</g>")
    return "".join(out)


def claim_svg(t):
    d = S.v("claim.draw", t)
    if d <= 0.001:
        return ""
    dim = S.v("dag.dim", t)
    (x1, y1) = POS["design"]
    cx, cy = WORKER
    sx, sy = x1 + R + 8, y1 + 16
    ex, ey = cx - 52, cy - 14
    ctrl = (505, 300)  # arc, routed clear of `store`
    out = [f'<g opacity="{dim:.3f}">',
           f'<path d="M{sx},{sy} Q{ctrl[0]},{ctrl[1]} {ex},{ey}" fill="none" stroke="{TEAL}" '
           f'stroke-width="4" stroke-linecap="round" pathLength="1" stroke-dasharray="1" '
           f'stroke-dashoffset="{1-d:.3f}"/>']
    if d > 0.96:
        out.append(f'<path d="M{ex},{ey} l-15,-6 l4,17 z" fill="{TEAL}"/>')
    lab = S.v("claim.label", t)
    if lab > 0.01:
        out.append(
            f'<text x="{ctrl[0]+44}" y="{ctrl[1]-8}" text-anchor="middle" font-family="{FONT}" '
            f'font-size="18" font-weight="bold" fill="{TEAL}" opacity="{lab:.3f}">claim</text>'
        )
    out.append("</g>")
    return "".join(out)


def worker_svg(t):
    s = S.v("worker.scale", t)
    if s <= 0.001:
        return ""
    cx, cy = WORKER
    cy += -7 * S.v("worker.bob", t)   # secondary action on impact
    accent = mix(MUTED, TEAL, S.v("worker.active", t))
    dim = S.v("dag.dim", t)
    return f"""<g opacity="{dim:.3f}" transform="translate({cx} {cy}) scale({s:.3f}) translate({-cx} {-cy})">
      <circle cx="{cx}" cy="{cy-56}" r="6" fill="none" stroke="{accent}" stroke-width="3"/>
      <line x1="{cx}" y1="{cy-50}" x2="{cx}" y2="{cy-37}" stroke="{accent}" stroke-width="3"/>
      <rect x="{cx-39}" y="{cy-37}" width="78" height="64" rx="14" fill="#FFFFFF"
            stroke="{accent}" stroke-width="3"/>
      <circle cx="{cx-14}" cy="{cy-12}" r="6" fill="{accent}"/>
      <circle cx="{cx+14}" cy="{cy-12}" r="6" fill="{accent}"/>
      <line x1="{cx-15}" y1="{cy+11}" x2="{cx+15}" y2="{cy+11}" stroke="{accent}"
            stroke-width="3" stroke-linecap="round"/>
      <text x="{cx}" y="{cy+52}" text-anchor="middle" font-family="{FONT}" font-size="17"
            fill="{NAVY}">worker-1</text>
    </g>"""


def storage_svg(t):
    on = S.v("store_.on", t)
    if on <= 0.001:
        return ""
    dx, dy = DB_C
    px, py = DOC_C
    arrow, lines = S.v("store_.arrow", t), S.v("store_.lines", t)
    doc_color = mix(MUTED, AMBER, min(1.0, arrow * 1.6))
    out = [f'<g opacity="{min(1.0, on):.3f}" transform="translate({dx} {dy}) '
           f'scale({on:.3f}) translate({-dx} {-dy})">']
    out.append(f"""
      <ellipse cx="{dx}" cy="{dy-24}" rx="38" ry="12" fill="#FFFFFF" stroke="{NAVY}" stroke-width="3"/>
      <path d="M{dx-38},{dy-24} L{dx-38},{dy+24} A38,12 0 0 0 {dx+38},{dy+24} L{dx+38},{dy-24}"
            fill="#FFFFFF" stroke="{NAVY}" stroke-width="3"/>
      <ellipse cx="{dx}" cy="{dy}" rx="38" ry="12" fill="none" stroke="{NAVY}" stroke-width="3"/>
      <text x="{dx}" y="{dy+56}" text-anchor="middle" font-family="{FONT}" font-size="18"
            font-weight="bold" fill="{NAVY}">SQLite</text>
      <text x="{dx}" y="{dy+77}" text-anchor="middle" font-family="{FONT}" font-size="15"
            fill="{MUTED}">live, gitignored</text>""")
    if arrow > 0.005:
        a1, a2 = dx + 54, px - 44
        out.append(f'<line x1="{a1}" y1="{dy}" x2="{a1 + (a2-a1)*arrow:.1f}" y2="{dy}" '
                   f'stroke="{AMBER}" stroke-width="3"/>')
        if arrow > 0.96:
            out.append(f'<path d="M{a2},{dy} l-13,-8 l0,16 z" fill="{AMBER}"/>'
                       f'<text x="{(a1+a2)/2}" y="{dy-17}" text-anchor="middle" '
                       f'font-family="{FONT}" font-size="17" fill="{AMBER}">flush</text>')
    out.append(f"""
      <path d="M{px-30},{py-32} L{px+13},{py-32} L{px+30},{py-15} L{px+30},{py+32} L{px-30},{py+32} Z"
            fill="#FFFFFF" stroke="{doc_color}" stroke-width="3" stroke-linejoin="round"/>
      <path d="M{px+13},{py-32} L{px+13},{py-15} L{px+30},{py-15}" fill="none"
            stroke="{doc_color}" stroke-width="3" stroke-linejoin="round"/>""")
    # Secondary action: the checkpoint visibly fills, one record at a time.
    for i in range(4):
        k = max(0.0, min(1.0, lines * 4 - i))
        if k > 0.01:
            ly = py - 4 + i * 11
            out.append(f'<line x1="{px-19}" y1="{ly}" x2="{px-19 + 38*k:.1f}" y2="{ly}" '
                       f'stroke="{AMBER}" stroke-width="3" stroke-linecap="round"/>')
    out.append(f"""
      <text x="{px}" y="{py+56}" text-anchor="middle" font-family="{FONT}" font-size="18"
            font-weight="bold" fill="{doc_color}">checkpoint</text>
      <text x="{px}" y="{py+77}" text-anchor="middle" font-family="{FONT}" font-size="15"
            fill="{MUTED}">Git tracks this</text></g>""")
    return "".join(out)


def chrome_svg(t):
    """Step dots plus cross-faded caption and command chip."""
    idx = 0
    for i, (start, _, _) in enumerate(BEATS):
        if t >= start - 0.2:
            idx = i
    out = []
    for i in range(len(BEATS)):
        active = 1.0 if i == idx else 0.0
        r = 6 + 1.5 * active
        out.append(f'<circle cx="{W - 38 - (len(BEATS)-1-i)*22}" cy="36" r="{r:.1f}" '
                   f'fill="{NAVY if i == idx else "#D5DBE4"}"/>')
    out.append(f'<rect x="{W/2-300}" y="404" width="600" height="42" rx="8" fill="{CHIP_BG}"/>')
    # Slow in / slow out applied to text too: beats cross-fade, never cut.
    for i, (start, cmd, cap) in enumerate(BEATS):
        nxt = BEATS[i + 1][0] if i + 1 < len(BEATS) else END + 1
        fade_in = max(0.0, min(1.0, (t - (start - 0.12)) / 0.3))
        fade_out = max(0.0, min(1.0, ((nxt - 0.22) - t) / 0.3))
        a = min(fade_in, fade_out)
        if a <= 0.01:
            continue
        out.append(f'<text x="{W/2}" y="432" text-anchor="middle" font-family="{MONO}" '
                   f'font-size="19" fill="{NAVY}" opacity="{a:.3f}">{cmd}</text>')
        out.append(f'<text x="{W/2}" y="478" text-anchor="middle" font-family="{FONT}" '
                   f'font-size="20" fill="{CAPTION}" opacity="{a:.3f}">{cap}</text>')
    return "".join(out)


def render(t):
    body = [f'<rect width="{W}" height="{H}" fill="{BG}"/>', chrome_svg(t),
            edge_svg(t), claim_svg(t), worker_svg(t), storage_svg(t)]
    body += [bead_svg(n, t) for n in ("design", "store", "docs")]
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" '
            f'viewBox="0 0 {W} {H}">' + "".join(body) + "</svg>")


def main():
    out_path = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                            "bead-lifecycle.gif")
    n = int(END * FPS)
    frames = []
    for i in range(n):
        png = cairosvg.svg2png(bytestring=render(i / FPS).encode("utf-8"),
                               output_width=W, output_height=H)
        frames.append(Image.open(io.BytesIO(png)).convert("RGB"))

    # Collapse identical consecutive frames into one longer frame, so the holds
    # between beats cost a frame each instead of twenty.
    step = int(1000 / FPS)
    kept, durations = [frames[0]], [step]
    for f in frames[1:]:
        if ImageChops.difference(f, kept[-1]).getbbox() is None:
            durations[-1] += step
        else:
            kept.append(f)
            durations.append(step)

    # One palette for the whole animation. Quantizing per frame would attach a
    # local palette to every frame and inflate the file.
    sample = Image.new("RGB", (W, len(kept) * 2))
    for i, f in enumerate(kept):
        sample.paste(f.resize((W, 2)), (0, i * 2))
    master = sample.quantize(colors=64, method=Image.MEDIANCUT)
    paletted = [f.quantize(palette=master, dither=Image.Dither.NONE) for f in kept]

    paletted[0].save(out_path, save_all=True, append_images=paletted[1:],
                     duration=durations, loop=0, optimize=True, disposal=1)

    print(f"wrote {out_path}")
    print(f"  {len(paletted)} frames (from {n} rendered), "
          f"{sum(durations)/1000:.1f}s loop, {os.path.getsize(out_path)/1024:.0f} KB")


if __name__ == "__main__":
    main()
