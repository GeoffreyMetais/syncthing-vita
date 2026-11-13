#!/usr/bin/env python3
"""Generate the LiveArea bubble icon (sce_sys/icon0.png): a PS Vita whose screen
shows a Syncthing-style emblem (ring + connected nodes).

The Vita requires a 128x128 8-bit indexed PNG. The icon is drawn at 8x and
downsampled for anti-aliasing. Run: python3 scripts/gen_icon.py
"""
import math
from pathlib import Path

from PIL import Image, ImageDraw

S = 8                      # supersampling factor
N = 128 * S                # canvas size
OUT = Path(__file__).resolve().parent.parent / "crates/vita-syncthing-app/assets/sce_sys/icon0.png"


def lerp(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def vertical_gradient(w, h, top, bottom):
    img = Image.new("RGBA", (w, h))
    px = img.load()
    for y in range(h):
        c = lerp(top, bottom, y / max(1, h - 1)) + (255,)
        for x in range(w):
            px[x, y] = c
    return img


def radial_gradient(size, inner, outer, center):
    img = Image.new("RGBA", (size, size))
    px = img.load()
    cx, cy = center
    rmax = math.hypot(size, size) * 0.62
    for y in range(size):
        for x in range(size):
            t = min(1.0, math.hypot(x - cx, y - cy) / rmax)
            px[x, y] = lerp(inner, outer, t) + (255,)
    return img


def u(v):
    """Icon units (0..128) -> canvas pixels."""
    return round(v * S)


def rrect(draw, box, r, **kw):
    draw.rounded_rectangle([u(c) for c in box], radius=u(r), **kw)


def circle(draw, cx, cy, r, **kw):
    draw.ellipse([u(cx - r), u(cy - r), u(cx + r), u(cy + r)], **kw)


def main():
    # Background: Syncthing blue, lighter toward the top-left.
    icon = radial_gradient(N, (54, 190, 238), (6, 72, 140), (N * 0.3, N * 0.25))
    d = ImageDraw.Draw(icon)

    # Soft drop shadow under the console.
    shadow = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle(
        [u(12), u(46), u(116), u(98)], radius=u(26), fill=(0, 20, 50, 110))
    shadow = shadow.resize((N // 16, N // 16), Image.BILINEAR).resize((N, N), Image.BILINEAR)
    icon.alpha_composite(shadow)

    # Console body: oblong "pill" with a thin light rim.
    body = (10, 40, 118, 92)
    rrect(d, body, 26, fill=(214, 224, 236))
    rrect(d, (11.5, 41.5, 116.5, 90.5), 24.5, fill=(30, 33, 40))
    # Subtle top highlight on the body.
    hl = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    ImageDraw.Draw(hl).rounded_rectangle(
        [u(14), u(43), u(114), u(62)], radius=u(18), fill=(255, 255, 255, 22))
    icon.alpha_composite(hl)

    # Screen with a blue gradient.
    sx0, sy0, sx1, sy1 = 37, 47, 91, 85
    screen = vertical_gradient(u(sx1 - sx0), u(sy1 - sy0), (40, 170, 225), (8, 96, 170))
    mask = Image.new("L", screen.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, *screen.size], radius=u(3), fill=255)
    rrect(d, (sx0 - 1.2, sy0 - 1.2, sx1 + 1.2, sy1 + 1.2), 4, fill=(8, 10, 14))
    icon.paste(screen, (u(sx0), u(sy0)), mask)

    # Syncthing-style emblem: white ring, three nodes on it, joined at a hub.
    cx, cy, r = (sx0 + sx1) / 2, (sy0 + sy1) / 2, 13.0
    white = (255, 255, 255)
    circle(d, cx, cy, r, outline=white, width=u(2.4))
    hub = (cx - 1.0, cy + 1.8)
    nodes = []
    for deg, nr in ((-40, 3.6), (160, 2.7), (88, 2.7)):
        a = math.radians(deg)
        nodes.append((cx + r * math.cos(a), cy + r * math.sin(a), nr))
    for nx, ny, _ in nodes:
        d.line([u(hub[0]), u(hub[1]), u(nx), u(ny)], fill=white, width=u(1.8))
    circle(d, *hub, 2.0, fill=white)
    for nx, ny, nr in nodes:
        circle(d, nx, ny, nr, fill=white)

    # Left side: D-pad and analog stick.
    pad = (205, 212, 222)
    dx, dy = 24, 58
    rrect(d, (dx - 6, dy - 2, dx + 6, dy + 2), 1, fill=pad)
    rrect(d, (dx - 2, dy - 6, dx + 2, dy + 6), 1, fill=pad)
    circle(d, 28, 78, 5.2, fill=(12, 13, 17))
    circle(d, 28, 78, 3.6, fill=(70, 74, 84))

    # Right side: face buttons and analog stick.
    bx, by, br = 104, 58, 5.0
    for ox, oy in ((0, -br), (br, 0), (0, br), (-br, 0)):
        circle(d, bx + ox, by + oy, 2.3, fill=pad)
    circle(d, 100, 78, 5.2, fill=(12, 13, 17))
    circle(d, 100, 78, 3.6, fill=(70, 74, 84))

    # Downsample, then quantize to the 8-bit palette the Vita requires.
    small = icon.resize((128, 128), Image.LANCZOS).convert("RGB")
    small.quantize(colors=256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE).save(OUT, optimize=True)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
