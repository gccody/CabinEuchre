//! The love2drs `Game` implementation: wires input, the active scene and the
//! render pipeline together, and applies settings (volume, CRT, fullscreen).

use love2drs::prelude::*;

use crate::cards::CardArt;
use crate::palette;
use crate::render::{Renderer, VIEW_H, VIEW_W};
use crate::scene::{Scene, Shared, Transition};
use crate::scenes::{Overlay, options::OptionsPanel, rules::RulesPanel, table::Table, title::Title};
use crate::settings::Settings;
use crate::sfx::Sfx;
use crate::text::PixelFont;

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
    /// The CRT strength F2 restores when it turns the effect back on.
    crt_restore: f32,
}

impl App {
    pub fn new(ctx: &mut Context) -> Result<Self> {
        let renderer = Renderer::new(&mut ctx.graphics)?;
        let settings = Settings::load();
        if settings.fullscreen {
            ctx.window.set_fullscreen(true);
        }
        let shared = Shared {
            font: PixelFont::new(&ctx.graphics),
            cards: CardArt::new(&ctx.graphics),
            sfx: Sfx::new(&ctx.audio),
            settings,
            time: 0.0,
            mouse: Vec2::ZERO,
            mouse_down: false,
        };
        shared.sfx.set_music_volume(shared.settings.music);
        shared.sfx.start_music();
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
            crt_restore: 1.0,
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

    fn sync_settings(&mut self, ctx: &mut Context) {
        let s = &mut self.shared.settings;
        ctx.audio.set_volume(s.volume);
        self.shared.sfx.set_music_volume(s.music);
        self.renderer.crt_strength = s.crt;
        if ctx.window.is_fullscreen() != s.fullscreen {
            ctx.window.set_fullscreen(s.fullscreen);
        }
    }
}

impl Game for App {
    fn update(&mut self, ctx: &mut Context, dt: f32) {
        let dt = self.fixed_dt.unwrap_or(dt);
        self.shared.time += dt;
        self.fade = (self.fade - dt * 3.5).max(0.0);
        self.sync_settings(ctx);
        self.renderer.update(&ctx.graphics, self.shared.time);
        self.shared.mouse = self.renderer.view().to_design(ctx.mouse.position());
        self.shared.mouse_down = ctx.mouse.is_down(MouseButton::Left);
        let t = self.scene.update(&mut self.shared, ctx, dt);
        self.apply(ctx, t);
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

    fn key_pressed(&mut self, ctx: &mut Context, key: Key, _repeat: bool) {
        // Global shortcuts.
        match key {
            Key::F11 => {
                let s = &mut self.shared.settings;
                s.fullscreen = !s.fullscreen;
                s.save();
                return;
            }
            Key::F2 => {
                // Toggle the CRT effect (handy for judging raw art).
                let s = &mut self.shared.settings;
                if s.crt > 0.0 {
                    self.crt_restore = s.crt;
                    s.crt = 0.0;
                } else {
                    s.crt = self.crt_restore;
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
        let pos = self.renderer.view().to_design(vec2(x, y));
        let t = self.scene.mouse_pressed(&mut self.shared, pos, button);
        self.apply(ctx, t);
    }

    fn quit(&mut self, _ctx: &mut Context) -> bool {
        self.shared.settings.save();
        false
    }
}
