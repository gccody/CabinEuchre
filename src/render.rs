//! The render pipeline every scene draws through:
//!
//! 1. Scenes draw in a fixed `VIEW_W x VIEW_H` (640x360) "design space".
//! 2. That space is rendered at `DENSITY`x into a `CANVAS_W x CANVAS_H`
//!    (960x540) pixel canvas, so pixel art authored at canvas density (cards,
//!    suit icons) gets 1.5x the detail while layouts keep their coordinates.
//! 3. The canvas starts each frame with the animated swirl background.
//! 4. The canvas is scaled to fit the window (letterboxed) through the CRT
//!    post-process shader, which samples with "sharp bilinear" filtering so
//!    pixels stay crisp and even at any window size.
//!
//! Use [`View::to_design`] to convert mouse positions into design space.

use love2drs::prelude::*;

use crate::palette;

/// Design resolution: the coordinate space every scene works in.
pub const VIEW_W: u32 = 640;
pub const VIEW_H: u32 = 360;

/// Canvas pixels per design pixel.
pub const DENSITY: f32 = 1.5;
/// The pixel canvas the design space is rendered into.
pub const CANVAS_W: u32 = 960;
pub const CANVAS_H: u32 = 540;

/// Rounds a design-space coordinate to the nearest canvas pixel.
pub fn snap(v: f32) -> f32 {
    (v * DENSITY).round() / DENSITY
}

/// Where the canvas lands in the window this frame.
#[derive(Debug, Clone, Copy)]
pub struct View {
    pub offset: Vec2,
    /// Window pixels per canvas pixel.
    pub scale: f32,
}

impl View {
    fn fit(window_w: f32, window_h: f32) -> Self {
        let scale = (window_w / CANVAS_W as f32).min(window_h / CANVAS_H as f32);
        let offset = vec2(
            ((window_w - CANVAS_W as f32 * scale) / 2.0).floor(),
            ((window_h - CANVAS_H as f32 * scale) / 2.0).floor(),
        );
        View { offset, scale }
    }

    /// Window coordinates (e.g. `ctx.mouse.position()`) -> design coordinates.
    pub fn to_design(self, window: Vec2) -> Vec2 {
        (window - self.offset) / self.scale / DENSITY
    }
}

pub struct Renderer {
    canvas: Canvas,
    background: Shader,
    crt: Shader,
    /// 0 = CRT off, 1 = full effect. Driven by the CRT strength option.
    pub crt_strength: f32,
    /// How strongly the background twists.
    pub swirl: f32,
    view: View,
}

impl Renderer {
    pub fn new(g: &mut Graphics) -> Result<Self> {
        let canvas = g.new_canvas(CANVAS_W, CANVAS_H);
        // Linear sampling is required by the shader's sharp-bilinear upscale.
        canvas.set_filter(FilterMode::Linear);
        let (w, h) = g.dimensions();
        Ok(Renderer {
            canvas,
            background: g.new_shader(include_str!("shaders/background.wgsl"))?,
            crt: g.new_shader(include_str!("shaders/crt.wgsl"))?,
            crt_strength: 1.0,
            swirl: 1.0,
            view: View::fit(w, h),
        })
    }

    pub fn view(&self) -> View {
        self.view
    }

    /// Call once per frame from `update` before anything reads [`Renderer::view`].
    pub fn update(&mut self, g: &Graphics, time: f32) {
        let (w, h) = g.dimensions();
        self.view = View::fit(w, h);
        let (cw, ch) = (CANVAS_W as f32, CANVAS_H as f32);
        self.background.send(0, [time, cw, ch, self.swirl]);
        self.background.send(1, palette::FELT_DEEP);
        self.background.send(2, palette::FELT);
        self.background.send(3, palette::FELT_GLOW);
        self.crt.send(0, [time, cw, ch, self.crt_strength]);
        // Physical window pixels per canvas pixel, for the sharp-bilinear sampler.
        self.crt.send(1, [self.view.scale * g.dpi_scale(), VIEW_H as f32, 0.0, 0.0]);
    }

    /// Runs `draw_scene` in design space on top of the background, then
    /// presents the result to the window through the CRT shader.
    pub fn frame(&mut self, g: &mut Graphics, draw_scene: impl FnOnce(&mut Graphics)) {
        let (w, h) = g.dimensions();
        self.view = View::fit(w, h);

        let background = &self.background;
        g.with_canvas(&self.canvas, |g| {
            g.clear(palette::FELT_DEEP);
            g.set_shader(Some(background));
            g.set_color(Color::WHITE);
            g.rectangle(Fill, 0.0, 0.0, CANVAS_W as f32, CANVAS_H as f32);
            g.set_shader(None);
            g.push_all();
            g.scale(DENSITY, DENSITY);
            draw_scene(g);
            g.pop();
        });

        g.set_background_color(Color::BLACK);
        g.set_color(Color::WHITE);
        g.set_shader(Some(&self.crt));
        g.draw_ex(
            &self.canvas,
            DrawParams::at(self.view.offset.x, self.view.offset.y).scale(self.view.scale),
        );
        g.set_shader(None);
    }
}
