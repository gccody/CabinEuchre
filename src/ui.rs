//! Small immediate-ish UI kit: rectangles, panels and springy buttons in the
//! house style: chamfered corners, thick dark outlines, brass trim, a beveled
//! face over a darker lip, and hard drop shadows.

use love2drs::prelude::*;

use crate::device;
use crate::juice::{Spring, approach};
use crate::render::DENSITY;
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

    /// This rect grown by `by` on every side.
    pub fn grown(&self, by: f32) -> Rect {
        Rect::new(self.x - by, self.y - by, self.w + by * 2.0, self.h + by * 2.0)
    }
}

/// How far past its edges a button answers a finger.
const TOUCH_SLOP: f32 = 4.0;

/// A rectangle with its corners cut at 45 degrees (`cut` design px). Convex,
/// so it fills as a single polygon. The house shape for panels and buttons.
pub fn chamfer(g: &mut Graphics, mode: DrawMode, x: f32, y: f32, w: f32, h: f32, cut: f32) {
    let c = cut.min(w / 2.0).min(h / 2.0).max(0.0);
    g.polygon(
        mode,
        &[x + c, y, x + w - c, y, x + w, y + c, x + w, y + h - c, x + w - c, y + h, x + c, y + h, x, y + h - c, x, y + c],
    );
}

/// A walnut panel: thick dark outline, brass rim, an inner brass hairline and
/// a brass stud in each corner, over a hard drop shadow.
pub fn panel(g: &mut Graphics, r: Rect, fill: Color) {
    g.set_color(palette::INK.with_alpha(palette::SHADOW_ALPHA));
    chamfer(g, Fill, r.x + 3.0, r.y + 4.0, r.w, r.h, 6.0);
    g.set_color(palette::INK);
    chamfer(g, Fill, r.x - 3.0, r.y - 3.0, r.w + 6.0, r.h + 6.0, 8.0);
    g.set_color(palette::BRASS_DARK);
    chamfer(g, Fill, r.x - 2.0, r.y - 2.0, r.w + 4.0, r.h + 4.0, 7.0);
    g.set_color(palette::BRASS);
    g.rectangle(Fill, r.x + 5.0, r.y - 2.0, r.w - 10.0, 1.0);
    g.set_color(fill);
    chamfer(g, Fill, r.x, r.y, r.w, r.h, 6.0);
    g.set_color(palette::BRASS.with_alpha(0.35 * fill.a));
    g.set_line_width(1.0);
    chamfer(g, Line, r.x + 4.5, r.y + 4.5, r.w - 9.0, r.h - 9.0, 3.0);
    for (sx, sy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
        let (x, y) = ((r.x + 3.0 + sx * (r.w - 9.0)).round(), (r.y + 3.0 + sy * (r.h - 9.0)).round());
        g.set_color(palette::INK.with_alpha(0.6));
        g.rectangle(Fill, x + 1.0, y + 1.0, 3.0, 3.0);
        g.set_color(palette::BRASS);
        g.rectangle(Fill, x, y, 3.0, 3.0);
        g.set_color(palette::BRASS_LIGHT);
        g.rectangle(Fill, x, y, 1.0, 1.0);
    }
}

/// A recessed box inside a panel (shadowed top edge, lit bottom edge).
pub fn inset(g: &mut Graphics, r: Rect, fill: Color) {
    g.set_color(fill);
    chamfer(g, Fill, r.x, r.y, r.w, r.h, 2.0);
    g.set_color(palette::INK.with_alpha(0.45 * fill.a));
    g.rectangle(Fill, r.x + 2.0, r.y, r.w - 4.0, 1.0);
    g.set_color(palette::PANEL_LIGHT.with_alpha(0.6 * fill.a));
    g.rectangle(Fill, r.x + 2.0, r.y + r.h - 1.0, r.w - 4.0, 1.0);
}

/// Full-screen dim behind modal panels.
pub fn dim_screen(g: &mut Graphics, alpha: f32) {
    g.set_color(palette::INK.with_alpha(alpha));
    if device::mobile() {
        // The canvas can reach well past 640x360 on a phone.
        g.rectangle(Fill, -2000.0, -2000.0, 4640.0, 4360.0);
    } else {
        g.rectangle(Fill, -20.0, -20.0, 700.0, 400.0);
    }
}

/// Dims the screen except inside `holes`, for a spotlight: horizontal bands
/// around the holes, so no spot gets dimmed twice.
pub fn dim_around(g: &mut Graphics, alpha: f32, holes: &[Rect]) {
    let (left, right, top, bottom) = if device::mobile() { (-2000.0, 2640.0, -2000.0, 2360.0) } else { (-20.0, 680.0, -20.0, 380.0) };
    g.set_color(palette::INK.with_alpha(alpha));
    let mut ys: Vec<f32> = vec![top, bottom];
    for h in holes {
        ys.push(h.y.clamp(top, bottom));
        ys.push((h.y + h.h).clamp(top, bottom));
    }
    ys.sort_by(f32::total_cmp);
    ys.dedup();
    for band in ys.windows(2) {
        let (y0, y1) = (band[0], band[1]);
        let mut spans: Vec<(f32, f32)> = holes
            .iter()
            .filter(|h| h.y <= y0 && h.y + h.h >= y1)
            .map(|h| (h.x, h.x + h.w))
            .collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut x = left;
        for (a, b) in spans {
            if a > x {
                g.rectangle(Fill, x, y0, a - x, y1 - y0);
            }
            x = x.max(b);
        }
        if x < right {
            g.rectangle(Fill, x, y0, right - x, y1 - y0);
        }
    }
}

/// Rough perceived brightness, to pick a text shadow that suits a fill.
fn luma(c: Color) -> f32 {
    0.299 * c.r + 0.587 * c.g + 0.114 * c.b
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
    /// Set by the scene when the gamepad / keyboard focus is on this button.
    /// Draws a pulsing brass ring and lifts the button like a hover.
    pub focused: bool,
    hovered: bool,
    /// 0..1, fades the focus ring in and out.
    focus: f32,
    clock: f32,
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
            focused: false,
            hovered: false,
            focus: 0.0,
            clock: 0.0,
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

    /// The primary action: a brass plaque with engraved (dark) lettering.
    pub fn brass(label: impl Into<String>, rect: Rect) -> Self {
        let mut b = Button::new(label, rect, palette::BRASS, palette::BRASS_DARK);
        b.text_color = palette::INK;
        b
    }

    pub fn grey(label: impl Into<String>, rect: Rect) -> Self {
        Button::new(label, rect, palette::GREY_BUTTON, palette::GREY_BUTTON_DARK)
    }

    pub fn scaled(mut self, s: f32) -> Self {
        self.text_scale = s;
        self
    }

    /// The area that answers presses: on a phone, a little more than what's drawn.
    fn hit(&self) -> Rect {
        if device::mobile() { self.rect.grown(TOUCH_SLOP) } else { self.rect }
    }

    /// Call every frame. Plays a soft tick when the mouse (or the focus) first arrives.
    pub fn update(&mut self, mouse: Vec2, dt: f32, sfx: &Sfx) {
        let over = self.hit().contains(mouse) || self.focused;
        if over && !self.hovered && self.enabled {
            sfx.play(Sound::Hover);
        }
        self.hovered = over;
        let target = if over && self.enabled { 1.0 } else { 0.0 };
        self.hover = approach(self.hover, target, 18.0, dt);
        self.pressed = (self.pressed - dt * 6.0).max(0.0);
        self.focus = approach(self.focus, if self.focused { 1.0 } else { 0.0 }, 16.0, dt);
        self.clock += dt;
        self.pop.update(dt);
    }

    /// Presses the button from the gamepad / keyboard focus: the same feedback
    /// as a click, including the "no" for a disabled button. Returns true if it activated.
    pub fn activate(&mut self, sfx: &Sfx) -> bool {
        if !self.enabled {
            self.deny(sfx);
            return false;
        }
        self.flash(sfx);
        true
    }

    /// The "can't do that" feedback: a buzz and a little squash.
    pub fn deny(&mut self, sfx: &Sfx) {
        sfx.play(Sound::Deny);
        self.pop.impulse(-1.5);
    }

    /// Returns true if a press at `pos` activates the button (and plays the feedback).
    pub fn click(&mut self, pos: Vec2, sfx: &Sfx) -> bool {
        if !self.hit().contains(pos) {
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
    /// A silent little bounce (something about the button changed).
    pub fn nudge(&mut self) {
        self.pop.impulse(-2.5);
    }

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

        let cut = 3.0;
        let base_y = r.y + lip_h;
        let base_x = (r.x + (r.w - w) / 2.0).round();
        g.set_color(palette::INK.with_alpha(0.4));
        chamfer(g, Fill, r.x + 2.0, base_y + 2.0, r.w, r.h, cut);
        if self.focus > 0.01 {
            // Focus ring: a pulsing brass band around the button and its lip, with a dark edge.
            let pulse = 0.8 + 0.2 * (self.clock * 6.0).sin();
            let a = self.focus * pulse;
            let (top, bottom) = (y.min(base_y), base_y + h);
            g.set_color(palette::INK.with_alpha(a));
            chamfer(g, Fill, x - 4.0, top - 4.0, w + 8.0, bottom - top + 8.0, cut + 4.0);
            g.set_color(palette::GOLD.with_alpha(a));
            chamfer(g, Fill, x - 3.0, top - 3.0, w + 6.0, bottom - top + 6.0, cut + 3.0);
        }
        // Dark outline around the lip and the body, then the lip, then the body.
        g.set_color(palette::INK);
        chamfer(g, Fill, base_x - 1.0, base_y - 1.0, w + 2.0, h + 2.0, cut + 1.0);
        chamfer(g, Fill, x - 1.0, y - 1.0, w + 2.0, h + 2.0, cut + 1.0);
        g.set_color(lip);
        chamfer(g, Fill, base_x, base_y, w, h, cut);
        g.set_color(body);
        chamfer(g, Fill, x, y, w, h, cut);
        // Bevel: a lit top edge and a shaded bottom edge.
        g.set_color(body.lerp(palette::WHITE, 0.35));
        g.rectangle(Fill, x + cut, y + 1.0, w - cut * 2.0, 1.0);
        g.set_color(body.lerp(lip, 0.5));
        g.rectangle(Fill, x + cut, y + h - 2.0, w - cut * 2.0, 1.0);


        if let Some((image, q)) = &self.icon {
            // Icons are authored at card-art density.
            let p = DrawParams::at(x + w / 2.0, y + h / 2.0)
                .scale(self.text_scale / DENSITY)
                .origin((q.w / 2.0).floor(), (q.h / 2.0).floor());
            g.draw_quad_ex(image, *q, p.color(text));
            return;
        }
        let mut t = Text::new(&self.label, x + w / 2.0, y + h / 2.0)
            .scale(self.text_scale)
            .color(text)
            .center();
        if luma(text) < 0.35 {
            // Dark lettering reads as engraved: a light highlight below instead of a shadow.
            t = t.shadow(body.lerp(palette::WHITE, 0.4));
        }
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
    /// Set by the scene when the gamepad / keyboard focus is on this slider's row.
    pub focused: bool,
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
            focused: false,
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
        if device::mobile() {
            // Most of the row's height, so a thumb can't miss it.
            return Rect::new(self.rect.x - 10.0, self.rect.y - 16.0, self.rect.w + 20.0, self.rect.h + 32.0);
        }
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

    /// Steps the value by 5% (`dir` = -1 or +1), for the gamepad and arrow keys.
    pub fn nudge(&mut self, dir: i32, sfx: &Sfx) {
        let v = ((self.value * 20.0).round() + dir as f32).clamp(0.0, 20.0) / 20.0;
        if (v - self.value).abs() > 0.001 {
            self.knob.impulse(3.0);
            self.set(v, sfx);
        } else {
            sfx.play(Sound::Deny);
            self.knob.impulse(-2.0);
        }
    }

    fn set(&mut self, v: f32, sfx: &Sfx) {
        if (v - self.value).abs() > 0.001 {
            self.value = v;
            sfx.play_pitched(Sound::Tick, 0.8 + v * 0.6);
        }
    }

    /// Call every frame. Returns true when a drag just finished (a good time to save).
    pub fn update(&mut self, mouse: Vec2, down: bool, dt: f32, sfx: &Sfx) -> bool {
        let over = self.hit().contains(mouse) || self.focused;
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
        g.rectangle(Fill, r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0);
        g.set_color(palette::PANEL_DARK);
        g.rectangle(Fill, r.x, r.y, r.w, r.h);
        if fill_w > 1.0 {
            let c = self.color.lerp(palette::WHITE, self.hover * 0.12);
            g.set_color(c);
            g.rectangle(Fill, r.x, r.y, fill_w, r.h);
            g.set_color(c.lerp(palette::WHITE, 0.3));
            g.rectangle(Fill, r.x, r.y, fill_w, 1.0);
        }
        // Notches every 25%, like a fader's scale.
        g.set_color(palette::MUTED.with_alpha(0.5));
        for i in 1..4 {
            g.rectangle(Fill, (r.x + r.w * i as f32 / 4.0).round(), r.y + r.h + 2.0, 1.0, 2.0);
        }
        // Brass fader knob with a center groove.
        let (kx, ky) = (r.x + fill_w, r.y + r.h / 2.0);
        let knob = if device::mobile() { 1.5 } else { 1.0 };
        let (kw, kh) = (8.0 * knob * self.knob.value, 14.0 * knob * self.knob.value);
        let (x, y) = (kx - kw / 2.0, ky - kh / 2.0);
        g.set_color(palette::INK.with_alpha(0.5));
        chamfer(g, Fill, x + 1.0, y + 2.0, kw, kh, 2.0);
        g.set_color(palette::INK);
        chamfer(g, Fill, x - 1.0, y - 1.0, kw + 2.0, kh + 2.0, 3.0);
        g.set_color(palette::BRASS);
        chamfer(g, Fill, x, y, kw, kh, 2.0);
        g.set_color(palette::BRASS_LIGHT);
        g.rectangle(Fill, x + 2.0, y + 1.0, kw - 4.0, 1.0);
        g.set_color(palette::BRASS_DARK);
        g.rectangle(Fill, x + 1.0, ky - 0.5, kw - 2.0, 1.0);
    }
}
