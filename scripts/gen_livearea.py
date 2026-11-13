#!/usr/bin/env python3
"""Generate the LiveArea images of the VPK (crates/vita-syncthing-app/assets/sce_sys/):

- icon0.png                      128x128 bubble icon
- livearea/contents/bg.png       840x500 LiveArea background
- livearea/contents/startup.png  280x158 gate ("Start") image

The artwork is a PS Vita whose screen shows a Syncthing-style emblem (ring and
connected nodes). The Vita requires 8-bit indexed PNGs. Everything is drawn at
SxS supersampling and downsampled for anti-aliasing.
Run: python3 scripts/gen_livearea.py
"""
import math
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

S = 4  # supersampling factor
ROOT = Path(__file__).resolve().parent.parent / "crates/vita-syncthing-app"
SCE_SYS = ROOT / "assets/sce_sys"
FONT_BOLD = ROOT / "assets/fonts/font_poppins_bold.ttf"
FONT_MEDIUM = ROOT / "assets/fonts/font_poppins_medium.ttf"

BLUE_LIGHT = (54, 190, 238)
BLUE_DARK = (6, 72, 140)
WHITE = (255, 255, 255)
PAD = (205, 212, 222)


def lerp(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def radial_gradient(w, h, inner, outer, center, reach):
    """Radial gradient; `center` and `reach` are fractions of the image size."""
    img = Image.new("RGBA", (w, h))
    px = img.load()
    cx, cy = center[0] * w, center[1] * h
    rmax = math.hypot(w, h) * reach
    for y in range(h):
        for x in range(w):
            t = min(1.0, math.hypot(x - cx, y - cy) / rmax)
            px[x, y] = lerp(inner, outer, t) + (255,)
    return img


def vertical_gradient(w, h, top, bottom):
    img = Image.new("RGBA", (w, h))
    d = ImageDraw.Draw(img)
    for y in range(h):
        d.line([(0, y), (w, y)], fill=lerp(top, bottom, y / max(1, h - 1)) + (255,))
    return img


class Pen:
    """Draws in "design units" mapped to canvas pixels by scale + offset."""

    def __init__(self, img, scale, ox=0.0, oy=0.0):
        self.img, self.k, self.ox, self.oy = img, scale, ox, oy
        self.d = ImageDraw.Draw(img)

    def p(self, x, y):
        return round(self.ox + x * self.k), round(self.oy + y * self.k)

    def box(self, x0, y0, x1, y1):
        return [*self.p(x0, y0), *self.p(x1, y1)]

    def n(self, v):
        return max(1, round(v * self.k))

    def rrect(self, box, r, **kw):
        self.d.rounded_rectangle(self.box(*box), radius=self.n(r), **kw)

    def circle(self, cx, cy, r, **kw):
        self.d.ellipse(self.box(cx - r, cy - r, cx + r, cy + r), **kw)

    def line(self, a, b, width, fill):
        self.d.line([*self.p(*a), *self.p(*b)], fill=fill, width=self.n(width))

    def overlay(self, draw_fn):
        """Draw a translucent layer: draw_fn(pen_on_layer)."""
        layer = Image.new("RGBA", self.img.size, (0, 0, 0, 0))
        draw_fn(Pen(layer, self.k, self.ox, self.oy))
        self.img.alpha_composite(layer)


def draw_emblem(pen, cx, cy, r, color=WHITE, ring_w=None):
    """Syncthing-style emblem: a ring with three nodes joined at an inner hub."""
    ring_w = ring_w or r * 0.185
    pen.circle(cx, cy, r, outline=color, width=pen.n(ring_w))
    hub = (cx - r * 0.077, cy + r * 0.138)
    nodes = []
    for deg, nr in ((-40, 0.277), (160, 0.208), (88, 0.208)):
        a = math.radians(deg)
        nodes.append((cx + r * math.cos(a), cy + r * math.sin(a), nr * r))
    for nx, ny, _ in nodes:
        pen.line(hub, (nx, ny), r * 0.138, color)
    pen.circle(*hub, r * 0.154, fill=color)
    for nx, ny, nr in nodes:
        pen.circle(nx, ny, nr, fill=color)


def draw_vita(pen):
    """PS Vita seen from the front, in a 128-unit design space (body x 10..118, y 40..92)."""
    # Soft drop shadow.
    shadow = Image.new("RGBA", pen.img.size, (0, 0, 0, 0))
    Pen(shadow, pen.k, pen.ox, pen.oy).rrect((12, 46, 116, 98), 26, fill=(0, 20, 50, 110))
    w, h = shadow.size
    shadow = shadow.resize((max(1, w // 16), max(1, h // 16)), Image.BILINEAR).resize((w, h), Image.BILINEAR)
    pen.img.alpha_composite(shadow)

    # Body: oblong pill with a thin light rim and a subtle top highlight.
    pen.rrect((10, 40, 118, 92), 26, fill=(214, 224, 236))
    pen.rrect((11.5, 41.5, 116.5, 90.5), 24.5, fill=(30, 33, 40))
    pen.overlay(lambda p: p.rrect((14, 43, 114, 62), 18, fill=(255, 255, 255, 22)))

    # Screen with a blue gradient.
    sx0, sy0, sx1, sy1 = 37, 47, 91, 85
    pen.rrect((sx0 - 1.2, sy0 - 1.2, sx1 + 1.2, sy1 + 1.2), 4, fill=(8, 10, 14))
    (x0, y0), (x1, y1) = pen.p(sx0, sy0), pen.p(sx1, sy1)
    screen = vertical_gradient(x1 - x0, y1 - y0, (40, 170, 225), (8, 96, 170))
    mask = Image.new("L", screen.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, *screen.size], radius=pen.n(3), fill=255)
    pen.img.paste(screen, (x0, y0), mask)
    draw_emblem(pen, (sx0 + sx1) / 2, (sy0 + sy1) / 2, 13.0, ring_w=2.4)

    # Left: D-pad and analog stick. Right: face buttons and analog stick.
    pen.rrect((18, 56, 30, 60), 1, fill=PAD)
    pen.rrect((22, 52, 26, 64), 1, fill=PAD)
    for ox, oy in ((0, -5), (5, 0), (0, 5), (-5, 0)):
        pen.circle(104 + ox, 58 + oy, 2.3, fill=PAD)
    for sx in (28, 100):
        pen.circle(sx, 78, 5.2, fill=(12, 13, 17))
        pen.circle(sx, 78, 3.6, fill=(70, 74, 84))


def draw_network(pen, w, h, seed_nodes, alpha):
    """Faint mesh of linked nodes (decoration of the LiveArea background)."""
    color = (255, 255, 255, alpha)

    def fn(p):
        for i, (ax, ay) in enumerate(seed_nodes):
            for bx, by in seed_nodes[i + 1:]:
                if math.hypot(ax - bx, ay - by) < 190:
                    p.line((ax, ay), (bx, by), 1.6, color)
        for x, y in seed_nodes:
            p.circle(x, y, 4.5, fill=color)

    pen.overlay(fn)


def save_indexed(img, size, path):
    path.parent.mkdir(parents=True, exist_ok=True)
    small = img.resize(size, Image.LANCZOS).convert("RGB")
    small.quantize(colors=256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE).save(path, optimize=True)
    print(f"wrote {path.relative_to(ROOT.parent.parent)}")


def text(img, xy, s, font_path, size, fill, anchor="la"):
    font = ImageFont.truetype(str(font_path), size * S)
    ImageDraw.Draw(img).text((xy[0] * S, xy[1] * S), s, font=font, fill=fill, anchor=anchor)


def make_icon():
    n = 128 * S
    img = radial_gradient(n, n, BLUE_LIGHT, BLUE_DARK, (0.3, 0.25), 0.62)
    draw_vita(Pen(img, S))
    save_indexed(img, (128, 128), SCE_SYS / "icon0.png")


def make_bg():
    w, h = 840, 500
    img = radial_gradient(w * S, h * S, BLUE_LIGHT, BLUE_DARK, (0.25, 0.2), 0.75)
    pen = Pen(img, S)
    # Mesh in the corners, leaving the centre (where the gate sits) clear.
    nodes = [(40, 420), (130, 470), (210, 400), (90, 330), (20, 250),
             (700, 60), (790, 140), (810, 30), (620, 20), (740, 230),
             (560, 470), (680, 420), (800, 470), (780, 330)]
    draw_network(pen, w, h, nodes, 38)
    # Large faint emblem on the right.
    pen.overlay(lambda p: draw_emblem(p, 700, 330, 120, color=(255, 255, 255, 34)))
    # Title, top-left.
    text(img, (40, 30), "Syncthing", FONT_BOLD, 44, WHITE)
    text(img, (42, 92), "File synchronization for PS Vita", FONT_MEDIUM, 20, (215, 238, 252))
    save_indexed(img, (w, h), SCE_SYS / "livearea/contents/bg.png")


def make_startup():
    w, h = 280, 158
    img = radial_gradient(w * S, h * S, BLUE_LIGHT, BLUE_DARK, (0.3, 0.2), 0.8)
    # Console centred at the top: body spans 108 design units -> 162 px.
    k = 1.5
    draw_vita(Pen(img, k * S, (w / 2 - 64 * k) * S, (-40 * k + 10) * S))
    text(img, (w / 2, 140), "Syncthing", FONT_BOLD, 26, WHITE, anchor="ms")
    save_indexed(img, (w, h), SCE_SYS / "livearea/contents/startup.png")


def main():
    make_icon()
    make_bg()
    make_startup()


if __name__ == "__main__":
    main()
