//! Title screen: the logo, a fan of the four jacks (click them to flip them
//! over), and the main menu. A gamepad or the arrow keys move a focus
//! highlight up and down the menu.
//!
//! When a game was saved, the menu leads with CONTINUE (and the saved score)
//! above NEW GAME. PLAY ONLINE opens the lobby (`lobby.rs`).

use std::f32::consts::PI;

use love2drs::prelude::*;

use crate::cards::Pose;
use crate::euchre::{Card, Match, Rank, Suit};
use crate::juice::{self, Spring};
use crate::palette;
use crate::render::{VIEW_H, VIEW_W};
use crate::savegame;
use crate::scene::{InputMode, NO_POINTER, Nav, Scene, Shared, Transition};
use crate::scenes::lobby::Lobby;
use crate::scenes::table::Table;
use crate::scenes::{Overlay, options::OptionsPanel, rules::RulesPanel};
use crate::sfx::Sound;
use crate::text::Text;
use crate::ui::{Button, Rect};

const TITLE: &str = "EUCHRE";
const CX: f32 = VIEW_W as f32 / 2.0;

struct FanCard {
    card: Card,
    home: Vec2,
    angle: f32,
    x: Spring,
    y: Spring,
    rot: Spring,
    face: f32,
    face_up: bool,
    hover: f32,
}

#[derive(Clone, Copy)]
enum Choice {
    Continue,
    Play,
    Online,
    Options,
    Rules,
    Quit,
}

pub struct Title {
    cards: Vec<FanCard>,
    letters: Vec<Spring>,
    buttons: Vec<(Choice, Button)>,
    overlay: Option<Overlay>,
    age: f32,
    /// Index into `buttons` for the gamepad / keyboard focus.
    focus: usize,
    /// The game in the save slot, offered as CONTINUE.
    saved: Option<Match>,
    /// How far the jacks, logo and tagline sit above their usual place, to
    /// make room for the menu ([`SAVED_LIFT`] with the extra CONTINUE row).
    lift: f32,
}

/// The header moves up this much to make room for the menu.
const LIFT: f32 = 12.0;
/// ...and this much when CONTINUE and its score line are there too.
const SAVED_LIFT: f32 = 28.0;
/// Space between the score line and the plaques above and below it.
const SUMMARY_GAP: f32 = 6.0;
/// How far a plaque's lip and drop shadow reach below its rect.
const PLAQUE_BELOW: f32 = 4.0;

/// The line under CONTINUE: the saved game's score.
fn saved_summary(m: &Match) -> String {
    format!("WE {} · THEY {} · PLAYING TO {}", m.scores[0], m.scores[1], m.target)
}

/// PLAY ONLINE: an amber plaque, warm like the lamp, so it stands apart from the rest.
fn online_button(rect: Rect) -> Button {
    Button::new("PLAY ONLINE", rect, palette::ORANGE, palette::ORANGE_DARK)
}

fn card_spring(v: f32) -> Spring {
    let mut s = Spring::new(v);
    s.stiffness = 120.0;
    s.damping = 14.0;
    s
}

impl Title {
    pub fn new() -> Self {
        let saved = savegame::load();
        let lift = if saved.is_some() { SAVED_LIFT } else { LIFT };
        let jacks = [Suit::Clubs, Suit::Diamonds, Suit::Spades, Suit::Hearts];
        let cards = jacks
            .iter()
            .enumerate()
            .map(|(i, &suit)| {
                let k = i as f32 - 1.5;
                let home = vec2(CX + k * 40.0, 80.0 + k * k * 4.0 - lift);
                let angle = k * 0.14;
                // Cards start below the screen and fly up, staggered.
                let mut y = card_spring(VIEW_H as f32 + 60.0 + i as f32 * 40.0);
                y.target = home.y;
                let mut rot = card_spring(angle + PI * 0.5);
                rot.target = angle;
                FanCard {
                    card: Card::new(Rank::Jack, suit),
                    home,
                    angle,
                    x: card_spring(home.x),
                    y,
                    rot,
                    face: 1.0,
                    face_up: true,
                    hover: 0.0,
                }
            })
            .collect();
        let letters = (0..TITLE.len()).map(|_| Spring::new(0.0)).collect();
        // A single column of plaques under the logo.
        let w = 112.0;
        let buttons = if saved.is_some() {
            // CONTINUE, its score line (SUMMARY_GAP below the plaque's lip and
            // shadow, and the same gap above NEW GAME), then the usual 24 px pitch.
            vec![
                (Choice::Continue, Button::brass("CONTINUE", Rect::centered(CX, 192.0, w + 24.0, 26.0)).scaled(1.5)),
                (Choice::Play, Button::red("NEW GAME", Rect::centered(CX, 242.0, w, 18.0))),
                (Choice::Online, online_button(Rect::centered(CX, 265.0, w, 18.0))),
                (Choice::Rules, Button::green("HOW TO PLAY", Rect::centered(CX, 288.0, w, 18.0))),
                (Choice::Options, Button::blue("OPTIONS", Rect::centered(CX, 311.0, w, 18.0))),
                (Choice::Quit, Button::grey("QUIT", Rect::centered(CX, 334.0, w, 18.0))),
            ]
        } else {
            vec![
                (Choice::Play, Button::brass("DEAL ME IN", Rect::centered(CX, 214.0, w + 24.0, 26.0)).scaled(1.5)),
                (Choice::Online, online_button(Rect::centered(CX, 247.0, w, 18.0))),
                (Choice::Rules, Button::green("HOW TO PLAY", Rect::centered(CX, 271.0, w, 18.0))),
                (Choice::Options, Button::blue("OPTIONS", Rect::centered(CX, 295.0, w, 18.0))),
                (Choice::Quit, Button::grey("QUIT", Rect::centered(CX, 319.0, w, 18.0))),
            ]
        };
        Title {
            cards,
            letters,
            buttons,
            overlay: None,
            age: 0.0,
            focus: 0,
            saved,
            lift,
        }
    }

    /// The title screen with a panel already open (debug entry point).
    pub fn with_overlay(overlay: Overlay) -> Self {
        let mut t = Title::new();
        t.overlay = Some(overlay);
        t
    }

    fn choose(&mut self, shared: &mut Shared, choice: Choice) -> Transition {
        shared.tap();
        match choice {
            Choice::Continue => {
                let table = match self.saved.take() {
                    Some(game) => Table::resume(shared, game),
                    None => Table::new(shared),
                };
                return Transition::Switch(Box::new(table));
            }
            // A new game leaves the save alone until it's saved over (or this
            // game finishes), so a stray click can't lose it.
            Choice::Play => return Transition::Switch(Box::new(Table::new(shared))),
            Choice::Online => return Transition::Switch(Box::new(Lobby::new(shared))),
            Choice::Options => {
                shared.sfx.play(Sound::Pop);
                self.overlay = Some(Overlay::Options(Box::new(OptionsPanel::new())));
            }
            Choice::Rules => {
                shared.sfx.play(Sound::Pop);
                self.overlay = Some(Overlay::Rules(Box::new(RulesPanel::new())));
            }
            Choice::Quit => return Transition::Quit,
        }
        Transition::None
    }

    fn card_under(&self, p: Vec2) -> Option<usize> {
        (0..self.cards.len()).rev().find(|&i| {
            let c = &self.cards[i];
            let local = Vec2::from_angle(-c.rot.value).rotate(p - vec2(c.x.value, c.y.value));
            local.x.abs() < 23.0 && local.y.abs() < 31.0
        })
    }
}

impl Scene for Title {
    fn update(&mut self, shared: &mut Shared, _ctx: &mut Context, dt: f32) -> Transition {
        self.age += dt;
        if let Some(o) = &mut self.overlay {
            o.update(shared, dt);
        }
        // Letters pop in one after another.
        for (i, l) in self.letters.iter_mut().enumerate() {
            if self.age > 0.25 + i as f32 * 0.07 && l.target == 0.0 {
                l.target = 1.0;
                shared.sfx.play_pitched(Sound::Tick, 1.0 + i as f32 * 0.08);
            }
            l.update(dt);
        }
        let mouse = if self.overlay.is_some() { NO_POINTER } else { shared.mouse };
        let hovered = self.card_under(mouse);
        for (i, c) in self.cards.iter_mut().enumerate() {
            let target_hover = if hovered == Some(i) { 1.0 } else { 0.0 };
            c.hover = approach(c.hover, target_hover, 14.0, dt);
            c.x.target = c.home.x;
            c.y.target = c.home.y - c.hover * 8.0;
            c.rot.target = c.angle + (mouse.x - c.home.x).clamp(-60.0, 60.0) * 0.0015 * c.hover;
            c.x.update(dt);
            c.y.update(dt);
            c.rot.update(dt);
            let target_face = if c.face_up { 1.0 } else { 0.0 };
            c.face = approach(c.face, target_face, 12.0, dt);
        }
        let focus = (shared.focus_visible() && self.overlay.is_none()).then_some(self.focus);
        for (i, (_, b)) in self.buttons.iter_mut().enumerate() {
            b.focused = focus == Some(i);
            b.update(mouse, dt, &shared.sfx);
        }
        Transition::None
    }

    fn draw(&mut self, shared: &Shared, g: &mut Graphics) {
        let t = shared.time;
        let font = &shared.font;

        // The jacks, with soft shadows.
        for (i, c) in self.cards.iter().enumerate() {
            let bob = juice::wobble(t, i as f32, 1.4, 2.0);
            let sway = juice::wobble(t, i as f32 + 5.0, 0.9, 0.025);
            let (x, y, r) = (c.x.value, c.y.value + bob, c.rot.value + sway);
            let flip = (1.0 - 2.0 * c.face).abs().max(0.05);
            let s = 1.0 + c.hover * 0.06;
            let face = (c.face >= 0.5).then_some(c.card);
            let pose = Pose::new(x, y, r, s * flip, s);
            shared.cards.draw(g, None, pose.offset(3.0, 5.0 + c.hover * 3.0), palette::INK.with_alpha(0.45));
            shared.cards.draw(g, face, pose, Color::WHITE);
        }

        // Logo: cream letters with a brass extrusion and a thick dark outline,
        // each bobbing and tilting on its own phase.
        let scale = 4.5;
        let depth = 4;
        let widths: Vec<f32> = TITLE.chars().map(|ch| font.width(&ch.to_string()) + 2.0).collect();
        let total = (widths.iter().sum::<f32>() - 2.0) * scale;
        let mut x = CX - total / 2.0;
        let cy = 150.0 - self.lift;
        for (i, ch) in TITLE.chars().enumerate() {
            let s = ch.to_string();
            let w = widths[i] * scale;
            let pop = self.letters[i].value;
            let lx = x + (w - 2.0 * scale) / 2.0;
            let ly = cy + juice::wobble(t, i as f32, 2.2, 3.0);
            let rot = juice::wobble(t, i as f32 + 7.0, 1.7, 0.05);
            if pop > 0.01 {
                let letter = |x: f32, y: f32, color: Color| {
                    Text::new(&s, x, y).scale(scale * pop).rotation(rot).center().color(color).no_shadow()
                };
                // Outline around both the face and the back of the extrusion.
                for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (1.0, 1.0), (-1.0, 1.0)] {
                    for d in [0, depth] {
                        let ox = lx + dx * 2.0 + d as f32 * 0.5;
                        let oy = ly + dy * 2.0 + d as f32;
                        letter(ox, oy, palette::INK).draw(g, font);
                    }
                }
                for d in (1..=depth).rev() {
                    let c = if d == 1 { palette::BRASS } else { palette::BRASS_DARK };
                    letter(lx + d as f32 * 0.5, ly + d as f32, c).draw(g, font);
                }
                letter(lx, ly, palette::CREAM).draw(g, font);
            }
            x += w;
        }
        Text::new("♣  A TRICK-TAKING CARD GAME  ♥", CX, 186.0 - self.lift)
            .color(palette::BRASS_LIGHT)
            .center()
            .draw(g, font);

        for (_, b) in &self.buttons {
            b.draw(g, font);
        }
        if let (Some(m), Some((_, b))) = (&self.saved, self.buttons.first()) {
            // Top-anchored, so the gap is measured from the plaque's real bottom edge.
            Text::new(&saved_summary(m), CX, b.rect.y + b.rect.h + PLAQUE_BELOW + SUMMARY_GAP)
                .color(palette::MUTED)
                .anchor(0.5, 0.0)
                .draw(g, font);
        }

        let pad = if self.overlay.is_some() {
            "D-pad: Choose   A: Select   B: Back"
        } else {
            "D-pad: Choose   A: Select   B: Quit button"
        };
        let footer = match shared.input {
            InputMode::Touch => "",
            _ => shared.hint(pad, "F2: Lamplight   F11: Fullscreen   Esc: Quit"),
        };
        Text::new(footer, 6.0, VIEW_H as f32 - 12.0).color(palette::MUTED).draw(g, font);

        if let Some(o) = &self.overlay {
            o.draw(shared, g);
        }
    }

    fn key_pressed(&mut self, shared: &mut Shared, key: Key) -> Transition {
        if let Some(o) = &mut self.overlay {
            if o.key_pressed(shared, key) {
                self.overlay = None;
            }
            return Transition::None;
        }
        match key {
            Key::Escape => Transition::Quit,
            Key::Enter | Key::Space => {
                self.buttons[0].1.flash(&shared.sfx);
                self.choose(shared, Choice::Play)
            }
            _ => Transition::None,
        }
    }

    fn nav(&mut self, shared: &mut Shared, nav: Nav) -> Transition {
        if let Some(o) = &mut self.overlay {
            if o.nav(shared, nav) {
                self.overlay = None;
            }
            return Transition::None;
        }
        let n = self.buttons.len() as i32;
        match nav {
            Nav::Up | Nav::Down => {
                self.focus = (self.focus as i32 + nav.dy()).rem_euclid(n) as usize;
                shared.tick();
            }
            // B jumps to QUIT rather than quitting outright.
            Nav::Back if self.focus != self.buttons.len() - 1 => {
                self.focus = self.buttons.len() - 1;
                shared.tick();
            }
            Nav::Confirm | Nav::Menu => {
                // Start presses the top plaque (continue, or deal); A presses the focused one.
                let i = if nav == Nav::Menu { 0 } else { self.focus };
                self.buttons[i].1.flash(&shared.sfx);
                let choice = self.buttons[i].0;
                return self.choose(shared, choice);
            }
            _ => {}
        }
        Transition::None
    }

    fn touch_pressed(&mut self, shared: &mut Shared, pos: Vec2) -> Transition {
        if let Some(o) = &mut self.overlay {
            o.touch_pressed(shared, pos);
        }
        Transition::None
    }

    fn touch_released(&mut self, shared: &mut Shared, pos: Vec2) -> Transition {
        if let Some(o) = &mut self.overlay {
            if o.touch_released(shared, pos) {
                self.overlay = None;
            }
            return Transition::None;
        }
        self.mouse_pressed(shared, pos, MouseButton::Left)
    }

    fn mouse_pressed(&mut self, shared: &mut Shared, pos: Vec2, button: MouseButton) -> Transition {
        if button != MouseButton::Left {
            return Transition::None;
        }
        if let Some(o) = &mut self.overlay {
            if o.mouse_pressed(shared, pos) {
                self.overlay = None;
            }
            return Transition::None;
        }
        for i in 0..self.buttons.len() {
            if self.buttons[i].1.click(pos, &shared.sfx) {
                let choice = self.buttons[i].0;
                return self.choose(shared, choice);
            }
        }
        if let Some(i) = self.card_under(pos) {
            let c = &mut self.cards[i];
            c.face_up = !c.face_up;
            c.rot.impulse(if pos.x < c.x.value { 3.0 } else { -3.0 });
            c.y.impulse(-120.0);
            shared.sfx.play(Sound::Flip);
        }
        Transition::None
    }
}
