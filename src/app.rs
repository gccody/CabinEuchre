//! The love2drs `Game` implementation: wires input, the active scene and the
//! render pipeline together, and applies settings (volume, atmosphere, fullscreen).
//!
//! Input routing: mouse, touch, keyboard and gamepad all end up as scene
//! callbacks in design space. The app tracks which device was used last
//! (`Shared::input`), turns the d-pad, left stick and arrow keys into repeating
//! [`Nav`] moves, and drops the mouse events the OS emulates from a touch so a
//! tap is never handled twice.

use std::time::{Duration, Instant};

use love2drs::prelude::*;

use crate::cards::CardArt;
use crate::palette;
use crate::render::{Renderer, VIEW_H, VIEW_W};
use crate::scene::{Buzz, InputMode, NO_POINTER, Nav, Scene, Shared, Transition};
use crate::scenes::{Overlay, options::OptionsPanel, rules::RulesPanel, table::Table, title::Title};
use crate::settings::Settings;
use crate::sfx::Sfx;
use crate::text::PixelFont;

/// How long after a touch mouse events are treated as emulated.
const TOUCH_MOUSE_GRACE: Duration = Duration::from_millis(600);
/// Stick or d-pad deflection that counts as a direction.
const STICK_THRESHOLD: f32 = 0.55;
/// Held directions repeat after this long, then at this interval.
const REPEAT_DELAY: f32 = 0.38;
const REPEAT_EVERY: f32 = 0.11;
/// Mouse travel (logical px) that switches back to mouse mode.
const MOUSE_WAKE: f32 = 6.0;

pub struct App {
    renderer: Renderer,
    shared: Shared,
    scene: Box<dyn Scene>,
    /// 1 = fully black, fades to 0 after a scene switch.
    fade: f32,
    /// Debug: `EUCHRE_FIXED_DT=n` steps exactly n/60 s per frame, so
    /// screenshot frame numbers map to predictable game time (and n > 1
    /// fast-forwards soak tests).
    fixed_dt: Option<f32>,
    /// The atmosphere strength F2 restores when it turns the effect back on.
    atmosphere_restore: f32,
    /// The finger that acts like the mouse (the first one down).
    primary_touch: Option<TouchId>,
    /// When the last touch event arrived. Mouse events shortly after are the
    /// OS's emulation of that touch and are ignored.
    last_touch: Option<Instant>,
    /// Mouse travel since the player switched away from the mouse, so a tiny
    /// bump of the desk doesn't hide the gamepad focus.
    mouse_travel: f32,
    /// Where the cursor was at the last mouse move. Travel is measured from
    /// here, so the first move event (from wherever the OS thinks it was) is free.
    last_cursor: Option<Vec2>,
    /// When the window last changed size. The cursor "moves" relative to the
    /// window then without the player touching the mouse.
    last_resize: Option<Instant>,
    /// Held-direction auto-repeat for the d-pad and left stick.
    repeat: Repeat,
    /// Whether the player last pressed something on a controller (rather than
    /// clicking or tapping). Gates rumble; mouse motion alone doesn't clear it,
    /// so a nudged mouse (or Steam turning the touchpad into a cursor) can't
    /// silence the controller.
    pad_active: bool,
    /// Rumble pulses waiting for their start time, delays counting down.
    pending_buzz: Vec<Buzz>,
    /// What the motors are doing now: `(low, high, seconds left)`, so a quick
    /// click during a long rumble adds to it instead of cutting it short.
    buzzing: (f32, f32, f32),
}

/// Auto-repeat for a held direction.
#[derive(Default)]
struct Repeat {
    dir: Option<Nav>,
    timer: f32,
}

impl Repeat {
    /// Feeds the direction held this frame; returns a move to make, if any.
    fn step(&mut self, held: Option<Nav>, dt: f32) -> Option<Nav> {
        if held != self.dir {
            self.dir = held;
            self.timer = REPEAT_DELAY;
            return held;
        }
        let dir = held?;
        self.timer -= dt;
        if self.timer <= 0.0 {
            self.timer += REPEAT_EVERY;
            return Some(dir);
        }
        None
    }
}

/// The direction a stick or d-pad vector points, if it is pushed far enough.
fn direction(v: Vec2) -> Option<Nav> {
    if v.x.abs().max(v.y.abs()) < STICK_THRESHOLD {
        None
    } else if v.x.abs() > v.y.abs() {
        Some(if v.x < 0.0 { Nav::Left } else { Nav::Right })
    } else {
        Some(if v.y < 0.0 { Nav::Up } else { Nav::Down })
    }
}

impl App {
    pub fn new(ctx: &mut Context) -> Result<Self> {
        let renderer = Renderer::new(&mut ctx.graphics)?;
        let settings = Settings::load();
        if settings.fullscreen {
            ctx.window.set_fullscreen(true);
        }
        let mut shared = Shared {
            font: PixelFont::new(&ctx.graphics),
            cards: CardArt::new(&ctx.graphics),
            sfx: Sfx::new(&ctx.audio),
            settings,
            time: 0.0,
            mouse: NO_POINTER,
            mouse_down: false,
            input: InputMode::Mouse,
            gamepad: ctx.gamepad.is_connected(),
            rumbles: Vec::new(),
        };
        // Debug: `EUCHRE_INPUT=pad|touch` starts as if that device was used last
        // (screenshots of the focus highlight and the pad hints without a controller).
        match std::env::var("EUCHRE_INPUT").as_deref() {
            Ok("pad") => {
                shared.input = InputMode::Buttons;
                shared.gamepad = true;
            }
            Ok("touch") => shared.input = InputMode::Touch,
            _ => {}
        }
        shared.sfx.set_music_volume(shared.settings.music);
        shared.sfx.start_music();
        // Held arrow keys repeat focus moves (every other key ignores repeats).
        ctx.keyboard.set_key_repeat(true);
        // Debug: `EUCHRE_SCENE=table|options|rules` jumps straight to a screen (for screenshots).
        let scene: Box<dyn Scene> = match std::env::var("EUCHRE_SCENE").as_deref() {
            Ok("table") => Box::new(Table::new(&shared)),
            Ok("options") => Box::new(Title::with_overlay(Overlay::Options(Box::new(OptionsPanel::new())))),
            Ok("rules") => Box::new(Title::with_overlay(Overlay::Rules(Box::new(RulesPanel::new())))),
            _ => Box::new(Title::new()),
        };
        Ok(App {
            renderer,
            shared,
            scene,
            fade: 1.0,
            atmosphere_restore: 1.0,
            primary_touch: None,
            last_touch: None,
            mouse_travel: 0.0,
            last_cursor: None,
            last_resize: None,
            repeat: Repeat::default(),
            pad_active: false,
            pending_buzz: Vec::new(),
            buzzing: (0.0, 0.0, 0.0),
            fixed_dt: std::env::var("EUCHRE_FIXED_DT")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
                .map(|n| n / 60.0),
        })
    }

    fn apply(&mut self, ctx: &mut Context, transition: Transition) {
        match transition {
            Transition::None => {}
            Transition::Switch(next) => {
                self.scene = next;
                self.fade = 1.0;
            }
            Transition::Quit => {
                self.shared.settings.save();
                ctx.quit();
            }
        }
    }

    /// Whether mouse events right now are probably the OS emulating a touch.
    fn touch_recent(&self, ctx: &Context) -> bool {
        ctx.touch.count() > 0 || self.last_touch.is_some_and(|t| t.elapsed() < TOUCH_MOUSE_GRACE)
    }

    fn set_input(&mut self, mode: InputMode) {
        if mode != InputMode::Mouse {
            self.mouse_travel = 0.0;
        }
        self.shared.input = mode;
    }

    fn nav(&mut self, ctx: &mut Context, nav: Nav) {
        self.set_input(InputMode::Buttons);
        let t = self.scene.nav(&mut self.shared, nav);
        self.apply(ctx, t);
    }

    /// The pointer this frame in design space, and whether it is pressed.
    fn pointer(&self, ctx: &Context) -> (Vec2, bool) {
        let view = self.renderer.view();
        match self.shared.input {
            InputMode::Mouse => (view.to_design(ctx.mouse.position()), ctx.mouse.is_down(MouseButton::Left)),
            InputMode::Touch => match self.primary_touch.and_then(|id| ctx.touch.position(id)) {
                Some(p) => (view.to_design(p), true),
                None => (NO_POINTER, false),
            },
            InputMode::Buttons => (NO_POINTER, false),
        }
    }

    /// D-pad and left stick: a move on press, then auto-repeat while held.
    fn poll_directions(&mut self, ctx: &mut Context, dt: f32) {
        if !ctx.window.has_focus() {
            self.repeat = Repeat::default();
            return;
        }
        let pad = &ctx.gamepad;
        let held = direction(pad.dpad()).or_else(|| direction(pad.left_stick()));
        // A d-pad tap shorter than a frame never shows up as held.
        let tapped = [
            (GamepadButton::DPadUp, Nav::Up),
            (GamepadButton::DPadDown, Nav::Down),
            (GamepadButton::DPadLeft, Nav::Left),
            (GamepadButton::DPadRight, Nav::Right),
        ]
        .into_iter()
        .find(|&(b, _)| pad.was_pressed(b))
        .map(|(_, n)| n)
        .filter(|&n| held != Some(n));
        let repeated = self.repeat.step(held, dt);
        for m in tapped.into_iter().chain(repeated) {
            self.pad_active = true;
            self.nav(ctx, m);
        }
    }

    /// Plays the rumble pulses scenes asked for on every pad, once each is
    /// due, if the player is on one. Pulses that overlap play together at the
    /// stronger of each motor.
    fn play_rumble(&mut self, ctx: &mut Context, dt: f32) {
        self.buzzing.2 -= dt;
        if self.buzzing.2 <= 0.0 {
            self.buzzing = (0.0, 0.0, 0.0);
        }
        let new = std::mem::take(&mut self.shared.rumbles);
        if !self.shared.settings.rumble || !self.pad_active {
            self.pending_buzz.clear();
            return;
        }
        self.pending_buzz.extend(new);
        let mut due = None::<(f32, f32, f32)>;
        self.pending_buzz.retain_mut(|b| {
            b.delay -= dt;
            if b.delay > 0.0 {
                return true;
            }
            let (l, h, s) = due.unwrap_or((0.0, 0.0, 0.0));
            due = Some((l.max(b.low), h.max(b.high), s.max(b.seconds)));
            false
        });
        let Some((low, high, seconds)) = due else { return };
        let (bl, bh, left) = self.buzzing;
        // Starting a new vibration replaces the old one, so a click during a
        // long rumble keeps the long one going for the rest of its time.
        let play = (low.max(bl), high.max(bh), seconds.max(left));
        self.buzzing = play;
        let ids: Vec<GamepadId> = ctx.gamepad.ids().collect();
        for id in ids {
            ctx.gamepad.set_vibration(id, play.0, play.1, play.2);
        }
    }

    fn sync_settings(&mut self, ctx: &mut Context) {
        let s = &mut self.shared.settings;
        ctx.audio.set_volume(s.volume);
        self.shared.sfx.set_music_volume(s.music);
        self.renderer.atmosphere = s.atmosphere;
        if ctx.window.is_fullscreen() != s.fullscreen {
            ctx.window.set_fullscreen(s.fullscreen);
        }
    }
}

impl Game for App {
    fn update(&mut self, ctx: &mut Context, dt: f32) {
        let real_dt = dt;
        let dt = self.fixed_dt.unwrap_or(dt);
        self.shared.time += dt;
        self.fade = (self.fade - dt * 3.5).max(0.0);
        self.sync_settings(ctx);
        self.renderer.update(&ctx.graphics, self.shared.time);
        self.poll_directions(ctx, real_dt);
        (self.shared.mouse, self.shared.mouse_down) = self.pointer(ctx);
        let t = self.scene.update(&mut self.shared, ctx, dt);
        self.apply(ctx, t);
        self.play_rumble(ctx, real_dt);
    }

    fn draw(&mut self, ctx: &mut Context) {
        let scene = &mut self.scene;
        let shared = &self.shared;
        let fade = self.fade;
        self.renderer.frame(&mut ctx.graphics, |g| {
            scene.draw(shared, g);
            if fade > 0.0 {
                g.set_color(palette::INK.with_alpha(fade));
                g.rectangle(Fill, 0.0, 0.0, VIEW_W as f32, VIEW_H as f32);
            }
        });
    }

    fn key_pressed(&mut self, ctx: &mut Context, key: Key, repeat: bool) {
        // The arrow keys move the focus, like the d-pad (and auto-repeat).
        let arrow = match key {
            Key::Up => Some(Nav::Up),
            Key::Down => Some(Nav::Down),
            Key::Left => Some(Nav::Left),
            Key::Right => Some(Nav::Right),
            _ => None,
        };
        if let Some(nav) = arrow {
            self.nav(ctx, nav);
            return;
        }
        if repeat {
            return;
        }
        // Once the focus is showing, Enter and Space press the focused control.
        if matches!(key, Key::Enter | Key::KpEnter | Key::Space) && self.shared.input == InputMode::Buttons {
            self.nav(ctx, Nav::Confirm);
            return;
        }
        // Global shortcuts.
        match key {
            Key::F11 => {
                let s = &mut self.shared.settings;
                s.fullscreen = !s.fullscreen;
                s.save();
                return;
            }
            Key::F2 => {
                // Toggle the lamplight post-process (handy for judging raw art).
                let s = &mut self.shared.settings;
                if s.atmosphere > 0.0 {
                    self.atmosphere_restore = s.atmosphere;
                    s.atmosphere = 0.0;
                } else {
                    s.atmosphere = self.atmosphere_restore;
                }
                s.save();
                return;
            }
            _ => {}
        }
        let t = self.scene.key_pressed(&mut self.shared, key);
        self.apply(ctx, t);
    }

    fn mouse_pressed(&mut self, ctx: &mut Context, x: f32, y: f32, button: MouseButton) {
        if self.touch_recent(ctx) {
            return; // the OS's emulated click for a tap that was already handled
        }
        self.set_input(InputMode::Mouse);
        self.pad_active = false;
        let pos = self.renderer.view().to_design(vec2(x, y));
        let t = self.scene.mouse_pressed(&mut self.shared, pos, button);
        self.apply(ctx, t);
    }

    fn mouse_moved(&mut self, ctx: &mut Context, x: f32, y: f32, _dx: f32, _dy: f32) {
        let pos = vec2(x, y);
        let last = self.last_cursor.replace(pos);
        let settling = self.last_resize.is_some_and(|t| t.elapsed() < TOUCH_MOUSE_GRACE);
        if self.shared.input == InputMode::Mouse || self.touch_recent(ctx) || settling {
            return;
        }
        let Some(last) = last else { return };
        self.mouse_travel += (pos - last).abs().element_sum();
        if self.mouse_travel > MOUSE_WAKE {
            self.set_input(InputMode::Mouse);
        }
    }

    fn resize(&mut self, _ctx: &mut Context, _w: f32, _h: f32) {
        self.last_resize = Some(Instant::now());
        self.last_cursor = None;
    }

    fn touch_pressed(&mut self, ctx: &mut Context, id: TouchId, x: f32, y: f32) {
        self.last_touch = Some(Instant::now());
        self.set_input(InputMode::Touch);
        self.pad_active = false;
        // Only the first finger acts; a second one resting on the screen is ignored.
        if self.primary_touch.is_some_and(|p| ctx.touch.get(p).is_some()) {
            return;
        }
        self.primary_touch = Some(id);
        let pos = self.renderer.view().to_design(vec2(x, y));
        self.shared.mouse = pos;
        self.shared.mouse_down = true;
        let t = self.scene.touch_pressed(&mut self.shared, pos);
        self.apply(ctx, t);
    }

    fn touch_moved(&mut self, _ctx: &mut Context, _id: TouchId, _x: f32, _y: f32, _dx: f32, _dy: f32) {
        self.last_touch = Some(Instant::now());
    }

    fn touch_released(&mut self, ctx: &mut Context, id: TouchId, x: f32, y: f32) {
        self.last_touch = Some(Instant::now());
        if self.primary_touch != Some(id) {
            return;
        }
        self.primary_touch = None;
        let pos = self.renderer.view().to_design(vec2(x, y));
        let t = self.scene.touch_released(&mut self.shared, pos);
        self.apply(ctx, t);
    }

    fn gamepad_added(&mut self, ctx: &mut Context, _id: GamepadId) {
        self.shared.gamepad = ctx.gamepad.is_connected();
    }

    fn gamepad_removed(&mut self, ctx: &mut Context, _id: GamepadId) {
        self.shared.gamepad = ctx.gamepad.is_connected();
    }

    fn gamepad_pressed(&mut self, ctx: &mut Context, _id: GamepadId, button: GamepadButton) {
        if !ctx.window.has_focus() {
            return;
        }
        self.pad_active = true;
        let nav = match button {
            GamepadButton::A => Nav::Confirm,
            GamepadButton::B => Nav::Back,
            GamepadButton::Start | GamepadButton::Back => Nav::Menu,
            GamepadButton::Y => Nav::Alt,
            GamepadButton::LeftShoulder | GamepadButton::LeftTrigger => Nav::PrevPage,
            GamepadButton::RightShoulder | GamepadButton::RightTrigger => Nav::NextPage,
            // The d-pad is polled in `update` so it can auto-repeat.
            _ => {
                self.set_input(InputMode::Buttons);
                return;
            }
        };
        self.nav(ctx, nav);
    }

    fn quit(&mut self, _ctx: &mut Context) -> bool {
        self.shared.settings.save();
        false
    }
}
