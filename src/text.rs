//! A hand-made bitmap pixel font and the text drawing helper every screen
//! uses. Glyphs are authored below as tiny ASCII grids, packed into one
//! nearest-filtered atlas at startup and drawn so every font pixel covers a
//! whole number of canvas pixels (2x2 at scale 1), keeping text chunky and crisp.
//!
//! Suit symbols (♣♦♠♥) are not font glyphs: they come from `suits::mask` at
//! canvas density, so they are as sharp and distinct as the card pips.
//!
//! Metrics (in font pixels): capitals are 7 tall, lowercase descenders add 2,
//! glyphs are 1-5 wide with 1 pixel of spacing, lines advance by 10. One
//! font pixel is [`UNIT`] design pixels at scale 1.

use std::collections::HashMap;

use love2drs::prelude::*;

use crate::euchre::Suit;
use crate::palette;
use crate::render::{DENSITY, snap};

/// Design pixels per font pixel at scale 1 (2 canvas pixels).
pub const UNIT: f32 = 2.0 / DENSITY;
/// Distance between baselines at scale 1, in design pixels.
pub const LINE_HEIGHT: f32 = 10.0 * UNIT;
const FONT_CAP: f32 = 7.0;
const FONT_LINE: f32 = 10.0;
const CELL_H: u32 = 9;
const SPACE_WIDTH: f32 = 3.0;
/// Suit symbols in text: mask size in canvas pixels.
const SUIT_SIZE: u32 = 11;

/// `#` = ink, anything else = empty. Rows may be omitted at the bottom.
const GLYPHS: &[(char, &[&str])] = &[
    ('A', &[".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"]),
    ('B', &["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####."]),
    ('C', &[".###.", "#...#", "#....", "#....", "#....", "#...#", ".###."]),
    ('D', &["####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####."]),
    ('E', &["#####", "#....", "#....", "####.", "#....", "#....", "#####"]),
    ('F', &["#####", "#....", "#....", "####.", "#....", "#....", "#...."]),
    ('G', &[".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".####"]),
    ('H', &["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"]),
    ('I', &["###", ".#.", ".#.", ".#.", ".#.", ".#.", "###"]),
    ('J', &["....#", "....#", "....#", "....#", "#...#", "#...#", ".###."]),
    ('K', &["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#"]),
    ('L', &["#....", "#....", "#....", "#....", "#....", "#....", "#####"]),
    ('M', &["#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#"]),
    ('N', &["#...#", "#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#"]),
    ('O', &[".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."]),
    ('P', &["####.", "#...#", "#...#", "####.", "#....", "#....", "#...."]),
    ('Q', &[".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#"]),
    ('R', &["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"]),
    ('S', &[".###.", "#...#", "#....", ".###.", "....#", "#...#", ".###."]),
    ('T', &["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."]),
    ('U', &["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."]),
    ('V', &["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#.."]),
    ('W', &["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "##.##", "#...#"]),
    ('X', &["#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#"]),
    ('Y', &["#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#.."]),
    ('Z', &["#####", "....#", "...#.", "..#..", ".#...", "#....", "#####"]),
    ('a', &[".....", ".....", ".###.", "....#", ".####", "#...#", ".####"]),
    ('b', &["#....", "#....", "####.", "#...#", "#...#", "#...#", "####."]),
    ('c', &["....", "....", ".###", "#...", "#...", "#...", ".###"]),
    ('d', &["....#", "....#", ".####", "#...#", "#...#", "#...#", ".####"]),
    ('e', &[".....", ".....", ".###.", "#...#", "#####", "#....", ".###."]),
    ('f', &["..##", ".#..", "####", ".#..", ".#..", ".#..", ".#.."]),
    ('g', &[".....", ".....", ".####", "#...#", "#...#", "#...#", ".####", "....#", ".###."]),
    ('h', &["#....", "#....", "####.", "#...#", "#...#", "#...#", "#...#"]),
    ('i', &["#", ".", "#", "#", "#", "#", "#"]),
    ('j', &["..#", "...", "..#", "..#", "..#", "..#", "..#", "..#", "##."]),
    ('k', &["#...", "#...", "#..#", "#.#.", "##..", "#.#.", "#..#"]),
    ('l', &["#.", "#.", "#.", "#.", "#.", "#.", ".#"]),
    ('m', &[".....", ".....", "##.#.", "#.#.#", "#.#.#", "#.#.#", "#.#.#"]),
    ('n', &[".....", ".....", "####.", "#...#", "#...#", "#...#", "#...#"]),
    ('o', &[".....", ".....", ".###.", "#...#", "#...#", "#...#", ".###."]),
    ('p', &[".....", ".....", "####.", "#...#", "#...#", "#...#", "####.", "#....", "#...."]),
    ('q', &[".....", ".....", ".####", "#...#", "#...#", "#...#", ".####", "....#", "....#"]),
    ('r', &["....", "....", "#.##", "##..", "#...", "#...", "#..."]),
    ('s', &["....", "....", ".###", "#...", ".##.", "...#", "###."]),
    ('t', &[".#.", ".#.", "###", ".#.", ".#.", ".#.", "..#"]),
    ('u', &[".....", ".....", "#...#", "#...#", "#...#", "#...#", ".####"]),
    ('v', &[".....", ".....", "#...#", "#...#", "#...#", ".#.#.", "..#.."]),
    ('w', &[".....", ".....", "#...#", "#...#", "#.#.#", "#.#.#", ".#.#."]),
    ('x', &[".....", ".....", "#...#", ".#.#.", "..#..", ".#.#.", "#...#"]),
    ('y', &[".....", ".....", "#...#", "#...#", "#...#", "#...#", ".####", "....#", ".###."]),
    ('z', &[".....", ".....", "#####", "...#.", "..#..", ".#...", "#####"]),
    ('0', &[".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###."]),
    ('1', &[".#.", "##.", ".#.", ".#.", ".#.", ".#.", "###"]),
    ('2', &[".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####"]),
    ('3', &[".###.", "#...#", "....#", "..##.", "....#", "#...#", ".###."]),
    ('4', &["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."]),
    ('5', &["#####", "#....", "####.", "....#", "....#", "#...#", ".###."]),
    ('6', &["..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###."]),
    ('7', &["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."]),
    ('8', &[".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."]),
    ('9', &[".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##.."]),
    ('!', &["#", "#", "#", "#", "#", ".", "#"]),
    ('?', &[".###.", "#...#", "....#", "...#.", "..#..", ".....", "..#.."]),
    ('.', &[".", ".", ".", ".", ".", ".", "#"]),
    (',', &["..", "..", "..", "..", "..", ".#", ".#", "#."]),
    (':', &[".", ".", "#", ".", ".", "#", "."]),
    (';', &["..", "..", ".#", "..", "..", ".#", ".#", "#."]),
    ('\'', &["#", "#"]),
    ('"', &["#.#", "#.#"]),
    ('-', &["....", "....", "....", "####"]),
    ('+', &[".....", "..#..", "..#..", "#####", "..#..", "..#.."]),
    ('=', &["....", "....", "####", "....", "####"]),
    ('/', &["....#", "...#.", "...#.", "..#..", ".#...", ".#...", "#...."]),
    ('(', &[".#", "#.", "#.", "#.", "#.", "#.", ".#"]),
    (')', &["#.", ".#", ".#", ".#", ".#", ".#", "#."]),
    ('[', &["##", "#.", "#.", "#.", "#.", "#.", "##"]),
    (']', &["##", ".#", ".#", ".#", ".#", ".#", "##"]),
    ('<', &["...#", "..#.", ".#..", "#...", ".#..", "..#.", "...#"]),
    ('>', &["#...", ".#..", "..#.", "...#", "..#.", ".#..", "#..."]),
    ('%', &["##...", "##..#", "...#.", "..#..", ".#...", "#..##", "...##"]),
    ('#', &[".....", ".#.#.", "#####", ".#.#.", "#####", ".#.#."]),
    ('*', &[".....", "..#..", "#.#.#", ".###.", "#.#.#", "..#.."]),
    ('_', &[".....", ".....", ".....", ".....", ".....", ".....", "#####"]),
    ('&', &[".##..", "#..#.", "#.#..", ".#...", "#.#.#", "#..#.", ".##.#"]),
    ('·', &[".", ".", ".", "#"]),
];

/// The raw rows of a glyph (used to paint rank labels onto the card art).
pub fn glyph_rows(ch: char) -> Option<&'static [&'static str]> {
    GLYPHS.iter().find(|(c, _)| *c == ch).map(|(_, rows)| *rows)
}

#[derive(Debug, Clone, Copy)]
struct Glyph {
    quad: Quad,
    /// Width in font pixels.
    width: f32,
    /// Atlas texels per font pixel (1 for letters, 2 for the sharper suit symbols).
    density: f32,
    /// Vertical offset in font pixels.
    dy: f32,
}

pub struct PixelFont {
    image: Image,
    glyphs: HashMap<char, Glyph>,
}

impl PixelFont {
    pub fn new(g: &Graphics) -> Self {
        let letters_w: u32 = GLYPHS.iter().map(|(_, rows)| glyph_width(rows) + 1).sum();
        let total_w = letters_w + 4 * (SUIT_SIZE + 1);
        let mut data = ImageData::new(total_w, CELL_H.max(SUIT_SIZE));
        let mut glyphs = HashMap::new();
        let mut x0 = 0;
        for &(ch, rows) in GLYPHS {
            let w = glyph_width(rows);
            for (y, row) in rows.iter().enumerate() {
                for (x, b) in row.bytes().enumerate() {
                    if b == b'#' {
                        data.set_pixel(x0 + x as u32, y as u32, Color::WHITE);
                    }
                }
            }
            let quad = Quad::new(x0 as f32, 0.0, w as f32, CELL_H as f32);
            glyphs.insert(ch, Glyph { quad, width: w as f32, density: 1.0, dy: 0.0 });
            x0 += w + 1;
        }
        for suit in Suit::ALL {
            let mask = crate::suits::mask(suit, SUIT_SIZE);
            for (i, &on) in mask.iter().enumerate() {
                if on {
                    let (x, y) = (i as u32 % SUIT_SIZE, i as u32 / SUIT_SIZE);
                    data.set_pixel(x0 + x, y, Color::WHITE);
                }
            }
            let n = SUIT_SIZE as f32;
            let quad = Quad::new(x0 as f32, 0.0, n, n);
            glyphs.insert(suit.symbol(), Glyph { quad, width: n / 2.0, density: 2.0, dy: 1.0 });
            x0 += SUIT_SIZE + 1;
        }
        let image = g.new_image_from_data(&data);
        image.set_filter(FilterMode::Nearest);
        PixelFont { image, glyphs }
    }

    fn glyph(&self, ch: char) -> Option<Glyph> {
        if ch == ' ' {
            return None;
        }
        self.glyphs
            .get(&ch)
            .or_else(|| self.glyphs.get(&ch.to_ascii_uppercase()))
            .or_else(|| self.glyphs.get(&'?'))
            .copied()
    }

    /// Horizontal advance in font pixels.
    fn advance(&self, ch: char) -> f32 {
        match self.glyph(ch) {
            Some(g) => g.width + 1.0,
            None => SPACE_WIDTH + 1.0,
        }
    }

    /// Width of a single line in font pixels.
    fn raw_line_width(&self, line: &str) -> f32 {
        let w: f32 = line.chars().map(|c| self.advance(c)).sum();
        (w - 1.0).max(0.0)
    }

    fn raw_width(&self, text: &str) -> f32 {
        text.lines().map(|l| self.raw_line_width(l)).fold(0.0, f32::max)
    }

    /// Width of a single line at scale 1, in design pixels.
    pub fn line_width(&self, line: &str) -> f32 {
        self.raw_line_width(line) * UNIT
    }

    /// Width of the widest line at scale 1, in design pixels.
    pub fn width(&self, text: &str) -> f32 {
        self.raw_width(text) * UNIT
    }

    /// Word-wraps `text` so no line is wider than `max_width` (design pixels, scale 1).
    /// Existing newlines are kept.
    pub fn wrap(&self, text: &str, max_width: f32) -> Vec<String> {
        let mut out = Vec::new();
        for para in text.split('\n') {
            let mut line = String::new();
            for word in para.split(' ') {
                let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
                if self.line_width(&candidate) > max_width && !line.is_empty() {
                    out.push(std::mem::take(&mut line));
                    line = word.to_string();
                } else {
                    line = candidate;
                }
            }
            out.push(line);
        }
        out
    }
}

fn glyph_width(rows: &[&str]) -> u32 {
    rows.iter().map(|r| r.len()).max().unwrap_or(1) as u32
}

/// A text draw call, configured builder-style:
/// `Text::new("PLAY", x, y).scale(2.0).center().draw(g, font)`.
///
/// Text has a hard drop shadow in `palette::INK` by default, and suit symbols
/// (♥♦) are tinted red automatically. Whole and half scales keep font pixels
/// on whole canvas pixels.
#[derive(Debug, Clone, Copy)]
pub struct Text<'a> {
    text: &'a str,
    x: f32,
    y: f32,
    scale: f32,
    color: Color,
    anchor: (f32, f32),
    shadow: Option<Color>,
    rotation: f32,
    wave: Option<(f32, f32)>,
    suit_colors: bool,
}

impl<'a> Text<'a> {
    pub fn new(text: &'a str, x: f32, y: f32) -> Self {
        Text {
            text,
            x,
            y,
            scale: 1.0,
            color: palette::CREAM,
            anchor: (0.0, 0.0),
            shadow: Some(palette::INK.with_alpha(palette::SHADOW_ALPHA)),
            rotation: 0.0,
            wave: None,
            suit_colors: true,
        }
    }

    pub fn scale(mut self, s: f32) -> Self {
        self.scale = s;
        self
    }

    pub fn color(mut self, c: Color) -> Self {
        self.color = c;
        self
    }

    /// Fractional anchor of the text block: (0, 0) top-left, (0.5, 0.5) center.
    /// Lines are aligned by the same horizontal fraction.
    pub fn anchor(mut self, ax: f32, ay: f32) -> Self {
        self.anchor = (ax, ay);
        self
    }

    pub fn center(self) -> Self {
        self.anchor(0.5, 0.5)
    }

    pub fn no_shadow(mut self) -> Self {
        self.shadow = None;
        self
    }

    pub fn shadow(mut self, c: Color) -> Self {
        self.shadow = Some(c);
        self
    }

    pub fn rotation(mut self, r: f32) -> Self {
        self.rotation = r;
        self
    }

    /// Per-letter vertical bob: `time` drives it, `amp` is in design pixels.
    pub fn wave(mut self, time: f32, amp: f32) -> Self {
        self.wave = Some((time, amp));
        self
    }

    pub fn plain_suits(mut self) -> Self {
        self.suit_colors = false;
        self
    }

    /// Size of the text block in font pixels (scale-independent).
    fn raw_block(&self, font: &PixelFont) -> Vec2 {
        let lines = self.text.lines().count().max(1) as f32;
        vec2(font.raw_width(self.text), (lines - 1.0) * FONT_LINE + FONT_CAP)
    }

    pub fn draw(self, g: &mut Graphics, font: &PixelFont) {
        let block = self.raw_block(font);
        let px = self.scale * UNIT;
        let (ax, ay) = self.anchor;
        g.push();
        if self.rotation == 0.0 {
            // Snap to whole canvas pixels so glyphs stay crisp.
            let tl = vec2(snap(self.x - ax * block.x * px), snap(self.y - ay * block.y * px));
            g.translate(tl.x, tl.y);
            g.scale(px, px);
        } else {
            g.translate(self.x, self.y);
            g.rotate(self.rotation);
            g.scale(px, px);
            g.translate(-ax * block.x, -ay * block.y);
        }
        if let Some(shadow) = self.shadow {
            self.draw_glyphs(g, font, block.x, vec2(0.0, 1.0), Some(shadow));
        }
        self.draw_glyphs(g, font, block.x, Vec2::ZERO, None);
        g.pop();
    }

    fn draw_glyphs(&self, g: &mut Graphics, font: &PixelFont, block_w: f32, offset: Vec2, shadow: Option<Color>) {
        let px = self.scale * UNIT;
        let mut index = 0usize;
        for (row, line) in self.text.lines().enumerate() {
            let mut x = ((block_w - font.raw_line_width(line)) * self.anchor.0).round() + offset.x;
            let y = row as f32 * FONT_LINE + offset.y;
            for ch in line.chars() {
                if let Some(glyph) = font.glyph(ch) {
                    // Bob in whole canvas pixels, whatever the scale.
                    let bob = match self.wave {
                        Some((t, amp)) => snap((t * 4.0 - index as f32 * 0.55).sin() * amp) / px,
                        None => 0.0,
                    };
                    let color = shadow.unwrap_or(match ch {
                        '♥' | '♦' if self.suit_colors => palette::SUIT_RED_LIGHT,
                        _ => self.color,
                    });
                    let params = DrawParams::at(x, y + glyph.dy + bob).scale(1.0 / glyph.density).color(color);
                    g.draw_quad_ex(&font.image, glyph.quad, params);
                }
                x += font.advance(ch);
                index += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_rows_are_consistent() {
        for (ch, rows) in GLYPHS {
            assert!(rows.len() <= CELL_H as usize, "{ch} too tall");
            let w = rows[0].len();
            for r in *rows {
                assert_eq!(r.chars().count(), w, "glyph {ch:?} has ragged rows");
            }
        }
    }

    #[test]
    fn no_duplicate_glyphs() {
        let mut seen = std::collections::HashSet::new();
        for (ch, _) in GLYPHS {
            assert!(seen.insert(*ch), "duplicate glyph {ch:?}");
        }
    }
}
