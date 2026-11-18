#!/usr/bin/env python3
"""Writes the Stellarator app icon sources.

    python3 make_icon.py [variant]

Writes, next to this script:
  icon-macos.svg   macOS: the 824 px squircle on the 1024 px canvas, with Apple's drop shadow
  icon.svg         Windows, Linux and the web favicon: the same squircle, nearly full-bleed, no shadow
  icon-square.svg  iOS home screen and web "maskable" icons: full-bleed square (the platform masks it)
  Stellarator.icon macOS 26 Icon Composer document: the gradient as the fill and the mark as a
                   white glass layer; the system draws the Liquid Glass (actool compiles it in CI)

The mark is the one in frontend/src/components/logo.tsx (two coils braided around a ring), drawn
in white glass over a plasma gradient: indigo at the top through violet to a copper glow, the
colours of the plasma a stellarator confines. docs/desktop.md ("The icon") lists the commands that
turn these into each platform's files.
"""
import json
import math
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
LOGO = os.path.join(HERE, "..", "..", "frontend", "src", "components", "logo.tsx")

VARIANTS = {
    # (stops top-left to bottom-right, plasma glow colour, glow opacity)
    "plasma": ([("0", "#0c1240"), ("0.42", "#35208a"), ("0.74", "#9a3191"), ("1", "#f0874c")], "#ff9e6e", 0.55),
    "violet": ([("0", "#120e3a"), ("0.5", "#4a24b8"), ("1", "#c0438f")], "#ff8f73", 0.5),
    "night": ([("0", "#070b26"), ("0.55", "#1d1f6b"), ("1", "#5a2a9e")], "#ff8a5c", 0.65),
    "fusion": ([("0", "#101645"), ("0.52", "#3a2596"), ("1", "#9a3aa6")], "#ff8566", 0.6),
}


def coil(sign, steps=120):
    """One coil of the mark, in its 24-unit space: r = 8 + sign * 1.8 * sin(5 theta) around (12, 12).

    This is the curve the 60-point polylines in frontend/src/components/logo.tsx sample (sign -1 is
    the ink coil, +1 the copper coil). Here it is drawn from the formula as a closed Catmull-Rom
    spline, so it stays smooth at 1024 px.
    """
    pts = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        r = 8 + sign * 1.8 * math.sin(5 * t)
        pts.append((12 + r * math.cos(t), 12 + r * math.sin(t)))
    d = [f"M{pts[0][0]:.3f} {pts[0][1]:.3f}"]
    for i in range(steps):
        p0, p1, p2, p3 = pts[i - 1], pts[i], pts[(i + 1) % steps], pts[(i + 2) % steps]
        c1 = (p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6)
        c2 = (p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6)
        d.append(f"C{c1[0]:.3f} {c1[1]:.3f} {c2[0]:.3f} {c2[1]:.3f} {p2[0]:.3f} {p2[1]:.3f}")
    return "".join(d) + "Z"


def mark_paths():
    return [coil(-1), coil(+1)]  # [ink coil, copper coil]


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


def svg(variant, half, shadow, square=False):
    stops, glow, glow_op = VARIANTS[variant]
    ink, copper = mark_paths()
    # square: full-bleed, for platforms that mask the icon themselves (iOS home screen, web
    # "maskable" icons); the mark stays inside the central 80 % safe zone.
    tile = "M0,0 H1024 V1024 H0 Z" if square else squircle(512, 512, half)
    k = half / 412  # 1.0 on the macOS grid
    scale = 25.5 * k  # the mark (about 19.6 units wide with its stroke) fills about 61 % of the tile
    off = 512 - 12 * scale
    sw = 1.85  # glyph stroke, in mark units
    stop_xml = "".join(f'<stop offset="{o}" stop-color="{c}"/>' for o, c in stops)
    tile_shadow = (
        '<filter id="tileShadow" x="-20%" y="-20%" width="140%" height="140%">'
        '<feDropShadow dx="0" dy="10" stdDeviation="12" flood-color="#000" flood-opacity="0.30"/></filter>'
        if shadow
        else ""
    )

    def glyph(which, stroke):
        # The coils are defined once in <defs>, in mark units; filters go on outer, unscaled
        # groups so their sizes are pixels.
        return (f'<g transform="translate({off:.2f} {off:.2f}) scale({scale:.4f})" fill="none" '
                f'stroke-width="{sw}" stroke-linejoin="round" stroke-linecap="round"><use href="#{which}" stroke="{stroke}"/></g>')

    px = lambda v: f"{v * k:.1f}"
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
<!-- Generated by make_icon.py ({variant}). Edit the script, not this file. -->
<defs>
  <path id="ink" d="{ink}"/>
  <path id="copper" d="{copper}"/>
  {tile_shadow}
  <clipPath id="tile"><path d="{tile}"/></clipPath>
  <linearGradient id="bg" x1="0.15" y1="0" x2="0.85" y2="1">{stop_xml}</linearGradient>
  <radialGradient id="glow" cx="512" cy="{512 + 20 * k:.1f}" r="{290 * k:.1f}" gradientUnits="userSpaceOnUse">
    <stop offset="0" stop-color="{glow}" stop-opacity="{glow_op}"/>
    <stop offset="0.5" stop-color="{glow}" stop-opacity="{glow_op * 0.38:.3f}"/>
    <stop offset="1" stop-color="{glow}" stop-opacity="0"/>
  </radialGradient>
  <radialGradient id="sheen" cx="512" cy="{512 - 430 * k:.1f}" r="{620 * k:.1f}" gradientUnits="userSpaceOnUse">
    <stop offset="0" stop-color="#fff" stop-opacity="0.18"/>
    <stop offset="1" stop-color="#fff" stop-opacity="0"/>
  </radialGradient>
  <linearGradient id="rim" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#fff" stop-opacity="0.60"/>
    <stop offset="0.30" stop-color="#fff" stop-opacity="0.10"/>
    <stop offset="0.75" stop-color="#fff" stop-opacity="0.04"/>
    <stop offset="1" stop-color="#fff" stop-opacity="0.25"/>
  </linearGradient>
  <!-- Glass body: near-white at the top, letting the plasma through towards the bottom. -->
  <linearGradient id="glass" x1="0" y1="{off + 2.5 * scale:.1f}" x2="0" y2="{off + 21.5 * scale:.1f}" gradientUnits="userSpaceOnUse">
    <stop offset="0" stop-color="#ffffff" stop-opacity="0.96"/>
    <stop offset="0.55" stop-color="#fdfaff" stop-opacity="0.86"/>
    <stop offset="1" stop-color="#f6eeff" stop-opacity="0.70"/>
  </linearGradient>
  <filter id="dropShadow" x="-25%" y="-25%" width="150%" height="150%">
    <feGaussianBlur in="SourceAlpha" stdDeviation="{px(13)}"/>
    <feOffset dy="{px(14)}" result="b"/>
    <feFlood flood-color="#0b0630" flood-opacity="0.50"/>
    <feComposite in2="b" operator="in"/>
  </filter>
  <filter id="castShadow" x="-25%" y="-25%" width="150%" height="150%">
    <feGaussianBlur in="SourceAlpha" stdDeviation="{px(7)}"/>
    <feOffset dy="{px(5)}" result="b"/>
    <feFlood flood-color="#1a0b4a" flood-opacity="0.30"/>
    <feComposite in2="b" operator="in"/>
  </filter>
  <!-- Rounded-tube shading: diffuse light from the upper left, multiplied into the glass. -->
  <filter id="bevel" x="-10%" y="-10%" width="120%" height="120%" color-interpolation-filters="sRGB">
    <feGaussianBlur in="SourceAlpha" stdDeviation="{px(6)}" result="blur"/>
    <feDiffuseLighting in="blur" surfaceScale="{px(5)}" diffuseConstant="1.05" lighting-color="#ffffff" result="light">
      <feDistantLight azimuth="235" elevation="58"/>
    </feDiffuseLighting>
    <feComponentTransfer in="light" result="soft"><feFuncR type="linear" slope="0.24" intercept="0.80"/><feFuncG type="linear" slope="0.24" intercept="0.78"/><feFuncB type="linear" slope="0.20" intercept="0.83"/></feComponentTransfer>
    <feComposite in="SourceGraphic" in2="soft" operator="arithmetic" k1="1" k2="0" k3="0" k4="0" result="shaded"/>
    <feComposite in="shaded" in2="SourceAlpha" operator="in"/>
  </filter>
  <!-- Light caught on the lower edge, as glass refracts it. -->
  <filter id="underlight" x="-10%" y="-10%" width="120%" height="120%" color-interpolation-filters="sRGB">
    <feGaussianBlur in="SourceAlpha" stdDeviation="{px(3.5)}" result="blur"/>
    <feSpecularLighting in="blur" surfaceScale="{px(4)}" specularConstant="0.8" specularExponent="18" lighting-color="#ffe9f2" result="spec">
      <feDistantLight azimuth="55" elevation="30"/>
    </feSpecularLighting>
    <feComposite in="spec" in2="SourceAlpha" operator="in"/>
  </filter>
  <!-- Specular: the bright top edge of the glass. -->
  <filter id="spec" x="-10%" y="-10%" width="120%" height="120%" color-interpolation-filters="sRGB">
    <feGaussianBlur in="SourceAlpha" stdDeviation="{px(4.5)}" result="blur"/>
    <feSpecularLighting in="blur" surfaceScale="{px(5)}" specularConstant="1.0" specularExponent="22" lighting-color="#ffffff" result="spec">
      <feDistantLight azimuth="235" elevation="48"/>
    </feSpecularLighting>
    <feComposite in="spec" in2="SourceAlpha" operator="in"/>
  </filter>
</defs>
<g{' filter="url(#tileShadow)"' if shadow else ''}><path d="{tile}" fill="url(#bg)"/></g>
<g clip-path="url(#tile)">
  <rect width="1024" height="1024" fill="url(#glow)"/>
  <rect width="1024" height="1024" fill="url(#sheen)"/>
  <g filter="url(#dropShadow)">{glyph("ink", "#000")}{glyph("copper", "#000")}</g>
  <g filter="url(#bevel)" opacity="0.66">{glyph("ink", "url(#glass)")}</g>
  <g filter="url(#spec)" opacity="0.55" style="mix-blend-mode:screen">{glyph("ink", "#fff")}</g>
  <g filter="url(#underlight)" opacity="0.35" style="mix-blend-mode:screen">{glyph("ink", "#fff")}</g>
  <g filter="url(#castShadow)">{glyph("copper", "#000")}</g>
  <!-- A solid base under the front coil: the back coil does not show through it. -->
  {glyph("copper", "#cfc3ea")}
  <g filter="url(#bevel)">{glyph("copper", "url(#glass)")}</g>
  <g filter="url(#spec)" opacity="0.85" style="mix-blend-mode:screen">{glyph("copper", "#fff")}</g>
  <g filter="url(#underlight)" opacity="0.45" style="mix-blend-mode:screen">{glyph("copper", "#fff")}</g>
</g>
{"" if square else f'<path d="{tile}" fill="none" stroke="url(#rim)" stroke-width="{5 * k:.1f}" clip-path="url(#tile)"/>'}
</svg>
"""


def icon_composer(variant):
    """Stellarator.icon: Icon Composer's document format (a folder with icon.json and assets)."""
    stops, _, _ = VARIANTS[variant]
    ink, copper = mark_paths()
    out = os.path.join(HERE, "Stellarator.icon")
    os.makedirs(os.path.join(out, "Assets"), exist_ok=True)

    def p3(hex_):
        r, g, b = (int(hex_[i : i + 2], 16) / 255 for i in (1, 3, 5))
        return f"srgb:{r:.5f},{g:.5f},{b:.5f},1.00000"

    # The glyph layers are white on a transparent 1024 canvas, the mark at the size it has in the
    # static icon (Icon Composer canvases are 1024 points, full bleed).
    scale = 25.5 * 512 / 412
    off = 512 - 12 * scale
    # The plasma glow behind the coils: Icon Composer's fill is a two-colour gradient, so the glow
    # is its own (non-glass) layer.
    _, glow, glow_op = VARIANTS[variant]
    with open(os.path.join(out, "Assets", "glow.svg"), "w") as f:
        f.write(
            '<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">'
            f'<defs><radialGradient id="g" cx="512" cy="537" r="360" gradientUnits="userSpaceOnUse">'
            f'<stop offset="0" stop-color="{glow}" stop-opacity="{glow_op}"/>'
            f'<stop offset="0.5" stop-color="{glow}" stop-opacity="{glow_op * 0.38:.3f}"/>'
            f'<stop offset="1" stop-color="{glow}" stop-opacity="0"/></radialGradient></defs>'
            '<rect width="1024" height="1024" fill="url(#g)"/></svg>\n'
        )
    for name, d in (("coil-back", ink), ("coil-front", copper)):
        with open(os.path.join(out, "Assets", f"{name}.svg"), "w") as f:
            f.write(
                f'<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">'
                f'<g transform="translate({off:.2f} {off:.2f}) scale({scale:.4f})" fill="none" stroke="#fff" '
                f'stroke-width="2.05" stroke-linejoin="round" stroke-linecap="round"><path d="{d}"/></g></svg>\n'
            )
    doc = {
        "fill": {
            "linear-gradient": [p3(stops[0][1]), p3(stops[-1][1])],
            "orientation": {"start": {"x": 0.3, "y": 0}, "stop": {"x": 0.7, "y": 1}},
        },
        "groups": [
            {
                "layers": [
                    {"name": "coil-front", "image-name": "coil-front.svg", "glass": True},
                    {"name": "coil-back", "image-name": "coil-back.svg", "glass": True, "opacity": 0.85},
                ],
                "shadow": {"kind": "neutral", "opacity": 0.5},
                "translucency": {"enabled": True, "value": 0.35},
                "specular": True,
            },
            {
                "layers": [{"name": "glow", "image-name": "glow.svg", "glass": False}],
                "shadow": {"kind": "none"},
            },
        ],
        "supported-platforms": {"squares": ["macOS"]},
    }
    with open(os.path.join(out, "icon.json"), "w") as f:
        json.dump(doc, f, indent=2)
        f.write("\n")


if __name__ == "__main__":
    variant = sys.argv[1] if len(sys.argv) > 1 else "fusion"
    with open(os.path.join(HERE, "icon-macos.svg"), "w") as f:
        f.write(svg(variant, 412, shadow=True))
    with open(os.path.join(HERE, "icon.svg"), "w") as f:
        f.write(svg(variant, 488, shadow=False))
    with open(os.path.join(HERE, "icon-square.svg"), "w") as f:
        f.write(svg(variant, 512, shadow=False, square=True))
    icon_composer(variant)
    print(f"wrote icon-macos.svg, icon.svg, icon-square.svg and Stellarator.icon ({variant})")
