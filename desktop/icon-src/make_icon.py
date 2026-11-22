#!/usr/bin/env python3
"""Writes the Stellarator app icon sources.

    python3 make_icon.py          write the files below
    python3 make_icon.py --logo   print the path of one strand piece, for frontend/src/components/logo.tsx

Writes, next to this script:
  icon-macos.svg   macOS before 26: Apple's 824 px rounded square on the 1024 px canvas, with Apple's
                   drop shadow
  icon.svg         Windows, Linux and the web favicon: the same rounded square, nearly full-bleed, no shadow
  icon-square.svg  iOS home screen and web "maskable" icons: full-bleed square (the platform masks it)
  Stellarator.icon macOS 26 Icon Composer document: the pixel field as a flat bottom layer and the mark
                   as a white glass layer; the system draws the Liquid Glass (actool compiles it in CI)

The mark is the one in frontend/src/components/logo.tsx: one bold strand that winds twice around a
ring and crosses itself five times, passing over and under in turn (a cinquefoil knot), from the
earlier mark's two coils braided around a ring. Its outline is a ring with five lobes. On the icon
it is white, over the pixel field: a 14 x 14 grid of square cells stepping from ink and navy at
the top left through periwinkle to coral at the bottom right, with an ordered (Bayer) dither where
two colours meet.

Icon Composer appearances: the default shows FIELD; dark shows FIELD_DARK, a darker field; tinted
and clear hide both fields, so the system's own background shows behind the glass mark.

docs/desktop.md ("The icon") lists the commands that turn these files into each platform's icons.
"""
import json
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))

# ---------------------------------------------------------------------------------------------
# The mark, in a 24-unit box centred on (12, 12).
#
# The strand's centre line is r = R + A cos(5t/2) + B cos(5t) for t from 0 to 4 pi, where t is the
# angle clockwise from the top. Its first turn (t from 0 to 2 pi) and its second lie on either side
# of the ring, r = R + A cos(5t/2) and r = R - A cos(5t/2) when B is 0, like the two coils of the
# earlier mark, and cross where cos(5t/2) = 0: five times around the ring. The B term eases the
# inner bends so the strand's edges stay smooth curves there. At each crossing one turn passes over
# and the other is cut, GAP units clear of the over strand on each side, so the mark is five
# identical pieces, each turned 72 degrees from the last.
R, A, B = 7.1, 2.6, 0.35  # ring radius, coil amplitude, inner-bend easing
WIDTH = 2.6  # strand width
GAP = 0.6  # space between a cut strand and the strand passing over it
EXTENT = R + A + B + WIDTH / 2  # 11.35: the mark's outer radius


def _centre(t):
    r = R + A * math.cos(2.5 * t) + B * math.cos(5 * t)
    return (12 + r * math.sin(t), 12 - r * math.cos(t))


def _edge(t, side):
    """A point on one edge of the strand: the centre line moved WIDTH / 2 along its normal."""
    r = R + A * math.cos(2.5 * t) + B * math.cos(5 * t)
    dr = -2.5 * A * math.sin(2.5 * t) - 5 * B * math.sin(5 * t)
    dx, dy = dr * math.sin(t) + r * math.cos(t), -dr * math.cos(t) + r * math.sin(t)
    n = math.hypot(dx, dy)
    x, y = _centre(t)
    return (x + side * WIDTH / 2 * dy / n, y - side * WIDTH / 2 * dx / n)


def _distance_to_strand(p, t_near):
    """Distance from p to the centre line near parameter t_near (within 0.7 radians of it)."""
    def d(u):
        x, y = _centre(u)
        return math.hypot(p[0] - x, p[1] - y)

    lo, hi = t_near - 0.7, t_near + 0.7
    best = min((lo + (hi - lo) * i / 140 for i in range(141)), key=d)
    lo, hi = best - 0.01, best + 0.01
    for _ in range(60):  # golden-section refinement
        m1, m2 = hi - (hi - lo) * 0.618, lo + (hi - lo) * 0.618
        if d(m1) < d(m2):
            hi = m2
        else:
            lo = m1
    return d((lo + hi) / 2)


def _cut(t_cross, side, direction):
    """Where an edge of the cut strand leaves the over strand's clearance at the crossing t_cross."""
    over = t_cross + 2 * math.pi  # the other turn through the same point
    inside, outside = t_cross, t_cross + direction * 0.8
    for _ in range(60):
        mid = (inside + outside) / 2
        if _distance_to_strand(_edge(mid, side), over) < WIDTH / 2 + GAP:
            inside = mid
        else:
            outside = mid
    return outside


def _piece_edges():
    """Parameter ranges of the two edges of the first piece: from one cut crossing, over the next
    crossing, to the cut crossing after it."""
    start = 3 * math.pi / 5  # crossings are at t = pi/5 + k * 2pi/5; the strand is cut at odd k
    end = start + 4 * math.pi / 5
    return [(+1, _cut(start, +1, +1), _cut(end, +1, -1)), (-1, _cut(end, -1, -1), _cut(start, -1, +1))]


def _curve(side, t0, t1, segments, xf):
    """An edge from t0 to t1 as cubic Beziers (Hermite tangents), through the transform xf."""
    h = 1e-5

    def at(t):
        return xf(_edge(t, side))

    def tangent(t):
        a, b = at(t - h), at(t + h)
        return ((b[0] - a[0]) / (2 * h), (b[1] - a[1]) / (2 * h))

    out = []
    for i in range(segments):
        a, b = t0 + (t1 - t0) * i / segments, t0 + (t1 - t0) * (i + 1) / segments
        k = (b - a) / 3
        pa, pb, ta, tb = at(a), at(b), tangent(a), tangent(b)
        out.append(((pa[0] + k * ta[0], pa[1] + k * ta[1]), (pb[0] - k * tb[0], pb[1] - k * tb[1]), pb))
    return at(t0), out


def mark_path(scale=1.0, cx=12.0, cy=12.0, segments=24, digits=2, pieces=5):
    """The mark's outline as SVG path data: the 24-unit box scaled by `scale` and centred on (cx, cy).

    pieces=1 gives only the first piece; the other four are it turned by 72, 144, 216 and 288 degrees.
    Each piece is one closed outline: one edge, a straight cut, the other edge back, a straight cut.
    """
    edges = _piece_edges()
    f = f"{{:.{digits}f}}"
    d = []
    for k in range(pieces):
        c, s = math.cos(2 * math.pi * k / 5), math.sin(2 * math.pi * k / 5)

        def xf(p):
            x, y = p[0] - 12, p[1] - 12
            return (cx + scale * (c * x - s * y), cy + scale * (s * x + c * y))

        for i, (side, t0, t1) in enumerate(edges):
            first, beziers = _curve(side, t0, t1, segments, xf)
            d.append(("M" if i == 0 else "L") + f.format(first[0]) + " " + f.format(first[1]))
            for c1, c2, p in beziers:
                d.append("C" + " ".join(f.format(v) for v in (*c1, *c2, *p)))
        d.append("Z")
    return "".join(d)


# ---------------------------------------------------------------------------------------------
# The pixel field: GRID x GRID square cells over the tile. Each cell takes the heat at its centre
# (0 cold, 1 hot), scaled to the palette steps. Where the heat falls between two steps, the cell
# takes one or the other by a 4 x 4 Bayer matrix, so the bands meet in a checkerboard. Only the
# middle part of each step is dithered (DITHER), so most cells are solid.
GRID = 14
DITHER = 1.8  # the fraction between two steps is stretched by this about 0.5: the middle 56 % dithers
# ink, navy, periwinkle, light periwinkle, coral
FIELD = ["#0b1515", "#232c78", "#5566dc", "#a1b5f8", "#fc885e"]
# Dark appearance: near-black ground, ink, deep navy, periwinkle, coral.
FIELD_DARK = ["#0b0b0b", "#0b1515", "#1c2463", "#5566dc", "#fc885e"]
BAYER = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]
# Icon Composer's fill under the field, top to bottom: navy to periwinkle; in dark, ink to deep navy.
FILL = ["#232c78", "#5566dc"]
FILL_DARK = ["#0b1515", "#1c2463"]


def heat(u, v):
    """Heat at (u, v) on the tile, 0 to 1 from the top left: a ramp from the top left to the bottom
    right, a hot spot near the bottom right corner, and three slow waves that bend the bands."""
    ramp = 0.30 * u + 0.62 * v
    hot = math.exp(-((u - 0.86) ** 2 + (v - 0.90) ** 2) / 0.14)
    waves = (0.075 * math.sin(4.6 * u + 3.1 * v + 0.8)
             + 0.05 * math.sin(7.3 * v - 2.2 * u + 2.0)
             + 0.04 * math.sin(6.0 * math.hypot(u - 0.3, v - 0.75)))
    return 0.58 * ramp + 0.50 * hot + waves + 0.03


def field_steps():
    """The palette step (0 to len(FIELD) - 1) of each cell, as rows top to bottom."""
    top = len(FIELD) - 1
    rows = []
    for j in range(GRID):
        row = []
        for i in range(GRID):
            s = max(0.0, min(1.0, heat((i + 0.5) / GRID, (j + 0.5) / GRID))) * top
            step = math.floor(s)
            f = max(0.0, min(1.0, (s - step - 0.5) * DITHER + 0.5))
            if f > (BAYER[j % 4][i % 4] + 0.5) / 16:
                step += 1
            row.append(min(step, top))
        rows.append(row)
    return rows


def field(palette, x0, y0, size):
    """The field over the square at (x0, y0), as SVG: the first colour as a square, then for each
    later colour one path covering every cell at that step or above.

    Each path lies inside the one before it, so a join between two cells is the edge of one path
    only. Drawing each cell or colour separately would leave two anti-aliased edges on the same
    pixels, and the colour beneath would show through as thin lines when the icon is drawn small.
    Cell edges are whole pixels.
    """
    edge = [round(size * k / GRID) for k in range(GRID + 1)]
    steps = field_steps()
    out = [f'<rect x="{x0}" y="{y0}" width="{size}" height="{size}" fill="{palette[0]}"/>']
    for level in range(1, len(palette)):
        d = []
        for j, row in enumerate(steps):
            i = 0
            while i < GRID:
                if row[i] < level:
                    i += 1
                    continue
                n = i
                while n + 1 < GRID and row[n + 1] >= level:
                    n += 1
                d.append(f"M{x0 + edge[i]} {y0 + edge[j]}H{x0 + edge[n + 1]}V{y0 + edge[j + 1]}H{x0 + edge[i]}Z")
                i = n + 1
        if d:
            out.append(f'<path d="{"".join(d)}" fill="{palette[level]}"/>')
    return "".join(out)


# ---------------------------------------------------------------------------------------------
# The static icons (SVG, rendered by resvg through `tauri icon`).
MARK_SIZE = 0.60  # the mark's width as a fraction of the tile


def squircle(cx, cy, half, steps=90):
    """Apple's icon shape: a rounded square with continuous corners.

    The corner radius is 22.45 % of the side (185 px on the 824 px macOS tile). Each corner curve
    starts 1.528 radii from the corner, like Apple's continuous corners, and is a superellipse
    quadrant (exponent 3.3) whose diagonal meets the circular corner's, so the sides stay straight.
    """
    r = 0.2245 * 2 * half
    c = 1.528 * r
    n = 3.3
    pts = []
    # Corner centres, clockwise from the top right, with the quadrant each one draws.
    corners = [(cx + half - c, cy - half + c, 270, 360), (cx + half - c, cy + half - c, 0, 90),
               (cx - half + c, cy + half - c, 90, 180), (cx - half + c, cy - half + c, 180, 270)]
    for ox, oy, a0, a1 in corners:
        for i in range(steps + 1):
            t = math.radians(a0 + (a1 - a0) * i / steps)
            co, si = math.cos(t), math.sin(t)
            pts.append(f"{ox + c * math.copysign(abs(co) ** (2 / n), co):.2f},{oy + c * math.copysign(abs(si) ** (2 / n), si):.2f}")
    return "M" + " L".join(pts) + " Z"


def svg(half, shadow, square=False):
    # square: full-bleed, for platforms that mask the icon themselves (iOS home screen, web
    # "maskable" icons); the mark stays inside the central 80 % safe zone.
    tile = "M0,0 H1024 V1024 H0 Z" if square else squircle(512, 512, half)
    k = half / 412  # 1.0 on the macOS grid
    scale = MARK_SIZE * half / EXTENT
    mark = mark_path(scale, 512, 512, segments=24)
    top, bottom = 512 - EXTENT * scale, 512 + EXTENT * scale
    tile_shadow = (
        '<filter id="tileShadow" x="-20%" y="-20%" width="140%" height="140%">'
        '<feDropShadow dx="0" dy="10" stdDeviation="12" flood-color="#000" flood-opacity="0.30"/></filter>'
        if shadow
        else ""
    )
    rim = "" if square else (
        f'<path d="{tile}" fill="none" stroke="url(#rim)" stroke-width="{4 * k:.1f}" clip-path="url(#tile)"/>'
    )
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
<!-- Generated by make_icon.py. Edit the script, not this file. -->
<defs>
  {tile_shadow}
  <clipPath id="tile"><path d="{tile}"/></clipPath>
  <!-- The mark: white at the top to periwinkle tint at the bottom. -->
  <linearGradient id="markFill" x1="0" y1="{top:.1f}" x2="0" y2="{bottom:.1f}" gradientUnits="userSpaceOnUse">
    <stop offset="0" stop-color="#ffffff"/>
    <stop offset="1" stop-color="#e3e8fd"/>
  </linearGradient>
  <filter id="markShadow" x="-20%" y="-20%" width="140%" height="140%">
    <feGaussianBlur in="SourceAlpha" stdDeviation="{14 * k:.1f}"/>
    <feOffset dy="{12 * k:.1f}" result="b"/>
    <feFlood flood-color="#0b1515" flood-opacity="0.45"/>
    <feComposite in2="b" operator="in"/>
    <feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge>
  </filter>
  <linearGradient id="rim" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#fff" stop-opacity="0.35"/>
    <stop offset="0.3" stop-color="#fff" stop-opacity="0.06"/>
    <stop offset="1" stop-color="#fff" stop-opacity="0.12"/>
  </linearGradient>
</defs>
<g{' filter="url(#tileShadow)"' if shadow else ''}><path d="{tile}" fill="{FIELD[0]}"/></g>
<g clip-path="url(#tile)">{field(FIELD, 512 - half, 512 - half, 2 * half)}</g>
<g filter="url(#markShadow)"><path d="{mark}" fill="url(#markFill)"/></g>
{rim}
</svg>
"""


# ---------------------------------------------------------------------------------------------
# The macOS 26 Icon Composer document.
def icon_composer():
    """Stellarator.icon: Icon Composer's document format (a folder with icon.json and Assets/).

    Layers are drawn on the full-bleed 1024 point canvas. The mark is a filled outline, not a
    stroke: actool crops each layer to its paths' geometry, which leaves out a stroke's width.
    """
    out = os.path.join(HERE, "Stellarator.icon")
    assets = os.path.join(out, "Assets")
    os.makedirs(assets, exist_ok=True)
    for name in os.listdir(assets):  # drop layers of earlier designs
        if name not in ("mark.svg", "field.svg", "field-dark.svg"):
            os.remove(os.path.join(assets, name))

    def write(name, body):
        with open(os.path.join(assets, name), "w") as f:
            f.write(f'<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">{body}</svg>\n')

    scale = MARK_SIZE * 512 / EXTENT
    write("mark.svg", f'<path d="{mark_path(scale, 512, 512, segments=24)}" fill="#fff"/>')
    write("field.svg", field(FIELD, 0, 0, 1024))
    write("field-dark.svg", field(FIELD_DARK, 0, 0, 1024))

    def srgb(hex_):
        r, g, b = (int(hex_[i : i + 2], 16) / 255 for i in (1, 3, 5))
        return f"srgb:{r:.5f},{g:.5f},{b:.5f},1.00000"

    def gradient(colors):
        return {"linear-gradient": [srgb(c) for c in colors],
                "orientation": {"start": {"x": 0.5, "y": 0}, "stop": {"x": 0.5, "y": 1}}}

    def hidden(default, dark, tinted):
        return [{"value": default}, {"appearance": "dark", "value": dark}, {"appearance": "tinted", "value": tinted}]

    # Every group carries shadow {kind, opacity} and translucency {enabled, value}, and every layer a
    # position, as Icon Composer writes them; actool rejects a document with these left out.
    position = {"scale": 1, "translation-in-points": [0, 0]}
    doc = {
        "fill-specializations": [
            {"value": gradient(FILL)},
            {"appearance": "dark", "value": gradient(FILL_DARK)},
        ],
        "groups": [
            {
                "layers": [{"glass": True, "image-name": "mark.svg", "name": "mark", "position": position}],
                "shadow": {"kind": "neutral", "opacity": 0.5},
                "translucency": {"enabled": True, "value": 0.3},
            },
            {
                # The field: flat (no glass, no specular highlight, no shadow). The default shows
                # the field, dark the darker field, tinted and clear neither.
                "layers": [
                    {"glass": False, "hidden-specializations": hidden(True, False, True),
                     "image-name": "field-dark.svg", "name": "field-dark", "position": position},
                    {"glass": False, "hidden-specializations": hidden(False, True, True),
                     "image-name": "field.svg", "name": "field", "position": position},
                ],
                "shadow": {"kind": "none", "opacity": 0},
                "specular": False,
                "translucency": {"enabled": False, "value": 0},
            },
        ],
        "supported-platforms": {"squares": "shared"},
    }
    with open(os.path.join(out, "icon.json"), "w") as f:
        json.dump(doc, f, indent=2)
        f.write("\n")


if __name__ == "__main__":
    if sys.argv[1:] == ["--logo"]:
        print(mark_path(segments=12, digits=2, pieces=1))
        sys.exit()
    with open(os.path.join(HERE, "icon-macos.svg"), "w") as f:
        f.write(svg(412, shadow=True))
    with open(os.path.join(HERE, "icon.svg"), "w") as f:
        f.write(svg(488, shadow=False))
    with open(os.path.join(HERE, "icon-square.svg"), "w") as f:
        f.write(svg(512, shadow=False, square=True))
    icon_composer()
    print("wrote icon-macos.svg, icon.svg, icon-square.svg and Stellarator.icon")
