#!/usr/bin/env python3
"""Generate a treemap-style app icon for rustdirstat.

Requires: pip install Pillow
Usage: python scripts/generate-icon.py
Output: assets/icon-256.png
"""
from PIL import Image, ImageDraw

SIZE = 256
PAD = 16
CORNER = 6

img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
draw = ImageDraw.Draw(img)

# Draw rounded background
draw.rounded_rectangle([0, 0, SIZE - 1, SIZE - 1], radius=32, fill=(38, 38, 38))

inner = SIZE - 2 * PAD
# Treemap-style colored blocks (relative coords within inner area)
blocks = [
    (0.00, 0.00, 0.47, 0.47, (76, 175, 80)),   # green - large directory
    (0.50, 0.00, 0.50, 0.32, (33, 150, 243)),   # blue
    (0.50, 0.35, 0.50, 0.12, (255, 152, 0)),    # orange
    (0.00, 0.50, 0.68, 0.50, (156, 39, 176)),   # purple - large directory
    (0.71, 0.50, 0.29, 0.27, (244, 67, 54)),    # red
    (0.71, 0.80, 0.29, 0.20, (0, 188, 212)),    # teal
]

for rx, ry, rw, rh, color in blocks:
    x1 = PAD + int(rx * inner)
    y1 = PAD + int(ry * inner)
    x2 = PAD + int((rx + rw) * inner) - 2
    y2 = PAD + int((ry + rh) * inner) - 2
    draw.rounded_rectangle([x1, y1, x2, y2], radius=CORNER, fill=color)

img.save("assets/icon-256.png")
print("Generated assets/icon-256.png")
