//! The render pipeline every scene draws through:
//!
//! 1. Scenes draw in a fixed `VIEW_W x VIEW_H` (640x360) "design space".
//! 2. That space is rendered into a canvas at the window's own resolution
//!    (letterboxed to 16:9), so rotated and scaled things have smooth edges.
//!    Pixel art (cards, suit icons, the font) is authored at `DENSITY` texels
//!    per design pixel and drawn through `shaders/pixel.wgsl`, which keeps
//!    texels chunky but anti-aliases their edges at any scale or rotation.
//! 3. The canvas starts each frame with the animated swirl background.
//! 4. The canvas is drawn to the window through the CRT post-process shader.
//!
//! Use [`View::to_design`] to convert mouse positions into design space.

use love2drs::prelude::*;

use crate::palette;

/// Design resolution: the coordinate space every scene works in.
pub const VIEW_W: u32 = 640;
pub const VIEW_H: u32 = 360;

/// Pixel-art texels per design pixel (cards, suit icons and the font are
/// authored at this density).
pub const DENSITY: f32 = 1.5;
/// Largest canvas width, to bound memory on huge displays.
const MAX_CANVAS_W: f32 = 3840.0;

/// Where the canvas lands in the window this frame.
#[derive(Debug, Clone, Copy)]
pub struct View {
    pub offset: Vec2,
    /// Window pixels per design pixel.
    pub scale: f32,
}

impl View {
    fn fit(window_w: f32, window_h: f32) -> Self {
        let scale = (window_w / VIEW_W as f32).min(window_h / VIEW_H as f32);
        let offset = vec2(
            ((window_w - VIEW_W as f32 * scale) / 2.0).floor(),
            ((window_h - VIEW_H as f32 * scale) / 2.0).floor(),
        );
        View { offset, scale }
    }

    /// Window coordinates (e.g. `ctx.mouse.position()`) -> design coordinates.
    pub fn to_design(self, window: Vec2) -> Vec2 {
        (window - self.offset) / self.scale
    }
}

/// Canvas size in pixels for a view: the physical size of the letterboxed
/// area, but never below `DENSITY` (so pixel art is never minified).
fn canvas_size(view: View, dpi: f32) -> (u32, u32) {
    let density = (view.scale * dpi).clamp(DENSITY, MAX_CANVAS_W / VIEW_W as f32);
    ((VIEW_W as f32 * density).round() as u32, (VIEW_H as f32 * density).round() as u32)
}

pub struct Renderer {
    canvas: Canvas,
    background: Shader,
    crt: Shader,
    pixel: Shader,
    /// 0 = CRT off, 1 = full effect. Driven by the CRT strength option.
    pub crt_strength: f32,
    /// How strongly the background twists.
    pub swirl: f32,
    view: View,
}

impl Renderer {
    pub fn new(g: &mut Graphics) -> Result<Self> {
        let (w, h) = g.dimensions();
        let view = View::fit(w, h);
        let (cw, ch) = canvas_size(view, g.dpi_scale());
        Ok(Renderer {
            canvas: new_canvas(g, cw, ch),
            background: g.new_shader(include_str!("shaders/background.wgsl"))?,
            crt: g.new_shader(include_str!("shaders/crt.wgsl"))?,
            pixel: g.new_shader(include_str!("shaders/pixel.wgsl"))?,
            crt_strength: 1.0,
            swirl: 1.0,
            view,
        })
    }

    pub fn view(&self) -> View {
        self.view
    }

    /// Call once per frame from `update` before anything reads [`Renderer::view`].
    pub fn update(&mut self, g: &Graphics, time: f32) {
        let (w, h) = g.dimensions();
        self.view = View::fit(w, h);
        let (cw, ch) = canvas_size(self.view, g.dpi_scale());
        let (cw, ch) = (cw as f32, ch as f32);
        self.background.send(0, [time, cw, ch, self.swirl]);
        self.background.send(1, palette::FELT_DEEP);
        self.background.send(2, palette::FELT);
        self.background.send(3, palette::FELT_GLOW);
        self.crt.send(0, [time, cw, ch, self.crt_strength]);
        // Physical window pixels per canvas pixel, for the sharp-bilinear sampler.
        let px_scale = self.view.scale * g.dpi_scale() / (cw / VIEW_W as f32);
        self.crt.send(1, [px_scale, VIEW_H as f32, 0.0, 0.0]);
    }

    /// Runs `draw_scene` in design space on top of the background, then
    /// presents the result to the window through the CRT shader.
    pub fn frame(&mut self, g: &mut Graphics, draw_scene: impl FnOnce(&mut Graphics)) {
        let (w, h) = g.dimensions();
        self.view = View::fit(w, h);
        let (cw, ch) = canvas_size(self.view, g.dpi_scale());
        if self.canvas.dimensions() != (cw, ch) {
            self.canvas = new_canvas(g, cw, ch);
        }
        let density = cw as f32 / VIEW_W as f32;

        let (background, pixel) = (&self.background, &self.pixel);
        g.with_canvas(&self.canvas, |g| {
            g.clear(palette::FELT_DEEP);
            g.set_shader(Some(background));
            g.set_color(Color::WHITE);
            g.rectangle(Fill, 0.0, 0.0, cw as f32, ch as f32);
            g.push_all();
            g.set_shader(Some(pixel));
            g.scale(density, density);
            draw_scene(g);
            g.pop();
            g.set_shader(None);
        });

        g.set_background_color(Color::BLACK);
        g.set_color(Color::WHITE);
        g.set_shader(Some(&self.crt));
        g.draw_ex(
            &self.canvas,
            DrawParams::at(self.view.offset.x, self.view.offset.y).scale(self.view.scale / density),
        );
        g.set_shader(None);
    }
}

fn new_canvas(g: &mut Graphics, w: u32, h: u32) -> Canvas {
    let canvas = g.new_canvas(w, h);
    // Linear sampling is required by the CRT shader's sharp-bilinear upscale.
    canvas.set_filter(FilterMode::Linear);
    canvas
}
