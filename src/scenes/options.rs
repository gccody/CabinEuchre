//! The options panel (opened from the title screen or the pause menu).
//! Volume, music and CRT strength are sliders; the rest step with arrows.
//! Changes apply live (the app syncs settings every frame) and are saved on close.

use love2drs::prelude::*;

use crate::juice::Spring;
use crate::palette;
use crate::scene::Shared;
use crate::settings::{SPEED_NAMES, Settings, TARGETS};
use crate::text::Text;
use crate::ui::{self, Button, Rect, Slider};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opt {
    Volume,
    Music,
    Crt,
    Speed,
    Target,
    Stick,
    Fullscreen,
}

const OPTS: [Opt; 7] = [Opt::Volume, Opt::Music, Opt::Crt, Opt::Speed, Opt::Target, Opt::Stick, Opt::Fullscreen];

const PANEL: Rect = Rect::new(140.0, 40.0, 360.0, 280.0);
const ROW_Y: f32 = PANEL.y + 46.0;
const ROW_H: f32 = 26.0;
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
                        if opt == Opt::Crt { palette::ORANGE } else { palette::BLUE },
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
            back: Button::orange("BACK", Rect::centered(PANEL.center().x, PANEL.y + PANEL.h - 20.0, 80.0, 18.0)),
            pop,
        }
    }

    pub fn update(&mut self, shared: &mut Shared, dt: f32) {
        self.pop.update(dt);
        for row in &mut self.rows {
            match &mut row.control {
                Control::Slider(s) => {
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
        self.back.update(shared.mouse, dt, &shared.sfx);
    }

    /// Returns true when the panel should close.
    pub fn mouse_pressed(&mut self, shared: &mut Shared, pos: Vec2) -> bool {
        if self.back.click(pos, &shared.sfx) {
            shared.settings.save();
            return true;
        }
        for row in &mut self.rows {
            match &mut row.control {
                Control::Slider(s) => {
                    s.value = slider_value(row.opt, &shared.settings).unwrap_or(s.value);
                    if s.press(pos, &shared.sfx) {
                        set_slider_value(row.opt, &mut shared.settings, s.value);
                    }
                }
                Control::Arrows(arrows) => {
                    let (l, r) = &mut **arrows;
                    if l.click(pos, &shared.sfx) {
                        step(row.opt, &mut shared.settings, -1);
                    } else if r.click(pos, &shared.sfx) {
                        step(row.opt, &mut shared.settings, 1);
                    }
                }
            }
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
        Text::new("OPTIONS", c.x, PANEL.y + 20.0).scale(2.0).center().draw(g, font);

        for row in &self.rows {
            let y = row.y;
            ui::inset(g, Rect::new(PANEL.x + 10.0, y - 11.0, PANEL.w - 20.0, 22.0), palette::PANEL_DARK);
            Text::new(label(row.opt), PANEL.x + 20.0, y).anchor(0.0, 0.5).draw(g, font);
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
        Opt::Crt => "CRT strength",
        Opt::Speed => "Game speed",
        Opt::Target => "Points to win",
        Opt::Stick => "Stick the dealer",
        Opt::Fullscreen => "Fullscreen",
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
        Opt::Crt => percent(s.crt),
        Opt::Speed => SPEED_NAMES[s.speed].into(),
        Opt::Target => s.target.to_string(),
        Opt::Stick => on_off(s.stick_the_dealer),
        Opt::Fullscreen => on_off(s.fullscreen),
    }
}

/// The current value of a slider option (`None` for arrow options).
fn slider_value(opt: Opt, s: &Settings) -> Option<f32> {
    match opt {
        Opt::Volume => Some(s.volume),
        Opt::Music => Some(s.music),
        Opt::Crt => Some(s.crt),
        _ => None,
    }
}

fn set_slider_value(opt: Opt, s: &mut Settings, v: f32) {
    match opt {
        Opt::Volume => s.volume = v,
        Opt::Music => s.music = v,
        Opt::Crt => s.crt = v,
        _ => {}
    }
}

fn step(opt: Opt, s: &mut Settings, dir: i32) {
    match opt {
        Opt::Speed => s.speed = (s.speed as i32 + dir).clamp(0, 2) as usize,
        Opt::Target => {
            let i = TARGETS.iter().position(|&t| t == s.target).unwrap_or(2) as i32;
            s.target = TARGETS[(i + dir).clamp(0, 2) as usize];
        }
        Opt::Stick => s.stick_the_dealer = !s.stick_the_dealer,
        Opt::Fullscreen => s.fullscreen = !s.fullscreen,
        Opt::Volume | Opt::Music | Opt::Crt => {}
    }
}
