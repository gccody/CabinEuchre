//! PLAY ONLINE: host a table or join one with its code, then wait at the
//! table for friends until someone deals.
//!
//! Stages:
//! - **Menu**: HOST A TABLE, JOIN WITH A CODE, your name.
//! - **Entry**: an on-screen keyboard for the five-letter code (or your
//!   name). Mouse, touch, gamepad and a real keyboard all type on it, so it
//!   works the same on a phone, a Steam Deck or a desktop.
//! - **Connecting**, then **Seated**: the room code in big brass tiles, the
//!   four seats (tap a bot's seat to move there), DEAL, INVITE (Steam) and LEAVE.
//!   Bots fill any seat nobody takes.
//! - **Failed**: why, and BACK.
//!
//! When the room deals (or you join a game in progress) the connection is
//! handed to the table scene.

use std::collections::VecDeque;

use love2drs::prelude::*;

use crate::euchre::protocol::{ClientMsg, SeatKind, SeatsInfo, ServerMsg, clean_code};
use crate::juice::Spring;
use crate::net::online::Join;
use crate::net::{Connection, Link, Status};
use crate::palette;
use crate::platform;
use crate::scene::{InputMode, Nav, Scene, Shared, Transition};
use crate::scenes::table::Table;
use crate::scenes::title::Title;
use crate::sfx::Sound;
use crate::text::Text;
use crate::ui::{self, Button, Rect};

const PANEL: Rect = Rect::new(130.0, 34.0, 380.0, 292.0);
const CX: f32 = PANEL.x + PANEL.w / 2.0;

/// Letters for codes (no I or O) and for names.
const CODE_KEYS: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ";
const NAME_KEYS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ-";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Code,
    Name,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Stage {
    Menu,
    Entry(Field),
    Connecting,
    Seated,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Act {
    Host,
    JoinCode,
    Name,
    Back,
    Key(char),
    Space,
    Erase,
    Done,
    Deal,
    Invite,
    Sit(usize),
    Leave,
}

pub struct Lobby {
    stage: Stage,
    buttons: Vec<(Act, Button)>,
    focus: usize,
    /// The code or name being typed.
    entry: String,
    link: Option<Link>,
    seats: Option<SeatsInfo>,
    /// Messages for the table, kept until it takes over.
    pending: VecDeque<ServerMsg>,
    code: Option<String>,
    note: Option<(String, f32)>,
    pop: Spring,
    age: f32,
    /// A DEAL request is out.
    dealing: bool,
    /// Seconds in the current stage.
    stage_age: f32,
    /// Debug (`EUCHRE_AUTOPLAY=1`): deal a few seconds after sitting down.
    autoplay: bool,
}

impl Lobby {
    /// The PLAY ONLINE menu.
    pub fn new(shared: &Shared) -> Self {
        let mut l = Lobby {
            stage: Stage::Menu,
            buttons: Vec::new(),
            focus: 0,
            entry: String::new(),
            link: None,
            seats: None,
            pending: VecDeque::new(),
            code: None,
            note: None,
            pop: Spring::new(0.85),
            age: 0.0,
            dealing: false,
            stage_age: 0.0,
            autoplay: std::env::var("EUCHRE_AUTOPLAY").is_ok_and(|v| v == "1"),
        };
        l.pop.target = 1.0;
        l.set_stage(Stage::Menu, shared);
        l
    }

    /// Hosting a new table straight away (debug: `EUCHRE_SCENE=host`).
    pub fn host(shared: &Shared) -> Self {
        let mut l = Lobby::new(shared);
        l.connect(Join::Create, shared);
        l
    }

    /// The code keyboard, with `typed` already entered (debug: `EUCHRE_SCENE=code`).
    pub fn entering_code(shared: &Shared, typed: &str) -> Self {
        let mut l = Lobby::new(shared);
        l.set_stage(Stage::Entry(Field::Code), shared);
        l.entry = typed.to_string();
        l
    }

    /// Straight to a room (an invite from a friend, or `+connect CODE`).
    pub fn join(shared: &Shared, code: &str) -> Self {
        let mut l = Lobby::new(shared);
        l.connect(Join::Code(code.to_string()), shared);
        l
    }

    fn connect(&mut self, join: Join, shared: &Shared) {
        let name = shared.settings.player_name();
        // A new room plays by the host's options.
        let create_query = format!(
            "?target={}&stick={}",
            shared.settings.target,
            u8::from(shared.settings.stick_the_dealer)
        );
        self.link = Some(Link::Online(Connection::open(join, &name, &create_query)));
        self.seats = None;
        self.pending.clear();
        self.code = None;
        self.dealing = false;
        self.set_stage(Stage::Connecting, shared);
    }

    fn set_stage(&mut self, stage: Stage, shared: &Shared) {
        self.stage = stage;
        self.stage_age = 0.0;
        self.focus = 0;
        self.buttons = self.build(shared);
        self.pop = Spring::new(0.92);
        self.pop.target = 1.0;
    }

    /// The controls for the current stage.
    fn build(&self, _shared: &Shared) -> Vec<(Act, Button)> {
        let mut b: Vec<(Act, Button)> = Vec::new();
        let bottom = PANEL.y + PANEL.h - 26.0;
        match &self.stage {
            Stage::Menu => {
                b.push((Act::Host, Button::brass("HOST A TABLE", Rect::centered(CX, 124.0, 160.0, 26.0)).scaled(1.5)));
                b.push((Act::JoinCode, Button::blue("JOIN WITH A CODE", Rect::centered(CX, 164.0, 160.0, 20.0))));
                b.push((Act::Name, Button::green("CHANGE NAME", Rect::centered(CX, 236.0, 110.0, 18.0))));
                b.push((Act::Back, Button::grey("BACK", Rect::centered(CX, bottom, 90.0, 18.0))));
            }
            Stage::Entry(field) => {
                let (keys, cols) = match field {
                    Field::Code => (CODE_KEYS, 8),
                    Field::Name => (NAME_KEYS, 9),
                };
                let (kw, kh, gap) = (24.0, 20.0, 5.0);
                let width = cols as f32 * kw + (cols - 1) as f32 * gap;
                let x0 = (CX - width / 2.0).round();
                for (i, ch) in keys.chars().enumerate() {
                    let (row, col) = (i / cols, i % cols);
                    let r = Rect::new(x0 + col as f32 * (kw + gap), 150.0 + row as f32 * (kh + gap), kw, kh);
                    let mut key = Button::new(ch.to_string(), r, palette::CREAM, palette::CREAM_SHADE.lerp(palette::INK, 0.3));
                    key.text_color = palette::INK;
                    b.push((Act::Key(ch), key));
                }
                let y = 150.0 + 3.0 * (kh + gap) + 14.0;
                let mut row = vec![(Act::Back, Button::grey("BACK", Rect::new(0.0, y, 64.0, 20.0)))];
                if *field == Field::Name {
                    row.push((Act::Space, Button::grey("SPACE", Rect::new(0.0, y, 64.0, 20.0))));
                }
                row.push((Act::Erase, Button::red("DEL", Rect::new(0.0, y, 52.0, 20.0))));
                let done = if *field == Field::Code { "JOIN" } else { "DONE" };
                row.push((Act::Done, Button::brass(done, Rect::new(0.0, y, 76.0, 20.0))));
                let total: f32 = row.iter().map(|(_, b)| b.rect.w).sum::<f32>() + 8.0 * (row.len() - 1) as f32;
                let mut x = (CX - total / 2.0).round();
                for (_, btn) in &mut row {
                    btn.rect.x = x;
                    x += btn.rect.w + 8.0;
                }
                b.extend(row);
            }
            Stage::Connecting => {
                b.push((Act::Leave, Button::grey("CANCEL", Rect::centered(CX, bottom, 90.0, 18.0))));
            }
            Stage::Seated => {
                for seat in 1..4 {
                    let p = seat_pos(seat);
                    b.push((Act::Sit(seat), Button::grey("SIT HERE", Rect::centered(p.x, p.y + 16.0, 62.0, 14.0))));
                }
                let mut row = vec![(Act::Deal, Button::brass("DEAL", Rect::new(0.0, bottom - 10.0, 84.0, 22.0)).scaled(1.2))];
                if platform::can_invite() {
                    row.push((Act::Invite, Button::blue("INVITE FRIENDS", Rect::new(0.0, bottom - 9.0, 104.0, 20.0))));
                }
                row.push((Act::Leave, Button::grey("LEAVE", Rect::new(0.0, bottom - 9.0, 64.0, 20.0))));
                let total: f32 = row.iter().map(|(_, b)| b.rect.w).sum::<f32>() + 10.0 * (row.len() - 1) as f32;
                let mut x = (CX - total / 2.0).round();
                for (_, btn) in &mut row {
                    btn.rect.x = x;
                    x += btn.rect.w + 10.0;
                }
                b.extend(row);
            }
            Stage::Failed(_) => {
                b.push((Act::Back, Button::brass("BACK", Rect::centered(CX, bottom, 90.0, 20.0))));
            }
        }
        b
    }

    /// Whether a control can be used right now (hidden ones can't be focused either).
    fn usable(&self, act: Act) -> bool {
        match act {
            Act::Sit(seat) => {
                self.seats.as_ref().is_some_and(|s| !s.playing && s.seats[seat].kind == SeatKind::Bot)
            }
            Act::Done => match self.stage {
                Stage::Entry(Field::Code) => clean_code(&self.entry).is_some(),
                _ => true,
            },
            Act::Deal => self.seats.as_ref().is_some_and(|s| !s.playing) && !self.dealing,
            _ => true,
        }
    }

    fn visible(&self, act: Act) -> bool {
        match act {
            Act::Sit(seat) => self.seats.as_ref().is_some_and(|s| s.seats[seat].kind == SeatKind::Bot),
            _ => true,
        }
    }

    fn press(&mut self, act: Act, shared: &mut Shared) -> Transition {
        shared.tap();
        match act {
            Act::Host => self.connect(Join::Create, shared),
            Act::JoinCode => {
                self.entry.clear();
                self.set_stage(Stage::Entry(Field::Code), shared);
            }
            Act::Name => {
                self.entry = shared.settings.player_name();
                if self.entry == "PLAYER" {
                    self.entry.clear();
                }
                self.set_stage(Stage::Entry(Field::Name), shared);
                // Start on DONE, so A confirms a name that's already fine.
                self.focus = self.buttons.iter().position(|(a, _)| *a == Act::Done).unwrap_or(0);
            }
            Act::Back => match self.stage {
                Stage::Menu => return Transition::Switch(Box::new(Title::new())),
                _ => {
                    self.link = None;
                    self.set_stage(Stage::Menu, shared);
                }
            },
            Act::Key(ch) => self.type_char(ch, shared),
            Act::Space => self.type_char(' ', shared),
            Act::Erase => {
                if self.entry.pop().is_some() {
                    shared.sfx.play(Sound::Pass);
                }
            }
            Act::Done => match self.stage {
                Stage::Entry(Field::Code) => {
                    if let Some(code) = clean_code(&self.entry) {
                        self.connect(Join::Code(code), shared);
                    }
                }
                Stage::Entry(Field::Name) => {
                    shared.settings.name = crate::euchre::protocol::clean_name(&self.entry);
                    shared.settings.save();
                    self.set_stage(Stage::Menu, shared);
                }
                _ => {}
            },
            Act::Deal => {
                if let Some(link) = &mut self.link {
                    link.send(ClientMsg::Start);
                    self.dealing = true;
                }
            }
            Act::Invite => {
                if let Some(code) = &self.code {
                    platform::invite_friends(code);
                }
            }
            Act::Sit(seat) => {
                if let Some(link) = &mut self.link {
                    link.send(ClientMsg::Sit { seat });
                    shared.sfx.play(Sound::CardSlide);
                }
            }
            Act::Leave => {
                if let Some(link) = &mut self.link {
                    link.send(ClientMsg::Leave);
                }
                self.link = None;
                platform::set_presence(None);
                self.set_stage(Stage::Menu, shared);
            }
        }
        Transition::None
    }

    fn type_char(&mut self, ch: char, shared: &mut Shared) {
        let max = match self.stage {
            Stage::Entry(Field::Code) => 5,
            _ => crate::euchre::protocol::MAX_NAME_LEN,
        };
        if self.entry.chars().count() >= max || (ch == ' ' && (self.entry.is_empty() || self.entry.ends_with(' '))) {
            shared.sfx.play(Sound::Deny);
            return;
        }
        self.entry.push(ch);
        shared.sfx.play_pitched(Sound::Tick, 1.0 + self.entry.len() as f32 * 0.05);
        // A full code jumps the focus to JOIN.
        if matches!(self.stage, Stage::Entry(Field::Code))
            && self.entry.len() == 5
            && let Some(i) = self.buttons.iter().position(|(a, _)| *a == Act::Done)
        {
            self.focus = i;
        }
    }

    /// Reads the room's messages. Returns the table once it has dealt.
    fn poll(&mut self, shared: &mut Shared) -> Option<Transition> {
        let mut deal = false;
        while let Some(msg) = self.link.as_mut()?.poll() {
            match &msg {
                ServerMsg::Welcome { room, .. } => {
                    self.code = Some(room.clone());
                }
                ServerMsg::Seats(s) => {
                    let people = s.seats.iter().filter(|x| x.kind != SeatKind::Bot).count();
                    if let Some(old) = &self.seats {
                        let before = old.seats.iter().filter(|x| x.kind != SeatKind::Bot).count();
                        if people > before {
                            shared.sfx.play(Sound::Pop);
                        }
                    }
                    if let Some(code) = &self.code {
                        platform::set_presence(Some((code, people)));
                    }
                    self.seats = Some(s.clone());
                }
                ServerMsg::Deal(_) | ServerMsg::Sync { .. } => deal = true,
                ServerMsg::Error { message, .. } => {
                    self.note = Some((message.clone(), 3.0));
                    self.dealing = false;
                }
                ServerMsg::Acted(_) => {}
            }
            self.pending.push_back(msg);
        }
        if self.stage == Stage::Connecting && self.code.is_some() && self.seats.is_some() {
            shared.sfx.play(Sound::Trump);
            if self.autoplay {
                eprintln!("[euchre] autoplay: seated at table {}", self.code.as_deref().unwrap_or("?"));
            }
            self.set_stage(Stage::Seated, shared);
        }
        if let Some(Status::Failed(why)) = self.link.as_ref().map(Link::status) {
            self.link = None;
            platform::set_presence(None);
            self.set_stage(Stage::Failed(why), shared);
            shared.sfx.play(Sound::Deny);
            return None;
        }
        if deal {
            let link = self.link.take().expect("checked above");
            let pending = std::mem::take(&mut self.pending);
            match Table::join(shared, link, self.seats.clone(), pending) {
                Ok(table) => return Some(Transition::Switch(Box::new(table))),
                Err(back) => {
                    let (link, pending) = *back;
                    self.link = Some(link);
                    self.pending = pending;
                }
            }
        }
        None
    }

    /// Moves the focus to the nearest usable control in a direction.
    fn move_focus(&mut self, nav: Nav, shared: &mut Shared) {
        let Some((_, from)) = self.buttons.get(self.focus) else { return };
        let here = from.rect.center();
        let dir = vec2(nav.dx() as f32, nav.dy() as f32);
        let mut best: Option<(usize, f32)> = None;
        for (i, (act, b)) in self.buttons.iter().enumerate() {
            if i == self.focus || !self.visible(*act) {
                continue;
            }
            let d = b.rect.center() - here;
            let along = d.dot(dir);
            if along <= 1.0 {
                continue;
            }
            let across = (d - dir * along).length();
            let score = along + across * 2.5;
            if best.is_none_or(|(_, s)| score < s) {
                best = Some((i, score));
            }
        }
        if let Some((i, _)) = best {
            self.focus = i;
            shared.tick();
        }
    }
}

/// Middle of the little table in the seated view.
const SEATS_Y: f32 = 204.0;

/// Where each seat's plate sits in the seated view (relative seats: you at the bottom).
fn seat_pos(seat: usize) -> Vec2 {
    let c = vec2(CX, SEATS_Y);
    [c + vec2(0.0, 44.0), c + vec2(-122.0, -6.0), c + vec2(0.0, -50.0), c + vec2(122.0, -6.0)][seat]
}

impl Scene for Lobby {
    fn update(&mut self, shared: &mut Shared, _ctx: &mut Context, dt: f32) -> Transition {
        self.age += dt;
        self.stage_age += dt;
        self.pop.update(dt);
        if let Some(t) = self.poll(shared) {
            return t;
        }
        if self.autoplay && self.stage == Stage::Seated && self.stage_age > 4.0 && self.usable(Act::Deal) {
            self.press(Act::Deal, shared);
        }
        if let Some((_, t)) = &mut self.note {
            *t -= dt;
            if *t <= 0.0 {
                self.note = None;
            }
        }
        let focus = shared.focus_visible();
        let enabled: Vec<bool> = self.buttons.iter().map(|(a, _)| self.usable(*a)).collect();
        let shown: Vec<bool> = self.buttons.iter().map(|(a, _)| self.visible(*a)).collect();
        for (i, (_, b)) in self.buttons.iter_mut().enumerate() {
            b.enabled = enabled[i];
            b.focused = focus && i == self.focus && shown[i];
            let mouse = if shown[i] { shared.mouse } else { crate::scene::NO_POINTER };
            b.update(mouse, dt, &shared.sfx);
        }
        Transition::None
    }

    fn draw(&mut self, shared: &Shared, g: &mut Graphics) {
        let font = &shared.font;
        g.push();
        let c = PANEL.center();
        g.translate(c.x, c.y);
        g.scale(self.pop.value, self.pop.value);
        g.translate(-c.x, -c.y);
        ui::panel(g, PANEL, palette::PANEL);
        let title = match &self.stage {
            Stage::Menu => "PLAY ONLINE",
            Stage::Entry(Field::Code) => "JOIN A TABLE",
            Stage::Entry(Field::Name) => "YOUR NAME",
            Stage::Connecting => "PLAY ONLINE",
            Stage::Seated => "YOUR TABLE",
            Stage::Failed(_) => "NO LUCK",
        };
        Text::new(title, CX, PANEL.y + 20.0).scale(2.0).color(palette::BRASS_LIGHT).center().draw(g, font);

        match &self.stage {
            Stage::Menu => {
                Text::new("Play with friends on any device.", CX, 78.0).color(palette::MUTED).center().draw(g, font);
                Text::new("Bots fill the empty seats.", CX, 92.0).color(palette::MUTED).center().draw(g, font);
                let name = shared.settings.player_name();
                let inset = Rect::centered(CX, 210.0, 200.0, 20.0);
                ui::inset(g, inset, palette::PANEL_DARK);
                Text::new(&format!("PLAYING AS  {name}"), CX, 210.0).color(palette::CREAM).center().draw(g, font);
            }
            Stage::Entry(field) => {
                let prompt = match field {
                    Field::Code => "Type the code your host sees.",
                    Field::Name => "What should the table call you?",
                };
                Text::new(prompt, CX, 78.0).color(palette::MUTED).center().draw(g, font);
                match field {
                    Field::Code => {
                        let letters: Vec<char> = self.entry.chars().collect();
                        draw_tiles(g, shared, &letters, 5, 112.0, self.age);
                    }
                    Field::Name => {
                        let inset = Rect::centered(CX, 112.0, 200.0, 26.0);
                        ui::inset(g, inset, palette::PANEL_DARK);
                        let caret = if (self.age * 2.0).fract() < 0.5 { "_" } else { " " };
                        let shown = format!("{}{caret}", self.entry);
                        Text::new(&shown, CX, 112.0).scale(1.5).color(palette::GOLD).center().draw(g, font);
                    }
                }
            }
            Stage::Connecting => {
                let dots = ".".repeat(1 + (self.age * 3.0) as usize % 3);
                Text::new(&format!("Pulling up a chair{dots}"), CX, 170.0)
                    .color(palette::CREAM)
                    .center()
                    .draw(g, font);
            }
            Stage::Seated => {
                if let Some(code) = &self.code {
                    Text::new("TABLE CODE", CX, 80.0).color(palette::MUTED).center().draw(g, font);
                    let letters: Vec<char> = code.chars().collect();
                    draw_tiles(g, shared, &letters, 5, 106.0, 99.0);
                    Text::new("Friends choose PLAY ONLINE, then JOIN WITH A CODE.", CX, 134.0)
                        .color(palette::MUTED)
                        .center()
                        .draw(g, font);
                }
                // A little felt oval with the four seats around it.
                let felt = Rect::centered(CX, SEATS_Y, 150.0, 50.0);
                g.set_color(palette::INK);
                ui::chamfer(g, Fill, felt.x - 1.0, felt.y - 1.0, felt.w + 2.0, felt.h + 2.0, 13.0);
                g.set_color(palette::FELT);
                ui::chamfer(g, Fill, felt.x, felt.y, felt.w, felt.h, 12.0);
                let (line1, line2) = match &self.seats {
                    Some(s) if s.playing => ("A game is on...".to_string(), String::new()),
                    Some(s) => (
                        format!("Playing to {}", s.target),
                        if s.stick_the_dealer { "Stick the dealer" } else { "Misdeals redeal" }.to_string(),
                    ),
                    None => (String::new(), String::new()),
                };
                Text::new(&line1, CX, SEATS_Y - 6.0).color(palette::CREAM).center().draw(g, font);
                Text::new(&line2, CX, SEATS_Y + 7.0).color(palette::MUTED).center().draw(g, font);
                if let Some(s) = &self.seats {
                    for seat in 0..4 {
                        let info = &s.seats[seat];
                        let p = seat_pos(seat);
                        let name = if seat == 0 { format!("{} (YOU)", info.name) } else { info.name.clone() };
                        let (color, tag) = match info.kind {
                            SeatKind::Human => (palette::CREAM, ""),
                            SeatKind::Away => (palette::DIM, " (AWAY)"),
                            SeatKind::Bot => (palette::MUTED, " (BOT)"),
                        };
                        let label = format!("{name}{tag}");
                        let w = font.width(&label) + 14.0;
                        let plate = Rect::centered(p.x, p.y, w, 13.0);
                        g.set_color(palette::INK);
                        ui::chamfer(g, Fill, plate.x - 1.0, plate.y - 1.0, plate.w + 2.0, plate.h + 2.0, 4.0);
                        g.set_color(if seat % 2 == 0 { palette::US_INK } else { palette::THEM_INK });
                        ui::chamfer(g, Fill, plate.x, plate.y, plate.w, plate.h, 3.0);
                        Text::new(&label, p.x, p.y).color(color).center().draw(g, font);
                    }
                }
            }
            Stage::Failed(why) => {
                for (i, line) in font.wrap(why, PANEL.w - 60.0).iter().enumerate() {
                    Text::new(line, CX, 150.0 + i as f32 * 14.0).color(palette::CREAM).center().draw(g, font);
                }
                Text::new("Check your connection and try again.", CX, 190.0)
                    .color(palette::MUTED)
                    .center()
                    .draw(g, font);
            }
        }

        for (act, b) in &self.buttons {
            if self.visible(*act) {
                b.draw(g, font);
            }
        }
        if let Some((note, _)) = &self.note {
            let w = font.width(note) + 16.0;
            g.set_color(palette::INK.with_alpha(0.75));
            ui::chamfer(g, Fill, CX - w / 2.0, PANEL.y + PANEL.h - 58.0, w, 15.0, 3.0);
            Text::new(note, CX, PANEL.y + PANEL.h - 50.5).color(palette::SUIT_RED_LIGHT).center().draw(g, font);
        }
        g.pop();

        let hint = match (&self.stage, shared.input) {
            (_, InputMode::Touch) => "",
            (Stage::Entry(_), _) => shared.hint("D-pad: Choose   A: Type   B: Delete", "Type, or click the keys   Enter: Done   Esc: Back"),
            _ => shared.hint("D-pad: Choose   A: Select   B: Back", "Esc: Back"),
        };
        Text::new(hint, 6.0, crate::render::VIEW_H as f32 - 12.0).color(palette::MUTED).draw(g, font);
    }

    fn key_pressed(&mut self, shared: &mut Shared, key: Key) -> Transition {
        if let Stage::Entry(field) = self.stage {
            match key {
                Key::Backspace | Key::Delete => return self.press(Act::Erase, shared),
                Key::Enter | Key::KpEnter if self.usable(Act::Done) => return self.press(Act::Done, shared),
                Key::Escape => return self.press(Act::Back, shared),
                Key::Space if field == Field::Name => return self.press(Act::Space, shared),
                Key::Minus if field == Field::Name => return self.press(Act::Key('-'), shared),
                _ => {}
            }
            let name = key.name();
            let mut chars = name.chars();
            if let (Some(ch), None) = (chars.next(), chars.next()) {
                let ch = ch.to_ascii_uppercase();
                let allowed = match field {
                    Field::Code => CODE_KEYS.contains(ch),
                    Field::Name => ch.is_ascii_alphanumeric(),
                };
                if allowed {
                    if let Some((_, b)) = self.buttons.iter_mut().find(|(a, _)| *a == Act::Key(ch)) {
                        b.flash(&shared.sfx);
                    }
                    self.type_char(ch, shared);
                } else {
                    shared.sfx.play(Sound::Deny);
                }
            }
            return Transition::None;
        }
        if key == Key::Escape {
            let back = if matches!(self.stage, Stage::Seated | Stage::Connecting) { Act::Leave } else { Act::Back };
            return self.press(back, shared);
        }
        Transition::None
    }

    fn nav(&mut self, shared: &mut Shared, nav: Nav) -> Transition {
        match nav {
            Nav::Up | Nav::Down | Nav::Left | Nav::Right => self.move_focus(nav, shared),
            Nav::Confirm => {
                let Some(&(act, _)) = self.buttons.get(self.focus) else { return Transition::None };
                let usable = self.usable(act);
                let b = &mut self.buttons[self.focus].1;
                if !usable {
                    b.deny(&shared.sfx);
                    return Transition::None;
                }
                b.flash(&shared.sfx);
                return self.press(act, shared);
            }
            Nav::Back => {
                return match self.stage {
                    // B deletes while typing, and backs out once the field is empty.
                    Stage::Entry(_) if !self.entry.is_empty() => self.press(Act::Erase, shared),
                    Stage::Seated | Stage::Connecting => self.press(Act::Leave, shared),
                    _ => self.press(Act::Back, shared),
                };
            }
            Nav::Menu => {
                if let Stage::Entry(_) = self.stage
                    && self.usable(Act::Done)
                {
                    return self.press(Act::Done, shared);
                }
                if self.stage == Stage::Seated && self.usable(Act::Deal) {
                    return self.press(Act::Deal, shared);
                }
            }
            _ => {}
        }
        Transition::None
    }

    fn mouse_pressed(&mut self, shared: &mut Shared, pos: Vec2, button: MouseButton) -> Transition {
        if button != MouseButton::Left {
            return Transition::None;
        }
        for i in 0..self.buttons.len() {
            let act = self.buttons[i].0;
            if !self.visible(act) {
                continue;
            }
            if self.buttons[i].1.click(pos, &shared.sfx) {
                self.focus = i;
                return self.press(act, shared);
            }
        }
        Transition::None
    }

    fn suspend(&mut self, _shared: &mut Shared) {
        if let Some(link) = &mut self.link {
            link.send(ClientMsg::Leave);
        }
        platform::set_presence(None);
    }
}

/// Five letter tiles (brass for typed letters, an empty slot otherwise).
fn draw_tiles(g: &mut Graphics, shared: &Shared, letters: &[char], n: usize, y: f32, age: f32) {
    let font = &shared.font;
    let (w, h, gap) = (26.0, 30.0, 6.0);
    let total = n as f32 * w + (n - 1) as f32 * gap;
    let x0 = (CX - total / 2.0).round();
    for i in 0..n {
        let r = Rect::new(x0 + i as f32 * (w + gap), y - h / 2.0, w, h);
        match letters.get(i) {
            Some(ch) => {
                g.set_color(palette::INK);
                ui::chamfer(g, Fill, r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 4.0, 4.0);
                g.set_color(palette::BRASS_DARK);
                ui::chamfer(g, Fill, r.x, r.y + 2.0, r.w, r.h, 3.0);
                g.set_color(palette::BRASS);
                ui::chamfer(g, Fill, r.x, r.y, r.w, r.h, 3.0);
                g.set_color(palette::BRASS_LIGHT);
                g.rectangle(Fill, r.x + 3.0, r.y + 1.0, r.w - 6.0, 1.0);
                Text::new(&ch.to_string(), r.center().x, r.center().y)
                    .scale(2.0)
                    .color(palette::INK)
                    .shadow(palette::BRASS_LIGHT)
                    .center()
                    .draw(g, font);
            }
            None => {
                ui::inset(g, r, palette::PANEL_DARK);
                if i == letters.len() && (age * 2.0).fract() < 0.5 {
                    g.set_color(palette::GOLD);
                    g.rectangle(Fill, r.x + 6.0, r.y + r.h - 7.0, r.w - 12.0, 2.0);
                }
            }
        }
    }
}
