//! Procedural pixel-art playing cards. Every face and the back are painted
//! into one atlas at startup (no image files), at 1.5 texels per design pixel
//! (`render::DENSITY`). The atlas is linear-filtered for the anti-aliasing
//! pixel shader, and every sprite's quad includes a 1-texel transparent
//! border (see [`BORDER`]) so its outline is smoothed too.
//!
//! Suit pips come from `suits::mask`, rank labels from the pixel font (drawn
//! 2x2 like UI text) and the court portraits from the sprites below (2x2).
//! Cards are `TEX_W x TEX_H` texels, or `CARD_W x CARD_H` design pixels.

use love2drs::prelude::*;

use crate::euchre::{Card, Rank, Suit};
use crate::palette;
use crate::render::DENSITY;
use crate::suits;
use crate::text;

/// Card size in atlas texels. Odd, so the art has a center pixel.
const TEX_W: u32 = 67;
const TEX_H: u32 = 91;
/// Card size in design pixels.
pub const CARD_W: f32 = TEX_W as f32 / DENSITY;
pub const CARD_H: f32 = TEX_H as f32 / DENSITY;
/// Transparent texels between sprites in the atlas (and around its edge).
const PAD: u32 = 3;
/// Transparent texels around each sprite that are included in its quad.
pub const BORDER: f32 = 1.0;
const COLS: u32 = 5;
/// UI suit icon sizes in texels, for draw scales 1, 2 and 3.
const ICON_SIZES: [u32; 3] = [19, 39, 58];

// Court portraits, 15x20 (only the top 14 rows, the bust, are used).
// k ink, s skin, g gold, w white, h hair, r red, c the suit's clothing color,
// '.' shows the frame background.
const KING: &[&str] = &[
    "..g....g....g..",
    "..gg..ggg..gg..",
    "..ggggggggggg..",
    "..ggrgggggrgg..",
    "..kkkkkkkkkkk..",
    "..ksssssssssk..",
    "..ksskssskssk..",
    "..ksssssssssk..",
    "..ksssskssssk..",
    "..kwwwssswwwk..",
    "..kwwwkkkwwwk..",
    ".kwwwwwwwwwwwk.",
    ".kkwwwwwwwwwkk.",
    "kcckwwwwwwwkcck",
    "kccckwwwwwkccck",
    "kcccckwwwkcccck",
    "kccccckwkccccck",
    "kcccccgggccccck",
    "kcccccgggccccck",
    "kkkkkkkkkkkkkkk",
];

const QUEEN: &[&str] = &[
    ".....g.g.g.....",
    ".....ggggg.....",
    "....hgggggh....",
    "...hhhhhhhhh...",
    "..hhkkkkkkkhh..",
    "..hkssssssskh..",
    "..hkskssskskh..",
    "..hkssssssskh..",
    "..hkssrrrsskh..",
    "..hhkssssskhh..",
    "..hhhkkkkkhhh..",
    ".hhhhkssskhhhh.",
    ".hhhcckkkcchhh.",
    "kcccccwwwccccck",
    "kcccccgwgccccck",
    "kccccccgcccccck",
    "kcwcccccccccwck",
    "kccccccgcccccck",
    "kccccccgcccccck",
    "kkkkkkkkkkkkkkk",
];

const JACK: &[&str] = &[
    "...........w...",
    "..........ww...",
    "....ccccccw....",
    "...cccccccc....",
    "..ggggggggggg..",
    "..hkssssssskh..",
    "..hkskssskskh..",
    "..hkssssssskh..",
    "..hkssskssskh..",
    "..hksskkksskh..",
    "...hkssssskh...",
    "....kkkkkkk....",
    ".....ksssk.....",
    "kccccwwwwwcccck",
    "kcccccwwwccccck",
    "kgcccccccccccgk",
    "kcgcccccccccgck",
    "kccgcccccccgcck",
    "kcccgcccccgccck",
    "kkkkkkkkkkkkkkk",
];

/// Benny's face, 24x24 at 2 texels per cell. `m` is his mouth, which opens a row further
/// while he's talking (see [`paint_sprite`]). k ink, r/R knit cap, c/C its band, s/S skin,
/// h/H beard, b black and r red for his buffalo-check shirt, '.' shows what's behind.
const BENNY: &[&str] = &[
    "........kkkkkkkk........",
    "......kkrrrrrrrrkk......",
    ".....krrrrrrrrrrrrk.....",
    "....krrRrrRrrRrrRrrk....",
    "....krrRrrRrrRrrRrrk....",
    "...kccCccCccCccCccCck...",
    "...kccCccCccCccCccCck...",
    "...kkkkkkkkkkkkkkkkkk...",
    "...khsssssssssssssshk...",
    "...khsssssssssssssshk...",
    "...khsseesssssseesshk...",
    "...khsseesssssseesshk...",
    "...khsssssssSSsssssshk..",
    "...khhsssHHHHHHssshhk...",
    "...khhhhHHHHHHHHhhhhk...",
    "...khhhhhHmmmmHhhhhhk...",
    "...khhhhhhHHHHhhhhhhk...",
    "....khhhhhhhhhhhhhhk....",
    "..kkrbkhhhhhhhhhhkbrkk..",
    ".krrbbrkkhhhhhhkkrbbrrk.",
    "krrbbrrbbkkkkkkbbrrbbrrk",
    "kbbrrbbrrbbrrbbrrbbrrbbk",
    "kbbrrbbrrbbrrbbrrbbrrbbk",
    "kkkkkkkkkkkkkkkkkkkkkkkk",
];

/// The row of [`BENNY`] under his mouth, and what it becomes when the mouth is open.
const BENNY_CHIN: (&str, &str) = ("...khhhhhhHHHHhhhhhhk...", "...khhhhhHmmmmHhhhhhk...");

/// The pointing glove, 14x18 at 2 texels per cell.
const GLOVE: &[&str] = &[
    "....kk........",
    "...kwwk.......",
    "...kwwk.......",
    "...kwwk.......",
    "...kwwkkk.....",
    "...kwwkwwkk...",
    "...kwwkwwkwkk.",
    "kk.kwwwwwkwkwk",
    "kwkkwwwwwwwwwk",
    "kwwkwwwwwwwwwk",
    ".kwwwwwwwwwwwk",
    ".kwwwwwwwwwwCk",
    "..kwwwwwwwwCk.",
    "..kwwwwwwwCCk.",
    "...kwwwwwCCk..",
    "...kBBBBBBBk..",
    "...kDDDDDDDk..",
    "...kkkkkkkkk..",
];

/// Where and how a card is drawn: center position, rotation and scale (in design space).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub x: f32,
    pub y: f32,
    pub rot: f32,
    pub sx: f32,
    pub sy: f32,
}

impl Pose {
    pub fn new(x: f32, y: f32, rot: f32, sx: f32, sy: f32) -> Self {
        Pose { x, y, rot, sx, sy }
    }

    /// The same pose moved by `(dx, dy)`.
    pub fn offset(self, dx: f32, dy: f32) -> Self {
        Pose { x: self.x + dx, y: self.y + dy, ..self }
    }
}

pub struct CardArt {
    image: Image,
    faces: Vec<Quad>,
    back: Quad,
    /// White suit icons (tinted when drawn): `icons[size_index][suit_index]`.
    icons: Vec<Vec<Quad>>,
    coach_image: Image,
    /// Benny's face: mouth shut, then open.
    benny: [Quad; 2],
    glove: Quad,
}

impl CardArt {
    /// Pivot used when drawing a card: its center texel (inside the quad's border).
    const ORIGIN: (f32, f32) = ((TEX_W / 2) as f32 + BORDER, (TEX_H / 2) as f32 + BORDER);

    pub fn new(g: &Graphics) -> Self {
        let rows: u32 = 25u32.div_ceil(COLS);
        let icon_y = PAD + rows * (TEX_H + PAD);
        let icons_h: u32 = ICON_SIZES.iter().map(|s| s + PAD).sum();
        let mut data = ImageData::new(PAD + COLS * (TEX_W + PAD), icon_y + icons_h);
        let cell = |i: u32| {
            let (cx, cy) = (i % COLS, i / COLS);
            ((PAD + cx * (TEX_W + PAD)) as i32, (PAD + cy * (TEX_H + PAD)) as i32)
        };
        let mut faces = Vec::new();
        for id in 0..24 {
            let (x, y) = cell(id as u32);
            paint_face(&mut Painter { d: &mut data, ox: x, oy: y }, Card::from_id(id));
            faces.push(bordered(x as u32, y as u32, TEX_W, TEX_H));
        }
        let (bx, by) = cell(24);
        paint_back(&mut Painter { d: &mut data, ox: bx, oy: by });

        let mut icons = Vec::new();
        let mut y0 = icon_y;
        for &size in &ICON_SIZES {
            let mut row = Vec::new();
            for (i, suit) in Suit::ALL.into_iter().enumerate() {
                let x0 = PAD + i as u32 * (size + PAD);
                for (k, &on) in suits::mask(suit, size).iter().enumerate() {
                    if on {
                        data.set_pixel(x0 + k as u32 % size, y0 + k as u32 / size, Color::WHITE);
                    }
                }
                row.push(bordered(x0, y0, size, size));
            }
            icons.push(row);
            y0 += size + PAD;
        }

        // Benny and the glove get a small atlas of their own, so the cards' sampling
        // stays exactly the same.
        let benny_w = 2 * BENNY[0].len() as u32;
        let benny_h = 2 * BENNY.len() as u32;
        let glove_x = 3 * PAD + 2 * benny_w;
        let (glove_w, glove_h) = (2 * GLOVE[0].len() as u32, 2 * GLOVE.len() as u32);
        let mut coach = ImageData::new(glove_x + glove_w + PAD, 2 * PAD + benny_h.max(glove_h));
        paint_sprite(&mut coach, PAD, PAD, BENNY, false);
        paint_sprite(&mut coach, 2 * PAD + benny_w, PAD, BENNY, true);
        paint_sprite(&mut coach, glove_x, PAD, GLOVE, false);
        let coach_image = g.new_image_from_data(&coach);
        coach_image.set_filter(FilterMode::Linear);
        coach_image.set_wrap(WrapMode::Clamp);

        let image = g.new_image_from_data(&data);
        image.set_filter(FilterMode::Linear);
        image.set_wrap(WrapMode::Clamp);
        CardArt {
            coach_image,
            benny: [bordered(PAD, PAD, benny_w, benny_h), bordered(2 * PAD + benny_w, PAD, benny_w, benny_h)],
            glove: bordered(glove_x, PAD, glove_w, glove_h),
            image,
            faces,
            back: bordered(bx as u32, by as u32, TEX_W, TEX_H),
            icons,
        }
    }

    pub fn image(&self) -> &Image {
        &self.image
    }

    /// The smallest white suit icon (about 13 design pixels), for buttons.
    pub fn icon(&self, suit: Suit) -> Quad {
        self.icons[0][suit.index()]
    }

    /// Draws a suit icon centered at `pos` in `color`, with a hard shadow.
    /// `scale` 1 is about 13 design pixels tall; whole scales are pixel-exact.
    pub fn draw_suit(&self, g: &mut Graphics, suit: Suit, pos: Vec2, scale: f32, rotation: f32, color: Color) {
        let k = (scale.round() as usize).clamp(1, 3);
        let q = self.icons[k - 1][suit.index()];
        let s = scale / k as f32 / DENSITY;
        let half = (q.w / 2.0).floor();
        let p = DrawParams::at(pos.x, pos.y).rotation(rotation).scale(s).origin(half, half);
        let shadow = palette::INK.with_alpha(palette::SHADOW_ALPHA * color.a);
        g.draw_quad_ex(&self.image, q, p.pos(pos.x, pos.y + scale).color(shadow));
        g.draw_quad_ex(&self.image, q, p.color(color));
    }

    /// Benny's face, centered on `at`.
    pub fn draw_portrait(&self, g: &mut Graphics, at: Vec2, mouth_open: bool, scale: f32) {
        let q = self.benny[usize::from(mouth_open)];
        let p = DrawParams::at(at.x, at.y).scale(scale / DENSITY).origin((q.w / 2.0).floor(), (q.h / 2.0).floor());
        g.draw_quad_ex(&self.coach_image, q, p.color(Color::WHITE));
    }

    /// The pointing glove, with its fingertip at `tip`.
    pub fn draw_pointer(&self, g: &mut Graphics, tip: Vec2, rotation: f32, alpha: f32) {
        // The fingertip is the top of the glove's fifth column of cells.
        let p = DrawParams::at(tip.x, tip.y).rotation(rotation).scale(1.0 / DENSITY).origin(BORDER + 9.0, BORDER);
        let shadow = palette::INK.with_alpha(palette::SHADOW_ALPHA * alpha);
        g.draw_quad_ex(&self.coach_image, self.glove, p.pos(tip.x + 2.0, tip.y + 3.0).color(shadow));
        g.draw_quad_ex(&self.coach_image, self.glove, p.color(Color::WHITE.with_alpha(alpha)));
    }

    /// The face quad, or the back for `None`.
    pub fn quad(&self, face: Option<Card>) -> Quad {
        match face {
            Some(c) => self.faces[c.id()],
            None => self.back,
        }
    }

    /// Draws a card at `pose`. `face = None` draws the back.
    pub fn draw(&self, g: &mut Graphics, face: Option<Card>, pose: Pose, tint: Color) {
        let (ox, oy) = Self::ORIGIN;
        g.draw_quad_ex(
            &self.image,
            self.quad(face),
            DrawParams::at(pose.x, pose.y)
                .rotation(pose.rot)
                .scale_xy(pose.sx / DENSITY, pose.sy / DENSITY)
                .origin(ox, oy)
                .color(tint),
        );
    }
}

/// The quad of a `w x h` sprite at `(x, y)`, grown by [`BORDER`] on every side.
pub fn bordered(x: u32, y: u32, w: u32, h: u32) -> Quad {
    Quad::new(x as f32 - BORDER, y as f32 - BORDER, w as f32 + 2.0 * BORDER, h as f32 + 2.0 * BORDER)
}

pub fn suit_color(suit: Suit) -> Color {
    if suit.is_red() { palette::SUIT_RED } else { palette::SUIT_BLACK }
}

/// Suit color for drawing on dark panels (black suits become cream).
pub fn suit_color_on_dark(suit: Suit) -> Color {
    if suit.is_red() { palette::SUIT_RED_LIGHT } else { palette::CREAM }
}

/// Paints one of the small sprites above into the atlas, each cell as 2x2 texels.
fn paint_sprite(data: &mut ImageData, x0: u32, y0: u32, rows: &[&str], mouth_open: bool) {
    for (y, row) in rows.iter().enumerate() {
        let row: &str = if mouth_open && *row == BENNY_CHIN.0 { BENNY_CHIN.1 } else { row };
        for (x, b) in row.bytes().enumerate() {
            let color = match b {
                b'k' | b'e' => palette::INK,
                b'r' => palette::BACK,
                b'R' => palette::BACK_DARK,
                b'b' => palette::BACK_BLACK,
                b'c' | b'w' => palette::CREAM,
                b'C' => palette::CREAM_SHADE,
                b's' => palette::SKIN,
                b'S' => palette::SKIN_SHADE,
                b'h' => palette::HAIR,
                b'H' => palette::HAIR_DARK,
                b'm' => palette::RED_DARK,
                b'B' => palette::BRASS,
                b'D' => palette::BRASS_DARK,
                _ => continue,
            };
            let (px, py) = (x0 + 2 * x as u32, y0 + 2 * y as u32);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                data.set_pixel(px + dx, py + dy, color);
            }
        }
    }
}

struct Painter<'a> {
    d: &'a mut ImageData,
    ox: i32,
    oy: i32,
}

const W: i32 = TEX_W as i32;
const H: i32 = TEX_H as i32;

impl Painter<'_> {
    fn px(&mut self, x: i32, y: i32, c: Color) {
        if x >= 0 && y >= 0 && x < W && y < H {
            self.d.set_pixel((self.ox + x) as u32, (self.oy + y) as u32, c);
        }
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.px(xx, yy, c);
            }
        }
    }

    /// Paints the same pixel rotated 180 degrees around the card center.
    fn px_rot(&mut self, x: i32, y: i32, rot180: bool, c: Color) {
        if rot180 { self.px(W - 1 - x, H - 1 - y, c) } else { self.px(x, y, c) }
    }

    /// Draws a font glyph with each font pixel as a `k x k` block.
    fn glyph(&mut self, x: i32, y: i32, rows: &[&str], k: i32, rot180: bool, c: Color) {
        for (ry, row) in rows.iter().enumerate() {
            for (rx, b) in row.bytes().enumerate() {
                if b == b'#' {
                    for dy in 0..k {
                        for dx in 0..k {
                            self.px_rot(x + rx as i32 * k + dx, y + ry as i32 * k + dy, rot180, c);
                        }
                    }
                }
            }
        }
    }

    /// Draws a suit pip of `size` texels with its top-left at `(x, y)`.
    fn pip(&mut self, suit: Suit, x: i32, y: i32, size: u32, rot180: bool, c: Color) {
        let n = size as i32;
        for (i, &on) in suits::mask(suit, size).iter().enumerate() {
            if on {
                let (mx, my) = (i as i32 % n, i as i32 / n);
                self.px_rot(x + mx, y + my, rot180, c);
            }
        }
    }

    /// A pip centered on `(cx, cy)`; pips in the bottom half point down, like real cards.
    fn pip_at(&mut self, suit: Suit, cx: i32, cy: i32, size: u32, c: Color) {
        let n = size as i32;
        let flip = cy > H / 2;
        let (x, y) = (cx - n / 2, cy - n / 2);
        if flip {
            // Rotate in place: mirror the mask around its own center.
            for (i, &on) in suits::mask(suit, size).iter().enumerate() {
                if on {
                    let (mx, my) = (i as i32 % n, i as i32 / n);
                    self.px(x + n - 1 - mx, y + n - 1 - my, c);
                }
            }
        } else {
            self.pip(suit, x, y, size, false, c);
        }
    }

    /// Court portrait bust (top 14 rows) at 2x2, optionally rotated 180 degrees.
    fn bust(&mut self, x: i32, y: i32, rows: &[&str], rot180: bool, color: impl Fn(u8) -> Option<Color>) {
        for (ry, row) in rows.iter().take(14).enumerate() {
            for (rx, b) in row.bytes().enumerate() {
                if let Some(c) = color(b) {
                    for dy in 0..2 {
                        for dx in 0..2 {
                            self.px_rot(x + rx as i32 * 2 + dx, y + ry as i32 * 2 + dy, rot180, c);
                        }
                    }
                }
            }
        }
    }

    /// Rounded card silhouette with a dark outline and a soft inner shade.
    fn card_base(&mut self, fill: Color, shade: Color) {
        let r = 4.0f32;
        let inside = |x: i32, y: i32, inset: f32| {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let (w, h) = (W as f32, H as f32);
            let cx = fx.clamp(r, w - r);
            let cy = fy.clamp(r, h - r);
            let (dx, dy) = (fx - cx, fy - cy);
            (dx * dx + dy * dy).sqrt() <= r - inset && fx >= inset && fy >= inset && fx <= w - inset && fy <= h - inset
        };
        for y in 0..H {
            for x in 0..W {
                if !inside(x, y, 0.0) {
                    continue;
                }
                let c = if !inside(x, y, 1.0) {
                    palette::INK
                } else if y >= H - 3 || x >= W - 2 {
                    shade
                } else {
                    fill
                };
                self.px(x, y, c);
            }
        }
    }
}

fn paint_face(p: &mut Painter, card: Card) {
    p.card_base(palette::CREAM, palette::CREAM_SHADE);
    let ink = suit_color(card.suit);
    let suit = card.suit;

    // Corner indices: rank (font pixels at 2x2) and a small pip, mirrored at the bottom.
    for rot in [false, true] {
        let mut x = 5;
        for ch in card.rank.label().chars() {
            let rows = text::glyph_rows(ch).expect("rank glyph");
            p.glyph(x, 5, rows, 2, rot, ink);
            x += rows[0].len() as i32 * 2 + 2;
        }
        p.pip(suit, 5, 21, 11, rot, ink);
    }

    let (cx, cy) = (W / 2, H / 2);
    match card.rank {
        Rank::Nine | Rank::Ten => {
            let mut spots: Vec<(i32, i32)> = Vec::new();
            for y in [24, 38, 52, 66] {
                spots.push((22, y));
                spots.push((44, y));
            }
            if card.rank == Rank::Nine {
                spots.push((cx, cy));
            } else {
                spots.push((cx, 31));
                spots.push((cx, 59));
            }
            for (x, y) in spots {
                p.pip_at(suit, x, y, 11, ink);
            }
        }
        Rank::Ace => {
            // A thin gold ring behind a big pip.
            for yy in -21..=21i32 {
                for xx in -21..=21i32 {
                    let d = ((xx * xx + yy * yy) as f32).sqrt();
                    if (18.5..19.6).contains(&d) {
                        p.px(cx + xx, cy + yy, palette::GOLD);
                    }
                }
            }
            p.pip(suit, cx - 14, cy - 14, 29, false, ink);
        }
        Rank::Jack | Rank::Queen | Rank::King => {
            let portrait = match card.rank {
                Rank::King => KING,
                Rank::Queen => QUEEN,
                _ => JACK,
            };
            let clothes = palette::COURT[suit.index()];
            let bg = clothes.lerp(palette::CREAM, 0.72);
            // Frame: border x 17..=49, y 16..=74; two busts and a gold divider.
            p.rect(17, 16, 33, 59, palette::INK);
            p.rect(18, 17, 31, 57, bg);
            let map = |b: u8| match b {
                b'k' => Some(palette::INK),
                b's' => Some(palette::SKIN),
                b'g' => Some(palette::GOLD),
                b'w' => Some(palette::WHITE),
                b'h' => Some(palette::HAIR),
                b'r' => Some(palette::SUIT_RED),
                b'c' => Some(clothes),
                _ => None,
            };
            p.bust(19, 17, portrait, false, map);
            p.bust(19, 17, portrait, true, map);
            p.rect(18, 45, 31, 1, palette::GOLD);
        }
    }
}

/// A pine tree for the badge on the card back.
const PINE: &[&str] = &[
    ".....#.....",
    "....###....",
    "...#####...",
    "....###....",
    "...#####...",
    "..#######..",
    "....###....",
    "..#######..",
    ".#########.",
    "...#####...",
    ".#########.",
    "###########",
    ".....#.....",
    ".....#.....",
];

fn paint_back(p: &mut Painter) {
    p.card_base(palette::BACK, palette::BACK_DARK);
    // Buffalo check: black bands cross red ones; where only one band runs the
    // weave reads as a dark red, with a twill hatch on the dark squares.
    let block = 8;
    for y in 2..H - 3 {
        for x in 2..W - 2 {
            let bx = ((x - 2) / block) % 2 == 1;
            let by = ((y - 2) / block) % 2 == 1;
            let c = match (bx, by) {
                (true, true) => {
                    if (x + y) % 4 == 0 { palette::BACK_DARK } else { palette::BACK_BLACK }
                }
                (true, false) | (false, true) => {
                    if (x + y) % 2 == 0 { palette::BACK_BLACK } else { palette::BACK_DARK }
                }
                (false, false) => palette::BACK,
            };
            p.px(x, y, c);
        }
    }
    let cream = palette::CREAM;
    p.rect(4, 4, W - 8, 1, cream);
    p.rect(4, H - 5, W - 8, 1, cream);
    p.rect(4, 4, 1, H - 8, cream);
    p.rect(W - 5, 4, 1, H - 8, cream);
    // Badge: a cream oval with a brass ring and a pine tree.
    let (cx, cy) = (W / 2, H / 2);
    for yy in -18..=18i32 {
        for xx in -15..=15i32 {
            let d = ((xx * xx) as f32 / (14.5 * 14.5) + (yy * yy) as f32 / (17.5 * 17.5)).sqrt();
            let c = if d <= 0.8 {
                Some(cream)
            } else if d <= 0.9 {
                Some(palette::BRASS)
            } else if d <= 1.0 {
                Some(palette::INK)
            } else {
                None
            };
            if let Some(c) = c {
                p.px(cx + xx, cy + yy, c);
            }
        }
    }
    p.glyph(cx - 5, cy - 7, PINE, 1, false, palette::GREEN_DARK);
}
