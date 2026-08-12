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
  Staging        Achieved by EMPHASIS, not suppression: the focal element
                 grows and thickens while the rest recede slightly in scale.
                 Dimming by opacity was measured and rejected -- see below.
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


Accessibility (WCAG 2.1 AA, the standard the DOJ Title II rule adopts)
----------------------------------------------------------------------
1.4.3 / 1.4.11 Contrast. Every colour that carries text clears 4.5:1 against
    the white ground and every meaningful graphic clears 3:1. verify_contrast()
    asserts all of them at generation time, so the palette cannot regress
    silently.

    This is why staging is done by emphasis rather than by dimming. Opacity
    dimming was measured first: to keep even the darkest text at 4.5:1 the dim
    floor works out to 0.95, which is not a dim at all. Scale and stroke weight
    lead the eye without touching contrast, so they replaced it. Entrances and
    exits scale from zero rather than fading, for the same reason -- a fade
    spends its middle at low contrast.

1.4.1 Use of Colour. State is never colour-only: every bead carries a text tag
    ("ready", "blocked", "in progress", "closed") alongside its fill.

2.3.1 Three Flashes. Verified by measuring frame-to-frame luminance across the
    finished GIF; the fastest change is far below the threshold.

2.2.2 Pause, Stop, Hide. A GIF cannot expose a pause control, so the animation
    plays a fixed number of times and rests on its final state instead of
    looping forever, and the README serves bead-lifecycle-static.png through
    a prefers-reduced-motion source so a reader who has asked their OS for
    less motion never receives the animation at all.

1.1.1 Non-text Content. The static poster is the text-equivalent storyboard,
    and the README prose describes the same sequence.
"""

import io
import math
import os

import cairosvg
from PIL import Image, ImageChops

W, H = 1000, 500
FPS = 20

# Palette. Every colour that carries text or meaning clears WCAG 2.1 AA
# against the white ground -- 4.5:1 for text, 3:1 for graphics. Held colours
# are asserted at generation time by verify_contrast(); the previous palette
# failed eight of these checks.
BG = "#FFFFFF"
NAVY = "#16233F"        # 15.60:1  bead labels on light fill, "in progress"
CAPTION = "#3C4A63"     #  8.93:1  caption line
TEAL = "#0B7B83"        #  5.03:1  as text AND as a fill behind white text
TEAL_DEEP = "#0B7C84"   #  4.96:1  in-progress dash ring
AMBER = "#A65D0A"       #  5.01:1  flush arrow and checkpoint label
MUTED_TX = "#626974"    #  5.54:1  "blocked" / "closed" tags, sublabels
MUTED_LN = "#7E8795"    #  3.63:1  outlines only, never text
MUTED_FILL = "#EFF2F6"
CHIP_BG = "#F4F6F9"

FONT = "DejaVu Sans, Helvetica, Arial, sans-serif"
MONO = "DejaVu Sans Mono, monospace"

R = 42
# Staging range. Emphasis is carried by scale and stroke weight because both
# are invisible to contrast; see the accessibility note above.
EMPH_LOW, EMPH_HI = 0.93, 1.05
GIF_LOOPS = 2  # then rest on the final frame (WCAG 2.2.2)

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
            t[f"{n}.emph"] = Track(1.0)
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
        t["dag.emph"] = Track(1.0)
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
        t["docs.emph"].to(T_DEP, 0.3, EMPH_LOW)            # staging: recede
        t["edge.draw"].to(T_DEP + 0.30, 0.55, 1.0, ease_in_out_cubic)
        t["edge.label"].to(T_DEP + 0.75, 0.25, 1.0, ease_out_cubic)
        # Follow-through: store recoils on impact, then settles blocked.
        t["store.squash"].to(T_DEP + 0.83, 0.13, 1.24, ease_out_cubic)
        t["store.squash"].to(T_DEP + 0.96, 0.42, 1.0, ease_out_elastic)
        t["store.blocked"].to(T_DEP + 0.88, 0.40, 1.0, ease_out_cubic)
        t["design.ready"].to(T_DEP + 0.95, 0.40, 1.0, ease_out_cubic)
        t["docs.ready"].to(T_DEP + 1.05, 0.40, 1.0, ease_out_cubic)
        t["docs.emph"].to(T_DEP + 1.0, 0.35, 1.0)

        # === 3. ready frontier ==========================================
        # Staging: the blocked bead recedes hardest -- it is the contrast that
        # defines the frontier. Ring pulses are emitted in render().
        t["store.emph"].to(T_READY, 0.35, EMPH_LOW)
        t["design.emph"].to(T_READY, 0.35, EMPH_HI)
        t["docs.emph"].to(T_READY + 0.15, 0.35, EMPH_HI)
        t["store.emph"].to(T_READY + 2.35, 0.3, 1.0)
        t["design.emph"].to(T_READY + 2.35, 0.3, 1.0)
        t["docs.emph"].to(T_READY + 2.4, 0.3, 1.0)

        # === 4. claim ===================================================
        t["worker.scale"].to(T_CLAIM, 0.45, 1.0, ease_out_back)
        t["docs.emph"].to(T_CLAIM + 0.1, 0.3, EMPH_LOW)
        t["design.emph"].to(T_CLAIM + 0.1, 0.3, EMPH_HI)
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
        t["docs.emph"].to(T_CLAIM + 1.6, 0.4, 1.0)
        t["design.emph"].to(T_CLAIM + 1.9, 0.4, 1.0)

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
        t["dag.emph"].to(T_FLUSH, 0.35, EMPH_LOW)          # staging: recede
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
    emph = S.v(f"{name}.emph", t) * S.v("dag.emph", t)
    # Emphasis is scale + stroke weight only. Neither affects contrast.
    scale *= emph
    sw = 2.6 + (emph - EMPH_LOW) * 12.3
    ready, blocked = S.v(f"{name}.ready", t), S.v(f"{name}.blocked", t)
    working, closed = S.v(f"{name}.working", t), S.v(f"{name}.closed", t)

    # Volume-preserving squash: widening always costs height.
    rx, ry = R * scale * sq, R * scale / sq

    fill, stroke, ink = MUTED_FILL, MUTED_LN, NAVY
    if ready > 0:
        fill, stroke, ink = (mix(fill, TEAL, ready), mix(stroke, TEAL, ready),
                             mix(ink, "#FFFFFF", ready))
    if blocked > 0:
        fill = mix(fill, MUTED_FILL, blocked)
        stroke, ink = mix(stroke, MUTED_LN, blocked), mix(ink, MUTED_TX, blocked)
    if working > 0:
        fill, stroke, ink = (mix(fill, TEAL, working), mix(stroke, NAVY, working),
                             mix(ink, "#FFFFFF", working))
    if closed > 0:
        fill = mix(fill, "#FFFFFF", closed)
        stroke, ink = mix(stroke, MUTED_LN, closed), mix(ink, MUTED_TX, closed)

    out = ["<g>"]

    # Ready-frontier pulses (beat 3). Exaggerated radius, staggered per bead so
    # the two do not read as one synchronized blink.
    if name in ("design", "docs"):
        for k, delay in enumerate((0.0, 0.75)):
            off = 0.0 if name == "design" else 0.3
            p = pulse(t, T_READY + 0.42 + delay + off, 0.95)
            if p > 0.01:
                out.append(
                    # Capped below the state tag at cy + R + 26. A teal ring
                    # sweeping across the teal "ready" text erases it.
                    f'<circle cx="{cx}" cy="{cy}" r="{R + 4 + 10 * (1 - (1 - p) ** 0.5):.1f}" '
                    f'fill="none" stroke="{TEAL}" stroke-width="3.5" opacity="{p * 0.5:.3f}"/>'
                )

    if working > 0.5:
        # Secondary action: work in progress is never still.
        ang = (t * 62) % 360
        out.append(
            f'<g transform="rotate({ang:.1f} {cx} {cy})">'
            f'<circle cx="{cx}" cy="{cy}" r="{rx + 9:.1f}" fill="none" stroke="{TEAL_DEEP}" '
            f'stroke-width="3" stroke-dasharray="9 8" stroke-linecap="round"/></g>'
        )

    out.append(
        f'<ellipse cx="{cx}" cy="{cy}" rx="{rx:.1f}" ry="{ry:.1f}" fill="{fill}" '
        f'stroke="{stroke}" stroke-width="3"/>'
    )
    if S.v(f"{name}.label", t) > 0.5:
        out.append(
            f'<text x="{cx}" y="{cy+7}" text-anchor="middle" font-family="{FONT}" '
            f'font-size="19" font-weight="bold" fill="{ink}">{name}</text>'
        )
    if closed > 0.35:
        # Keep the name legible: a bare tick loses which bead closed, and that
        # is the one thing this beat is about.
        bx, by = cx + rx * 0.72, cy - ry * 0.72
        g = ease_out_back(min(1.0, closed))
        out.append(
            f'<g transform="translate({bx} {by}) scale({g:.3f})">'
            f'<circle r="14" fill="#FFFFFF" stroke="{MUTED_TX}" stroke-width="2.5"/>'
            f'<path d="M-6,0 l4,5 l8,-9" fill="none" stroke="{MUTED_TX}" stroke-width="3" '
            f'stroke-linecap="round" stroke-linejoin="round"/></g>'
        )

    tag, tcol = None, stroke
    if closed > 0.5:
        tag, tcol = "closed", MUTED_TX
    elif working > 0.5:
        tag, tcol = "in progress", NAVY
    elif blocked > 0.5:
        tag, tcol = "blocked", MUTED_TX
    elif ready > 0.5:
        tag, tcol = "ready", TEAL
    if tag:
        out.append(
            f'<text x="{cx}" y="{cy+R+30}" text-anchor="middle" font-family="{FONT}" '
            f'font-size="17" fill="{tcol}">{tag}</text>'
        )
    out.append("</g>")
    return "".join(out)


def edge_svg(t):
    draw = S.v("edge.draw", t)
    if draw <= 0.001:
        return ""
    sat = S.v("edge.satisfied", t)
    esw = 2.6 + (S.v("dag.emph", t) - EMPH_LOW) * 12.3
    (x1, y1), (x2, _) = POS["design"], POS["store"]
    sx, ex = x1 + R + 8, x2 - R - 15
    color = mix(NAVY, MUTED_TX, sat)
    dash = ' stroke-dasharray="8 7"' if sat > 0.5 else ""
    cur = sx + (ex - sx) * draw
    out = ["<g>",
           f'<line x1="{sx}" y1="{y1}" x2="{cur:.1f}" y2="{y1}" stroke="{color}" '
           f'stroke-width="{esw:.2f}"{dash}/>']
    if draw > 0.985:
        head = ease_out_back(min(1.0, (draw - 0.985) / 0.015 if draw < 1 else 1.0))
        out.append(
            f'<g transform="translate({ex} {y1}) scale({head:.3f})">'
            f'<path d="M0,0 l-13,-8 l0,16 z" fill="{color}"/></g>'
        )
    if S.v("edge.label", t) > 0.5:
        txt = "satisfied" if sat > 0.5 else "blocks"
        out.append(
            f'<text x="{(sx+ex)/2}" y="{y1-17}" text-anchor="middle" font-family="{FONT}" '
            f'font-size="17" fill="{color}">{txt}</text>'
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
    (x1, y1) = POS["design"]
    cx, cy = WORKER
    sx, sy = x1 + R + 8, y1 + 16
    ex, ey = cx - 52, cy - 14
    ctrl = (505, 300)  # arc, routed clear of `store`
    out = ["<g>",
           f'<path d="M{sx},{sy} Q{ctrl[0]},{ctrl[1]} {ex},{ey}" fill="none" stroke="{TEAL}" '
           f'stroke-width="4" stroke-linecap="round" pathLength="1" stroke-dasharray="1" '
           f'stroke-dashoffset="{1-d:.3f}"/>']
    if d > 0.96:
        out.append(f'<path d="M{ex},{ey} l-15,-6 l4,17 z" fill="{TEAL}"/>')
    if S.v("claim.label", t) > 0.5:
        out.append(
            f'<text x="{ctrl[0]+44}" y="{ctrl[1]-8}" text-anchor="middle" font-family="{FONT}" '
            f'font-size="18" font-weight="bold" fill="{TEAL}">claim</text>'
        )
    out.append("</g>")
    return "".join(out)


def worker_svg(t):
    s = S.v("worker.scale", t)
    if s <= 0.001:
        return ""
    cx, cy = WORKER
    cy += -7 * S.v("worker.bob", t)   # secondary action on impact
    accent = mix(MUTED_LN, TEAL, S.v("worker.active", t))
    s *= S.v("dag.emph", t)
    return f"""<g transform="translate({cx} {cy}) scale({s:.3f}) translate({-cx} {-cy})">
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
    doc_color = mix(MUTED_TX, AMBER, min(1.0, arrow * 1.6))
    out = [f'<g transform="translate({dx} {dy}) scale({on:.3f}) '
           f'translate({-dx} {-dy})">']
    out.append(f"""
      <ellipse cx="{dx}" cy="{dy-24}" rx="38" ry="12" fill="#FFFFFF" stroke="{NAVY}" stroke-width="3"/>
      <path d="M{dx-38},{dy-24} L{dx-38},{dy+24} A38,12 0 0 0 {dx+38},{dy+24} L{dx+38},{dy-24}"
            fill="#FFFFFF" stroke="{NAVY}" stroke-width="3"/>
      <ellipse cx="{dx}" cy="{dy}" rx="38" ry="12" fill="none" stroke="{NAVY}" stroke-width="3"/>
      <text x="{dx}" y="{dy+56}" text-anchor="middle" font-family="{FONT}" font-size="18"
            font-weight="bold" fill="{NAVY}">SQLite</text>
      <text x="{dx}" y="{dy+77}" text-anchor="middle" font-family="{FONT}" font-size="15"
            fill="{MUTED_TX}">live, gitignored</text>""")
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
            fill="{MUTED_TX}">Git tracks this</text></g>""")
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
    # Exactly one beat's text is on screen, at full contrast. Cross-fading the
    # caption would park text below 4.5:1 for the duration of the fade, so the
    # beats cut instead.
    _, cmd, cap = BEATS[idx]
    out.append(f'<text x="{W/2}" y="432" text-anchor="middle" font-family="{MONO}" '
               f'font-size="19" fill="{NAVY}">{cmd}</text>')
    out.append(f'<text x="{W/2}" y="478" text-anchor="middle" font-family="{FONT}" '
               f'font-size="20" fill="{CAPTION}">{cap}</text>')
    return "".join(out)


def render(t):
    body = [f'<rect width="{W}" height="{H}" fill="{BG}"/>', chrome_svg(t),
            edge_svg(t), claim_svg(t), worker_svg(t), storage_svg(t)]
    body += [bead_svg(n, t) for n in ("design", "store", "docs")]
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" '
            f'viewBox="0 0 {W} {H}">' + "".join(body) + "</svg>")



# --------------------------------------------------------------------------
# accessibility
# --------------------------------------------------------------------------
def _luminance(hexc):
    c = [int(hexc[i:i + 2], 16) / 255 for i in (1, 3, 5)]
    c = [x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c]
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def contrast(a, b):
    la, lb = _luminance(a), _luminance(b)
    return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)


def verify_contrast():
    """Assert WCAG 2.1 AA for every colour pair the animation actually holds.

    Run at generation time so the palette cannot regress silently. 4.5:1 for
    text, 3:1 for large bold text and for graphics that carry meaning. The
    two decorative overlays -- the ready pulse ring and the ripple glow -- are
    exempt: both are redundant emphasis over information already given as text.
    """
    checks = [
        ("caption 20px", CAPTION, BG, 4.5),
        ("command chip 19px", NAVY, CHIP_BG, 4.5),
        ("bead label on light fill", NAVY, MUTED_FILL, 3.0),
        ("bead label on teal fill", "#FFFFFF", TEAL, 4.5),
        ("tag: ready", TEAL, BG, 4.5),
        ("tag: in progress", NAVY, BG, 4.5),
        ("tag: blocked / closed", MUTED_TX, BG, 4.5),
        ("edge label: blocks", NAVY, BG, 4.5),
        ("edge label: satisfied", MUTED_TX, BG, 4.5),
        ("label: claim", TEAL, BG, 4.5),
        ("label: flush", AMBER, BG, 4.5),
        ("label: checkpoint", AMBER, BG, 4.5),
        ("label: SQLite", NAVY, BG, 4.5),
        ("sublabels 15px", MUTED_TX, BG, 4.5),
        ("worker caption", NAVY, BG, 4.5),
        ("bead outline", MUTED_LN, BG, 3.0),
        ("teal bead fill", TEAL, BG, 3.0),
        ("edge stroke", NAVY, BG, 3.0),
        ("in-progress ring", TEAL_DEEP, BG, 3.0),
        ("flush arrow", AMBER, BG, 3.0),
        ("closed badge outline", MUTED_LN, BG, 3.0),
    ]
    bad = [(n, f, b, need, contrast(f, b)) for n, f, b, need in checks
           if contrast(f, b) < need]
    if bad:
        lines = "\n".join(f"    {n}: {f} on {b} = {r:.2f}:1, need {need}:1"
                           for n, f, b, need, r in bad)
        raise SystemExit(f"WCAG AA contrast failures:\n{lines}")
    worst = min(checks, key=lambda c: contrast(c[1], c[2]) / c[3])
    print(f"  contrast: {len(checks)} pairs pass WCAG AA "
          f"(tightest margin: {worst[0]} at {contrast(worst[1], worst[2]):.2f}:1)")


def verify_flash_rate(frames, durations):
    """WCAG 2.3.1: no more than three luminance flashes per second."""
    lums = []
    for f in frames:
        small = f.convert("L").resize((32, 16))
        px = small.load()
        lums.append(sum(px[x, y] for y in range(16) for x in range(32)) / 512)
    flashes, t, last_dir, window = 0, 0.0, 0, []
    for i in range(1, len(lums)):
        delta = lums[i] - lums[i - 1]
        d = 1 if delta > 6 else (-1 if delta < -6 else 0)
        if d and d != last_dir:
            window.append(t)
            last_dir = d
        t += durations[i] / 1000
        window = [x for x in window if x > t - 1.0]
        flashes = max(flashes, len(window))
    if flashes > 3:
        raise SystemExit(f"WCAG 2.3.1: {flashes} luminance flashes in one second")
    print(f"  flash rate: peak {flashes}/s (limit 3/s)")


def write_poster(path):
    """Static storyboard served to readers who ask for reduced motion.

    One settled frame per beat, so it carries the same information the
    animation does without any motion at all.
    """
    stills = [2.60, 5.35, 7.60, 11.35, 14.40, 18.10]
    tile_w = 760
    tile_h = round(tile_w * H / W)
    sheet = Image.new("RGB", (tile_w * 2, tile_h * 3), BG)
    for i, ts in enumerate(stills):
        png = cairosvg.svg2png(bytestring=render(ts).encode("utf-8"),
                               output_width=tile_w, output_height=tile_h)
        sheet.paste(Image.open(io.BytesIO(png)).convert("RGB"),
                    ((i % 2) * tile_w, (i // 2) * tile_h))
    sheet.quantize(colors=64, method=Image.MEDIANCUT,
                   dither=Image.Dither.NONE).save(path, optimize=True)
    print(f"  poster: {os.path.basename(path)} "
          f"{sheet.width}x{sheet.height}, {os.path.getsize(path)/1024:.0f} KB")


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    out_path = os.path.join(here, "bead-lifecycle.gif")
    verify_contrast()
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

    # A GIF cannot offer a pause control, so it plays a fixed number of times
    # and rests on its final state rather than moving forever (WCAG 2.2.2).
    paletted[0].save(out_path, save_all=True, append_images=paletted[1:],
                     duration=durations, loop=GIF_LOOPS, optimize=True, disposal=1)
    verify_flash_rate(kept, durations)
    write_poster(os.path.join(here, "bead-lifecycle-static.png"))

    print(f"wrote {out_path}")
    print(f"  {len(paletted)} frames (from {n} rendered), "
          f"{sum(durations)/1000:.1f}s loop, {os.path.getsize(out_path)/1024:.0f} KB")


if __name__ == "__main__":
    main()
