#!/usr/bin/env python3
"""Rasterise the original YumoCut YC mark without external artwork."""

from pathlib import Path
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets" / "icons"
SIZE = 1024
SCALE = SIZE / 256

image = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
draw = ImageDraw.Draw(image)
draw.rounded_rectangle(
    tuple(round(v * SCALE) for v in (8, 8, 248, 248)),
    radius=round(56 * SCALE),
    fill=(17, 22, 38, 255),
)

def line(points, colour, width=22):
    draw.line(
        [(round(x * SCALE), round(y * SCALE)) for x, y in points],
        fill=colour,
        width=round(width * SCALE),
        joint="curve",
    )
    radius = width * SCALE / 2
    for x, y in (points[0], points[-1]):
        x, y = x * SCALE, y * SCALE
        draw.ellipse((x - radius, y - radius, x + radius, y + radius), fill=colour)

line([(48, 60), (91, 104), (134, 60)], (97, 230, 207, 255))
line([(91, 104), (91, 194)], (97, 230, 207, 255))
# The open right-hand C, drawn as a high-resolution arc.
box = tuple(round(v * SCALE) for v in (126, 62, 211, 194))
draw.arc(box, start=42, end=318, fill=(245, 247, 255, 255), width=round(22 * SCALE))
radius = 11 * SCALE
for x, y in ((194.3, 82.2), (194.3, 173.8)):
    x, y = x * SCALE, y * SCALE
    draw.ellipse((x - radius, y - radius, x + radius, y + radius), fill=(245, 247, 255, 255))

image.resize((256, 256), Image.Resampling.LANCZOS).save(OUT / "yumocut_logo_256.png")
image.resize((256, 256), Image.Resampling.LANCZOS).save(
    OUT / "yumocut.ico",
    sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
)
print("Wrote YumoCut icon assets")
