#!/usr/bin/env python3
"""Paints assets/icon.png (phones) and assets/icon.ico (the Windows .exe), the
Cabin Euchre app icon.

    python tools/make_icon.py [output.png]

Needs Pillow. Everything is painted on a 128x128 texel grid with the colors from
src/palette.rs, the same way src/cards.rs paints the cards, then scaled up 8x.
The cards are tilted after scaling, so texels stay square and edges stay smooth.

Android shows the icon in the middle 66% of its adaptive icon and fills the rest
with the corner color, so the outer ring has to stay one flat color. iOS masks the
corners itself and refuses transparency, so the square is always fully opaque.
"""
import math
import sys

from PIL import Image

GRID = 128
TEXEL = 8
SIZE = GRID * TEXEL
SUPERSAMPLE = 2
# Everything Windows asks for, from the title bar and Explorer lists up to large tiles.
ICO_SIZES = [(n, n) for n in (16, 20, 24, 32, 40, 48, 64, 128, 256)]


def hex_rgb(value):
    return (value >> 16 & 255, value >> 8 & 255, value & 255, 255)


# src/palette.rs
WOOD_DARK = hex_rgb(0x24140d)
WOOD = hex_rgb(0x55321f)
WOOD_LIGHT = hex_rgb(0x7a4b2e)
FELT_DEEP = hex_rgb(0x0b2830)
FELT = hex_rgb(0x175260)
FELT_GLOW = hex_rgb(0x2b7d84)
LAMP = hex_rgb(0xffd49a)
INK = hex_rgb(0x1b110c)
BRASS_DARK = hex_rgb(0x7f5520)
CREAM = hex_rgb(0xfbefd8)
CREAM_SHADE = hex_rgb(0xe4cfab)
GOLD = hex_rgb(0xf2bf4f)
GREEN_DARK = hex_rgb(0x255433)
BACK = hex_rgb(0xb9343c)
BACK_DARK = hex_rgb(0x7a1d26)
BACK_BLACK = hex_rgb(0x2a1215)

# The font's J (src/text.rs) and the hand-drawn 13 px spade (src/suits.rs).
J = ["....#", "....#", "....#", "....#", "#...#", "#...#", ".###."]
SPADE = [
    "......#......",
    ".....###.....",
    "....#####....",
    "...#######...",
    "..#########..",
    ".###########.",
    "#############",
    "#############",
    "#############",
    ".####.#.####.",
    "......#......",
    ".....###.....",
    "...#######...",
]
BAYER = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]


class Canvas:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.image = Image.new("RGBA", (w, h), (0, 0, 0, 0))
        self.pixels = self.image.load()

    def px(self, x, y, c):
        if 0 <= x < self.w and 0 <= y < self.h:
            self.pixels[x, y] = c

    def rect(self, x, y, w, h, c):
        for yy in range(y, y + h):
            for xx in range(x, x + w):
                self.px(xx, yy, c)

    def rows(self, x, y, rows, k, c, flip=False):
        """Stamps a `#` mask at k texels per mask pixel. `flip` turns it 180 degrees
        around the canvas, for a card's mirrored corner."""
        for j, row in enumerate(rows):
            for i, ch in enumerate(row):
                if ch != "#":
                    continue
                for dy in range(k):
                    for dx in range(k):
                        xx, yy = x + i * k + dx, y + j * k + dy
                        if flip:
                            xx, yy = self.w - 1 - xx, self.h - 1 - yy
                        self.px(xx, yy, c)


def card_base(w, h, fill, shade):
    """A rounded card with a dark outline and a shaded edge, as in cards.rs."""
    card = Canvas(w, h)
    r = 5.0

    def inside(x, y, inset):
        fx, fy = x + 0.5, y + 0.5
        cx, cy = min(max(fx, r), w - r), min(max(fy, r), h - r)
        near = math.hypot(fx - cx, fy - cy) <= r - inset
        return near and inset <= fx <= w - inset and inset <= fy <= h - inset

    for y in range(h):
        for x in range(w):
            if not inside(x, y, 0):
                continue
            if not inside(x, y, 2):
                c = INK
            elif y >= h - 5 or x >= w - 4:
                c = shade
            else:
                c = fill
            card.px(x, y, c)
    return card


def plaid_back(w, h):
    """The card back's buffalo check, at a larger block so it survives small sizes."""
    card = card_base(w, h, BACK, BACK_DARK)
    block = 13
    for y in range(3, h - 3):
        for x in range(3, w - 3):
            bx = (x - 3) // block % 2 == 1
            by = (y - 3) // block % 2 == 1
            if bx and by:
                c = BACK_DARK if (x + y) % 4 == 0 else BACK_BLACK
            elif bx or by:
                c = BACK_BLACK if (x + y) % 2 == 0 else BACK_DARK
            else:
                c = BACK
            card.px(x, y, c)
    return card


def pine(c, x, base, half, height):
    """A tiered pine with its trunk's foot at (x, base)."""
    c.rect(x - 1, base - 3, 3, 3, WOOD_DARK)
    tiers = 3
    top = base - 3 - height
    for t in range(tiers):
        tier_top = top + t * height // (tiers + 1)
        tier_h = height // tiers + 3
        tier_half = half * (t + 2) // (tiers + 1)
        for j in range(tier_h):
            w = 1 + (tier_half * j) // max(tier_h - 1, 1)
            y = tier_top + j
            if y >= base - 3:
                break
            c.rect(x - w, y, 2 * w + 1, 1, GREEN_DARK)


def cabin_scene(w, h):
    """The picture on the card: a snowy log cabin with its lamp on, at night."""
    s = Canvas(w, h)
    # The sky brightens toward the horizon in ordered-dither bands, like the table.
    for y in range(h):
        for x in range(w):
            t = y / (h * 0.8) * 16
            s.px(x, y, FELT if BAYER[y % 4][x % 4] < t else FELT_DEEP)
    for x, y in [(7, 5), (17, 12), (27, 4), (45, 6), (58, 13), (53, 3), (36, 11)]:
        s.px(x, y, CREAM)

    ground = h - 8
    cx = w // 2
    # Pines behind the cabin.
    pine(s, 8, ground + 1, 11, 31)
    pine(s, w - 9, ground + 1, 11, 37)

    # Chimney and smoke.
    s.rect(cx + 11, ground - 37, 7, 13, WOOD_DARK)
    s.rect(cx + 12, ground - 36, 5, 12, WOOD)
    s.rect(cx + 10, ground - 39, 9, 3, CREAM)
    for x, y in [(cx + 15, ground - 43), (cx + 17, ground - 47)]:
        s.rect(x, y, 2, 2, CREAM_SHADE)

    # Log walls.
    wall_w, wall_h = 42, 21
    left, top = cx - wall_w // 2, ground - wall_h
    s.rect(left - 1, top, wall_w + 2, wall_h + 1, INK)
    for row in range(wall_h):
        band = row % 4
        color = WOOD_LIGHT if band in (0, 1) else WOOD if band == 2 else WOOD_DARK
        s.rect(left, top + row, wall_w, 1, color)

    # Roof: a chunky gable with a thick cap of snow.
    roof_top = top - 21
    roof_half = wall_w // 2 + 6
    for j in range(22):
        half = roof_half * (j + 1) // 22
        y = roof_top + j
        s.rect(cx - half - 1, y, 2 * half + 2, 1, INK)
        if j >= 2:
            s.rect(cx - half + 2, y, 2 * half - 4, 1, WOOD if j % 3 else WOOD_DARK)
    for j in range(22):
        half = roof_half * (j + 1) // 22
        y = roof_top + j
        cap = 3 + (j * 3 // 22)
        if half > 1:
            s.rect(cx - half, y, cap, 1, CREAM)
            s.rect(cx + half - cap, y, cap, 1, CREAM)
        if j < 6:
            s.rect(cx - half, y, 2 * half, 1, CREAM)

    # The door and the lit window are what you see first.
    dx, dy = cx - 16, ground - 16
    s.rect(dx - 1, dy - 1, 11, 17, INK)
    s.rect(dx, dy, 9, 16, WOOD_DARK)
    s.rect(dx + 1, dy + 1, 7, 15, BACK_DARK)
    s.rect(dx + 6, dy + 8, 1, 2, GOLD)

    wx, wy = cx + 2, ground - 16
    s.rect(wx - 2, wy - 2, 17, 15, INK)
    s.rect(wx - 1, wy - 1, 15, 13, BRASS_DARK)
    s.rect(wx, wy, 13, 11, GOLD)
    s.rect(wx + 1, wy + 1, 11, 9, LAMP)
    s.rect(wx + 6, wy, 1, 11, BRASS_DARK)
    s.rect(wx, wy + 5, 13, 1, BRASS_DARK)

    # Snow on the ground, with the window's light on it.
    s.rect(0, ground, w, h - ground, CREAM)
    s.rect(0, ground, w, 1, CREAM_SHADE)
    s.rect(wx - 1, ground + 1, 15, 2, LAMP)
    s.rect(wx + 1, ground + 3, 11, 2, LAMP)
    return s


def face_card(w, h):
    """The front card: the jack's corners around a picture of the cabin."""
    card = card_base(w, h, CREAM, CREAM_SHADE)
    for flip in (False, True):
        card.rows(7, 7, J, 2, INK, flip)
        card.rows(21, 7, SPADE, 1, INK, flip)
    frame_top, frame_bottom = 23, h - 23
    frame_left, frame_w = 6, w - 12
    card.rect(frame_left, frame_top, frame_w, frame_bottom - frame_top, INK)
    scene = cabin_scene(frame_w - 2, frame_bottom - frame_top - 2)
    card.image.alpha_composite(scene.image, (frame_left + 1, frame_top + 1))
    return card


def background():
    """Felt under a lamp: a pool of light in two bands with dithered edges. It ends
    well before the icon's edge, which stays one flat color."""
    bg = Canvas(GRID, GRID)
    cx, cy = GRID / 2, GRID / 2 - 2

    def lit(x, y, radius):
        d = math.hypot(x + 0.5 - cx, y + 0.5 - cy)
        return (radius - d) / 8 + 0.5 > BAYER[y % 4][x % 4] / 16

    for y in range(GRID):
        for x in range(GRID):
            bg.px(x, y, FELT_GLOW if lit(x, y, 38) else FELT if lit(x, y, 54) else FELT_DEEP)
    return bg


def place(target, card, angle, cx, cy):
    """Scales a texel canvas up, tilts it and composites it with a drop shadow,
    centered on (cx, cy) in texels."""
    k = TEXEL * SUPERSAMPLE
    big = card.image.resize((card.w * k, card.h * k), Image.NEAREST)
    big = big.rotate(angle, resample=Image.BICUBIC, expand=True)
    x, y = round(cx * k - big.width / 2), round(cy * k - big.height / 2)
    dark = Image.new("RGBA", big.size, INK)
    dark.putalpha(big.getchannel("A").point(lambda a: int(a * 0.55)))
    target.alpha_composite(dark, (x + 2 * k, y + 3 * k))
    target.alpha_composite(big, (x, y))


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "assets/icon.png"
    k = TEXEL * SUPERSAMPLE
    art = background().image.resize((GRID * k, GRID * k), Image.NEAREST)
    place(art, plaid_back(68, 96), 16, 49, 64)
    place(art, face_card(80, 108), -7, 76, 62)
    art = art.resize((SIZE, SIZE), Image.LANCZOS).convert("RGB")
    art.save(out, optimize=True)
    art.save(out.rsplit(".", 1)[0] + ".ico", sizes=ICO_SIZES)


if __name__ == "__main__":
    main()
