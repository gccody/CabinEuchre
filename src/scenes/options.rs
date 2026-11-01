//! The options panel (opened from the title screen or the pause menu).
//! Volume, music and atmosphere are sliders; the rest step with arrows.
//! Changes apply live (the app syncs settings every frame) and are saved on close.
//!
//! With a gamepad or the arrow keys, up/down picks a row (the BACK plaque is
//! the last stop) and left/right changes it.

use love2drs::prelude::*;

use crate::juice::Spring;
use crate::palette;
use crate::scene::{Nav, Shared};
use crate::sfx::Sound;
use crate::settings::{SPEED_NAMES, Settings, TARGETS};
use crate::text::Text;
use crate::ui::{self, Button, Rect, Slider};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opt {
    Volume,
    Music,
    Atmosphere,
    Speed,
    Target,
    Stick,
    Fullscreen,
    Rumble,
}

const OPTS: [Opt; 8] = [
    Opt::Volume,
    Opt::Music,
    Opt::Atmosphere,
    Opt::Speed,
    Opt::Target,
    Opt::Stick,
    Opt::Fullscreen,
    Opt::Rumble,
];

const PANEL: Rect = Rect::new(140.0, 40.0, 360.0, 280.0);
const ROW_Y: f32 = PANEL.y + 46.0;
const ROW_H: f32 = 24.0;
/// Left edge and width of the control column.
const CTRL_X: f32 = PANEL.x + 172.0;
const CTRL_W: f32 = 168.0;

enum Control {
    Slider(Slider),
    Arrows(Box<(Button, Button)>),
}

struct Row {
    opt: Opt,
    y: f32,
    control: Control,
}

pub struct OptionsPanel {
    rows: Vec<Row>,
    back: Button,
    pop: Spring,
    /// Gamepad / keyboard focus: a row index, or `rows.len()` for BACK.
    focus: usize,
}

impl OptionsPanel {
    pub fn new() -> Self {
        let defaults = Settings::default();
        let rows = OPTS
            .iter()
            .enumerate()
            .map(|(i, &opt)| {
                let y = ROW_Y + i as f32 * ROW_H;
                let control = match slider_value(opt, &defaults) {
                    // The value is mirrored from the settings every frame.
                    Some(v) => Control::Slider(Slider::new(
                        Rect::new(CTRL_X, y - 3.0, CTRL_W - 52.0, 7.0),
                        v,
                        if opt == Opt::Atmosphere { palette::ORANGE } else { palette::GREEN },
                    )),
                    None => Control::Arrows(Box::new((
                        Button::grey("<", Rect::new(CTRL_X, y - 8.0, 18.0, 16.0)),
                        Button::grey(">", Rect::new(CTRL_X + CTRL_W - 18.0, y - 8.0, 18.0, 16.0)),
                    ))),
                };
                Row { opt, y, control }
            })
            .collect();
        let mut pop = Spring::new(0.9);
        pop.target = 1.0;
        OptionsPanel {
            rows,
            back: Button::brass("BACK", Rect::centered(PANEL.center().x, PANEL.y + PANEL.h - 20.0, 80.0, 18.0)),
            pop,
            focus: 0,
        }
    }

    pub fn update(&mut self, shared: &mut Shared, dt: f32) {
        self.pop.update(dt);
        let focus = shared.focus_visible().then_some(self.focus);
        for (i, row) in self.rows.iter_mut().enumerate() {
            match &mut row.control {
                Control::Slider(s) => {
                    s.focused = focus == Some(i);
                    // Mirror the setting unless the player is dragging this slider.
                    if !s.is_dragging() {
                        s.value = slider_value(row.opt, &shared.settings).unwrap_or(s.value);
                    }
                    let released = s.update(shared.mouse, shared.mouse_down, dt, &shared.sfx);
                    if s.is_dragging() || released {
                        set_slider_value(row.opt, &mut shared.settings, s.value);
                    }
                    if released {
                        shared.settings.save();
                    }
                }
                Control::Arrows(arrows) => {
                    let (l, r) = &mut **arrows;
                    l.update(shared.mouse, dt, &shared.sfx);
                    r.update(shared.mouse, dt, &shared.sfx);
                }
            }
        }
        self.back.focused = focus == Some(self.rows.len());
        self.back.update(shared.mouse, dt, &shared.sfx);
    }

    /// Returns true when the panel should close.
    pub fn mouse_pressed(&mut self, shared: &mut Shared, pos: Vec2) -> bool {
        self.press(shared, pos, true, true)
    }

    /// A finger down starts dragging a slider; buttons wait for the release.
    pub fn touch_pressed(&mut self, shared: &mut Shared, pos: Vec2) {
        self.press(shared, pos, true, false);
    }

    /// A tap on a button. Returns true when the panel should close.
    pub fn touch_released(&mut self, shared: &mut Shared, pos: Vec2) -> bool {
        self.press(shared, pos, false, true)
    }

    fn press(&mut self, shared: &mut Shared, pos: Vec2, sliders: bool, buttons: bool) -> bool {
        if buttons && self.back.click(pos, &shared.sfx) {
            shared.settings.save();
            return true;
        }
        for row in &mut self.rows {
            match &mut row.control {
                Control::Slider(s) if sliders => {
                    s.value = slider_value(row.opt, &shared.settings).unwrap_or(s.value);
                    if s.press(pos, &shared.sfx) {
                        set_slider_value(row.opt, &mut shared.settings, s.value);
                    }
                }
                Control::Arrows(arrows) if buttons => {
                    let (l, r) = &mut **arrows;
                    if l.click(pos, &shared.sfx) {
                        step(row.opt, &mut shared.settings, -1, false);
                    } else if r.click(pos, &shared.sfx) {
                        step(row.opt, &mut shared.settings, 1, false);
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Returns true when the panel should close.
    pub fn nav(&mut self, shared: &mut Shared, nav: Nav) -> bool {
        let stops = self.rows.len() + 1;
        match nav {
            Nav::Up | Nav::Down => {
                self.focus = (self.focus as i32 + nav.dy()).rem_euclid(stops as i32) as usize;
                // The BACK plaque ticks by itself when the focus lands on it.
                if self.focus < self.rows.len() {
                    shared.sfx.play(Sound::Hover);
                }
            }
            Nav::Left | Nav::Right | Nav::Confirm => {
                let Some(row) = self.rows.get_mut(self.focus) else {
                    if nav == Nav::Confirm {
                        self.back.flash(&shared.sfx);
                        shared.settings.save();
                        shared.tap();
                        return true;
                    }
                    return false;
                };
                // Confirm cycles forward through a row's values.
                let (dir, wrap) = if nav == Nav::Confirm { (1, true) } else { (nav.dx(), false) };
                match &mut row.control {
                    Control::Slider(s) => {
                        if nav != Nav::Confirm {
                            s.value = slider_value(row.opt, &shared.settings).unwrap_or(s.value);
                            s.nudge(dir, &shared.sfx);
                            set_slider_value(row.opt, &mut shared.settings, s.value);
                        }
                    }
                    Control::Arrows(arrows) => {
                        let (l, r) = &mut **arrows;
                        let arrow = if dir < 0 { l } else { r };
                        if step(row.opt, &mut shared.settings, dir, wrap) {
                            arrow.flash(&shared.sfx);
                            shared.tap();
                        } else {
                            // Already at the end: the arrow says "no".
                            arrow.deny(&shared.sfx);
                        }
                    }
                }
            }
            Nav::Back | Nav::Menu => {
                self.back.flash(&shared.sfx);
                shared.settings.save();
                shared.tap();
                return true;
            }
            _ => {}
        }
        false
    }

    pub fn key_pressed(&mut self, shared: &mut Shared, key: Key) -> bool {
        if matches!(key, Key::Escape | Key::Enter | Key::Space) {
            self.back.flash(&shared.sfx);
            shared.settings.save();
            return true;
        }
        false
    }

    pub fn draw(&self, shared: &Shared, g: &mut Graphics) {
        let font = &shared.font;
        ui::dim_screen(g, 0.55);
        g.push();
        let c = PANEL.center();
        g.translate(c.x, c.y);
        g.scale(self.pop.value, self.pop.value);
        g.translate(-c.x, -c.y);

        ui::panel(g, PANEL, palette::PANEL);
        Text::new("OPTIONS", c.x, PANEL.y + 20.0).scale(2.0).color(palette::BRASS_LIGHT).center().draw(g, font);

        let focus = shared.focus_visible().then_some(self.focus);
        for (i, row) in self.rows.iter().enumerate() {
            let y = row.y;
            let inset = Rect::new(PANEL.x + 10.0, y - 10.0, PANEL.w - 20.0, 20.0);
            let focused = focus == Some(i);
            if focused {
                // A pulsing brass frame around the focused row.
                let pulse = 0.75 + 0.25 * (shared.time * 6.0).sin();
                g.set_color(palette::GOLD.with_alpha(pulse));
                ui::chamfer(g, Fill, inset.x - 2.0, inset.y - 2.0, inset.w + 4.0, inset.h + 4.0, 4.0);
            }
            let fill = if focused { palette::PANEL_DARK.lerp(palette::PANEL, 0.5) } else { palette::PANEL_DARK };
            ui::inset(g, inset, fill);
            let label_color = if focused { palette::GOLD } else { palette::CREAM };
            Text::new(label(row.opt), PANEL.x + 20.0, y).color(label_color).anchor(0.0, 0.5).draw(g, font);
            let value = value_text(row.opt, &shared.settings);
            match &row.control {
                Control::Slider(s) => {
                    s.draw(g);
                    Text::new(&value, CTRL_X + CTRL_W, y)
                        .color(palette::GOLD)
                        .anchor(1.0, 0.5)
                        .draw(g, font);
                }
                Control::Arrows(arrows) => {
                    let (l, r) = &**arrows;
                    Text::new(&value, CTRL_X + CTRL_W / 2.0, y)
                        .color(palette::GOLD)
                        .center()
                        .draw(g, font);
                    l.draw(g, font);
                    r.draw(g, font);
                }
            }
        }
        Text::new("Points and stick the dealer apply to new games", c.x, PANEL.y + PANEL.h - 44.0)
            .color(palette::MUTED)
            .center()
            .draw(g, font);
        self.back.draw(g, font);
        g.pop();
    }
}

fn label(opt: Opt) -> &'static str {
    match opt {
        Opt::Volume => "Volume",
        Opt::Music => "Music",
        Opt::Atmosphere => "Atmosphere",
        Opt::Speed => "Game speed",
        Opt::Target => "Points to win",
        Opt::Stick => "Stick the dealer",
        Opt::Fullscreen => "Fullscreen",
        Opt::Rumble => "Controller rumble",
    }
}

fn percent(v: f32) -> String {
    format!("{}%", (v * 100.0).round() as i32)
}

fn on_off(b: bool) -> String {
    if b { "On".into() } else { "Off".into() }
}

fn value_text(opt: Opt, s: &Settings) -> String {
    match opt {
        Opt::Volume => percent(s.volume),
        Opt::Music => percent(s.music),
        Opt::Atmosphere => percent(s.atmosphere),
        Opt::Speed => SPEED_NAMES[s.speed].into(),
        Opt::Target => s.target.to_string(),
        Opt::Stick => on_off(s.stick_the_dealer),
        Opt::Fullscreen => on_off(s.fullscreen),
        Opt::Rumble => on_off(s.rumble),
    }
}

/// The current value of a slider option (`None` for arrow options).
fn slider_value(opt: Opt, s: &Settings) -> Option<f32> {
    match opt {
        Opt::Volume => Some(s.volume),
        Opt::Music => Some(s.music),
        Opt::Atmosphere => Some(s.atmosphere),
        _ => None,
    }
}

fn set_slider_value(opt: Opt, s: &mut Settings, v: f32) {
    match opt {
        Opt::Volume => s.volume = v,
        Opt::Music => s.music = v,
        Opt::Atmosphere => s.atmosphere = v,
        _ => {}
    }
}

/// Steps an arrow option by `dir`, clamping at the ends (or wrapping around
/// when `wrap`). Returns whether the value changed.
fn step(opt: Opt, s: &mut Settings, dir: i32, wrap: bool) -> bool {
    let next = |i: usize, n: usize| -> usize {
        let j = i as i32 + dir;
        if wrap { j.rem_euclid(n as i32) as usize } else { j.clamp(0, n as i32 - 1) as usize }
    };
    let before = s.clone();
    match opt {
        Opt::Speed => s.speed = next(s.speed, SPEED_NAMES.len()),
        Opt::Target => {
            let i = TARGETS.iter().position(|&t| t == s.target).unwrap_or(2);
            s.target = TARGETS[next(i, TARGETS.len())];
        }
        Opt::Stick => s.stick_the_dealer = !s.stick_the_dealer,
        Opt::Fullscreen => s.fullscreen = !s.fullscreen,
        Opt::Rumble => s.rumble = !s.rumble,
        Opt::Volume | Opt::Music | Opt::Atmosphere => {}
    }
    *s != before
}
