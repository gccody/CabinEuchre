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
use crate::device;
use crate::juice;
use crate::palette;
use crate::record::PlayerRecord;
use crate::render::{Renderer, VIEW_H, VIEW_W};
use crate::scene::{Buzz, InputMode, NO_POINTER, Nav, Scene, Shared, Transition};
use crate::scenes::trophies::{TrophyBanners, TrophyPanel};
use crate::scenes::{Overlay, lobby::Lobby, options::OptionsPanel, rules::RulesPanel, table::Table, title::Title};
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
/// Frames a second once nothing moves quickly: plenty for the lamp's sway, the
/// dust and the gentle pulses, and a third of the work of 60. A third of 60 and a
/// sixth of 120, so frames stay evenly spaced on the usual displays.
const IDLE_FPS: f32 = 20.0;
/// How long things must have been still before the frame rate drops, so the
/// tail of a motion and the short gaps in a sequence stay smooth.
const SETTLE_TIME: f32 = 0.75;

pub struct App {
    renderer: Renderer,
    shared: Shared,
    scene: Box<dyn Scene>,
    /// Banners for trophies just earned, over every scene.
    banners: TrophyBanners,
    /// 1 = fully black, fades to 0 after a scene switch.
    fade: f32,
    /// Debug: `EUCHRE_FIXED_DT=n` steps exactly n/60 s per frame, so
    /// screenshot frame numbers map to predictable game time (and n > 1
    /// fast-forwards soak tests).
    fixed_dt: Option<f32>,
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
    /// Seconds since anything moved quickly (see [`App::frame_cap`]).
    still_for: f32,
    /// Debug: `EUCHRE_FPS_LOG=1` logs the frame rate every second, and how
    /// much of it was capped: `(frames, capped frames, seconds)` so far.
    fps_log: Option<(u32, u32, f32)>,
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
        let renderer = Renderer::new(&mut ctx.graphics, &ctx.window)?;
        let settings = Settings::load();
        if settings.fullscreen && !device::mobile() {
            ctx.window.set_fullscreen(true);
        }
        let mut shared = Shared {
            font: PixelFont::new(&ctx.graphics),
            cards: CardArt::new(&ctx.graphics),
            sfx: Sfx::new(&ctx.audio),
            settings,
            record: PlayerRecord::load(),
            time: 0.0,
            mouse: NO_POINTER,
            mouse_down: false,
            // A phone has no mouse to wait for, so no mouse-and-keyboard hints.
            input: if device::mobile() { InputMode::Touch } else { InputMode::Mouse },
            rumbles: Vec::new(),
        };
        // Debug: `EUCHRE_INPUT=pad|touch` starts as if that device was used last
        // (screenshots of the focus highlight without a controller).
        match device::var("EUCHRE_INPUT").as_deref() {
            Some("pad") => shared.input = InputMode::Buttons,
            Some("touch") => shared.input = InputMode::Touch,
            _ => {}
        }
        shared.sfx.set_music_volume(shared.settings.music);
        shared.sfx.start_music();
        // Held arrow keys repeat focus moves (every other key ignores repeats).
        ctx.keyboard.set_key_repeat(true);
        crate::platform::init();
        // Everything goes to the store at every launch: it ignores what it has, and this
        // catches whatever was earned offline, signed out or before an earlier failure.
        crate::platform::report_achievements(&shared.record.data.unlocked);
        // Debug: `EUCHRE_SCENE=table|continue|online|options|rules` jumps straight to a screen (for screenshots).
        let scene: Box<dyn Scene> = match device::var("EUCHRE_SCENE").as_deref() {
            // Launched from a friend's Steam invite or "Join Game".
            _ if let Some(request) = crate::platform::take_join_request() => Box::new(Lobby::from_request(&shared, request)),
            Some("table") => Box::new(Table::new(&shared)),
            Some("learn") => {
                let lesson = device::var("EUCHRE_LESSON").and_then(|l| l.parse().ok()).unwrap_or(0);
                let scores = device::var("EUCHRE_LESSON_SCORE")
                    .and_then(|s| {
                        let (us, them) = s.split_once('-')?;
                        Some([us.parse().ok()?, them.parse().ok()?])
                    })
                    .unwrap_or([0, 0]);
                Box::new(Table::tutorial(&shared, lesson, scores))
            }
            Some("online") => Box::new(Lobby::new(&shared)),
            Some("host") => Box::new(Lobby::host(&shared)),
            Some("code") => Box::new(Lobby::entering_code(&shared, "KQJ")),
            Some("continue") => match crate::savegame::load() {
                Some(game) => Box::new(Table::resume(&shared, game)),
                None => Box::new(Title::new()),
            },
            Some("options") => Box::new(Title::with_overlay(Overlay::Options(Box::new(OptionsPanel::new())))),
            Some("rules") => Box::new(Title::with_overlay(Overlay::Rules(Box::new(RulesPanel::new())))),
            Some("trophies") => Box::new(Title::with_overlay(Overlay::Trophies(Box::new(TrophyPanel::new())))),
            _ => Box::new(Title::new()),
        };
        Ok(App {
            renderer,
            shared,
            scene,
            banners: TrophyBanners::new(),
            fade: 1.0,
            primary_touch: None,
            last_touch: None,
            mouse_travel: 0.0,
            last_cursor: None,
            last_resize: None,
            repeat: Repeat::default(),
            pad_active: false,
            pending_buzz: Vec::new(),
            buzzing: (0.0, 0.0, 0.0),
            still_for: 0.0,
            fps_log: device::var("EUCHRE_FPS_LOG").map(|_| (0, 0, 0.0)),
            fixed_dt: device::var("EUCHRE_FIXED_DT")
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
                self.scene.suspend(&mut self.shared);
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

    /// Trophies move between the record, the store and the banners.
    fn follow_trophies(&mut self, dt: f32) {
        let fresh = self.shared.record.take_fresh();
        crate::platform::report_achievements(&fresh);
        self.banners.push(fresh);
        self.shared.record.merge_from_store(crate::platform::take_store_trophies());
        self.banners.update(&mut self.shared, dt);
    }

    /// Draws at the display's full rate while anything moves quickly (springs,
    /// eases, shakes, a finger or button held, a sound just played), and drops
    /// to [`IDLE_FPS`] once it has all been still for [`SETTLE_TIME`]. That's what
    /// saves the battery: the table spends most of its time waiting on someone.
    /// Input wakes the loop at once (love2drs does that), and whatever it moves
    /// lifts the cap from the next frame on.
    fn frame_cap(&mut self, ctx: &mut Context, real_dt: f32) {
        let moving = juice::take_awake() || self.fade > 0.0 || self.shared.mouse_down;
        self.still_for = if moving { 0.0 } else { self.still_for + real_dt };
        // Screenshots and soak runs step a fixed time per frame: no reason to wait.
        let idle = self.still_for > SETTLE_TIME && self.fixed_dt.is_none();
        ctx.timer.set_max_fps(idle.then_some(IDLE_FPS));
        if let Some((frames, capped, seconds)) = &mut self.fps_log {
            *frames += 1;
            *capped += u32::from(idle);
            *seconds += real_dt;
            if *seconds >= 1.0 {
                eprintln!("[euchre] {frames} frames in {seconds:.2} s, {capped} of them at the idle rate");
                self.fps_log = Some((0, 0, 0.0));
            }
        }
    }

    fn sync_settings(&mut self, ctx: &mut Context) {
        let s = &mut self.shared.settings;
        ctx.audio.set_volume(s.volume);
        self.shared.sfx.set_music_volume(s.music);
        self.renderer.atmosphere = s.atmosphere;
        if !device::mobile() && ctx.window.is_fullscreen() != s.fullscreen {
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
        self.renderer.update(&ctx.graphics, &ctx.window, self.shared.time);
        self.poll_directions(ctx, real_dt);
        (self.shared.mouse, self.shared.mouse_down) = self.pointer(ctx);
        crate::platform::update();
        if let Some(request) = crate::platform::take_join_request() {
            // A friend's invite (or "Join Game") from the Steam overlay: leave
            // whatever we're doing (an offline game is saved) and go to their table.
            self.scene.suspend(&mut self.shared);
            self.shared.settings.save();
            let lobby = Lobby::from_request(&self.shared, request);
            self.apply(ctx, Transition::Switch(Box::new(lobby)));
        }
        let t = self.scene.update(&mut self.shared, ctx, dt);
        self.apply(ctx, t);
        self.follow_trophies(dt);
        self.play_rumble(ctx, real_dt);
        self.frame_cap(ctx, real_dt);
    }

    fn draw(&mut self, ctx: &mut Context) {
        let scene = &mut self.scene;
        let banners = &self.banners;
        let shared = &self.shared;
        let fade = self.fade;
        self.renderer.frame(&mut ctx.graphics, |g| {
            scene.draw(shared, g);
            if fade > 0.0 {
                if device::mobile() {
                    crate::ui::dim_screen(g, fade);
                } else {
                    g.set_color(palette::INK.with_alpha(fade));
                    g.rectangle(Fill, 0.0, 0.0, VIEW_W as f32, VIEW_H as f32);
                }
            }
            banners.draw(shared, g);
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
        // Global shortcut.
        if key == Key::F11 {
            let s = &mut self.shared.settings;
            s.fullscreen = !s.fullscreen;
            s.save();
            return;
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

    /// The OS took the finger (a back gesture, a call, losing focus). It isn't a tap and no release
    /// follows. Anything it was dragging lets go next frame, when the pointer is gone. Android reuses
    /// finger ids, so forget this one or the next finger looks like a second one resting on the screen.
    fn touch_cancelled(&mut self, _ctx: &mut Context, id: TouchId, _x: f32, _y: f32) {
        self.last_touch = Some(Instant::now());
        if self.primary_touch == Some(id) {
            self.primary_touch = None;
        }
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

    fn suspended(&mut self, _ctx: &mut Context) {
        self.primary_touch = None;
        self.scene.background(&mut self.shared);
        self.shared.settings.save();
    }

    fn quit(&mut self, _ctx: &mut Context) -> bool {
        self.scene.suspend(&mut self.shared);
        self.shared.settings.save();
        false
    }
}
