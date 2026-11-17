#!/usr/bin/env python3
"""Generate assets/logo/crab.svg from assets/logo/crab.txt."""
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
COLORS = {"O": "#ff6b2c", "L": "#ffa266", "D": "#b8401a", "W": "#ffffff", "K": "#1b1b1f"}
PX = 16

rows = [l for l in (ROOT / "assets/logo/crab.txt").read_text().splitlines() if l and not l.startswith("#")]
w, h = max(map(len, rows)) * PX, len(rows) * PX
rects = [
    f'<rect x="{x * PX}" y="{y * PX}" width="{PX}" height="{PX}" fill="{COLORS[c]}"/>'
    for y, row in enumerate(rows)
    for x, c in enumerate(row)
    if c in COLORS
]
svg = (
    f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w * 2}" height="{h * 2}" '
    f'shape-rendering="crispEdges">\n  <title>rusteezy</title>\n  ' + "\n  ".join(rects) + "\n</svg>\n"
)
(ROOT / "assets/logo/crab.svg").write_text(svg)
print(f"assets/logo/crab.svg ({w}x{h})")
