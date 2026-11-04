#!/usr/bin/env python3
"""Paints the store's trophy images into store/trophies/ (python tools/make_trophies.py, needs Pillow).

Each is 1024x1024 RGB with no transparency, the size App Store Connect asks for. They're chunky pixel art
in the game's colors: a brass medallion on walnut with a felt center. Game Center shows them in a
circle, so everything that matters stays inside the medallion. The ids come from docs/achievements.md.
"""

import math
import re
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "store" / "trophies"
GRID = 64
SCALE = 16
C = GRID / 2 - 0.5


def rgb(h):
    return ((h >> 16) & 255, (h >> 8) & 255, h & 255, 255)


# src/palette.rs
WOOD_DARK, WOOD, WOOD_LIGHT = rgb(0x24140D), rgb(0x55321F), rgb(0x7A4B2E)
FELT_DEEP, FELT, FELT_GLOW = rgb(0x0B2830), rgb(0x175260), rgb(0x2B7D84)
INK, LAMP = rgb(0x1B110C), rgb(0xFFD49A)
BRASS, BRASS_DARK, BRASS_LIGHT = rgb(0xD9A441), rgb(0x7F5520), rgb(0xF7D887)
CREAM, CREAM_SHADE = rgb(0xFBEFD8), rgb(0xE4CFAB)
PAPER, PAPER_SHADE, PAPER_RULE = rgb(0xF4E8C8), rgb(0xDCCB9F), rgb(0xA9BFCC)
RED, RED_DARK, GOLD = rgb(0xD0464F), rgb(0x7C2129), rgb(0xF2BF4F)
# Only these images use them: a pine and a night lake.
PINE, PINE_LIGHT = rgb(0x2F6B45), rgb(0x4F9A63)
LAKE, LAKE_LIGHT = rgb(0x2F7FB0), rgb(0x7CC0DC)
STEEL, STEEL_DARK = rgb(0xC7CDD2), rgb(0x7C858C)
BLACK = rgb(0x2A211D)

GLYPHS = {
    "0": ["01110", "10001", "10011", "10101", "11001", "10001", "01110"],
    "1": ["00100", "01100", "00100", "00100", "00100", "00100", "01110"],
    "2": ["01110", "10001", "00001", "00110", "01000", "10000", "11111"],
    "3": ["11110", "00001", "00001", "01110", "00001", "00001", "11110"],
    "5": ["11111", "10000", "11110", "00001", "00001", "10001", "01110"],
    "J": ["00111", "00010", "00010", "00010", "00010", "10010", "01100"],
}
SUITS = {
    "heart": ["0110110", "1111111", "1111111", "1111111", "0111110", "0011100", "0001000"],
    "diamond": ["0001000", "0011100", "0111110", "1111111", "0111110", "0011100", "0001000"],
    "club": ["0011100", "0011100", "1101011", "1111111", "1101011", "0001000", "0011100"],
    "spade": ["0001000", "0011100", "0111110", "1111111", "1101011", "0001000", "0011100"],
}


class Art:
    def __init__(self):
        self.layer = Image.new("RGBA", (GRID, GRID), (0, 0, 0, 0))
        self.d = ImageDraw.Draw(self.layer)

    def poly(self, pts, c):
        self.d.polygon([(round(x), round(y)) for x, y in pts], fill=c)

    def rect(self, x0, y0, x1, y1, c):
        self.d.rectangle([x0, y0, x1, y1], fill=c)

    def disc(self, x, y, r, c):
        self.d.ellipse([x - r, y - r, x + r, y + r], fill=c)

    def line(self, pts, c, w=1):
        self.d.line(pts, fill=c, width=w)

    def px(self, x, y, c):
        self.d.point((x, y), fill=c)

    def mask(self, rows, x, y, c, s=1):
        for j, row in enumerate(rows):
            for i, bit in enumerate(row.replace(" ", "")):
                if bit == "1":
                    self.rect(x + i * s, y + j * s, x + i * s + s - 1, y + j * s + s - 1, c)

    def text(self, s, cx, y, c, scale=1):
        w = (len(s) * 6 - 1) * scale
        x = round(cx - w / 2 + 0.5)
        for ch in s:
            self.mask(GLYPHS[ch], x, y, c, scale)
            x += 6 * scale

    def suit(self, name, x, y, s=1):
        self.mask(SUITS[name], x, y, RED if name in ("heart", "diamond") else BLACK, s)

    def card(self, x0, y0, x1, y1, angle=0.0):
        """A card as a polygon, turned about its center. Returns a point mapper for what's on it."""
        cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
        a = math.radians(angle)

        def turn(x, y):
            dx, dy = x - cx, y - cy
            return (cx + dx * math.cos(a) - dy * math.sin(a), cy + dx * math.sin(a) + dy * math.cos(a))

        corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
        self.poly([turn(*p) for p in corners], CREAM)
        return turn


def outlined(layer):
    """A hard ink outline and drop shadow, like everything in the game."""
    src = layer.load()
    out = layer.copy()
    dst = out.load()
    for y in range(GRID):
        for x in range(GRID):
            if src[x, y][3] == 0:
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    nx, ny = x + dx, y + dy
                    if 0 <= nx < GRID and 0 <= ny < GRID and src[nx, ny][3] > 0:
                        dst[x, y] = INK
                        break
    shadow = Image.new("RGBA", (GRID, GRID), (0, 0, 0, 0))
    pixels = shadow.load()
    drawn = out.load()
    for y in range(GRID - 1):
        for x in range(GRID - 1):
            if drawn[x, y][3] > 0:
                pixels[x + 1, y + 1] = (11, 30, 36, 255)
    shadow.alpha_composite(out)
    return shadow


BAYER = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]


def medallion():
    """Walnut planks, a brass ring and felt lit by the lamp from above, in dithered bands."""
    img = Image.new("RGBA", (GRID, GRID), WOOD)
    p = img.load()
    for y in range(GRID):
        band = y // 11
        for x in range(GRID):
            grain = ((x * 7 + band * 13) // 9 + band) % 5 == 0 and (x + y * 3) % 4 != 0
            p[x, y] = WOOD_LIGHT if grain and band % 2 == 0 else WOOD
            if y % 11 == 10:
                p[x, y] = WOOD_DARK
            if (x + band * 23) % 37 == 0:
                p[x, y] = WOOD_DARK
    for y in range(GRID):
        for x in range(GRID):
            r = math.hypot(x - C, y - C)
            if r > 31:
                continue
            if r > 30:
                p[x, y] = INK
            elif r > 26:
                # Lit from the top left.
                light = ((x - C) * -0.7 + (y - C) * -0.7) / r
                p[x, y] = BRASS_LIGHT if light > 0.55 else BRASS_DARK if light < -0.45 else BRASS
            elif r > 25:
                p[x, y] = INK
            else:
                glow = 1.0 - math.hypot(x - C, y - (C - 8)) / 30
                t = glow * 2.1 + (BAYER[y % 4][x % 4] / 16 - 0.5) * 0.45
                p[x, y] = FELT_GLOW if t > 1.25 else FELT if t > 0.3 else FELT_DEEP
    # Four studs in the ring.
    for a in (45, 135, 225, 315):
        x = round(C + 28 * math.cos(math.radians(a)))
        y = round(C + 28 * math.sin(math.radians(a)))
        p[x, y] = INK
        p[x - 1, y - 1] = BRASS_LIGHT
    return img


def stars(a, spots, c=BRASS_LIGHT):
    for x, y in spots:
        a.px(x, y, c)
        a.px(x + 1, y, c)
        a.px(x - 1, y, c)
        a.px(x, y + 1, c)
        a.px(x, y - 1, c)


def star_points(cx, cy, r, inner=0.42, turn=-90):
    pts = []
    for i in range(10):
        rad = r if i % 2 == 0 else r * inner
        t = math.radians(turn + i * 36)
        pts.append((cx + rad * math.cos(t), cy + rad * math.sin(t)))
    return pts


# Each paints inside a radius of about 23 from the middle.


def first_game(a):  # a chair pulled up
    a.rect(23, 14, 26, 48, WOOD_LIGHT)
    a.rect(37, 14, 40, 48, WOOD_LIGHT)
    a.rect(23, 14, 40, 18, BRASS)
    a.rect(23, 23, 40, 25, BRASS_DARK)
    a.poly([(19, 32), (44, 32), (47, 38), (16, 38)], BRASS)
    a.rect(16, 38, 47, 39, BRASS_DARK)
    a.rect(18, 40, 21, 50, WOOD_LIGHT)
    a.rect(42, 40, 45, 50, WOOD_LIGHT)


def first_win(a):  # the cabin, smoke going
    a.rect(38, 15, 42, 28, RED_DARK)
    a.disc(44, 12, 2, CREAM_SHADE)
    a.disc(40, 10, 1, CREAM)
    a.poly([(12, 31), (32, 15), (52, 31)], RED)
    a.rect(16, 31, 47, 47, WOOD_LIGHT)
    for y in (34, 38, 42, 46):
        a.rect(16, y, 47, y, WOOD)
    a.rect(28, 36, 35, 47, BRASS_DARK)
    a.px(34, 42, BRASS_LIGHT)
    a.rect(18, 35, 24, 41, LAMP)
    a.rect(39, 35, 45, 41, LAMP)
    a.rect(21, 35, 21, 41, INK)
    a.rect(42, 35, 42, 41, INK)


def tutorial(a):  # a gold star on Benny's note
    a.poly([(16, 13), (47, 13), (47, 50), (16, 50)], PAPER)
    for y in (20, 26, 32, 38, 44):
        a.rect(17, y, 46, y, PAPER_RULE)
    a.rect(20, 13, 20, 49, PAPER_SHADE)
    a.poly(star_points(32, 32, 15), GOLD)
    a.poly(star_points(31, 31, 7, 0.5), BRASS_LIGHT)


def euchre(a):  # their card, crossed out
    a.card(19, 11, 44, 51)
    a.suit("club", 25, 24, 2)
    a.line([(16, 14), (47, 48)], RED, 5)
    a.line([(47, 14), (16, 48)], RED, 5)


def march(a):  # all five tricks
    for i, angle in enumerate((-44, -22, 0, 22, 44)):
        t = math.radians(angle)
        cx, cy = 32 + 17 * math.sin(t), 36 - 15 * math.cos(t) + 4
        turn = a.card(cx - 7, cy - 11, cx + 7, cy + 11, angle)
        x, y = turn(cx, cy)
        a.suit(("spade", "heart", "club", "diamond", "spade")[i], round(x) - 3, round(y) - 7)
        lines = [turn(cx - 7, cy + 11), turn(cx + 7, cy + 11)]
        a.line(lines, CREAM_SHADE, 1)


def wing(a, side):
    # Brass and feathered along the bottom, so it can't be taken for a sleeve.
    x = lambda dx: 31.5 + side * dx
    feathers = [(8, 25), (21, 13), (22, 21), (19, 20), (20, 28), (16, 26), (16, 33), (12, 31), (11, 38), (8, 36)]
    a.poly([(x(dx), y) for dx, y in feathers], BRASS)
    a.line([(x(9), 26), (x(19), 16)], BRASS_LIGHT)
    a.line([(x(9), 29), (x(17), 21)], BRASS_DARK)
    a.line([(x(9), 33), (x(14), 27)], BRASS_DARK)


def lone_made(a):  # one card, flying
    wing(a, -1)
    wing(a, 1)
    a.card(24, 17, 39, 50)
    a.text("J", 32, 20, BLACK, 2)
    a.suit("spade", 28, 36)
    a.rect(24, 50, 39, 50, CREAM_SHADE)


def lone_march(a):  # a loon, alone on the lake
    a.poly([(10, 40), (53, 40), (47, 50), (16, 50)], LAKE)
    a.line([(16, 45), (24, 45)], LAKE_LIGHT)
    a.line([(34, 47), (43, 47)], LAKE_LIGHT)
    a.poly([(13, 37), (21, 28), (40, 27), (44, 34), (41, 41), (20, 41)], BLACK)
    a.disc(43, 21, 5, BLACK)
    a.rect(40, 23, 46, 31, BLACK)
    a.poly([(47, 20), (56, 22), (47, 24)], STEEL_DARK)
    a.rect(44, 19, 45, 20, RED)
    a.rect(41, 28, 45, 29, CREAM)
    for x, y in ((22, 31), (26, 33), (30, 30), (34, 33), (37, 30), (24, 36), (28, 37), (33, 37)):
        a.rect(x, y, x + 1, y, CREAM)


def stop_lone(a):  # stop right there
    r, q = 20, 8.3
    a.poly(
        [(32 - q, 32 - r), (32 + q, 32 - r), (32 + r, 32 - q), (32 + r, 32 + q), (32 + q, 32 + r), (32 - q, 32 + r), (32 - r, 32 + q), (32 - r, 32 - q)],
        RED,
    )
    a.poly(
        [(32 - q + 1, 32 - r + 2), (32 + q - 1, 32 - r + 2), (32 + r - 2, 32 - q + 1), (32 + r - 2, 32 + q - 1), (32 + q - 1, 32 + r - 2), (32 - q + 1, 32 + r - 2), (32 - r + 2, 32 + q - 1), (32 - r + 2, 32 - q + 1)],
        RED_DARK,
    )
    a.poly(
        [(32 - q + 1, 32 - r + 3), (32 + q - 1, 32 - r + 3), (32 + r - 3, 32 - q + 1), (32 + r - 3, 32 + q - 1), (32 + q - 1, 32 + r - 3), (32 - q + 1, 32 + r - 3), (32 - r + 3, 32 + q - 1), (32 - r + 3, 32 - q + 1)],
        RED,
    )
    a.rect(20, 28, 43, 35, CREAM)
    a.rect(20, 35, 43, 35, CREAM_SHADE)


def person(a, x, y, shirt):
    a.disc(x, y - 7, 3, CREAM_SHADE)
    a.rect(x - 3, y - 3, x + 3, y + 5, shirt)


def partner_ride(a):  # the canoe: one paddles, one rides
    a.poly([(10, 44), (53, 44), (46, 51), (17, 51)], LAKE)
    a.line([(14, 49), (22, 49)], LAKE_LIGHT)
    a.line([(40, 49), (48, 49)], LAKE_LIGHT)
    person(a, 24, 30, RED)
    person(a, 41, 30, BRASS)
    a.poly([(9, 34), (14, 38), (50, 38), (55, 34), (51, 46), (13, 46)], RED_DARK)
    a.rect(14, 38, 50, 39, BRASS)
    # The paddle goes over the side, in front of the hull. It's on top of the canoe here, so it
    # needs its own ink: the outline pass only draws where the art meets the felt.
    a.line([(48, 19), (38, 45)], INK, 4)
    a.poly([(34, 42), (41, 45), (38, 52), (32, 49)], INK)
    a.line([(47, 19), (37, 45)], WOOD_LIGHT, 2)
    a.poly([(35, 43), (40, 45), (37, 50), (33, 48)], WOOD_LIGHT)


def stuck(a):  # a card nailed down
    a.card(20, 12, 43, 52, 8)
    a.suit("heart", 25, 29, 2)
    a.rect(26, 10, 37, 13, STEEL)
    a.rect(26, 13, 37, 14, STEEL_DARK)
    a.poly([(29, 15), (34, 15), (34, 30), (32, 37), (31, 37), (29, 30)], STEEL)
    a.rect(33, 15, 34, 29, STEEL_DARK)


def both_bowers(a):  # the right and the left
    left = a.card(12, 15, 35, 49, -13)
    x, y = left(23, 26)
    a.text("J", x, round(y) - 7, BLACK, 2)
    x, y = left(23, 40)
    a.suit("club", round(x) - 3, round(y) - 3)
    right = a.card(29, 15, 52, 49, 13)
    x, y = right(40, 26)
    a.text("J", x, round(y) - 7, BLACK, 2)
    x, y = right(40, 40)
    a.suit("spade", round(x) - 3, round(y) - 3)


def all_suits(a):  # one of each
    for name, x, y in (("spade", 12, 12), ("heart", 33, 12), ("club", 12, 33), ("diamond", 33, 33)):
        a.rect(x, y, x + 18, y + 18, CREAM)
        a.rect(x, y + 18, x + 18, y + 18, CREAM_SHADE)
        a.suit(name, x + 2, y + 2, 2)


def shutout(a):  # a skunk
    a.poly([(30, 34), (20, 34), (13, 24), (12, 14), (19, 11), (26, 17), (28, 26)], BLACK)
    a.poly([(15, 16), (19, 13), (23, 19), (24, 28), (20, 25)], CREAM)
    a.poly([(21, 35), (28, 28), (42, 28), (49, 34), (49, 44), (22, 44)], BLACK)
    a.poly([(25, 33), (30, 29), (41, 29), (47, 34), (41, 33), (31, 34)], CREAM)
    a.disc(49, 36, 5, BLACK)
    a.poly([(52, 37), (57, 40), (52, 42)], BLACK)
    a.px(57, 40, RED)
    a.px(50, 35, CREAM)
    a.rect(47, 29, 49, 32, BLACK)
    a.rect(25, 44, 28, 49, BLACK)
    a.rect(42, 44, 45, 49, BLACK)
    a.rect(31, 44, 34, 48, BLACK)


def comeback(a):  # down, then all the way up
    a.line([(11, 22), (20, 39), (27, 31), (36, 42)], STEEL_DARK, 3)
    a.line([(36, 42), (44, 27)], GOLD, 5)
    # The head points along the shaft, so it reads as an arrow.
    tip, base = (51, 12), (42.5, 29)
    length = math.hypot(tip[0] - base[0], tip[1] - base[1])
    nx, ny = -(tip[1] - base[1]) / length, (tip[0] - base[0]) / length
    a.poly([tip, (base[0] + nx * 10, base[1] + ny * 10), (base[0] - nx * 10, base[1] - ny * 10)], GOLD)
    a.poly([(tip[0] - 2, tip[1] + 5), (base[0] + nx * 4 + 1, base[1] + ny * 4 - 2), (base[0] - nx * 4 + 1, base[1] - ny * 4 - 2)], BRASS_LIGHT)


def clean(a):  # a clean score sheet
    a.poly([(16, 11), (47, 11), (47, 52), (16, 52)], PAPER)
    a.rect(16, 11, 47, 15, PAPER_SHADE)
    for y in (20, 26, 32, 38, 44, 50):
        a.rect(17, y, 46, y, PAPER_RULE)
    a.line([(22, 33), (29, 42), (42, 20)], PINE, 5)


def tear(a, cx, top, bottom, w, c, lean=0.0):
    """A drop of flame: a point at the top, round at the bottom, its tip leaning over."""
    left, right = [], []
    for i in range(25):
        t = i / 24
        half = w * t**0.8 * math.sqrt(max(0.0, 1 - t**5))
        x = cx + lean * (1 - t) ** 2
        y = top + (bottom - top) * t
        left.append((x - half, y))
        right.append((x + half, y))
    a.poly(left + right[::-1], c)


def flame(a, cx, top, bottom, w):
    tall = bottom - top
    tear(a, cx - w * 0.62, top + tall * 0.34, bottom, w * 0.5, RED, -3)
    tear(a, cx + w * 0.62, top + tall * 0.22, bottom, w * 0.52, RED, 3)
    tear(a, cx, top, bottom, w * 0.85, RED, 2)
    tear(a, cx, top + tall * 0.3, bottom, w * 0.6, BRASS, -2)
    tear(a, cx, top + tall * 0.5, bottom, w * 0.42, BRASS_LIGHT, 1)


def streak_3(a):
    flame(a, 32, 12, 51, 15)
    a.text("3", 32, 33, BLACK, 2)


def streak_5(a):  # hotter
    flame(a, 32, 8, 53, 20)
    for x, y in ((14, 22), (50, 18), (53, 33), (11, 36)):
        a.rect(x, y, x + 1, y + 1, BRASS_LIGHT)
    a.text("5", 32, 33, BLACK, 2)


def cup(a, body, light, dark):
    a.line([(16, 18), (11, 18), (11, 26), (19, 32)], dark, 3)
    a.line([(47, 18), (52, 18), (52, 26), (44, 32)], dark, 3)
    a.poly([(16, 12), (47, 12), (45, 30), (37, 39), (26, 39), (18, 30)], body)
    a.rect(20, 13, 23, 28, light)
    a.rect(16, 12, 47, 13, light)
    a.rect(29, 39, 34, 44, dark)
    a.rect(22, 44, 41, 47, body)
    a.rect(22, 47, 41, 49, dark)


def wins_10(a):
    cup(a, STEEL, CREAM, STEEL_DARK)
    a.text("10", 32, 18, BLACK, 2)


def wins_50(a):  # a lodge's best
    cup(a, GOLD, BRASS_LIGHT, BRASS_DARK)
    a.text("50", 32, 18, BLACK, 2)
    stars(a, ((10, 39), (53, 39), (32, 7)))


def euchre_25(a):  # a thorny branch
    a.line([(14, 49), (49, 15)], PINE, 4)
    a.poly([(52, 12), (42, 14), (49, 22)], PINE_LIGHT)
    for t in (0.2, 0.42, 0.64):
        x, y = 14 + 35 * t, 49 - 34 * t
        a.poly([(x - 2, y - 3), (x - 9, y - 9), (x + 1, y - 5)], RED)
    for t in (0.1, 0.32, 0.54):
        x, y = 14 + 35 * t, 49 - 34 * t
        a.poly([(x + 3, y + 1), (x + 10, y + 5), (x + 4, y + 4)], RED)
    a.rect(27, 35, 51, 51, PAPER)
    a.rect(27, 51, 51, 51, PAPER_SHADE)
    a.text("25", 39, 37, BLACK, 2)


def hands_100(a):  # the moon's well up
    a.disc(30, 22, 12, CREAM)
    a.d.ellipse([29, 7, 46, 27], fill=(0, 0, 0, 0))
    a.rect(14, 37, 49, 51, PAPER)
    a.rect(14, 51, 49, 51, PAPER_SHADE)
    a.text("100", 32, 40, BLACK, 1)
    stars(a, ((42, 14), (48, 25), (37, 8)))


def hands_1000(a):  # a whole summer at the lake
    a.disc(32, 30, 12, GOLD)
    a.disc(30, 28, 6, BRASS_LIGHT)
    for t in range(-80, 81, 40):
        r = math.radians(t - 90)
        a.line([(32 + 15 * math.cos(r), 30 + 15 * math.sin(r)), (32 + 20 * math.cos(r), 30 + 20 * math.sin(r))], GOLD, 2)
    a.poly([(9, 34), (54, 34), (51, 44), (12, 44)], LAKE)
    a.rect(12, 37, 22, 37, LAKE_LIGHT)
    a.rect(38, 40, 49, 40, LAKE_LIGHT)
    a.rect(26, 36, 37, 36, BRASS_LIGHT)
    a.rect(16, 44, 47, 53, PAPER)
    a.text("1000", 32, 45, BLACK, 1)


def online_win(a):  # calling long distance
    for r, c in ((10, BRASS_LIGHT), (16, BRASS), (22, BRASS_DARK)):
        a.d.arc([32 - r, 21 - r, 32 + r, 21 + r], 200, 340, fill=c, width=2)
    a.disc(32, 21, 3, RED)
    a.poly([(31, 24), (33, 24), (43, 52), (39, 52), (32, 33), (25, 52), (21, 52)], STEEL)
    a.line([(28, 38), (36, 38)], STEEL_DARK)
    a.line([(26, 44), (38, 44)], STEEL_DARK)


def full_table(a):  # someone in every chair
    a.poly([(20, 20), (43, 20), (43, 43), (20, 43)], WOOD_LIGHT)
    a.rect(23, 23, 40, 40, FELT_GLOW)
    a.rect(29, 26, 34, 37, CREAM)
    for x, y, c in ((32, 12, RED), (32, 51, BRASS), (12, 32, LAKE_LIGHT), (51, 32, PINE_LIGHT)):
        a.disc(x, y, 6, c)
        a.disc(x, y, 3, CREAM_SHADE)


def listed_ids():
    text = (ROOT / "docs" / "achievements.md").read_text(encoding="utf8")
    return re.findall(r"^\| `(\w+)` \|", text, re.M)


def main():
    painters = {name: fn for name, fn in globals().items() if callable(fn) and name in listed_ids()}
    missing = set(listed_ids()) - set(painters)
    assert not missing, f"no painting for {sorted(missing)}"
    OUT.mkdir(parents=True, exist_ok=True)
    for name in listed_ids():
        art = Art()
        painters[name](art)
        img = medallion()
        icon = outlined(art.layer)
        # Nothing may stray past the felt onto the ring.
        pixels = icon.load()
        for y in range(GRID):
            for x in range(GRID):
                if math.hypot(x - C, y - C) > 25:
                    pixels[x, y] = (0, 0, 0, 0)
        img.alpha_composite(icon)
        img = img.convert("RGB").resize((GRID * SCALE, GRID * SCALE), Image.NEAREST)
        img.save(OUT / f"{name}.png", dpi=(72, 72), optimize=True)
    print(f"painted {len(painters)} trophies into {OUT}")


if __name__ == "__main__":
    main()
