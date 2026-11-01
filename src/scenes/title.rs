//! Title screen: the logo, a fan of the four jacks (click them to flip them
//! over), and the main menu.

use std::f32::consts::PI;

use love2drs::prelude::*;

use crate::cards::Pose;
use crate::euchre::{Card, Rank, Suit};
use crate::juice::{self, Spring};
use crate::palette;
use crate::render::{VIEW_H, VIEW_W};
use crate::scene::{Scene, Shared, Transition};
use crate::scenes::table::Table;
use crate::scenes::{Overlay, options::OptionsPanel, rules::RulesPanel};
use crate::sfx::Sound;
use crate::text::Text;
use crate::ui::{self, Button, Rect};

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
    Play,
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
}

fn card_spring(v: f32) -> Spring {
    let mut s = Spring::new(v);
    s.stiffness = 120.0;
    s.damping = 14.0;
    s
}

impl Title {
    pub fn new() -> Self {
        let jacks = [Suit::Clubs, Suit::Diamonds, Suit::Spades, Suit::Hearts];
        let cards = jacks
            .iter()
            .enumerate()
            .map(|(i, &suit)| {
                let k = i as f32 - 1.5;
                let home = vec2(CX + k * 40.0, 100.0 + k * k * 4.0);
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
        let row_y = 268.0;
        let buttons = vec![
            (Choice::Play, Button::blue("PLAY", Rect::centered(CX, row_y, 104.0, 30.0)).scaled(1.5)),
            (Choice::Options, Button::orange("OPTIONS", Rect::centered(CX - 118.0, row_y + 4.0, 92.0, 22.0))),
            (Choice::Rules, Button::green("HOW TO PLAY", Rect::centered(CX + 118.0, row_y + 4.0, 92.0, 22.0))),
            (Choice::Quit, Button::red("QUIT", Rect::centered(CX, row_y + 34.0, 60.0, 18.0))),
        ];
        Title {
            cards,
            letters,
            buttons,
            overlay: None,
            age: 0.0,
        }
    }

    /// The title screen with a panel already open (debug entry point).
    pub fn with_overlay(overlay: Overlay) -> Self {
        let mut t = Title::new();
        t.overlay = Some(overlay);
        t
    }

    fn choose(&mut self, shared: &mut Shared, choice: Choice) -> Transition {
        match choice {
            Choice::Play => return Transition::Switch(Box::new(Table::new(shared))),
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
        let mouse = if self.overlay.is_some() { vec2(-99.0, -99.0) } else { shared.mouse };
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
        for (_, b) in &mut self.buttons {
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

        // Logo: letters bob and tilt on their own phase, with a thick outline.
        let scale = 4.5;
        let widths: Vec<f32> = TITLE.chars().map(|ch| font.width(&ch.to_string()) + 2.0).collect();
        let total = (widths.iter().sum::<f32>() - 2.0) * scale;
        let mut x = CX - total / 2.0;
        let cy = 182.0;
        for (i, ch) in TITLE.chars().enumerate() {
            let s = ch.to_string();
            let w = widths[i] * scale;
            let pop = self.letters[i].value;
            let lx = x + (w - 2.0 * scale) / 2.0;
            let ly = cy + juice::wobble(t, i as f32, 2.2, 3.0);
            let rot = juice::wobble(t, i as f32 + 7.0, 1.7, 0.05);
            let color = if i % 2 == 0 { palette::CREAM } else { palette::GOLD };
            if pop > 0.01 {
                for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                    Text::new(&s, lx + dx * 2.0, ly + dy * 2.0)
                        .scale(scale * pop)
                        .rotation(rot)
                        .center()
                        .color(palette::INK)
                        .no_shadow()
                        .draw(g, font);
                }
                Text::new(&s, lx, ly)
                    .scale(scale * pop)
                    .rotation(rot)
                    .center()
                    .color(color)
                    .shadow(palette::INK.with_alpha(0.6))
                    .draw(g, font);
            }
            x += w;
        }
        Text::new("A TRICK-TAKING CARD GAME", CX, 218.0)
            .color(palette::CREAM)
            .center()
            .draw(g, font);

        ui::panel(g, Rect::centered(CX, 284.0, 360.0, 74.0), palette::PANEL_DARK.with_alpha(0.85));
        for (_, b) in &self.buttons {
            b.draw(g, font);
        }

        Text::new("F2: CRT   F11: Fullscreen   Esc: Quit", 6.0, VIEW_H as f32 - 12.0)
            .color(palette::MUTED)
            .draw(g, font);

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
