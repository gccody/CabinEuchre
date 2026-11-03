//! PLAY ONLINE: host a table or join one with its code, then wait at the
//! table for friends until someone deals.
//!
//! Stages:
//! - **Menu**: HOST A TABLE, JOIN WITH A CODE, your name.
//! - **Entry**: an on-screen keyboard for the six-letter code (or your
//!   name). Mouse, touch, gamepad and a real keyboard all type on it, so it
//!   works the same on a phone, a Steam Deck or a desktop.
//! - **Connecting**, then **Seated**: the room code in big brass tiles, the
//!   four seats (tap a bot's seat to move there), INVITE (Steam) and LEAVE.
//!   Bots fill any seat nobody takes. Only the host gets DEAL. Tables are
//!   private: a Steam friend's "Join Game" only asks, and the host gets LET IN /
//!   NO. The host can MAKE PUBLIC so friends sit straight down. The code always
//!   gets you in. If a game is on, you're dealt in at the next hand.
//! - **Knocking**: asked to join by "Join Game", waiting for the host.
//! - **Failed**: why, and BACK.
//!
//! When the room deals (or you join a game in progress) the connection is
//! handed to the table scene.

use std::collections::VecDeque;
use std::time::Duration;

use love2drs::prelude::*;

use crate::device;
use crate::euchre::protocol::{CODE_LEN, ClientMsg, ErrorCode, KnockInfo, SeatKind, SeatsInfo, ServerMsg, clean_code};
use crate::juice::Spring;
use crate::net::online::Join;
use crate::net::{Connection, Link, Status, seat_memory};
use crate::palette;
use crate::platform;
use crate::scene::{Nav, Scene, Shared, Transition};
use crate::scenes::table::Table;
use crate::scenes::title::Title;
use crate::sfx::Sound;
use crate::text::Text;
use crate::ui::{self, Button, Rect};

const PANEL: Rect = Rect::new(130.0, 34.0, 380.0, 292.0);
const CX: f32 = PANEL.x + PANEL.w / 2.0;
/// On a phone the panel fills the screen (same center), so keys and buttons
/// can be finger-sized. The seated view splits into two columns: the code and
/// the table's settings on the left, the seats on the right.
const MOBILE_PANEL: Rect = Rect::new(14.0, 8.0, 612.0, 344.0);
const MOBILE_LEFT_X: f32 = 170.0;
const MOBILE_RIGHT_X: f32 = 470.0;
/// Middle of the little table in the phone's seated view.
const MOBILE_SEATS_Y: f32 = 165.0;
/// How far the top of the phone's menu and keyboard screens sits above the desktop's.
const MOBILE_TEXT_LIFT: f32 = 22.0;

fn panel() -> Rect {
    if device::mobile() { MOBILE_PANEL } else { PANEL }
}

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
    /// Asked to join by handle; the host hasn't answered yet.
    Knocking,
    Seated,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Act {
    /// Back to the table we still hold a seat at.
    Rejoin,
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
    /// The host flips the table between private and public.
    Public(bool),
    /// The host's answer to a request to join.
    Admit(u32),
    Refuse(u32),
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
    /// What friends see instead of the code (Steam rich presence).
    handle: Option<String>,
    /// Requests to join waiting for our answer (only while we host).
    knocks: Vec<KnockInfo>,
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
            handle: None,
            knocks: Vec::new(),
            note: None,
            pop: Spring::new(0.85),
            age: 0.0,
            dealing: false,
            stage_age: 0.0,
            autoplay: crate::device::var("EUCHRE_AUTOPLAY").is_some_and(|v| v == "1"),
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

    /// Where a friend's invite (`+connect CODE`: sit straight down) or
    /// "Join Game" (`+knock HANDLE`: ask the host) leads.
    pub fn from_request(shared: &Shared, request: platform::JoinRequest) -> Self {
        let mut l = Lobby::new(shared);
        let join = match request {
            platform::JoinRequest::Code(code) => Join::Code(code),
            platform::JoinRequest::Knock(handle) => Join::Knock(handle),
        };
        l.connect(join, shared);
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
        self.handle = None;
        self.knocks.clear();
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

    /// Rebuilds the controls in place (the seats or requests changed),
    /// keeping the focus on the same control where it still exists.
    fn rebuild(&mut self, shared: &Shared) {
        let was = self.buttons.get(self.focus).map(|(a, _)| *a);
        self.buttons = self.build(shared);
        self.focus = was.and_then(|a| self.buttons.iter().position(|(b, _)| *b == a)).unwrap_or_else(|| self.seated_focus());
    }

    /// Where the focus starts when sitting down: on DEAL, not on something a second
    /// press would change by accident. A guest has no DEAL, and LEAVE is the last resort.
    fn seated_focus(&self) -> usize {
        let find = |wanted: &dyn Fn(Act) -> bool| self.buttons.iter().position(|(a, _)| wanted(*a));
        find(&|a| a == Act::Deal)
            .or_else(|| find(&|a| a != Act::Leave && self.visible(a) && self.usable(a)))
            .or_else(|| find(&|a| a == Act::Leave))
            .unwrap_or(0)
    }

    /// Gets up from the table. While we're still connecting nothing has happened yet:
    /// a seat we held from before stays ours, along with the memory of it, and only a
    /// seat we might just have been given goes back.
    fn give_up_seat(&mut self) {
        let arrived = self.code.is_some();
        if let Some(link) = &mut self.link
            && (arrived || !link.rejoining())
        {
            link.send(ClientMsg::Leave);
        }
        if arrived {
            seat_memory::forget();
        }
    }

    /// We answer requests to join and choose public or private.
    fn hosting(&self) -> bool {
        self.seats.as_ref().is_some_and(|s| s.host == Some(0))
    }

    /// The controls for the current stage.
    fn build(&self, shared: &Shared) -> Vec<(Act, Button)> {
        if device::mobile() {
            return self.build_mobile();
        }
        let mut b: Vec<(Act, Button)> = Vec::new();
        let bottom = PANEL.y + PANEL.h - 26.0;
        match &self.stage {
            Stage::Menu => {
                if let Some(code) = seat_memory::code() {
                    let label = format!("REJOIN TABLE {code}");
                    b.push((Act::Rejoin, Button::new(label, Rect::centered(CX, 85.0, 160.0, 20.0), palette::ORANGE, palette::ORANGE_DARK)));
                }
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
            Stage::Connecting | Stage::Knocking => {
                b.push((Act::Leave, Button::grey("CANCEL", Rect::centered(CX, bottom, 90.0, 18.0))));
            }
            Stage::Seated => {
                // The host's answer to the first request comes first, so the
                // focus lands on LET IN.
                if let Some(k) = self.knocks.first().filter(|_| self.hosting()) {
                    b.push((Act::Admit(k.id), Button::green("LET IN", Rect::centered(CX - 34.0, KNOCK_Y + 10.0, 60.0, 18.0))));
                    b.push((Act::Refuse(k.id), Button::red("NO", Rect::centered(CX + 34.0, KNOCK_Y + 10.0, 60.0, 18.0))));
                }
                if self.hosting() {
                    let public = self.seats.as_ref().is_some_and(|s| s.public);
                    let label = if public { "MAKE PRIVATE" } else { "MAKE PUBLIC" };
                    let x = self.privacy_left(shared) + self.privacy_width(shared) + PRIVACY_GAP;
                    b.push((Act::Public(!public), Button::grey(label, Rect::new(x, PRIVACY_Y - 8.0, PRIVACY_BUTTON_W, 16.0))));
                }
                for seat in 1..4 {
                    let p = seat_pos(seat);
                    b.push((Act::Sit(seat), Button::grey("SIT HERE", Rect::centered(p.x, p.y + 16.0, 62.0, 14.0))));
                }
                let mut row = Vec::new();
                if self.hosting() {
                    row.push((Act::Deal, Button::brass("DEAL", Rect::new(0.0, bottom - 10.0, 84.0, 22.0)).scaled(1.2)));
                }
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

    /// [`Lobby::build`] for a phone: the same controls, sized for fingers.
    fn build_mobile(&self) -> Vec<(Act, Button)> {
        let mut b: Vec<(Act, Button)> = Vec::new();
        let bottom = MOBILE_PANEL.y + MOBILE_PANEL.h - 32.0;
        // Lays a row of buttons out side by side, centered.
        let spread = |row: &mut Vec<(Act, Button)>, gap: f32| {
            let total: f32 = row.iter().map(|(_, b)| b.rect.w).sum::<f32>() + gap * (row.len() - 1) as f32;
            let mut x = (CX - total / 2.0).round();
            for (_, btn) in row.iter_mut() {
                btn.rect.x = x;
                x += btn.rect.w + gap;
            }
        };
        match &self.stage {
            Stage::Menu => {
                if let Some(code) = seat_memory::code() {
                    let label = format!("REJOIN TABLE {code}");
                    b.push((Act::Rejoin, Button::new(label, Rect::centered(CX, 62.0, 300.0, 38.0), palette::ORANGE, palette::ORANGE_DARK).scaled(1.2)));
                }
                b.push((Act::Host, Button::brass("HOST A TABLE", Rect::centered(CX, 108.0, 300.0, 46.0)).scaled(2.0)));
                b.push((Act::JoinCode, Button::blue("JOIN WITH A CODE", Rect::centered(CX, 160.0, 300.0, 42.0)).scaled(1.5)));
                b.push((Act::Name, Button::green("CHANGE NAME", Rect::centered(CX, 240.0, 220.0, 40.0)).scaled(1.5)));
                b.push((Act::Back, Button::grey("BACK", Rect::centered(CX, bottom, 160.0, 40.0)).scaled(1.5)));
            }
            Stage::Entry(field) => {
                let (keys, cols) = match field {
                    Field::Code => (CODE_KEYS, 8),
                    Field::Name => (NAME_KEYS, 9),
                };
                let (kw, kh, gap) = (58.0, 44.0, 6.0);
                let width = cols as f32 * kw + (cols - 1) as f32 * gap;
                let x0 = (CX - width / 2.0).round();
                for (i, ch) in keys.chars().enumerate() {
                    let (row, col) = (i / cols, i % cols);
                    let r = Rect::new(x0 + col as f32 * (kw + gap), 112.0 + row as f32 * (kh + gap), kw, kh);
                    let mut key = Button::new(ch.to_string(), r, palette::CREAM, palette::CREAM_SHADE.lerp(palette::INK, 0.3)).scaled(1.5);
                    key.text_color = palette::INK;
                    b.push((Act::Key(ch), key));
                }
                let y = 112.0 + 3.0 * (kh + gap) + 6.0;
                let mut row = vec![(Act::Back, Button::grey("BACK", Rect::new(0.0, y, 110.0, 44.0)).scaled(1.5))];
                if *field == Field::Name {
                    row.push((Act::Space, Button::grey("SPACE", Rect::new(0.0, y, 110.0, 44.0)).scaled(1.5)));
                }
                row.push((Act::Erase, Button::red("DEL", Rect::new(0.0, y, 96.0, 44.0)).scaled(1.5)));
                let done = if *field == Field::Code { "JOIN" } else { "DONE" };
                row.push((Act::Done, Button::brass(done, Rect::new(0.0, y, 130.0, 44.0)).scaled(1.5)));
                spread(&mut row, 12.0);
                b.extend(row);
            }
            Stage::Connecting | Stage::Knocking => {
                b.push((Act::Leave, Button::grey("CANCEL", Rect::centered(CX, bottom, 160.0, 40.0)).scaled(1.5)));
            }
            Stage::Seated => {
                if let Some(k) = self.knocks.first().filter(|_| self.hosting()) {
                    let y = MOBILE_SEATS_Y + 24.0;
                    b.push((Act::Admit(k.id), Button::green("LET IN", Rect::centered(MOBILE_RIGHT_X - 58.0, y, 104.0, 40.0)).scaled(1.5)));
                    b.push((Act::Refuse(k.id), Button::red("NO", Rect::centered(MOBILE_RIGHT_X + 58.0, y, 104.0, 40.0)).scaled(1.5)));
                }
                if self.hosting() {
                    let public = self.seats.as_ref().is_some_and(|s| s.public);
                    let label = if public { "MAKE PRIVATE" } else { "MAKE PUBLIC" };
                    b.push((Act::Public(!public), Button::grey(label, Rect::centered(MOBILE_LEFT_X, 186.0, 200.0, 38.0)).scaled(1.2)));
                }
                for seat in 1..4 {
                    let p = seat_pos(seat);
                    b.push((Act::Sit(seat), Button::grey("SIT HERE", Rect::centered(p.x, p.y + 28.0, 88.0, 32.0))));
                }
                let mut row = Vec::new();
                if self.hosting() {
                    row.push((Act::Deal, Button::brass("DEAL", Rect::new(0.0, bottom - 24.0, 150.0, 46.0)).scaled(2.0)));
                }
                if platform::can_invite() {
                    row.push((Act::Invite, Button::blue("INVITE FRIENDS", Rect::new(0.0, bottom - 22.0, 160.0, 42.0)).scaled(1.2)));
                }
                row.push((Act::Leave, Button::grey("LEAVE", Rect::new(0.0, bottom - 22.0, 110.0, 42.0)).scaled(1.5)));
                spread(&mut row, 14.0);
                b.extend(row);
            }
            Stage::Failed(_) => {
                b.push((Act::Back, Button::brass("BACK", Rect::centered(CX, bottom, 160.0, 40.0)).scaled(1.5)));
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
            Act::Deal => self.hosting() && self.seats.as_ref().is_some_and(|s| !s.playing) && !self.dealing,
            _ => true,
        }
    }

    fn visible(&self, act: Act) -> bool {
        match act {
            // Hidden under a request to join.
            Act::Sit(seat) => {
                self.seats.as_ref().is_some_and(|s| s.seats[seat].kind == SeatKind::Bot) && !self.knock_showing()
            }
            _ => true,
        }
    }

    /// The private / public note: a tag and what it means for friends.
    fn privacy_note(&self) -> (&'static str, &'static str) {
        let public = self.seats.as_ref().is_some_and(|s| s.public);
        match (public, self.hosting()) {
            (false, true) => ("PRIVATE", "Friends ask you first"),
            (false, false) => ("PRIVATE", "Friends ask the host first"),
            (true, _) => ("PUBLIC", "Friends can join freely"),
        }
    }

    fn privacy_width(&self, shared: &Shared) -> f32 {
        let (tag, rest) = self.privacy_note();
        shared.font.width(&format!("{tag}  {rest}"))
    }

    /// Where the note starts: the note and the host's button are centered together.
    fn privacy_left(&self, shared: &Shared) -> f32 {
        let button = if self.hosting() { PRIVACY_GAP + PRIVACY_BUTTON_W } else { 0.0 };
        (CX - (self.privacy_width(shared) + button) / 2.0).round()
    }

    /// A guest at a table with no game on.
    fn waiting_for_host(&self) -> bool {
        !self.hosting() && self.seats.as_ref().is_some_and(|s| !s.playing)
    }

    fn knock_showing(&self) -> bool {
        self.hosting() && !self.knocks.is_empty()
    }

    fn press(&mut self, act: Act, shared: &mut Shared) -> Transition {
        shared.tap();
        match act {
            Act::Rejoin => {
                if let Some(code) = seat_memory::code() {
                    self.connect(Join::Code(code), shared);
                }
            }
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
                    // Nothing typed means no name of your own: back to the store's, or PLAYER.
                    shared.settings.name = if self.entry.trim().is_empty() {
                        String::new()
                    } else {
                        crate::euchre::protocol::clean_name(&self.entry)
                    };
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
                self.give_up_seat();
                self.link = None;
                platform::set_presence(None);
                self.set_stage(Stage::Menu, shared);
            }
            Act::Public(public) => {
                if let Some(link) = &mut self.link {
                    link.send(ClientMsg::SetPublic { public });
                    shared.sfx.play(Sound::CardSlide);
                }
            }
            Act::Admit(id) | Act::Refuse(id) => {
                if let Some(link) = &mut self.link {
                    link.send(ClientMsg::Answer { id, accept: matches!(act, Act::Admit(_)) });
                }
                // Gone at once; the room's next list (or seats) confirms it.
                self.knocks.retain(|k| k.id != id);
                self.rebuild(shared);
            }
        }
        Transition::None
    }

    fn type_char(&mut self, ch: char, shared: &mut Shared) {
        let max = match self.stage {
            Stage::Entry(Field::Code) => CODE_LEN,
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
            && self.entry.len() == CODE_LEN
            && let Some(i) = self.buttons.iter().position(|(a, _)| *a == Act::Done)
        {
            self.focus = i;
        }
    }

    /// Reads the room's messages. Returns the table once it has dealt.
    fn poll(&mut self, shared: &mut Shared) -> Option<Transition> {
        let mut deal = false;
        let mut rebuild = false;
        while let Some(msg) = self.link.as_mut()?.poll() {
            match &msg {
                ServerMsg::Welcome { room, handle, .. } => {
                    self.code = Some(room.clone());
                    self.handle = Some(handle.clone());
                }
                ServerMsg::Waiting => {
                    if self.stage == Stage::Connecting {
                        self.set_stage(Stage::Knocking, shared);
                    }
                }
                ServerMsg::Knocks { requests } => {
                    if requests.iter().any(|k| !self.knocks.iter().any(|o| o.id == k.id)) {
                        shared.sfx.play(Sound::Pop);
                        shared.buzz(0.0, 0.0, 0.5, 0.08);
                    }
                    self.knocks = requests.clone();
                    rebuild = true;
                }
                ServerMsg::Seats(s) => {
                    let people = s.seats.iter().filter(|x| x.kind != SeatKind::Bot).count();
                    if let Some(old) = &self.seats {
                        let before = old.seats.iter().filter(|x| x.kind != SeatKind::Bot).count();
                        if people > before {
                            shared.sfx.play(Sound::Pop);
                        }
                    }
                    if let Some(handle) = &self.handle {
                        platform::set_presence(Some((handle, people)));
                    }
                    if s.host != Some(0) {
                        // Somebody else answers the door now.
                        self.knocks.clear();
                    }
                    self.seats = Some(s.clone());
                    rebuild = true;
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
        if rebuild && self.stage == Stage::Seated {
            self.rebuild(shared);
        }
        if matches!(self.stage, Stage::Connecting | Stage::Knocking) && self.code.is_some() && self.seats.is_some() {
            shared.sfx.play(Sound::Trump);
            if self.autoplay || crate::device::var("EUCHRE_SCENE").is_some() {
                // Debug runs: what netcheck needs to join (the code) or knock (the handle).
                eprintln!(
                    "[euchre] seated at table {}, handle {}",
                    self.code.as_deref().unwrap_or("?"),
                    self.handle.as_deref().unwrap_or("?")
                );
            }
            self.set_stage(Stage::Seated, shared);
            self.focus = self.seated_focus();
        }
        if self.stage == Stage::Seated
            && matches!(self.link.as_ref().map(Link::status), Some(Status::Connecting | Status::Reconnecting))
        {
            // Whatever we pressed while the connection was down never got there.
            self.dealing = false;
            self.note = Some(("Reconnecting...".into(), 0.3));
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
const SEATS_Y: f32 = 218.0;
/// The private / public line under the code.
const PRIVACY_Y: f32 = 144.0;
/// The host's MAKE PUBLIC button, after the note.
const PRIVACY_BUTTON_W: f32 = 88.0;
const PRIVACY_GAP: f32 = 10.0;
/// Middle of a request to join, drawn over the felt.
const KNOCK_Y: f32 = SEATS_Y - 2.0;

/// Where each seat's plate sits in the seated view (relative seats: you at the bottom).
fn seat_pos(seat: usize) -> Vec2 {
    if device::mobile() {
        let c = vec2(MOBILE_RIGHT_X, MOBILE_SEATS_Y);
        return [c + vec2(0.0, 58.0), c + vec2(-100.0, -20.0), c + vec2(0.0, -96.0), c + vec2(100.0, -20.0)][seat];
    }
    let c = vec2(CX, SEATS_Y);
    [c + vec2(0.0, 44.0), c + vec2(-126.0, -6.0), c + vec2(0.0, -48.0), c + vec2(126.0, -6.0)][seat]
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
        let panel = panel();
        let mobile = device::mobile();
        // The phone's menu and keyboard screens start higher, to fit the bigger controls.
        let lift = if mobile { MOBILE_TEXT_LIFT } else { 0.0 };
        let c = panel.center();
        g.translate(c.x, c.y);
        g.scale(self.pop.value, self.pop.value);
        g.translate(-c.x, -c.y);
        ui::panel(g, panel, palette::PANEL);
        let title = match &self.stage {
            Stage::Menu => "PLAY ONLINE",
            Stage::Entry(Field::Code) => "JOIN A TABLE",
            Stage::Entry(Field::Name) => "YOUR NAME",
            Stage::Connecting => "PLAY ONLINE",
            Stage::Knocking => "KNOCK KNOCK",
            Stage::Seated => "YOUR TABLE",
            Stage::Failed(_) => "NO LUCK",
        };
        Text::new(title, CX, panel.y + 20.0).scale(2.0).color(palette::BRASS_LIGHT).center().draw(g, font);

        match &self.stage {
            Stage::Menu => {
                // REJOIN takes this spot when there's a seat to go back to.
                if !self.buttons.iter().any(|(a, _)| *a == Act::Rejoin) {
                    Text::new("Play with friends on any device.", CX, 78.0 - lift).color(palette::MUTED).center().draw(g, font);
                    Text::new("Bots fill the empty seats.", CX, 92.0 - lift).color(palette::MUTED).center().draw(g, font);
                }
                let name = shared.settings.player_name();
                let name_y = if mobile { 200.0 } else { 210.0 };
                let inset = Rect::centered(CX, name_y, 200.0, 20.0);
                ui::inset(g, inset, palette::PANEL_DARK);
                Text::new(&format!("PLAYING AS  {name}"), CX, name_y).color(palette::CREAM).center().draw(g, font);
            }
            Stage::Entry(field) => {
                let prompt = match field {
                    Field::Code => "Type the code your host sees.",
                    Field::Name => "What should the table call you?",
                };
                Text::new(prompt, CX, 78.0 - lift).color(palette::MUTED).center().draw(g, font);
                match field {
                    Field::Code => {
                        let letters: Vec<char> = self.entry.chars().collect();
                        draw_tiles(g, shared, CX, &letters, 112.0 - lift * 1.2, self.age);
                    }
                    Field::Name => {
                        let name_y = 112.0 - lift * 1.2;
                        let inset = Rect::centered(CX, name_y, 200.0, 26.0);
                        ui::inset(g, inset, palette::PANEL_DARK);
                        let caret = if (self.age * 2.0).fract() < 0.5 { "_" } else { " " };
                        let shown = format!("{}{caret}", self.entry);
                        Text::new(&shown, CX, name_y).scale(1.5).color(palette::GOLD).center().draw(g, font);
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
            Stage::Knocking => {
                let dots = ".".repeat(1 + (self.age * 3.0) as usize % 3);
                Text::new("You asked to join this table.", CX, 150.0).color(palette::CREAM).center().draw(g, font);
                Text::new(&format!("Waiting for the host to let you in{dots}"), CX, 170.0)
                    .color(palette::MUTED)
                    .center()
                    .draw(g, font);
            }
            Stage::Seated if mobile => self.draw_seated_mobile(shared, g),
            Stage::Seated => {
                if let Some(code) = &self.code {
                    Text::new("TABLE CODE", CX, 76.0).color(palette::MUTED).center().draw(g, font);
                    let letters: Vec<char> = code.chars().collect();
                    draw_tiles(g, shared, CX, &letters, 100.0, 99.0);
                    Text::new("Anyone with the code can sit right down.", CX, 126.0)
                        .color(palette::MUTED)
                        .center()
                        .draw(g, font);
                }
                if let Some(s) = &self.seats {
                    let (tag, rest) = self.privacy_note();
                    let left = self.privacy_left(shared);
                    let color = if s.public { palette::GOLD } else { palette::CREAM };
                    Text::new(tag, left, PRIVACY_Y).color(color).draw(g, font);
                    Text::new(rest, left + font.width(&format!("{tag}  ")), PRIVACY_Y)
                        .color(palette::MUTED)
                        .draw(g, font);
                }
                // A little felt oval with the four seats around it.
                let felt = Rect::centered(CX, SEATS_Y, 150.0, 50.0);
                g.set_color(palette::INK);
                ui::chamfer(g, Fill, felt.x - 1.0, felt.y - 1.0, felt.w + 2.0, felt.h + 2.0, 13.0);
                g.set_color(palette::FELT);
                ui::chamfer(g, Fill, felt.x, felt.y, felt.w, felt.h, 12.0);
                let (line1, line2) = match &self.seats {
                    Some(s) if s.playing => ("A game is on...".to_string(), "In from the next hand".to_string()),
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
                if self.waiting_for_host() {
                    Text::new("Waiting for the host to deal", CX, 279.0).color(palette::MUTED).center().draw(g, font);
                }
                if self.knock_showing() {
                    let k = &self.knocks[0];
                    let more = if self.knocks.len() > 1 { format!(" (+{})", self.knocks.len() - 1) } else { String::new() };
                    let line = format!("{} asks to join{more}", k.name);
                    let card = Rect::centered(CX, KNOCK_Y, (font.width(&line) + 20.0).max(164.0), 50.0);
                    g.set_color(palette::INK);
                    ui::chamfer(g, Fill, card.x - 1.0, card.y - 1.0, card.w + 2.0, card.h + 4.0, 6.0);
                    g.set_color(palette::PANEL_DARK);
                    ui::chamfer(g, Fill, card.x, card.y, card.w, card.h, 5.0);
                    g.set_color(palette::BRASS);
                    ui::chamfer(g, Line, card.x + 2.0, card.y + 2.0, card.w - 4.0, card.h - 4.0, 4.0);
                    Text::new(&line, CX, KNOCK_Y - 12.0)
                        .color(palette::CREAM)
                        .center()
                        .draw(g, font);
                }
            }
            Stage::Failed(why) => {
                for (i, line) in font.wrap(why, panel.w - 60.0).iter().enumerate() {
                    Text::new(line, CX, 150.0 + i as f32 * 14.0).color(palette::CREAM).center().draw(g, font);
                }
                let advice = if why == ErrorCode::Declined.describe() {
                    "Ask them for the table code instead."
                } else {
                    "Check your connection and try again."
                };
                Text::new(advice, CX, 190.0)
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
            let note_y = if mobile { panel.y + panel.h - 84.0 } else { panel.y + panel.h - 58.0 };
            ui::chamfer(g, Fill, CX - w / 2.0, note_y, w, 15.0, 3.0);
            Text::new(note, CX, note_y + 7.5).color(palette::SUIT_RED_LIGHT).center().draw(g, font);
        }
        g.pop();
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
            let back =
                if matches!(self.stage, Stage::Seated | Stage::Connecting | Stage::Knocking) { Act::Leave } else { Act::Back };
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
                    Stage::Seated | Stage::Connecting | Stage::Knocking => self.press(Act::Leave, shared),
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
        self.give_up_seat();
        // Give it a moment to get out before the game exits.
        if let Some(link) = &mut self.link {
            link.finish(Duration::from_millis(300));
        }
        platform::set_presence(None);
    }
}

impl Lobby {
    /// The seated view on a phone: the code and the table's settings on the
    /// left, the four seats around a little table on the right.
    fn draw_seated_mobile(&self, shared: &Shared, g: &mut Graphics) {
        let font = &shared.font;
        let (lx, rx, sy) = (MOBILE_LEFT_X, MOBILE_RIGHT_X, MOBILE_SEATS_Y);
        if let Some(code) = &self.code {
            Text::new("TABLE CODE", lx, 50.0).color(palette::MUTED).center().draw(g, font);
            let letters: Vec<char> = code.chars().collect();
            draw_tiles(g, shared, lx, &letters, 76.0, 99.0);
            Text::new("Anyone with the code can", lx, 106.0).color(palette::MUTED).center().draw(g, font);
            Text::new("sit right down.", lx, 118.0).color(palette::MUTED).center().draw(g, font);
        }
        if let Some(s) = &self.seats {
            let (tag, rest) = self.privacy_note();
            let width = font.width(&format!("{tag}  {rest}"));
            let left = lx - width / 2.0;
            let color = if s.public { palette::GOLD } else { palette::CREAM };
            Text::new(tag, left, 144.0).color(color).anchor(0.0, 0.5).draw(g, font);
            Text::new(rest, left + font.width(&format!("{tag}  ")), 144.0).color(palette::MUTED).anchor(0.0, 0.5).draw(g, font);
            let (line1, line2) = if s.playing {
                ("A game is on...".to_string(), "You're in from the next hand.".to_string())
            } else {
                (format!("Playing to {}", s.target), if s.stick_the_dealer { "Stick the dealer" } else { "Misdeals redeal" }.to_string())
            };
            Text::new(&line1, lx, 222.0).color(palette::CREAM).center().draw(g, font);
            Text::new(&line2, lx, 236.0).color(palette::MUTED).center().draw(g, font);
            if self.waiting_for_host() {
                Text::new("Waiting for the host to deal", lx, 256.0).color(palette::MUTED).center().draw(g, font);
            }
        }
        let felt = Rect::centered(rx, sy, 96.0, 50.0);
        g.set_color(palette::INK);
        ui::chamfer(g, Fill, felt.x - 1.0, felt.y - 1.0, felt.w + 2.0, felt.h + 2.0, 13.0);
        g.set_color(palette::FELT);
        ui::chamfer(g, Fill, felt.x, felt.y, felt.w, felt.h, 12.0);
        Text::new("♣♦♠♥", rx, sy).color(palette::CREAM).center().draw(g, font);
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
                let plate = Rect::centered(p.x, p.y, w, 15.0);
                g.set_color(palette::INK);
                ui::chamfer(g, Fill, plate.x - 1.0, plate.y - 1.0, plate.w + 2.0, plate.h + 2.0, 4.0);
                g.set_color(if seat % 2 == 0 { palette::US_INK } else { palette::THEM_INK });
                ui::chamfer(g, Fill, plate.x, plate.y, plate.w, plate.h, 3.0);
                Text::new(&label, p.x, p.y).color(color).center().draw(g, font);
            }
        }
        if self.knock_showing() {
            let k = &self.knocks[0];
            let more = if self.knocks.len() > 1 { format!(" (+{})", self.knocks.len() - 1) } else { String::new() };
            let line = format!("{} asks to join{more}", k.name);
            let card = Rect::centered(rx, sy + 6.0, 300.0, 88.0);
            g.set_color(palette::INK);
            ui::chamfer(g, Fill, card.x - 1.0, card.y - 1.0, card.w + 2.0, card.h + 4.0, 6.0);
            g.set_color(palette::PANEL_DARK);
            ui::chamfer(g, Fill, card.x, card.y, card.w, card.h, 5.0);
            g.set_color(palette::BRASS);
            ui::chamfer(g, Line, card.x + 2.0, card.y + 2.0, card.w - 4.0, card.h - 4.0, 4.0);
            Text::new(&line, rx, sy - 22.0).scale(1.2).color(palette::CREAM).center().draw(g, font);
        }
    }
}

/// A code's letter tiles (brass for typed letters, an empty slot otherwise), centered on `cx`.
fn draw_tiles(g: &mut Graphics, shared: &Shared, cx: f32, letters: &[char], y: f32, age: f32) {
    let font = &shared.font;
    let n = CODE_LEN;
    let (w, h, gap) = (26.0, 30.0, 6.0);
    let total = n as f32 * w + (n - 1) as f32 * gap;
    let x0 = (cx - total / 2.0).round();
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
