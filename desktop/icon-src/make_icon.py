#!/usr/bin/env python3
"""Writes the Stellarator app icon sources.

    python3 make_icon.py          write the files below
    python3 make_icon.py --logo   print MARK for frontend/src/components/logo.tsx

Writes, next to this script:
  icon-macos.svg   macOS before 26: Apple's 824 px rounded square on the 1024 px canvas, with Apple's
                   drop shadow
  icon.svg         Windows, Linux and the web favicon: the same rounded square, nearly full-bleed, no shadow
  icon-square.svg  iOS home screen and web "maskable" icons: full-bleed square (the platform masks it)
  Stellarator.icon macOS 26 Icon Composer document: the gradient as the icon's fill and the mark as one
                   white glass layer; the system draws the Liquid Glass (actool compiles it in CI)

The mark is the one in frontend/src/components/logo.tsx: one thick ring with five soft lobes, the
shape of a stellarator's five-period plasma ring seen from above. The icon is the white mark over a
two-colour linear gradient, GRADIENT from top to bottom. In the static icons the mark is white
with a little transparency, a bright inner edge at the top and a soft shadow, a still picture of
the glass that macOS 26 draws. In Stellarator.icon the dark appearance uses GRADIENT_DARK; the
tinted and clear appearances are left to the system.

docs/desktop.md ("The icon") lists the commands that turn these files into each platform's icons.
"""
import json
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))

# ---------------------------------------------------------------------------------------------
# The mark, in a 24-unit box centred on (12, 12): a band WIDTH wide around the centre line
# r = R + A cos(5t), where t is the angle clockwise from the top (a lobe points up). Both edges are
# the centre line moved WIDTH / 2 along its normal, so the ring has the same thickness all round;
# the outer edge has five soft lobes and the inner edge is a pentagon with rounded corners.
# The inner corners stay round while WIDTH / 2 is less than the centre line's tightest bend
# (3.8 units, at the lobes).
R, A = 8.3, 0.45  # centre-line radius, lobe amplitude
WIDTH = 4.8  # ring thickness: 3.2 px at 16 px
EXTENT = R + A + WIDTH / 2  # 11.15: the mark's outer radius

# Top to bottom: periwinkle to deep indigo; in the dark appearance, a darker pair of the same hues.
GRADIENT = ("#5566dc", "#2c3684")
GRADIENT_DARK = ("#2c3684", "#141a40")


def _edge(t, side):
    """A point on the inner (side -1) or outer (side +1) edge, in mark units."""
    r = R + A * math.cos(5 * t)
    dr = -5 * A * math.sin(5 * t)
    x, y = 12 + r * math.sin(t), 12 - r * math.cos(t)
    dx, dy = dr * math.sin(t) + r * math.cos(t), -dr * math.cos(t) + r * math.sin(t)
    n = math.hypot(dx, dy)
    return (x + side * WIDTH / 2 * dy / n, y - side * WIDTH / 2 * dx / n)


def mark_path(scale=1.0, cx=12.0, cy=12.0, segments=40, digits=2):
    """The mark as SVG path data (fill it with the even-odd rule): the outer and inner edges as
    closed curves of cubic Beziers, the 24-unit box scaled by `scale` and centred on (cx, cy)."""
    h = 1e-5
    f = f"{{:.{digits}f}}"
    d = []
    for side in (-1, +1):
        def at(t):
            x, y = _edge(t, side)
            return (cx + scale * (x - 12), cy + scale * (y - 12))

        def tangent(t):
            a, b = at(t - h), at(t + h)
            return ((b[0] - a[0]) / (2 * h), (b[1] - a[1]) / (2 * h))

        start = at(0)
        d.append("M" + f.format(start[0]) + " " + f.format(start[1]))
        for i in range(segments):
            a, b = 2 * math.pi * i / segments, 2 * math.pi * (i + 1) / segments
            k = (b - a) / 3
            pa, pb, ta, tb = at(a), at(b), tangent(a), tangent(b)
            d.append("C" + " ".join(f.format(v) for v in (pa[0] + k * ta[0], pa[1] + k * ta[1],
                                                           pb[0] - k * tb[0], pb[1] - k * tb[1], *pb)))
        d.append("Z")
    return "".join(d)


# ---------------------------------------------------------------------------------------------
# The static icons (SVG, rendered by resvg through `tauri icon`).
MARK_SIZE = 0.62  # the mark's width as a fraction of the tile


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
    mark = mark_path(scale, 512, 512, segments=48)
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
  <clipPath id="markClip"><path d="{mark}" clip-rule="evenodd"/></clipPath>
  <linearGradient id="bg" x1="0" y1="{512 - half}" x2="0" y2="{512 + half}" gradientUnits="userSpaceOnUse">
    <stop offset="0" stop-color="{GRADIENT[0]}"/>
    <stop offset="1" stop-color="{GRADIENT[1]}"/>
  </linearGradient>
  <!-- The glass body: nearly opaque white at the top, letting more of the gradient through below. -->
  <linearGradient id="body" x1="0" y1="{top:.1f}" x2="0" y2="{bottom:.1f}" gradientUnits="userSpaceOnUse">
    <stop offset="0" stop-color="#ffffff" stop-opacity="0.94"/>
    <stop offset="1" stop-color="#ffffff" stop-opacity="0.72"/>
  </linearGradient>
  <!-- The bright inner edge: strongest at the top, where the light comes from. -->
  <linearGradient id="edge" x1="0" y1="{top:.1f}" x2="0" y2="{bottom:.1f}" gradientUnits="userSpaceOnUse">
    <stop offset="0" stop-color="#ffffff" stop-opacity="1"/>
    <stop offset="0.5" stop-color="#ffffff" stop-opacity="0.35"/>
    <stop offset="1" stop-color="#ffffff" stop-opacity="0.7"/>
  </linearGradient>
  <!-- A soft shadow around the mark, cut away under it so the body stays clear. -->
  <filter id="markShadow" x="-20%" y="-20%" width="140%" height="140%">
    <feGaussianBlur in="SourceAlpha" stdDeviation="{16 * k:.1f}"/>
    <feOffset dy="{14 * k:.1f}" result="b"/>
    <feFlood flood-color="{GRADIENT_DARK[1]}" flood-opacity="0.45"/>
    <feComposite in2="b" operator="in"/>
    <feComposite in2="SourceAlpha" operator="out"/>
  </filter>
  <linearGradient id="rim" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#fff" stop-opacity="0.35"/>
    <stop offset="0.3" stop-color="#fff" stop-opacity="0.06"/>
    <stop offset="1" stop-color="#fff" stop-opacity="0.12"/>
  </linearGradient>
</defs>
<g{' filter="url(#tileShadow)"' if shadow else ''}><path d="{tile}" fill="url(#bg)"/></g>
<g clip-path="url(#tile)">
  <g filter="url(#markShadow)"><path d="{mark}" fill="#000" fill-rule="evenodd"/></g>
  <path d="{mark}" fill="url(#body)" fill-rule="evenodd"/>
  <path d="{mark}" fill="none" stroke="url(#edge)" stroke-width="{8 * k:.1f}" clip-path="url(#markClip)"/>
</g>
{rim}
</svg>
"""


# ---------------------------------------------------------------------------------------------
# The macOS 26 Icon Composer document.
def icon_composer():
    """Stellarator.icon: Icon Composer's document format (a folder with icon.json and Assets/).

    The mark layer is drawn on the full-bleed 1024 point canvas as a filled outline, not a stroke:
    actool crops each layer to its paths' geometry, which leaves out a stroke's width.
    """
    out = os.path.join(HERE, "Stellarator.icon")
    assets = os.path.join(out, "Assets")
    os.makedirs(assets, exist_ok=True)
    for name in os.listdir(assets):  # drop layers of earlier designs
        if name != "mark.svg":
            os.remove(os.path.join(assets, name))
    scale = MARK_SIZE * 512 / EXTENT
    with open(os.path.join(assets, "mark.svg"), "w") as f:
        f.write('<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">'
                f'<path d="{mark_path(scale, 512, 512, segments=48)}" fill="#fff" fill-rule="evenodd"/></svg>\n')

    def srgb(hex_):
        r, g, b = (int(hex_[i : i + 2], 16) / 255 for i in (1, 3, 5))
        return f"srgb:{r:.5f},{g:.5f},{b:.5f},1.00000"

    def gradient(colors):
        return {"linear-gradient": [srgb(c) for c in colors],
                "orientation": {"start": {"x": 0.5, "y": 0}, "stop": {"x": 0.5, "y": 1}}}

    # Every group carries shadow {kind, opacity} and translucency {enabled, value}, and every layer a
    # position, as Icon Composer writes them; actool rejects a document with these left out.
    doc = {
        "fill-specializations": [
            {"value": gradient(GRADIENT)},
            {"appearance": "dark", "value": gradient(GRADIENT_DARK)},
        ],
        "groups": [
            {
                "layers": [{"glass": True, "image-name": "mark.svg", "name": "mark",
                            "position": {"scale": 1, "translation-in-points": [0, 0]}}],
                "shadow": {"kind": "neutral", "opacity": 0.5},
                "translucency": {"enabled": True, "value": 0.4},
            },
        ],
        "supported-platforms": {"squares": "shared"},
    }
    with open(os.path.join(out, "icon.json"), "w") as f:
        json.dump(doc, f, indent=2)
        f.write("\n")


if __name__ == "__main__":
    if sys.argv[1:] == ["--logo"]:
        print(mark_path(segments=25, digits=2))
        sys.exit()
    with open(os.path.join(HERE, "icon-macos.svg"), "w") as f:
        f.write(svg(412, shadow=True))
    with open(os.path.join(HERE, "icon.svg"), "w") as f:
        f.write(svg(488, shadow=False))
    with open(os.path.join(HERE, "icon-square.svg"), "w") as f:
        f.write(svg(512, shadow=False, square=True))
    icon_composer()
    print("wrote icon-macos.svg, icon.svg, icon-square.svg and Stellarator.icon")
