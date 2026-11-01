//! Small immediate-ish UI kit: rectangles, chunky panels and springy buttons
//! in the house style (saturated fill, darker lip underneath, hard shadow).

use love2drs::prelude::*;

use crate::juice::Spring;
use crate::render::{DENSITY, snap};
use crate::palette;
use crate::sfx::{Sfx, Sound};
use crate::text::{PixelFont, Text};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Rect { x, y, w, h }
    }

    /// A rect of size `w x h` centered on `(cx, cy)`.
    pub fn centered(cx: f32, cy: f32, w: f32, h: f32) -> Self {
        Rect::new((cx - w / 2.0).round(), (cy - h / 2.0).round(), w, h)
    }

    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.x && p.x < self.x + self.w && p.y >= self.y && p.y < self.y + self.h
    }

    pub fn center(&self) -> Vec2 {
        vec2(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// A dark rounded panel with a lighter edge and a hard drop shadow.
pub fn panel(g: &mut Graphics, r: Rect, fill: Color) {
    g.set_color(palette::INK.with_alpha(palette::SHADOW_ALPHA));
    g.rounded_rectangle(Fill, r.x + 2.0, r.y + 3.0, r.w, r.h, 4.0);
    g.set_color(palette::PANEL_EDGE);
    g.rounded_rectangle(Fill, r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0, 5.0);
    g.set_color(fill);
    g.rounded_rectangle(Fill, r.x, r.y, r.w, r.h, 4.0);
}

/// A recessed box inside a panel.
pub fn inset(g: &mut Graphics, r: Rect, fill: Color) {
    g.set_color(fill);
    g.rounded_rectangle(Fill, r.x, r.y, r.w, r.h, 3.0);
}

/// Full-screen dim behind modal panels.
pub fn dim_screen(g: &mut Graphics, alpha: f32) {
    g.set_color(palette::INK.with_alpha(alpha));
    g.rectangle(Fill, -20.0, -20.0, 700.0, 400.0);
}

pub struct Button {
    pub rect: Rect,
    pub label: String,
    pub color: Color,
    pub lip: Color,
    pub text_color: Color,
    pub text_scale: f32,
    pub enabled: bool,
    /// Tints ♥/♦ in the label red (off for cream "card" buttons that set `text_color`).
    pub suit_tint: bool,
    /// An image drawn instead of the label (tinted with `text_color`, scaled by `text_scale`).
    pub icon: Option<(Image, Quad)>,
    hovered: bool,
    hover: f32,
    pressed: f32,
    pop: Spring,
}

impl Button {
    pub fn new(label: impl Into<String>, rect: Rect, color: Color, lip: Color) -> Self {
        Button {
            rect,
            label: label.into(),
            color,
            lip,
            text_color: palette::CREAM,
            text_scale: 1.0,
            enabled: true,
            suit_tint: true,
            icon: None,
            hovered: false,
            hover: 0.0,
            pressed: 0.0,
            pop: Spring::new(1.0),
        }
    }

    pub fn red(label: impl Into<String>, rect: Rect) -> Self {
        Button::new(label, rect, palette::RED, palette::RED_DARK)
    }

    pub fn blue(label: impl Into<String>, rect: Rect) -> Self {
        Button::new(label, rect, palette::BLUE, palette::BLUE_DARK)
    }

    pub fn green(label: impl Into<String>, rect: Rect) -> Self {
        Button::new(label, rect, palette::GREEN, palette::GREEN_DARK)
    }

    pub fn orange(label: impl Into<String>, rect: Rect) -> Self {
        Button::new(label, rect, palette::ORANGE, palette::ORANGE_DARK)
    }

    pub fn grey(label: impl Into<String>, rect: Rect) -> Self {
        Button::new(label, rect, palette::GREY_BUTTON, palette::GREY_BUTTON_DARK)
    }

    pub fn scaled(mut self, s: f32) -> Self {
        self.text_scale = s;
        self
    }

    /// Call every frame. Plays a soft tick when the mouse first enters.
    pub fn update(&mut self, mouse: Vec2, dt: f32, sfx: &Sfx) {
        let over = self.rect.contains(mouse);
        if over && !self.hovered && self.enabled {
            sfx.play(Sound::Hover);
        }
        self.hovered = over;
        let target = if over && self.enabled { 1.0 } else { 0.0 };
        self.hover = approach(self.hover, target, 18.0, dt);
        self.pressed = (self.pressed - dt * 6.0).max(0.0);
        self.pop.update(dt);
    }

    /// Returns true if a press at `pos` activates the button (and plays the feedback).
    pub fn click(&mut self, pos: Vec2, sfx: &Sfx) -> bool {
        if !self.rect.contains(pos) {
            return false;
        }
        if !self.enabled {
            sfx.play(Sound::Deny);
            self.pop.impulse(-1.5);
            return false;
        }
        sfx.play(Sound::Select);
        self.pressed = 1.0;
        self.pop.impulse(-2.5);
        true
    }

    /// Triggers the press animation without a click (keyboard shortcuts).
    pub fn flash(&mut self, sfx: &Sfx) {
        sfx.play(Sound::Select);
        self.pressed = 1.0;
        self.pop.impulse(-2.5);
    }

    pub fn draw(&self, g: &mut Graphics, font: &PixelFont) {
        let r = self.rect;
        let (body, lip, text) = if self.enabled {
            (self.color.lerp(palette::WHITE, self.hover * 0.12), self.lip, self.text_color)
        } else {
            (palette::GREY_BUTTON_DARK, palette::PANEL_DARK, palette::DIM)
        };
        let lip_h = 2.0;
        let sink = (self.pressed * 2.0).min(lip_h);
        let raise = (self.hover * 1.0).round();
        let s = self.pop.value;
        let (w, h) = (r.w * s, r.h * s);
        let (x, y) = ((r.x + (r.w - w) / 2.0).round(), (r.y + (r.h - h) / 2.0 - raise + sink).round());

        g.set_color(palette::INK.with_alpha(0.45));
        g.rounded_rectangle(Fill, r.x + 1.0, r.y + lip_h + 2.0, r.w, r.h, 3.0);
        g.set_color(lip);
        g.rounded_rectangle(Fill, x, r.y + lip_h, w, h, 3.0);
        g.set_color(body);
        g.rounded_rectangle(Fill, x, y, w, h, 3.0);

        if let Some((image, q)) = &self.icon {
            // Icons are authored at canvas density.
            let p = DrawParams::at(snap(x + w / 2.0), snap(y + h / 2.0))
                .scale(self.text_scale / DENSITY)
                .origin((q.w / 2.0).floor(), (q.h / 2.0).floor());
            g.draw_quad_ex(image, *q, p.color(text));
            return;
        }
        let mut t = Text::new(&self.label, x + w / 2.0, y + h / 2.0)
            .scale(self.text_scale)
            .color(text)
            .center();
        if !self.suit_tint {
            t = t.plain_suits();
        }
        t.draw(g, font);
    }
}

/// A draggable 0..=1 slider in steps of 5%. Click anywhere on the track to
/// jump there, then drag; it ticks as the value changes.
pub struct Slider {
    pub rect: Rect,
    pub value: f32,
    pub color: Color,
    dragging: bool,
    hover: f32,
    knob: Spring,
}

impl Slider {
    pub fn new(rect: Rect, value: f32, color: Color) -> Self {
        Slider {
            rect,
            value,
            color,
            dragging: false,
            hover: 0.0,
            knob: Spring::new(1.0),
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// The area that accepts clicks: the track plus some slack for the knob.
    fn hit(&self) -> Rect {
        Rect::new(self.rect.x - 6.0, self.rect.y - 6.0, self.rect.w + 12.0, self.rect.h + 12.0)
    }

    fn value_at(&self, x: f32) -> f32 {
        let v = ((x - self.rect.x) / self.rect.w).clamp(0.0, 1.0);
        (v * 20.0).round() / 20.0
    }

    /// Starts a drag if `pos` is on the slider. Returns true if it was.
    pub fn press(&mut self, pos: Vec2, sfx: &Sfx) -> bool {
        if !self.hit().contains(pos) {
            return false;
        }
        self.dragging = true;
        self.knob.impulse(4.0);
        self.set(self.value_at(pos.x), sfx);
        true
    }

    fn set(&mut self, v: f32, sfx: &Sfx) {
        if (v - self.value).abs() > 0.001 {
            self.value = v;
            sfx.play_pitched(Sound::Tick, 0.8 + v * 0.6);
        }
    }

    /// Call every frame. Returns true when a drag just finished (a good time to save).
    pub fn update(&mut self, mouse: Vec2, down: bool, dt: f32, sfx: &Sfx) -> bool {
        let over = self.hit().contains(mouse);
        self.hover = approach(self.hover, if over || self.dragging { 1.0 } else { 0.0 }, 18.0, dt);
        self.knob.target = if self.dragging { 1.25 } else { 1.0 + self.hover * 0.1 };
        self.knob.update(dt);
        if self.dragging {
            if down {
                self.set(self.value_at(mouse.x), sfx);
            } else {
                self.dragging = false;
                return true;
            }
        }
        false
    }

    pub fn draw(&self, g: &mut Graphics) {
        let r = self.rect;
        let fill_w = (r.w * self.value).max(0.0);
        g.set_color(palette::INK);
        g.rounded_rectangle(Fill, r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0, 3.0);
        g.set_color(palette::PANEL_LIGHT);
        g.rounded_rectangle(Fill, r.x, r.y, r.w, r.h, 2.0);
        if fill_w > 1.0 {
            g.set_color(self.color.lerp(palette::WHITE, self.hover * 0.12));
            g.rounded_rectangle(Fill, r.x, r.y, fill_w, r.h, 2.0);
        }
        let (kx, ky) = (r.x + fill_w, r.y + r.h / 2.0);
        let (kw, kh) = (7.0 * self.knob.value, 13.0 * self.knob.value);
        g.set_color(palette::INK.with_alpha(0.5));
        g.rounded_rectangle(Fill, kx - kw / 2.0 + 1.0, ky - kh / 2.0 + 2.0, kw, kh, 2.0);
        g.set_color(palette::INK);
        g.rounded_rectangle(Fill, kx - kw / 2.0 - 1.0, ky - kh / 2.0 - 1.0, kw + 2.0, kh + 2.0, 3.0);
        g.set_color(palette::CREAM);
        g.rounded_rectangle(Fill, kx - kw / 2.0, ky - kh / 2.0, kw, kh, 2.0);
    }
}
