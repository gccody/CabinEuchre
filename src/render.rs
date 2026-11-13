//! The render pipeline every scene draws through:
//!
//! 1. Scenes draw in a fixed `VIEW_W x VIEW_H` (640x360) "design space".
//! 2. That space is drawn straight into the window at its own resolution
//!    (letterboxed to 16:9), so rotated and scaled things have smooth edges.
//!    Pixel art (cards, suit icons, the font) is authored at `DENSITY` texels
//!    per design pixel and drawn through `shaders/pixel.wgsl`, which keeps
//!    texels chunky but anti-aliases their edges at any scale or rotation.
//! 3. Each frame starts with the cabin tabletop (`background.wgsl`): walnut
//!    planks, a felt mat and a slowly swaying lamp. It's as chunky as the pixel
//!    art, so it's worked out once per texel into a small canvas, which
//!    `table.wgsl` lays under the scene (on a phone, a tenth of the shader work).
//!    The lamp and the dust drift slowly, so that small canvas is only redrawn
//!    [`TABLE_FPS`] times a second, however fast the display is.
//! 4. Everything drawn gets the lamplight grade (`grade.wgsl`: warm grade,
//!    vignette and film grain) in its own shader, so a frame is one pass over
//!    the window rather than a scene canvas and a pass to grade it.
//! 5. A window with fewer pixels than the pixel art has texels (under 960x540)
//!    is the exception: there the scene goes into a canvas at `DENSITY`, which
//!    `lamplight.wgsl` shrinks and grades on its way to the window.
//!
//! Use [`View::to_design`] to convert mouse positions into design space.
//!
//! On phones the design space sits inside the OS's safe area (clear of notches
//! and the gesture bar) instead of being letterboxed, and the view covers
//! the whole screen, so the tabletop runs edge to edge. [`bounds`] says how
//! far past 640x360 the player can see and reach, so the table can spread out.

use std::sync::atomic::{AtomicU32, Ordering};

use love2drs::prelude::*;
use love2drs::window::{ReservedKind, ReservedRegion, Window};

use crate::device;
use crate::juice::approach;
use crate::palette;
use crate::ui::Rect;

/// Design resolution: the coordinate space every scene works in.
pub const VIEW_W: u32 = 640;
pub const VIEW_H: u32 = 360;

/// Pixel-art texels per design pixel (cards, suit icons and the font are
/// authored at this density).
pub const DENSITY: f32 = 1.5;
/// How often the tabletop is worked out again. Its fastest motion is the lamp's
/// pool of light at under 12 texels a second, so no texel changes more often.
const TABLE_FPS: f32 = 30.0;

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

/// A fold running down the screen, in design space, while a foldable is partly open.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fold {
    pub left: f32,
    pub right: f32,
    /// How far the layouts have moved out of its way, from 0 to 1, so nothing snaps.
    pub amount: f32,
}

impl Fold {
    pub fn center(&self) -> f32 {
        (self.left + self.right) / 2.0
    }

    pub fn half_width(&self) -> f32 {
        (self.right - self.left) / 2.0
    }
}

/// `left`, `right` and `amount` of the fold; an `amount` of 0 means there isn't one.
static FOLD: [AtomicU32; 3] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];

/// The fold the table should keep clear of. Only ever on a partly open foldable.
pub fn fold() -> Option<Fold> {
    if !device::mobile() {
        return None;
    }
    let [left, right, amount] = [0, 1, 2].map(|i| f32::from_bits(FOLD[i].load(Ordering::Relaxed)));
    (amount > 0.0).then_some(Fold { left, right, amount })
}

fn set_fold(fold: Option<Fold>) {
    let Fold { left, right, amount } = fold.unwrap_or(Fold { left: 0.0, right: 0.0, amount: 0.0 });
    for (slot, v) in FOLD.iter().zip([left, right, amount]) {
        slot.store(v.to_bits(), Ordering::Relaxed);
    }
}

/// The fold in `regions` as `(left, right)` in design space. The game is locked to
/// landscape, so only a fold running down the screen divides it.
fn fold_in(regions: &[ReservedRegion], view: &View) -> Option<(f32, f32)> {
    let region = regions.iter().find(|r| r.kind == ReservedKind::Division && r.height >= r.width)?;
    let design = |x: f32| (x - view.offset.x) / view.scale;
    Some((design(region.x), design(region.x + region.width)))
}

/// Where the design space lands in the window this frame.
#[derive(Debug, Clone, Copy)]
pub struct View {
    /// Window position of the design space's origin.
    pub offset: Vec2,
    /// Window pixels per design pixel.
    pub scale: f32,
    /// The part of design space that is drawn: 640x360 on a desktop, the
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

    /// Phone: how large 640x360 can be inside the margins. `reserved` includes
    /// space for bars that aren't showing yet, so the size never changes when they do.
    fn safe_scale(window_w: f32, window_h: f32, reserved: Margins) -> f32 {
        let Margins { side, top, bottom } = reserved;
        let (usable_w, usable_h) = ((window_w - side * 2.0).max(1.0), (window_h - top - bottom).max(1.0));
        (usable_w / VIEW_W as f32).min(usable_h / VIEW_H as f32)
    }

    /// Phone: where the top of 640x360 goes: centered in the space that's really
    /// clear right now, so with no bar showing it's centered on the screen.
    fn safe_y(window_h: f32, clear: Margins, scale: f32) -> f32 {
        let usable_h = (window_h - clear.top - clear.bottom).max(1.0);
        (clear.top + (usable_h - VIEW_H as f32 * scale) / 2.0).floor()
    }

    /// Phone: the design space at this scale and height, with the view covering
    /// the whole screen. The returned bounds stay out of the `reserved` margins.
    fn placed(window_w: f32, window_h: f32, reserved: Margins, scale: f32, y: f32) -> (Self, Rect) {
        let Margins { side, top, bottom } = reserved;
        let (usable_w, usable_h) = ((window_w - side * 2.0).max(1.0), (window_h - top - bottom).max(1.0));
        let offset = vec2(((window_w - VIEW_W as f32 * scale) / 2.0).floor(), y);
        let visible = Rect::new(-offset.x / scale, -offset.y / scale, window_w / scale, window_h / scale);
        let bounds = Rect::new((side - offset.x) / scale, (top - offset.y) / scale, usable_w / scale, usable_h / scale);
        (View { offset, scale, visible }, bounds)
    }

    /// Window coordinates (e.g. `ctx.mouse.position()`) -> design coordinates.
    pub fn to_design(self, window: Vec2) -> Vec2 {
        (window - self.offset) / self.scale
    }
}

/// Whether a view has at least a window pixel for every texel of the pixel art,
/// so the scene can be drawn straight into the window.
fn fits_window(view: View, dpi: f32) -> bool {
    view.scale * dpi >= DENSITY
}

/// The scene canvas's size in pixels for a view that doesn't fit the window:
/// `DENSITY` pixels per design pixel, so pixel art is never minified.
fn canvas_size(view: View) -> (u32, u32) {
    ((view.visible.w * DENSITY).round() as u32, (view.visible.h * DENSITY).round() as u32)
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
/// with the first touch), so its height is set aside from the start. Previews
/// can do the same with `EUCHRE_RESERVE=24`.
fn minimum_margins() -> Margins {
    let bottom = if cfg!(target_os = "android") {
        24.0
    } else {
        std::env::var("EUCHRE_RESERVE").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0)
    };
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

/// Debug: `EUCHRE_FOLD=left,right` (window units) pretends a foldable is partly open
/// when previewing the phone layouts on a desktop.
fn preview_regions(window_h: f32) -> Vec<ReservedRegion> {
    let edges: Vec<f32> = std::env::var("EUCHRE_FOLD")
        .map(|v| v.split(',').filter_map(|n| n.parse().ok()).collect())
        .unwrap_or_default();
    match edges[..] {
        [left, right] => vec![ReservedRegion { kind: ReservedKind::Division, x: left, y: 0.0, width: right - left, height: window_h }],
        _ => Vec::new(),
    }
}

/// Where a phone's view sits, remembered from frame to frame.
#[derive(Debug, Clone, Copy, Default)]
struct Placement {
    /// The window size the rest was found for.
    window: (f32, f32),
    /// The bars and cutouts seen so far (not counting space set aside for later).
    clear: Margins,
    /// The view's height in the window. `None` until there's a window to fit.
    y: Option<f32>,
    /// The fold last seen, as `(left, right)`, and how far the layouts have moved for it.
    fold: Option<(f32, f32)>,
    fold_amount: f32,
}

impl Placement {
    /// Takes in this frame's margins, which only ever grow while the window keeps
    /// its size, so bars that come and go don't keep moving the game.
    fn see(&mut self, window: (f32, f32), margins: Margins) {
        if self.window == window {
            self.clear = self.clear.at_least(margins);
        } else {
            *self = Placement { window, clear: margins, ..Placement::default() };
        }
    }

    /// Glides toward `target`, so a bar appearing moves the game smoothly.
    fn glide(&mut self, target: f32, dt: f32) -> f32 {
        let y = match self.y {
            Some(y) if (y - target).abs() >= 0.5 => approach(y, target, 14.0, dt),
            _ => target,
        };
        self.y = Some(y);
        y.floor()
    }

    /// Eases toward this frame's fold, and keeps the last one while easing away from it.
    fn ease_fold(&mut self, seen: Option<(f32, f32)>, dt: f32) -> Option<Fold> {
        self.fold = seen.or(self.fold);
        let target = if seen.is_some() { 1.0 } else { 0.0 };
        self.fold_amount = approach(self.fold_amount, target, 14.0, dt);
        if (self.fold_amount - target).abs() < 0.002 {
            self.fold_amount = target;
        }
        let (left, right) = self.fold?;
        (self.fold_amount > 0.0).then_some(Fold { left, right, amount: self.fold_amount })
    }
}

/// The view for a window of this size, on this device.
fn view_for(w: f32, h: f32, window: &Window, placement: &mut Placement, dt: f32) -> View {
    if !device::mobile() {
        return View::fit(w, h);
    }
    let safe = if device::NATIVE_MOBILE { window.safe_area() } else { preview_safe(w, h) };
    placement.see((w, h), Margins::from_safe(w, h, safe));
    let clear = placement.clear;
    let reserved = clear.at_least(minimum_margins());
    let scale = View::safe_scale(w, h, reserved);
    let y = placement.glide(View::safe_y(h, clear, scale), dt);
    let (view, bounds) = View::placed(w, h, reserved, scale, y);
    set_bounds(bounds);
    let regions = if device::NATIVE_MOBILE { window.reserved_regions() } else { preview_regions(h) };
    set_fold(placement.ease_fold(fold_in(&regions, &view), dt));
    view
}

pub struct Renderer {
    /// What the scene is drawn into when the window is too small for it (see
    /// [`fits_window`]). `None` otherwise, which is nearly always.
    canvas: Option<Canvas>,
    /// The tabletop at one pixel per texel (see [`table_texels`]).
    table: Canvas,
    /// What `table` was last drawn for, or `None` if it has to be drawn.
    table_drawn: Option<TableKey>,
    /// Which [`TABLE_FPS`] step `time` is in.
    table_step: f32,
    background: Shader,
    /// Lays `table` under the scene.
    table_blit: Shader,
    lamplight: Shader,
    pixel: Shader,
    /// 0 = lamplight grade off, 1 = full effect. Driven by the Atmosphere option.
    pub atmosphere: f32,
    view: View,
    placement: Placement,
    /// `time` at the last [`Renderer::update`].
    last_time: f32,
}

/// A shader that takes part in the lamplight grade, which is appended to it.
fn graded_shader(g: &mut Graphics, effect: &str) -> Result<Shader> {
    g.new_shader(&format!("{effect}\n{}", include_str!("shaders/grade.wgsl")))
}

impl Renderer {
    pub fn new(g: &mut Graphics, window: &Window) -> Result<Self> {
        let (w, h) = g.dimensions();
        let mut placement = Placement::default();
        let view = view_for(w, h, window, &mut placement, 0.0);
        let (_, (tw, th)) = table_texels(view.visible);
        Ok(Renderer {
            canvas: None,
            table: new_table_canvas(g, tw, th),
            table_drawn: None,
            table_step: 0.0,
            background: graded_shader(g, include_str!("shaders/background.wgsl"))?,
            table_blit: graded_shader(g, include_str!("shaders/table.wgsl"))?,
            lamplight: graded_shader(g, include_str!("shaders/lamplight.wgsl"))?,
            pixel: graded_shader(g, include_str!("shaders/pixel.wgsl"))?,
            atmosphere: 1.0,
            view,
            placement,
            last_time: 0.0,
        })
    }

    pub fn view(&self) -> View {
        self.view
    }

    /// Call once per frame from `update` before anything reads [`Renderer::view`].
    pub fn update(&mut self, g: &Graphics, window: &Window, time: f32) {
        let (w, h) = g.dimensions();
        let dt = (time - self.last_time).max(0.0);
        self.last_time = time;
        self.view = view_for(w, h, window, &mut self.placement, dt);
        self.background.send(0, [time, 0.0, 0.0, 0.0]);
        // The quarter step keeps a time that lands right on a step (as every other
        // frame of a fixed-step screenshot run does) on the same side of it each time.
        self.table_step = (time * TABLE_FPS + 0.25).floor();
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
    }

    /// Runs `draw_scene` in design space on top of the tabletop, lit by the lamp.
    pub fn frame(&mut self, g: &mut Graphics, draw_scene: impl FnOnce(&mut Graphics)) {
        if !device::mobile() {
            let (w, h) = g.dimensions();
            self.view = View::fit(w, h);
        }
        let direct = fits_window(self.view, g.dpi_scale());
        self.draw_table(g, direct);
        g.set_background_color(Color::BLACK);
        if direct {
            self.canvas = None;
            self.draw_to_window(g, draw_scene);
        } else {
            self.draw_through_canvas(g, draw_scene);
        }
    }

    /// Redraws the tabletop's canvas if it's due, and tells `table.wgsl` about it.
    /// `graded` bakes the lamplight's warm tint and vignette into it, for laying it
    /// straight into the window.
    fn draw_table(&mut self, g: &mut Graphics, graded: bool) {
        let visible = self.view.visible;
        let (first, (tw, th)) = table_texels(visible);
        if self.table.dimensions() != (tw, th) {
            self.table = new_table_canvas(g, tw, th);
            self.table_drawn = None;
        }
        let key = TableKey {
            step: self.table_step,
            visible: [visible.x, visible.y, visible.w, visible.h],
            graded: graded.then_some(self.atmosphere),
        };
        if self.table_drawn != Some(key) {
            self.table_drawn = Some(key);
            let background = &self.background;
            background.send(9, [first.x, first.y, 0.0, 0.0]);
            background.send(10, [self.atmosphere, if graded { 1.0 } else { 0.0 }, 0.0, 0.0]);
            g.with_canvas(&self.table, |g| {
                // Cleared rather than loaded: every pixel is drawn, and a phone's GPU
                // would otherwise fetch the old ones first.
                g.clear(palette::WOOD_DARK);
                g.push_all();
                // Alpha carries the luma, so it's written rather than blended with.
                g.set_blend_mode(BlendMode::Replace);
                g.set_shader(Some(background));
                g.set_color(Color::WHITE);
                g.rectangle(Fill, 0.0, 0.0, tw as f32, th as f32);
                g.pop();
            });
        }
        self.table_blit.send(1, [visible.x, visible.y, visible.w, visible.h]);
        self.table_blit.send(2, [tw as f32 - 1.0, th as f32 - 1.0, 0.0, 0.0]);
    }

    /// Tells the shaders that draw the scene whether to grade it themselves, and
    /// where the view is in the window (`origin` and `size`, in pixels).
    fn send_grade(&self, here: bool, origin: Vec2, size: Vec2) {
        // The grain changes 12 times a second.
        let grain = (self.last_time * 12.0).floor();
        for shader in [&self.table_blit, &self.pixel] {
            shader.send(12, [grain, self.atmosphere, if here { 1.0 } else { 0.0 }, 0.0]);
            shader.send(13, [origin.x, origin.y, size.x, size.y]);
            shader.send(14, palette::LAMP);
        }
    }

    /// The usual way: the tabletop and the scene go straight into the window, each
    /// draw grading its own color.
    fn draw_to_window(&mut self, g: &mut Graphics, draw_scene: impl FnOnce(&mut Graphics)) {
        let dpi = g.dpi_scale();
        let visible = self.view.visible;
        let (first, (tw, th)) = table_texels(visible);
        // Window pixels per design pixel, and the view's rect in window pixels. It
        // starts on a whole pixel, so the pixel art's texels land where a canvas's would.
        let density = self.view.scale * dpi;
        let origin = ((self.view.offset + vec2(visible.x, visible.y) * self.view.scale) * dpi).round();
        let size = vec2(visible.w, visible.h) * density;
        self.send_grade(true, origin, size);
        self.table_blit.send(0, [size.x, size.y, first.x, first.y]);
        self.table_blit.send(3, [origin.x, origin.y, 0.0, 0.0]);

        // The same rect in the window's own units, which is what drawing takes.
        let (at, extent) = (origin / dpi, size / dpi);
        g.push_all();
        // Design space can be drawn past its edges (a card flying in), and on a
        // desktop the window can be wider or taller than the view.
        g.set_scissor(at.x, at.y, extent.x, extent.y);
        g.set_color(Color::WHITE);
        g.set_shader(Some(&self.table_blit));
        g.draw_ex(&self.table, DrawParams::at(at.x, at.y).scale_xy(extent.x / tw as f32, extent.y / th as f32));
        g.set_shader(Some(&self.pixel));
        g.translate(at.x, at.y);
        g.scale(density / dpi, density / dpi);
        g.translate(-visible.x, -visible.y);
        draw_scene(g);
        g.pop();
    }

    /// A window too small for the pixel art: the scene goes into a canvas at
    /// `DENSITY`, which the lamplight shader shrinks and grades into the window.
    fn draw_through_canvas(&mut self, g: &mut Graphics, draw_scene: impl FnOnce(&mut Graphics)) {
        let visible = self.view.visible;
        let (cw, ch) = canvas_size(self.view);
        if self.canvas.as_ref().is_none_or(|c| c.dimensions() != (cw, ch)) {
            self.canvas = Some(new_canvas(g, cw, ch));
        }
        let Some(canvas) = &self.canvas else { return };
        let (first, (tw, th)) = table_texels(visible);
        let (cw, ch) = (cw as f32, ch as f32);
        let density = cw / visible.w;
        self.send_grade(false, Vec2::ZERO, vec2(cw, ch));
        self.table_blit.send(0, [cw, ch, first.x, first.y]);
        self.table_blit.send(3, [0.0; 4]);
        self.lamplight.send(0, [self.last_time, cw, ch, self.atmosphere]);
        // Physical window pixels per canvas pixel, for the sharp-bilinear sampler.
        self.lamplight.send(1, [self.view.scale * g.dpi_scale() / density, 0.0, 0.0, 0.0]);
        self.lamplight.send(2, palette::LAMP);

        let (table_blit, pixel, table) = (&self.table_blit, &self.pixel, &self.table);
        g.with_canvas(canvas, |g| {
            g.clear(palette::WOOD_DARK);
            g.set_color(Color::WHITE);
            g.set_shader(Some(table_blit));
            g.draw_ex(table, DrawParams::at(0.0, 0.0).scale_xy(cw / tw as f32, ch / th as f32));
            g.push_all();
            g.set_shader(Some(pixel));
            g.scale(density, density);
            g.translate(-visible.x, -visible.y);
            draw_scene(g);
            g.pop();
            g.set_shader(None);
        });

        g.set_color(Color::WHITE);
        g.set_shader(Some(&self.lamplight));
        let at = self.view.offset + vec2(visible.x, visible.y) * self.view.scale;
        g.draw_ex(canvas, DrawParams::at(at.x, at.y).scale(self.view.scale / density));
        g.set_shader(None);
    }
}

/// What the tabletop canvas was drawn for. It's drawn again when any of it changes.
#[derive(Debug, Clone, Copy, PartialEq)]
struct TableKey {
    step: f32,
    visible: [f32; 4],
    /// The lamplight strength graded into it, if it is.
    graded: Option<f32>,
}

/// The tabletop's texels that cover the visible part of design space: the first
/// one's index (counted from design space's origin) and how many across and down.
fn table_texels(visible: Rect) -> (Vec2, (u32, u32)) {
    let first = (vec2(visible.x, visible.y) * DENSITY).floor();
    let last = (vec2(visible.x + visible.w, visible.y + visible.h) * DENSITY).ceil();
    let size = (last - first).max(Vec2::ONE);
    (first, (size.x as u32, size.y as u32))
}

fn new_table_canvas(g: &mut Graphics, w: u32, h: u32) -> Canvas {
    let canvas = g.new_canvas(w, h);
    // `table.wgsl` reads whole texels, so no filtering is wanted.
    canvas.set_filter(FilterMode::Nearest);
    canvas
}

fn new_canvas(g: &mut Graphics, w: u32, h: u32) -> Canvas {
    let canvas = g.new_canvas(w, h);
    // Linear sampling is required by the lamplight shader's sharp-bilinear upscale.
    canvas.set_filter(FilterMode::Linear);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A phone in landscape with 24 units set aside at the bottom, as on Android.
    const W: f32 = 1280.0;
    const H: f32 = 590.0;
    const RESERVED: Margins = Margins { side: 0.0, top: 0.0, bottom: 24.0 };

    fn center_y(clear: Margins) -> f32 {
        let scale = View::safe_scale(W, H, RESERVED);
        View::safe_y(H, clear, scale) + VIEW_H as f32 * scale / 2.0
    }

    #[test]
    fn no_bar_showing_centers_on_the_screen() {
        assert!((center_y(Margins::default()) - H / 2.0).abs() <= 1.0);
    }

    #[test]
    fn a_bar_showing_centers_above_it() {
        assert!((center_y(RESERVED) - (H - RESERVED.bottom) / 2.0).abs() <= 1.0);
    }

    #[test]
    fn the_size_stays_the_same_when_the_bar_appears() {
        let scale = View::safe_scale(W, H, Margins::default().at_least(RESERVED));
        assert_eq!(scale, View::safe_scale(W, H, RESERVED.at_least(RESERVED)));
    }

    #[test]
    fn a_fold_down_the_screen_lands_in_design_space() {
        let view = View { offset: vec2(0.0, 50.0), scale: 951.0 / 640.0, visible: DESIGN_RECT };
        let region = |kind, width, height| ReservedRegion { kind, x: 455.5, y: 0.0, width, height };
        let (left, right) = fold_in(&[region(ReservedKind::Division, 40.0, 669.0)], &view).unwrap();
        assert!((left - 306.5).abs() < 0.1 && (right - 333.5).abs() < 0.1);
        assert_eq!(fold_in(&[region(ReservedKind::Occlusion, 40.0, 40.0)], &view), None);
        assert_eq!(fold_in(&[region(ReservedKind::Division, 669.0, 40.0)], &view), None);
    }

    #[test]
    fn a_fold_eases_in_and_out_and_then_goes_away() {
        let mut placement = Placement::default();
        let eased = placement.ease_fold(Some((306.0, 334.0)), 0.016).unwrap();
        assert!(eased.amount > 0.0 && eased.amount < 1.0);
        for _ in 0..200 {
            placement.ease_fold(Some((306.0, 334.0)), 0.016);
        }
        assert_eq!(placement.fold_amount, 1.0);
        let leaving = placement.ease_fold(None, 0.016).unwrap();
        assert!(leaving.amount > 0.0 && leaving.amount < 1.0 && leaving.left == 306.0);
        for _ in 0..200 {
            placement.ease_fold(None, 0.016);
        }
        assert_eq!(placement.ease_fold(None, 0.016), None);
    }

    #[test]
    fn bounds_stay_out_of_the_reserved_space_wherever_the_view_sits() {
        let scale = View::safe_scale(W, H, RESERVED);
        for y in [0.0, 12.0] {
            let (view, bounds) = View::placed(W, H, RESERVED, scale, y);
            let bottom = view.offset.y + (bounds.y + bounds.h) * scale;
            assert!((bottom - (H - RESERVED.bottom)).abs() < 0.01);
        }
    }

    #[test]
    fn a_bar_appearing_glides_and_then_settles() {
        let mut p = Placement { y: Some(12.0), ..Placement::default() };
        let first = p.glide(0.0, 1.0 / 60.0);
        assert!(first > 0.0 && first < 12.0, "moves part of the way: {first}");
        for _ in 0..120 {
            p.glide(0.0, 1.0 / 60.0);
        }
        assert_eq!(p.y, Some(0.0));
    }

    #[test]
    fn margins_only_grow_until_the_window_changes() {
        let mut p = Placement::default();
        p.see((W, H), RESERVED);
        p.see((W, H), Margins::default());
        assert_eq!(p.clear, RESERVED);
        p.y = Some(5.0);
        p.see((H, W), Margins::default());
        assert_eq!(p.clear, Margins::default());
        assert_eq!(p.y, None, "a new window size places the view right away");
    }
}
