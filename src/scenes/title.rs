//! Title screen: the logo, a fan of the four jacks (click them to flip them
//! over), and the main menu. A gamepad or the arrow keys move a focus
//! highlight up and down the menu.
//!
//! When a game was saved, the menu leads with CONTINUE (and the saved score)
//! above NEW GAME, which asks first: there's one save slot, and leaving the new
//! game would overwrite it. PLAY ONLINE opens the lobby (`lobby.rs`). LEARN TO
//! PLAY starts the tutorial, and asks first if there's a lesson to pick back up.

use std::f32::consts::PI;

use love2drs::prelude::*;

use crate::cards::Pose;
use crate::device;
use crate::euchre::{Card, Match, Rank, Suit};
use crate::juice::{self, Spring};
use crate::palette;
use crate::render::{VIEW_H, VIEW_W};
use crate::savegame;
use crate::scene::{NO_POINTER, Nav, Scene, Shared, Transition};
use crate::scenes::lobby::Lobby;
use crate::scenes::table::Table;
use crate::scenes::{Overlay, options::OptionsPanel, rules::RulesPanel};
use crate::sfx::Sound;
use crate::text::Text;
use crate::ui::{Button, Rect};

/// Seconds between the logo's letters popping in.
const LETTER_GAP: f32 = 0.04;
/// Line spacing when the logo takes two lines.
const LOGO_ROW: f32 = 52.0;
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
    Learn,
    Options,
    Rules,
    Quit,
}

/// What a two-button question on the title is about.
#[derive(Clone, Copy, PartialEq)]
enum Asking {
    /// "Start a new game over your saved one?"
    NewGame,
    /// "Pick the tutorial back up?" The lesson (from 0) and the score then.
    Lesson(usize, [u32; 2]),
}

/// A question with two answers. Keeping what you have is the first choice, so a
/// stray double press can't lose a game.
struct NewGameCheck {
    asking: Asking,
    keep: Button,
    start: Button,
    /// Whether the focus is on NEW GAME rather than on keeping the save.
    on_start: bool,
    pop: Spring,
}

impl NewGameCheck {
    fn new(asking: Asking) -> Self {
        let mut pop = Spring::new(0.85);
        pop.target = 1.0;
        let (keep_label, start_label) = match asking {
            Asking::NewGame => ("KEEP MY GAME", "NEW GAME"),
            Asking::Lesson(..) => ("PICK IT UP", "START OVER"),
        };
        let (keep, start) = if device::mobile() {
            (
                Button::brass(keep_label, Rect::centered(CX - 124.0, 228.0, 232.0, 48.0)).scaled(1.5),
                Button::red(start_label, Rect::centered(CX + 124.0, 228.0, 232.0, 48.0)).scaled(1.5),
            )
        } else {
            (
                Button::brass(keep_label, Rect::centered(CX - 66.0, 212.0, 124.0, 24.0)),
                Button::red(start_label, Rect::centered(CX + 66.0, 212.0, 124.0, 24.0)),
            )
        };
        NewGameCheck { asking, keep, start, on_start: false, pop }
    }

    fn panel() -> Rect {
        if device::mobile() { Rect::centered(CX, 180.0, 540.0, 180.0) } else { Rect::centered(CX, 180.0, 320.0, 112.0) }
    }

    fn draw(&self, saved: Option<&Match>, shared: &Shared, g: &mut Graphics) {
        let font = &shared.font;
        crate::ui::dim_screen(g, 0.55);
        let panel = NewGameCheck::panel();
        let c = panel.center();
        g.push();
        g.translate(c.x, c.y);
        g.scale(self.pop.value, self.pop.value);
        g.translate(-c.x, -c.y);
        crate::ui::panel(g, panel, palette::PANEL);
        let (title_y, line_y, title) = if device::mobile() { (panel.y + 34.0, panel.y + 70.0, 2.0) } else { (panel.y + 20.0, panel.y + 42.0, 1.5) };
        match self.asking {
            Asking::NewGame => {
                Text::new("START A NEW GAME?", CX, title_y).scale(title).color(palette::BRASS_LIGHT).center().draw(g, font);
                Text::new("It replaces your saved game when you leave it.", CX, line_y).color(palette::CREAM).center().draw(g, font);
                if let Some(m) = saved {
                    Text::new(&saved_summary(m), CX, line_y + 14.0).color(palette::MUTED).center().draw(g, font);
                }
            }
            Asking::Lesson(lesson, scores) => {
                Text::new("WELCOME BACK!", CX, title_y).scale(title).color(palette::BRASS_LIGHT).center().draw(g, font);
                let line = format!("Pick up at lesson {} of {}?", lesson + 1, crate::euchre::tutorial::LESSONS);
                Text::new(&line, CX, line_y).color(palette::CREAM).center().draw(g, font);
                let score = format!("WE {} · THEY {}", scores[0], scores[1]);
                Text::new(&score, CX, line_y + 14.0).color(palette::MUTED).center().draw(g, font);
            }
        }
        self.keep.draw(g, font);
        self.start.draw(g, font);
        g.pop();
    }
}

pub struct Title {
    cards: Vec<FanCard>,
    letters: Vec<Spring>,
    /// The logo has finished popping in once.
    logo_shown: bool,
    buttons: Vec<(Choice, Button)>,
    overlay: Option<Overlay>,
    /// NEW GAME was pressed with a game saved, and is waiting for an answer.
    check: Option<NewGameCheck>,
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

/// Where the jacks, logo and tagline sit: `(center x, fan y, logo y, tagline y)`.
/// On a phone they take the left of the screen, beside a column of big plaques.
fn header(lift: f32) -> (f32, f32, f32, f32) {
    if device::mobile() {
        (184.0, 92.0, 172.0, 268.0)
    } else {
        (CX, 80.0 - lift, 150.0 - lift, 186.0 - lift)
    }
}

/// The logo, line by line: one line across the table on a desktop, two on a
/// phone, where the menu column takes the right of the screen.
fn logo_lines() -> &'static [&'static str] {
    if device::mobile() { &["CABIN", "EUCHRE"] } else { &["CABIN EUCHRE"] }
}

/// The phone's menu: a column of plaques a thumb can't miss. There's no QUIT:
/// Android's back gesture quits, and iOS apps don't.
fn mobile_buttons(saved: bool) -> Vec<(Choice, Button)> {
    let (x, w, h) = (470.0, 236.0, 42.0);
    let row = |y: f32| Rect::centered(x, y, w, h);
    // Two small plaques side by side, sharing a row.
    let half = |y: f32, side: f32| Rect::centered(x + side * 61.0, y, 114.0, h);
    if saved {
        vec![
            (Choice::Continue, Button::brass("CONTINUE", Rect::centered(x, 71.0, w, 50.0)).scaled(2.0)),
            (Choice::Play, Button::red("NEW GAME", row(143.0)).scaled(1.5)),
            (Choice::Online, online_button(row(194.0)).scaled(1.5)),
            (Choice::Learn, Button::green("LEARN TO PLAY", row(245.0)).scaled(1.5)),
            (Choice::Rules, Button::blue("RULES", half(296.0, -1.0)).scaled(1.5)),
            (Choice::Options, Button::blue("OPTIONS", half(296.0, 1.0)).scaled(1.5)),
        ]
    } else {
        vec![
            (Choice::Play, Button::brass("DEAL ME IN", Rect::centered(x, 99.0, w, 52.0)).scaled(2.0)),
            (Choice::Online, online_button(row(158.0)).scaled(1.5)),
            (Choice::Learn, Button::green("LEARN TO PLAY", row(212.0)).scaled(1.5)),
            (Choice::Rules, Button::blue("RULES", half(266.0, -1.0)).scaled(1.5)),
            (Choice::Options, Button::blue("OPTIONS", half(266.0, 1.0)).scaled(1.5)),
        ]
    }
}

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
        let (head_x, fan_y, _, _) = header(lift);
        let jacks = [Suit::Clubs, Suit::Diamonds, Suit::Spades, Suit::Hearts];
        let cards = jacks
            .iter()
            .enumerate()
            .map(|(i, &suit)| {
                let k = i as f32 - 1.5;
                let home = vec2(head_x + k * 40.0, fan_y + k * k * 4.0);
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
        let letters = logo_lines().concat().chars().map(|_| Spring::new(0.0)).collect();
        // A single column of plaques under the logo.
        let w = 112.0;
        // RULES and OPTIONS share a row, as wide as the plaque at the top.
        let half = |y: f32, side: f32| Rect::centered(CX + side * 34.0, y, 66.0, 18.0);
        let buttons = if device::mobile() {
            mobile_buttons(saved.is_some())
        } else if saved.is_some() {
            // CONTINUE, its score line (SUMMARY_GAP below the plaque's lip and
            // shadow, and the same gap above NEW GAME), then the usual 24 px pitch.
            vec![
                (Choice::Continue, Button::brass("CONTINUE", Rect::centered(CX, 192.0, w + 24.0, 26.0)).scaled(1.5)),
                (Choice::Play, Button::red("NEW GAME", Rect::centered(CX, 242.0, w, 18.0))),
                (Choice::Online, online_button(Rect::centered(CX, 265.0, w, 18.0))),
                (Choice::Learn, Button::green("LEARN TO PLAY", Rect::centered(CX, 288.0, w, 18.0))),
                (Choice::Rules, Button::blue("RULES", half(311.0, -1.0))),
                (Choice::Options, Button::blue("OPTIONS", half(311.0, 1.0))),
                (Choice::Quit, Button::grey("QUIT", Rect::centered(CX, 334.0, w, 18.0))),
            ]
        } else {
            vec![
                (Choice::Play, Button::brass("DEAL ME IN", Rect::centered(CX, 214.0, w + 24.0, 26.0)).scaled(1.5)),
                (Choice::Online, online_button(Rect::centered(CX, 247.0, w, 18.0))),
                (Choice::Learn, Button::green("LEARN TO PLAY", Rect::centered(CX, 271.0, w, 18.0))),
                (Choice::Rules, Button::blue("RULES", half(295.0, -1.0))),
                (Choice::Options, Button::blue("OPTIONS", half(295.0, 1.0))),
                (Choice::Quit, Button::grey("QUIT", Rect::centered(CX, 319.0, w, 18.0))),
            ]
        };
        Title {
            cards,
            letters,
            logo_shown: false,
            buttons,
            overlay: None,
            check: None,
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
            // There's one save slot, and leaving a new game saves over it, so ask first.
            Choice::Play if self.saved.is_some() => {
                shared.sfx.play(Sound::Pop);
                self.check = Some(NewGameCheck::new(Asking::NewGame));
            }
            // The save stays until it's saved over (or this game finishes).
            Choice::Play => return Transition::Switch(Box::new(Table::new(shared))),
            Choice::Online => return Transition::Switch(Box::new(Lobby::new(shared))),
            // Part way through the tutorial: ask whether to carry on.
            Choice::Learn => match shared.settings.lesson {
                Some((lesson, scores)) => {
                    shared.sfx.play(Sound::Pop);
                    self.check = Some(NewGameCheck::new(Asking::Lesson(lesson, scores)));
                }
                None => return Transition::Switch(Box::new(Table::tutorial(shared, 0, [0, 0]))),
            },
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

    /// Answers the new game question.
    fn answer_check(&mut self, shared: &mut Shared, start: bool) -> Transition {
        let asking = self.check.take().map(|c| c.asking);
        shared.tap();
        if let Some(Asking::Lesson(lesson, scores)) = asking {
            let (lesson, scores) = if start { (0, [0, 0]) } else { (lesson, scores) };
            return Transition::Switch(Box::new(Table::tutorial(shared, lesson, scores)));
        }
        if start {
            return Transition::Switch(Box::new(Table::new(shared)));
        }
        shared.sfx.play(Sound::Select);
        Transition::None
    }

    /// A little tag pointing at LEARN TO PLAY, until the tutorial has been played.
    fn draw_new_here(&self, shared: &Shared, g: &mut Graphics) {
        let Some((_, b)) = self.buttons.iter().find(|(c, _)| matches!(c, Choice::Learn)) else { return };
        let font = &shared.font;
        let label = "NEW? START HERE";
        let w = font.width(label) + 14.0;
        let nudge = (shared.time * 3.5).sin() * 1.5;
        let mid = b.rect.center().y;
        let mobile = device::mobile();
        // On a phone the column runs close to the logo, so the tag stands above the plaque's corner.
        let (x, y) = if mobile { (b.rect.x + b.rect.w - w, b.rect.y - 8.0 + nudge) } else { (b.rect.x + b.rect.w + 12.0 + nudge, mid - 7.0) };
        g.set_color(palette::INK);
        crate::ui::chamfer(g, Fill, x - 1.0, y - 1.0, w + 2.0, 16.0, 4.0);
        if !mobile {
            g.polygon(Fill, &[x + 1.0, mid - 6.0, x - 7.0, mid, x + 1.0, mid + 6.0]);
        }
        g.set_color(palette::BRASS);
        crate::ui::chamfer(g, Fill, x, y, w, 14.0, 3.0);
        if !mobile {
            g.polygon(Fill, &[x + 1.0, mid - 4.0, x - 5.0, mid, x + 1.0, mid + 4.0]);
        }
        g.set_color(palette::BRASS_LIGHT);
        g.rectangle(Fill, x + 3.0, y, w - 6.0, 1.0);
        Text::new(label, x + w / 2.0, y + 7.0).color(palette::INK).shadow(palette::BRASS_LIGHT.with_alpha(0.5)).center().draw(g, font);
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
        // Letters pop in one after another. The logo is wider than the panels,
        // so it ducks out of the way while one is open.
        let logo = logo_lines().concat();
        let showing = self.overlay.is_none() && self.check.is_none();
        for (i, (l, ch)) in self.letters.iter_mut().zip(logo.chars()).enumerate() {
            if !showing {
                l.target = 0.0;
            } else if self.age > 0.25 + i as f32 * LETTER_GAP && l.target == 0.0 {
                l.target = 1.0;
                if ch != ' ' && !self.logo_shown {
                    shared.sfx.play_pitched(Sound::Tick, 1.0 + i as f32 * 0.04);
                }
            }
            l.update(dt);
        }
        // Only the first time gets the rising ticks, not every closed panel.
        self.logo_shown |= showing && self.letters.iter().all(|l| l.target == 1.0);
        if let Some(check) = &mut self.check {
            check.pop.update(dt);
            let focus = shared.focus_visible();
            check.keep.focused = focus && !check.on_start;
            check.start.focused = focus && check.on_start;
            check.keep.update(shared.mouse, dt, &shared.sfx);
            check.start.update(shared.mouse, dt, &shared.sfx);
        }
        let mouse = if self.overlay.is_some() || self.check.is_some() { NO_POINTER } else { shared.mouse };
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
        let focus = (shared.focus_visible() && self.overlay.is_none() && self.check.is_none()).then_some(self.focus);
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
        let (head_x, _, logo_y, tag_y) = header(self.lift);
        let mut i = 0;
        for (row, line) in logo_lines().iter().enumerate() {
            let widths: Vec<f32> = line.chars().map(|ch| font.width(&ch.to_string()) + 2.0).collect();
            let total = (widths.iter().sum::<f32>() - 2.0) * scale;
            let mut x = head_x - total / 2.0;
            let cy = logo_y + row as f32 * LOGO_ROW;
            for (ch, width) in line.chars().zip(&widths) {
                let s = ch.to_string();
                let w = width * scale;
                let pop = self.letters[i].value;
                let lx = x + (w - 2.0 * scale) / 2.0;
                let ly = cy + juice::wobble(t, i as f32, 2.2, 3.0);
                let rot = juice::wobble(t, i as f32 + 7.0, 1.7, 0.05);
                if pop > 0.01 && ch != ' ' {
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
                i += 1;
            }
        }
        Text::new("♣  A TRICK-TAKING CARD GAME  ♥", head_x, tag_y)
            .color(palette::BRASS_LIGHT)
            .center()
            .draw(g, font);

        for (_, b) in &self.buttons {
            b.draw(g, font);
        }
        if !shared.settings.tutorial_done && shared.settings.lesson.is_none() {
            self.draw_new_here(shared, g);
        }
        if let (Some(m), Some((_, b))) = (&self.saved, self.buttons.first()) {
            // Top-anchored, so the gap is measured from the plaque's real bottom edge.
            Text::new(&saved_summary(m), b.rect.center().x, b.rect.y + b.rect.h + PLAQUE_BELOW + SUMMARY_GAP)
                .color(palette::MUTED)
                .anchor(0.5, 0.0)
                .draw(g, font);
        }

        if let Some(o) = &self.overlay {
            o.draw(shared, g);
        }
        if let Some(check) = &self.check {
            check.draw(self.saved.as_ref(), shared, g);
        }
    }

    fn key_pressed(&mut self, shared: &mut Shared, key: Key) -> Transition {
        if let Some(o) = &mut self.overlay {
            if o.key_pressed(shared, key) {
                self.overlay = None;
            }
            return Transition::None;
        }
        if let Some(check) = &self.check {
            let start = check.on_start;
            return match key {
                Key::Escape => {
                    self.check = None;
                    shared.sfx.play(Sound::Select);
                    Transition::None
                }
                Key::Enter | Key::KpEnter | Key::Space => self.answer_check(shared, start),
                _ => Transition::None,
            };
        }
        match key {
            Key::Escape if device::back_quits() => Transition::Quit,
            // The top button: CONTINUE when there's a save, never a new game over it.
            Key::Enter | Key::Space => {
                let choice = self.buttons[0].0;
                self.buttons[0].1.flash(&shared.sfx);
                self.choose(shared, choice)
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
        if let Some(check) = &mut self.check {
            return match nav {
                Nav::Left | Nav::Right | Nav::Up | Nav::Down => {
                    check.on_start = !check.on_start;
                    shared.tick();
                    Transition::None
                }
                Nav::Confirm => {
                    let start = check.on_start;
                    self.answer_check(shared, start)
                }
                Nav::Back | Nav::Menu => {
                    self.check = None;
                    shared.tap();
                    shared.sfx.play(Sound::Select);
                    Transition::None
                }
                _ => Transition::None,
            };
        }
        let n = self.buttons.len() as i32;
        match nav {
            Nav::Up | Nav::Down => {
                self.focus = (self.focus as i32 + nav.dy()).rem_euclid(n) as usize;
                shared.tick();
            }
            // RULES and OPTIONS share a row.
            Nav::Left | Nav::Right if matches!(self.buttons[self.focus].0, Choice::Rules | Choice::Options) => {
                let want = |c: &Choice| if nav == Nav::Left { matches!(c, Choice::Rules) } else { matches!(c, Choice::Options) };
                if let Some(i) = self.buttons.iter().position(|(c, _)| want(c))
                    && i != self.focus
                {
                    self.focus = i;
                    shared.tick();
                }
            }
            // B jumps to QUIT rather than quitting outright.
            Nav::Back if !device::mobile() && self.focus != self.buttons.len() - 1 => {
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
        if let Some(check) = &mut self.check {
            if check.keep.click(pos, &shared.sfx) {
                return self.answer_check(shared, false);
            }
            if check.start.click(pos, &shared.sfx) {
                return self.answer_check(shared, true);
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
