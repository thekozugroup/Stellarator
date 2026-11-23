#!/usr/bin/env python3
"""Writes the Stellarator app icons from the logo mark, app/assets/mark.svg.

    python3 app/icon-src/make_icon.py            the SVG sources and every PNG below
    python3 app/icon-src/make_icon.py --no-png   the SVG sources only

The PNGs are rendered with resvg (`cargo install resvg`), the renderer `tauri icon` used before.

Writes, next to this script:
  icon-macos.svg    macOS before 26: Apple's 824 px rounded square on the 1024 px canvas, with
                    Apple's drop shadow
  icon.svg          Windows, Linux and the web: the same rounded square, nearly full-bleed, no shadow
  icon-square.svg   a full-bleed square, for places that mask the icon themselves (store/images/icon)
  Stellarator.icon  the macOS 26 Icon Composer document: the gradient as the icon's fill and the mark
                    as one white glass layer; the system draws the Liquid Glass (release-nsh.yml and
                    nsh.yml compile it with actool)

and the PNGs listed in PNGS: the app's icons (app/icons/, which nsh.json names; `nsh package
--target web` makes the browser's favicon, touch icon and manifest icons from icon.png) and the
icon About shows (app/assets/icon-192.png).

The mark is the path in app/assets/mark.svg, a 24-unit box centred on (12, 12): one thick ring with
five soft lobes, the shape of a stellarator's five-period plasma ring seen from above. The icon is
the white mark over a two-colour linear gradient, GRADIENT from top to bottom. In the static icons
the mark is white with a little transparency, a bright inner edge at the top and a soft shadow, a
still picture of the glass that macOS 26 draws. In Stellarator.icon the dark appearance uses
GRADIENT_DARK; the tinted and clear appearances are left to the system.
"""
import json
import math
import os
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
APP = os.path.dirname(HERE)
MARK_SVG = os.path.join(APP, "assets", "mark.svg")

# Top to bottom: periwinkle to deep indigo; in the dark appearance, a darker pair of the same hues.
GRADIENT = ("#5566dc", "#2c3684")
GRADIENT_DARK = ("#2c3684", "#141a40")

# Every PNG: (path from app/, source SVG, size in px).
PNGS = [
    ("icons/icon-macos.png", "icon-macos.svg", 1024),
    ("icons/icon.png", "icon.svg", 1024),
    ("assets/icon-192.png", "icon.svg", 192),
]


def read_mark():
    """The mark's path data from app/assets/mark.svg: absolute M, C and Z commands only."""
    svg = open(MARK_SVG).read()
    d = re.search(r'<path[^>]*\sd="([^"]+)"', svg).group(1)
    if re.search(r"[^MCZ0-9.\s,-]", d):
        raise SystemExit(f"{MARK_SVG}: the path may use only absolute M, C and Z commands")
    return d


def commands(d):
    """Path data as (command, [(x, y), ...]) in order."""
    out = []
    for letter, args in re.findall(r"([MCZ])([^MCZ]*)", d):
        nums = [float(n) for n in re.findall(r"-?[\d.]+", args)]
        out.append((letter, list(zip(nums[0::2], nums[1::2]))))
    return out


MARK = commands(read_mark())
# The mark's outer radius in its 24-unit box: the farthest point the path passes through (each
# command's last point) from the centre. 11.15 for the current mark.
EXTENT = max(math.hypot(pts[-1][0] - 12, pts[-1][1] - 12) for _, pts in MARK if pts)


def mark_path(scale, cx, cy):
    """The mark's path data with the 24-unit box scaled by `scale` and centred on (cx, cy)."""
    return "".join(
        letter + " ".join(f"{cx + scale * (x - 12):.2f} {cy + scale * (y - 12):.2f}" for x, y in pts)
        for letter, pts in MARK
    )


# ---------------------------------------------------------------------------------------------
# The static icons.
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
    mark = mark_path(scale, 512, 512)
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
                f'<path d="{mark_path(scale, 512, 512)}" fill="#fff" fill-rule="evenodd"/></svg>\n')

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


def render_pngs():
    """Each of PNGS from its SVG source, with resvg."""
    resvg = shutil.which("resvg")
    if not resvg:
        raise SystemExit("resvg not found: `cargo install resvg`, or run with --no-png")
    for target, source, size in PNGS:
        out = os.path.join(APP, target)
        os.makedirs(os.path.dirname(out), exist_ok=True)
        subprocess.run([resvg, "-w", str(size), "-h", str(size), os.path.join(HERE, source), out], check=True)
        print(f"wrote app/{target}")


if __name__ == "__main__":
    with open(os.path.join(HERE, "icon-macos.svg"), "w") as f:
        f.write(svg(412, shadow=True))
    with open(os.path.join(HERE, "icon.svg"), "w") as f:
        f.write(svg(488, shadow=False))
    with open(os.path.join(HERE, "icon-square.svg"), "w") as f:
        f.write(svg(512, shadow=False, square=True))
    icon_composer()
    print("wrote icon-macos.svg, icon.svg, icon-square.svg and Stellarator.icon")
    if "--no-png" not in sys.argv[1:]:
        render_pngs()
