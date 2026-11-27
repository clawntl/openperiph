#!/usr/bin/env python3
"""Draws OpenPeriph's icon: a ring fading from pale green (top left) to blue (bottom right) with a
white dot, on a dark navy rounded square. Writes the Linux SVG, a 256 px PNG (window icon) and
the Windows .ico. Run from the repository root: python3 tools/make_icons.py"""

from PIL import Image, ImageDraw
import numpy as np

GREEN = (142, 230, 168)
BLUE = (120, 162, 236)
TOP, BOTTOM = (36, 42, 56), (21, 25, 33)
BORDER = (52, 60, 76)


def draw(size):
    s = 4 * size  # supersample
    y, x = np.mgrid[0:s, 0:s].astype(np.float32) / s
    # rounded square
    r = 0.22
    dx = np.maximum(np.abs(x - 0.5) - (0.5 - r) + 0.02, 0)
    dy = np.maximum(np.abs(y - 0.5) - (0.5 - r) + 0.02, 0)
    d = np.sqrt(dx * dx + dy * dy)
    inside = np.clip((r - d) * s, 0, 1)
    border = np.clip((r - d) * s, 0, 1) - np.clip((r - d - 0.012) * s, 0, 1)
    bg = np.empty((s, s, 3), np.float32)
    for k in range(3):
        bg[..., k] = TOP[k] + (BOTTOM[k] - TOP[k]) * y
        bg[..., k] = bg[..., k] * (1 - border) + BORDER[k] * border
    # ring
    cx = cy = 0.5
    dist = np.sqrt((x - cx) ** 2 + (y - cy) ** 2)
    ring = np.clip((0.065 - np.abs(dist - 0.27)) * s, 0, 1)
    t = np.clip(((x - 0.2) + (y - 0.2)) / 1.2, 0, 1)
    rgb = bg.copy()
    for k in range(3):
        c = GREEN[k] + (BLUE[k] - GREEN[k]) * t
        rgb[..., k] = rgb[..., k] * (1 - ring) + c * ring
    # dot on the ring, top right
    a = np.deg2rad(-45)
    px, py = cx + 0.27 * np.cos(a), cy + 0.27 * np.sin(a)
    dd = np.sqrt((x - px) ** 2 + (y - py) ** 2)
    halo = np.clip((0.10 - dd) * s, 0, 1)
    dot = np.clip((0.068 - dd) * s, 0, 1)
    for k in range(3):
        rgb[..., k] = rgb[..., k] * (1 - halo) + bg[..., k] * halo  # gap around the dot
        rgb[..., k] = rgb[..., k] * (1 - dot) + 255 * dot
    img = Image.fromarray(np.dstack([rgb, inside * 255]).clip(0, 255).astype(np.uint8), "RGBA")
    return img.resize((size, size), Image.LANCZOS)


big = draw(256)
big.save("assets/icons/openperiph.png", optimize=True)
draw(512).save("packaging/linux/openperiph-512.png", optimize=True)
big.save("assets/icons/openperiph.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])

svg = f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="rgb{TOP}"/><stop offset="1" stop-color="rgb{BOTTOM}"/>
    </linearGradient>
    <linearGradient id="ring" x1="0.2" y1="0.2" x2="0.95" y2="0.95">
      <stop offset="0" stop-color="rgb{GREEN}"/><stop offset="1" stop-color="rgb{BLUE}"/>
    </linearGradient>
  </defs>
  <rect x="5" y="5" width="246" height="246" rx="51" fill="url(#bg)" stroke="rgb{BORDER}" stroke-width="3"/>
  <circle cx="128" cy="128" r="69" fill="none" stroke="url(#ring)" stroke-width="33"/>
  <circle cx="176.8" cy="79.2" r="25.6" fill="url(#bg)"/>
  <circle cx="176.8" cy="79.2" r="17.4" fill="#fff"/>
</svg>
"""
open("packaging/linux/openperiph.svg", "w").write(svg)
print("icons written")
