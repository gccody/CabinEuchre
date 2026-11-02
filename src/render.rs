//! The render pipeline every scene draws through:
//!
//! 1. Scenes draw in a fixed `VIEW_W x VIEW_H` (640x360) "design space".
//! 2. That space is rendered into a canvas at the window's own resolution
//!    (letterboxed to 16:9), so rotated and scaled things have smooth edges.
//!    Pixel art (cards, suit icons, the font) is authored at `DENSITY` texels
//!    per design pixel and drawn through `shaders/pixel.wgsl`, which keeps
//!    texels chunky but anti-aliases their edges at any scale or rotation.
//! 3. The canvas starts each frame with the cabin tabletop (`background.wgsl`):
//!    walnut planks, a felt mat and a slowly swaying lamp.
//! 4. The canvas is drawn to the window through the lamplight post-process
//!    (`lamplight.wgsl`): warm grade, vignette and film grain.
//!
//! Use [`View::to_design`] to convert mouse positions into design space.
//!
//! On phones the design space sits inside the OS's safe area (clear of notches
//! and the gesture bar) instead of being letterboxed, and the canvas covers
//! the whole screen, so the tabletop runs edge to edge. [`bounds`] says how
//! far past 640x360 the player can see and reach, so the table can spread out.

use std::sync::atomic::{AtomicU32, Ordering};

use love2drs::prelude::*;
use love2drs::window::Window;

use crate::device;
use crate::palette;
use crate::ui::Rect;

/// Design resolution: the coordinate space every scene works in.
pub const VIEW_W: u32 = 640;
pub const VIEW_H: u32 = 360;

/// Pixel-art texels per design pixel (cards, suit icons and the font are
/// authored at this density).
pub const DENSITY: f32 = 1.5;
/// Largest canvas width, to bound memory on huge displays.
const MAX_CANVAS_W: f32 = 3840.0;

/// The whole design space: everything the player sees on a desktop.
const DESIGN_RECT: Rect = Rect::new(0.0, 0.0, VIEW_W as f32, VIEW_H as f32);

/// The safe area in design space, for the layouts ([`DESIGN_RECT`] on desktop).
static BOUNDS: [AtomicU32; 4] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];

/// The part of design space that's on screen and clear of notches and gesture
/// bars. It always contains 640x360, and goes past it on screens that aren't
/// 16:9. On a desktop it's exactly 640x360.
pub fn bounds() -> Rect {
    if !device::mobile() {
        return DESIGN_RECT;
    }
    let [x, y, w, h] = [0, 1, 2, 3].map(|i| f32::from_bits(BOUNDS[i].load(Ordering::Relaxed)));
    Rect::new(x, y, w, h)
}

fn set_bounds(r: Rect) {
    for (slot, v) in BOUNDS.iter().zip([r.x, r.y, r.w, r.h]) {
        slot.store(v.to_bits(), Ordering::Relaxed);
    }
}

/// Where the design space and canvas land in the window this frame.
#[derive(Debug, Clone, Copy)]
pub struct View {
    /// Window position of the design space's origin.
    pub offset: Vec2,
    /// Window pixels per design pixel.
    pub scale: f32,
    /// The part of design space the canvas covers: 640x360 on a desktop, the
    /// whole screen on a phone.
    pub visible: Rect,
}

impl View {
    /// Desktop: 640x360 letterboxed in the window.
    fn fit(window_w: f32, window_h: f32) -> Self {
        let scale = (window_w / VIEW_W as f32).min(window_h / VIEW_H as f32);
        let offset = vec2(
            ((window_w - VIEW_W as f32 * scale) / 2.0).floor(),
            ((window_h - VIEW_H as f32 * scale) / 2.0).floor(),
        );
        View { offset, scale, visible: DESIGN_RECT }
    }

    /// Phone: 640x360 as large as it fits inside the margins, with the canvas
    /// covering the whole screen.
    fn fit_safe(window_w: f32, window_h: f32, m: Margins) -> (Self, Rect) {
        let Margins { side, top, bottom } = m;
        let (usable_w, usable_h) = ((window_w - side * 2.0).max(1.0), (window_h - top - bottom).max(1.0));
        let scale = (usable_w / VIEW_W as f32).min(usable_h / VIEW_H as f32);
        let offset = vec2(
            ((window_w - VIEW_W as f32 * scale) / 2.0).floor(),
            (top + (usable_h - VIEW_H as f32 * scale) / 2.0).floor(),
        );
        let visible = Rect::new(-offset.x / scale, -offset.y / scale, window_w / scale, window_h / scale);
        let bounds = Rect::new((side - offset.x) / scale, (top - offset.y) / scale, usable_w / scale, usable_h / scale);
        (View { offset, scale, visible }, bounds)
    }

    /// Window coordinates (e.g. `ctx.mouse.position()`) -> design coordinates.
    pub fn to_design(self, window: Vec2) -> Vec2 {
        (window - self.offset) / self.scale
    }
}

/// Canvas size in pixels for a view: the physical size of the visible area,
/// but never below `DENSITY` (so pixel art is never minified).
fn canvas_size(view: View, dpi: f32) -> (u32, u32) {
    let density = (view.scale * dpi).clamp(DENSITY, MAX_CANVAS_W / view.visible.w);
    ((view.visible.w * density).round() as u32, (view.visible.h * density).round() as u32)
}

/// How far the OS's bars and cutouts reach into a phone's screen, in window units.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Margins {
    /// The same on both sides: turning the phone around moves the notch to
    /// the other side, and that shouldn't shift the game.
    side: f32,
    top: f32,
    bottom: f32,
}

impl Margins {
    fn from_safe(window_w: f32, window_h: f32, safe: (f32, f32, f32, f32)) -> Self {
        let (x, y, w, h) = safe;
        Margins {
            side: x.max(window_w - x - w).max(0.0),
            top: y.max(0.0),
            bottom: (window_h - y - h).max(0.0),
        }
    }

    fn at_least(self, o: Margins) -> Self {
        Margins { side: self.side.max(o.side), top: self.top.max(o.top), bottom: self.bottom.max(o.bottom) }
    }
}

/// Android's gesture bar comes and goes (it's hidden at launch, and appears
/// with the first touch), so its height is set aside from the start.
fn minimum_margins() -> Margins {
    let bottom = if cfg!(target_os = "android") { 24.0 } else { 0.0 };
    Margins { bottom, ..Margins::default() }
}

/// Debug: `EUCHRE_SAFE=left,top,right,bottom` (window units) pretends to have
/// notches and a gesture bar when previewing the phone layouts on a desktop.
fn preview_safe(window_w: f32, window_h: f32) -> (f32, f32, f32, f32) {
    let insets: Vec<f32> = std::env::var("EUCHRE_SAFE")
        .map(|v| v.split(',').filter_map(|n| n.parse().ok()).collect())
        .unwrap_or_default();
    match insets[..] {
        [l, t, r, b] => (l, t, window_w - l - r, window_h - t - b),
        _ => (0.0, 0.0, window_w, window_h),
    }
}

/// The window size [`Renderer::margins`] were found for, and the margins.
type SeenMargins = ((f32, f32), Margins);

/// The view for a window of this size, on this device. On a phone the margins
/// only ever grow while the window keeps its size, so bars that come and go
/// don't make the whole table jump.
fn view_for(w: f32, h: f32, window: &Window, seen: &mut SeenMargins) -> View {
    if !device::mobile() {
        return View::fit(w, h);
    }
    let safe = if device::NATIVE_MOBILE { window.safe_area() } else { preview_safe(w, h) };
    let margins = Margins::from_safe(w, h, safe).at_least(minimum_margins());
    *seen = if seen.0 == (w, h) { ((w, h), seen.1.at_least(margins)) } else { ((w, h), margins) };
    let (view, bounds) = View::fit_safe(w, h, seen.1);
    set_bounds(bounds);
    view
}

pub struct Renderer {
    canvas: Canvas,
    background: Shader,
    lamplight: Shader,
    pixel: Shader,
    /// 0 = post-process off, 1 = full effect. Driven by the Atmosphere option.
    pub atmosphere: f32,
    view: View,
    margins: SeenMargins,
}

impl Renderer {
    pub fn new(g: &mut Graphics, window: &Window) -> Result<Self> {
        let (w, h) = g.dimensions();
        let mut margins = ((w, h), Margins::default());
        let view = view_for(w, h, window, &mut margins);
        let (cw, ch) = canvas_size(view, g.dpi_scale());
        Ok(Renderer {
            canvas: new_canvas(g, cw, ch),
            background: g.new_shader(include_str!("shaders/background.wgsl"))?,
            lamplight: g.new_shader(include_str!("shaders/lamplight.wgsl"))?,
            pixel: g.new_shader(include_str!("shaders/pixel.wgsl"))?,
            atmosphere: 1.0,
            view,
            margins,
        })
    }

    pub fn view(&self) -> View {
        self.view
    }

    /// Call once per frame from `update` before anything reads [`Renderer::view`].
    pub fn update(&mut self, g: &Graphics, window: &Window, time: f32) {
        let (w, h) = g.dimensions();
        self.view = view_for(w, h, window, &mut self.margins);
        let (cw, ch) = canvas_size(self.view, g.dpi_scale());
        let (cw, ch) = (cw as f32, ch as f32);
        self.background.send(0, [time, cw, ch, 0.0]);
        let colors = [
            palette::WOOD_DARK,
            palette::WOOD,
            palette::WOOD_LIGHT,
            palette::FELT_DEEP,
            palette::FELT,
            palette::FELT_GLOW,
            palette::LAMP,
        ];
        for (i, c) in colors.into_iter().enumerate() {
            self.background.send(i + 1, c);
        }
        let v = self.view.visible;
        self.background.send(8, [v.x, v.y, v.w, v.h]);
        self.lamplight.send(0, [time, cw, ch, self.atmosphere]);
        // Physical window pixels per canvas pixel, for the sharp-bilinear sampler.
        let px_scale = self.view.scale * g.dpi_scale() / (cw / self.view.visible.w);
        self.lamplight.send(1, [px_scale, 0.0, 0.0, 0.0]);
        self.lamplight.send(2, palette::LAMP);
    }

    /// Runs `draw_scene` in design space on top of the background, then
    /// presents the result to the window through the lamplight shader.
    pub fn frame(&mut self, g: &mut Graphics, draw_scene: impl FnOnce(&mut Graphics)) {
        if !device::mobile() {
            let (w, h) = g.dimensions();
            self.view = View::fit(w, h);
        }
        let (cw, ch) = canvas_size(self.view, g.dpi_scale());
        if self.canvas.dimensions() != (cw, ch) {
            self.canvas = new_canvas(g, cw, ch);
        }
        let visible = self.view.visible;
        let density = cw as f32 / visible.w;

        let (background, pixel) = (&self.background, &self.pixel);
        g.with_canvas(&self.canvas, |g| {
            g.clear(palette::WOOD_DARK);
            g.set_shader(Some(background));
            g.set_color(Color::WHITE);
            g.rectangle(Fill, 0.0, 0.0, cw as f32, ch as f32);
            g.push_all();
            g.set_shader(Some(pixel));
            g.scale(density, density);
            g.translate(-visible.x, -visible.y);
            draw_scene(g);
            g.pop();
            g.set_shader(None);
        });

        g.set_background_color(Color::BLACK);
        g.set_color(Color::WHITE);
        g.set_shader(Some(&self.lamplight));
        let at = self.view.offset + vec2(visible.x, visible.y) * self.view.scale;
        g.draw_ex(&self.canvas, DrawParams::at(at.x, at.y).scale(self.view.scale / density));
        g.set_shader(None);
    }
}

fn new_canvas(g: &mut Graphics, w: u32, h: u32) -> Canvas {
    let canvas = g.new_canvas(w, h);
    // Linear sampling is required by the lamplight shader's sharp-bilinear upscale.
    canvas.set_filter(FilterMode::Linear);
    canvas
}
