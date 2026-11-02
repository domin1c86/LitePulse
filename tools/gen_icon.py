"""Generate the multi-size LitePulse .ico used for the executable icon.

Run with the bundled Python (Pillow is available):
    python tools/gen_icon.py
"""

from __future__ import annotations

import math
import os

from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(ROOT, "assets")

SS = 4  # supersampling factor
BASE = 256


def rounded_background(size: int) -> Image.Image:
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    pad = int(size * 0.035)
    radius = int(size * 0.22)
    draw.rounded_rectangle(
        (pad, pad, size - pad, size - pad),
        radius=radius,
        fill=(27, 30, 36, 255),
        outline=(45, 50, 60, 255),
        width=max(1, int(size * 0.008)),
    )
    return img


def draw_ring(img: Image.Image) -> None:
    size = img.width
    draw = ImageDraw.Draw(img)
    cx = cy = size / 2.0
    outer = size * 0.34
    thickness = size * 0.095
    inner = outer - thickness
    start = -90.0
    extent = 292.0

    # Track + filled arc. The arc is drawn on a supersampled canvas, then
    # downsampled, so the result stays smooth at 16 px.
    bbox = (cx - outer, cy - outer, cx + outer, cy + outer)
    draw.arc(bbox, start=0, end=360, fill=(96, 105, 120, 150), width=int(thickness))
    draw.arc(bbox, start=start, end=start + extent, fill=(76, 154, 255, 255), width=int(thickness))

    # Small pulse peak inside the ring.
    points = []
    for i in range(101):
        t = i / 100.0
        x = cx - inner * 0.62 + inner * 1.24 * t
        if t < 0.30:
            y = cy + inner * 0.10
        elif t < 0.45:
            y = cy + inner * 0.10 - inner * 0.45 * ((t - 0.30) / 0.15)
        elif t < 0.60:
            y = cy - inner * 0.35 + inner * 0.80 * ((t - 0.45) / 0.15)
        elif t < 0.76:
            y = cy + inner * 0.45 - inner * 0.35 * ((t - 0.60) / 0.16)
        else:
            y = cy + inner * 0.10
        points.append((x, y))
    draw.line(points, fill=(236, 241, 248, 255), width=max(1, int(size * 0.035)), joint="curve")


def main() -> None:
    os.makedirs(ASSETS, exist_ok=True)
    size = BASE * SS
    img = rounded_background(size)
    draw_ring(img)
    img = img.resize((BASE, BASE), Image.LANCZOS)

    out = os.path.join(ASSETS, "litepulse.ico")
    sizes = [(16, 16), (20, 20), (24, 24), (28, 28), (32, 32), (40, 40), (48, 48), (64, 64), (128, 128), (256, 256)]
    img.save(out, format="ICO", sizes=sizes)
    print(f"wrote {out} ({os.path.getsize(out)} bytes)")


if __name__ == "__main__":
    main()
