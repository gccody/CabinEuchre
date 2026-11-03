//! Checks that need the real game: the actual Lobby, Table and Title scenes, the
//! real network client, a real window and GPU. Not part of a plain `cargo test`,
//! since it opens windows:
//!
//! ```sh
//! cargo test --test scene_checks                  # every check, each in its own process
//! cargo test --test scene_checks -- pause_focus   # only checks with this in their name
//! ```
//!
//! Each started as the proof of a bug found before the first release, and asserts the
//! correct behaviour. Each runs with its own empty config folder, so it never touches
//! your real settings, saved game or remembered seat. Checks that need the room server
//! expect `cd server && npx wrangler dev` on ws://127.0.0.1:8787 and say SKIPPED
//! without it.
//!
//! `shot_*` aren't checks: they set a scene up for a screenshot, for screens no
//! `EUCHRE_SCENE` reaches. `EUCHRE_WINDOW` and `EUCHRE_MOBILE` work as in the game:
//! `LOVE2DRS_SCREENSHOT=x.png LOVE2DRS_SCREENSHOT_FRAME=30 <test exe> --proof shot_guest_lobby`

#![allow(dead_code, unused_imports, unused_variables, clippy::type_complexity)]

#[path = "../src/app.rs"]
mod app;
#[path = "../src/assets.rs"]
mod assets;
#[path = "../src/cards.rs"]
mod cards;
#[path = "../src/device.rs"]
mod device;
use euchre_core as euchre;
#[path = "../src/juice.rs"]
mod juice;
#[path = "../src/net/mod.rs"]
mod net;
#[path = "../src/palette.rs"]
mod palette;
#[path = "../src/platform/mod.rs"]
mod platform;
#[path = "../src/render.rs"]
mod render;
#[path = "../src/savegame.rs"]
mod savegame;
#[path = "../src/scene.rs"]
mod scene;
#[path = "../src/scenes/mod.rs"]
mod scenes;
#[path = "../src/settings.rs"]
mod settings;
#[path = "../src/sfx.rs"]
mod sfx;
#[path = "../src/suits.rs"]
mod suits;
#[path = "../src/text.rs"]
mod text;
#[path = "../src/ui.rs"]
mod ui;

use std::collections::VecDeque;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use love2drs::prelude::*;
use tungstenite::protocol::CloseFrame;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::{Message, WebSocket};

use euchre::Match;
use euchre::protocol::{ClientMsg, HandStart, KnockInfo, SeatInfo, SeatKind, SeatsInfo, ServerMsg};
use net::online::Join;
use net::{Connection, Link, Status, seat_memory};
use scene::{InputMode, NO_POINTER, Nav, Scene, Shared, Transition};
use scenes::lobby::Lobby;
use scenes::table::Table;
use scenes::title::Title;

/// Where the real room server is: `wrangler dev`, unless `EUCHRE_PROOF_PORT` says otherwise.
fn port() -> String {
    std::env::var("EUCHRE_PROOF_PORT").unwrap_or_else(|_| "8787".into())
}

/// `EUCHRE_PROOF_SERVER=wss://...` runs the checks that only need a server against that one.
fn real_server() -> String {
    std::env::var("EUCHRE_PROOF_SERVER").unwrap_or_else(|_| format!("ws://127.0.0.1:{}", port()))
}

// ----------------------------------------------------------------------
// Running the checks
// ----------------------------------------------------------------------

const PROOFS: &[(&str, fn())] = &[
    ("guest_cant_deal_from_the_lobby", guest_cant_deal_from_the_lobby),
    ("host_can_deal_from_the_lobby", host_can_deal_from_the_lobby),
    ("confirmed_new_game_starts_fresh", confirmed_new_game_starts_fresh),
    ("helper_second_window", helper_second_window),
    ("shot_new_game_dialog", shot_new_game_dialog),
    ("shot_guest_lobby", shot_guest_lobby),
    ("shot_guest_lobby_mid_game", shot_guest_lobby_mid_game),
    ("cancel_connecting_keeps_remembered_seat", cancel_connecting_keeps_remembered_seat),
    ("cancelling_a_rejoin_keeps_the_seat", cancelling_a_rejoin_keeps_the_seat),
    ("pause_focus_shift_doesnt_make_table_public", pause_focus_shift_doesnt_make_table_public),
    ("pause_focus_shift_doesnt_refuse_a_friend", pause_focus_shift_doesnt_refuse_a_friend),
    ("seat_update_right_after_the_deal_is_applied", seat_update_right_after_the_deal_is_applied),
    ("closing_from_the_lobby_forgets_the_seat", closing_from_the_lobby_forgets_the_seat),
    ("new_game_keeps_the_earlier_save", new_game_keeps_the_earlier_save),
    ("clearing_your_name_goes_back_to_the_default", clearing_your_name_goes_back_to_the_default),
    ("second_window_doesnt_steal_the_seat", second_window_doesnt_steal_the_seat),
    ("a_replaced_connection_stays_closed", a_replaced_connection_stays_closed),
    ("leaving_while_reconnecting_still_leaves", leaving_while_reconnecting_still_leaves),
    ("server_restart_marks_absent_players_away", server_restart_marks_absent_players_away),
    ("idle_connection_stays_up", idle_connection_stays_up),
    ("release_exe_has_no_console_window", release_exe_has_no_console_window),
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(i) = args.iter().position(|a| a == "--proof") {
        let name = &args[i + 1];
        let proof = PROOFS.iter().find(|(n, _)| n == name).expect("no such proof").1;
        std::panic::set_hook(Box::new(|info| {
            println!("PANIC: {info}");
            std::process::exit(1);
        }));
        proof();
        pass();
    }
    let filter: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    let exe = std::env::current_exe().unwrap();
    let mut failed = 0;
    let wanted = |n: &str| !n.starts_with("helper_") && !n.starts_with("shot_") && (filter.is_empty() || filter.iter().any(|f| n.contains(f.as_str())));
    for (name, _) in PROOFS.iter().filter(|(n, _)| wanted(n)) {
        let dir = std::env::temp_dir().join(format!("euchre-proof-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = std::process::Command::new(&exe)
            .args(["--proof", name])
            .env("APPDATA", &dir)
            .env("HOME", &dir)
            .env_remove("EUCHRE_SERVER")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        let line = text.lines().rfind(|l| l.starts_with("FAILED:") || l.starts_with("PANIC:") || l.starts_with("OK") || l.starts_with("SKIP:") || l.starts_with("(setup")).unwrap_or("(no result)");
        let verdict = match out.status.code() {
            Some(0) => "ok",
            Some(77) => "SKIPPED",
            Some(2) => "SETUP FAILED",
            _ => {
                failed += 1;
                "FAILED"
            }
        };
        println!("{verdict:<15} {name}\n                {line}");
        let _ = std::fs::remove_dir_all(&dir);
    }
    println!("\n{failed} failed");
    std::process::exit(if failed > 0 { 1 } else { 0 });
}

fn fail(msg: impl std::fmt::Display) -> ! {
    println!("FAILED: {msg}");
    std::process::exit(1);
}

fn pass() -> ! {
    println!("OK");
    std::process::exit(0);
}

fn skip(msg: &str) -> ! {
    println!("SKIP: {msg}");
    std::process::exit(77);
}

fn use_server(url: &str) {
    // Safe: only the main thread is running at this point.
    unsafe { std::env::set_var("EUCHRE_SERVER", url) };
}

fn real_server_or_skip() {
    let address = format!("127.0.0.1:{}", port());
    if std::env::var("EUCHRE_PROOF_SERVER").is_err() && TcpStream::connect_timeout(&address.parse().unwrap(), Duration::from_millis(500)).is_err() {
        skip("no room server here (cd server && npx wrangler dev)");
    }
    use_server(&real_server());
}

// ----------------------------------------------------------------------
// A scripted server: says exactly what a proof tells it to, records replies
// ----------------------------------------------------------------------

#[derive(Clone)]
struct Fake {
    url: String,
    got: Arc<Mutex<Vec<ClientMsg>>>,
    say: Sender<ServerMsg>,
}

impl Fake {
    fn start() -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let got = Arc::new(Mutex::new(Vec::new()));
        let (say, said) = mpsc::channel::<ServerMsg>();
        let log = got.clone();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            ws.get_ref().set_read_timeout(Some(Duration::from_millis(10))).unwrap();
            loop {
                while let Ok(m) = said.try_recv() {
                    let _ = ws.send(Message::text(serde_json::to_string(&m).unwrap()));
                }
                match ws.read() {
                    Ok(Message::Text(t)) => {
                        if let Ok(m) = serde_json::from_str::<ClientMsg>(t.as_str()) {
                            log.lock().unwrap().push(m);
                        }
                    }
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                    Err(_) => return,
                }
            }
        });
        use_server(&url);
        Fake { url, got, say }
    }

    fn send(&self, m: ServerMsg) {
        self.say.send(m).unwrap();
    }

    fn got(&self) -> Vec<ClientMsg> {
        self.got.lock().unwrap().clone()
    }
}

fn welcome() -> ServerMsg {
    ServerMsg::Welcome { room: "KQJTAB".into(), token: "tok".into(), handle: "ab".repeat(32) }
}

fn seats(host: Option<usize>, seat1: SeatKind) -> ServerMsg {
    let seat = |name: &str, kind| SeatInfo { name: name.into(), kind };
    ServerMsg::Seats(SeatsInfo {
        seats: [seat("ME", SeatKind::Human), seat("BOB", seat1), seat("BENNY", SeatKind::Bot), seat("JULES", SeatKind::Bot)],
        playing: true,
        public: false,
        host,
        target: 10,
        stick_the_dealer: true,
    })
}

fn deal() -> ServerMsg {
    let cards: Vec<_> = euchre::Card::deck().into_iter().take(5).collect();
    ServerMsg::Deal(HandStart {
        game: 1,
        target: 10,
        stick_the_dealer: true,
        scores: [0, 0],
        hand_number: 1,
        dealer: 3,
        cards,
        up_card: euchre::Card::from_id(23),
    })
}

fn knocks(ids: &[u32]) -> ServerMsg {
    ServerMsg::Knocks { requests: ids.iter().map(|&id| KnockInfo { id, name: "CY".into() }).collect() }
}

// ----------------------------------------------------------------------
// A real window running a real scene, driven by a script
// ----------------------------------------------------------------------

type Run<'a> = Box<dyn FnOnce(&mut H) + 'static>;
type Cond = Box<dyn FnMut(&mut H) -> bool + 'static>;

enum Step {
    Do(Run<'static>),
    /// Waits until the condition holds. A timeout fails the proof with this text.
    Until(&'static str, f32, Cond),
    Wait(f32),
}

struct H {
    shared: Shared,
    scene: Option<Box<dyn Scene>>,
}

impl H {
    fn scene(&mut self) -> &mut Box<dyn Scene> {
        self.scene.as_mut().expect("a scene is running")
    }

    fn apply(&mut self, t: Transition) {
        if let Transition::Switch(next) = t {
            self.scene = Some(next);
        }
    }

    fn nav(&mut self, nav: Nav) {
        self.shared.input = InputMode::Buttons;
        let mut scene = self.scene.take().unwrap();
        let t = scene.nav(&mut self.shared, nav);
        self.scene = Some(scene);
        self.apply(t);
    }
}

struct Harness {
    renderer: render::Renderer,
    h: H,
    steps: VecDeque<Step>,
    waiting: f32,
    since: f32,
}

fn script(make_scene: impl FnOnce(&Shared) -> Box<dyn Scene> + 'static, steps: Vec<Step>) -> ! {
    let (w, h) = std::env::var("EUCHRE_WINDOW")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((640, 360));
    let conf = love2drs::Conf::new("Cabin Euchre check").size(w, h);
    let result = love2drs::run(conf, move |ctx: &mut Context| {
        let shared = Shared {
            font: text::PixelFont::new(&ctx.graphics),
            cards: cards::CardArt::new(&ctx.graphics),
            sfx: sfx::Sfx::new(&ctx.audio),
            settings: settings::Settings::load(),
            time: 0.0,
            mouse: NO_POINTER,
            mouse_down: false,
            input: InputMode::Buttons,
            rumbles: Vec::new(),
        };
        ctx.audio.set_volume(0.0);
        let scene = make_scene(&shared);
        let renderer = render::Renderer::new(&mut ctx.graphics, &ctx.window)?;
        Ok(Harness { renderer, h: H { shared, scene: Some(scene) }, steps: steps.into(), waiting: 0.0, since: 0.0 })
    });
    fail(format!("the window closed before the script finished: {:?}", result.err().map(|e| e.to_string())));
}

impl Game for Harness {
    fn draw(&mut self, ctx: &mut Context) {
        let scene = self.h.scene.as_mut().unwrap();
        let shared = &self.h.shared;
        self.renderer.frame(&mut ctx.graphics, |g| scene.draw(shared, g));
    }

    fn update(&mut self, ctx: &mut Context, dt: f32) {
        self.h.shared.time += dt;
        self.renderer.update(&ctx.graphics, &ctx.window, self.h.shared.time);
        let mut scene = self.h.scene.take().unwrap();
        let t = scene.update(&mut self.h.shared, ctx, dt);
        self.h.scene = Some(scene);
        self.h.apply(t);
        self.h.shared.rumbles.clear();
        if self.waiting > 0.0 {
            self.waiting -= dt;
            return;
        }
        self.since += dt;
        loop {
            match self.steps.pop_front() {
                None => pass(),
                Some(Step::Do(f)) => f(&mut self.h),
                Some(Step::Wait(s)) => {
                    self.waiting = s;
                    self.since = 0.0;
                    return;
                }
                Some(Step::Until(what, secs, mut cond)) => {
                    if cond(&mut self.h) {
                        self.since = 0.0;
                        continue;
                    }
                    if self.since > secs {
                        println!("(setup problem) gave up waiting: {what}");
                        std::process::exit(2);
                    }
                    self.steps.push_front(Step::Until(what, secs, cond));
                    return;
                }
            }
        }
    }
}

fn act(f: impl FnOnce(&mut H) + 'static) -> Step {
    Step::Do(Box::new(f))
}

fn nav(n: Nav) -> Step {
    act(move |h| h.nav(n))
}

/// A seat at a table where the game hasn't started, with the host somewhere.
fn seated(host: Option<usize>, playing: bool) -> ServerMsg {
    let mut seated = seats(host, SeatKind::Human);
    if let ServerMsg::Seats(s) = &mut seated {
        s.playing = playing;
    }
    seated
}

/// The lobby, sitting at a table the fake server runs.
fn in_lobby(fake: &Fake, seats: ServerMsg, then: Vec<Step>) -> ! {
    let f = fake.clone();
    let mut steps = vec![
        act(move |_| {
            f.send(welcome());
            f.send(seats);
        }),
        Step::Until("seat remembered", 5.0, Box::new(|_| seat_memory::code().is_some())),
        Step::Wait(0.2),
    ];
    steps.extend(then);
    script(|shared| Box::new(Lobby::host(shared)), steps)
}

fn remember_seat(code: &str, token: &str) {
    let dir = settings::config_dir().unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("online_seat.txt"), format!("{code} {token}\n")).unwrap();
}

/// A live online table, where the fake server made us the host (or not).
fn online_table(fake: &Fake, greeting: Vec<ServerMsg>) -> Box<dyn FnOnce(&Shared) -> Box<dyn Scene>> {
    for m in greeting {
        fake.send(m);
    }
    Box::new(|shared: &Shared| {
        let mut link = Link::Online(Connection::open(Join::Code("KQJTAB".into()), "ME", ""));
        let until = Instant::now() + Duration::from_secs(5);
        let mut pending = VecDeque::new();
        // Everything arrives together, as it does when a frame is slow or a phone wakes up.
        std::thread::sleep(Duration::from_millis(400));
        loop {
            match Table::join(shared, link, None, pending) {
                Ok(t) => return Box::new(t) as Box<dyn Scene>,
                Err(back) => {
                    (link, pending) = *back;
                    if Instant::now() > until {
                        println!("(setup problem) no deal arrived");
                        std::process::exit(2);
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
    })
}

// ----------------------------------------------------------------------
// Proofs
// ----------------------------------------------------------------------

/// Only the host deals. A guest has no DEAL, and no key or button sends one.
fn guest_cant_deal_from_the_lobby() {
    let fake = Fake::start();
    let g = fake.clone();
    in_lobby(
        &fake,
        seated(Some(1), false),
        vec![
            nav(Nav::Menu),
            nav(Nav::Confirm),
            Step::Wait(0.4),
            act(move |_| {
                if g.got().iter().any(|m| matches!(m, ClientMsg::Start)) {
                    fail("a guest sent Start from the lobby");
                }
            }),
        ],
    );
}

fn host_can_deal_from_the_lobby() {
    let fake = Fake::start();
    let g = fake.clone();
    in_lobby(
        &fake,
        seated(Some(0), false),
        vec![
            nav(Nav::Confirm),
            Step::Wait(0.4),
            act(move |_| {
                if !g.got().iter().any(|m| matches!(m, ClientMsg::Start)) {
                    fail("the focus should start on DEAL, and pressing it should send Start");
                }
            }),
        ],
    );
}

fn shot_guest_lobby() {
    let fake = Fake::start();
    in_lobby(&fake, seated(Some(1), false), vec![Step::Wait(1000.0)]);
}

fn shot_guest_lobby_mid_game() {
    let fake = Fake::start();
    in_lobby(&fake, seated(Some(1), true), vec![Step::Wait(1000.0)]);
}

/// A game worth keeping is in the save slot, and this shows the question NEW GAME asks.
fn a_saved_game() {
    let mut old = Match::new(10, true, 111);
    old.scores = [8, 9];
    savegame::write(&old);
    assert!(savegame::load().is_some_and(|m| m.id == 111));
}

fn shot_new_game_dialog() {
    a_saved_game();
    script(|_| Box::new(Title::new()), vec![nav(Nav::Down), nav(Nav::Confirm), Step::Wait(1000.0)]);
}

/// Saying yes to the question really does start a new game.
fn confirmed_new_game_starts_fresh() {
    a_saved_game();
    script(
        |_| Box::new(Title::new()),
        vec![
            nav(Nav::Down), // CONTINUE -> NEW GAME
            nav(Nav::Confirm),
            nav(Nav::Right), // KEEP MY GAME -> NEW GAME
            nav(Nav::Confirm),
            Step::Wait(0.2),
            nav(Nav::Menu),
            nav(Nav::Up), // RESUME wraps around to SAVE & QUIT
            nav(Nav::Confirm),
            Step::Wait(0.1),
            act(|_| {
                if savegame::load().is_none_or(|m| m.id == 111) {
                    fail("NEW GAME, then yes, then SAVE & QUIT should have saved the new game");
                }
            }),
        ],
    );
}

/// REJOIN TABLE is offered. You press HOST A TABLE, then change your mind and press CANCEL
/// before it connects. You never left the first table, so REJOIN should still be there.
fn cancel_connecting_keeps_remembered_seat() {
    let _fake = Fake::start();
    remember_seat("KQJTAB", "my-token");
    script(
        |shared| Box::new(Lobby::new(shared)),
        vec![
            nav(Nav::Down), // REJOIN -> HOST A TABLE
            nav(Nav::Confirm),
            Step::Wait(0.05),
            nav(Nav::Back), // CANCEL
            act(|_| match seat_memory::code() {
                Some(c) if c == "KQJTAB" => {}
                _ => fail("CANCEL on a different table erased the remembered seat at KQJTAB: REJOIN is gone and the token is lost"),
            }),
        ],
    );
}

/// You press REJOIN TABLE and then CANCEL while it's connecting. Cancelling a reconnect
/// shouldn't give the seat up: nothing has happened yet from the player's point of view.
fn cancelling_a_rejoin_keeps_the_seat() {
    let fake = Fake::start();
    let g = fake.clone();
    remember_seat("KQJTAB", "my-token");
    script(
        |shared| Box::new(Lobby::new(shared)),
        vec![
            nav(Nav::Confirm), // REJOIN
            Step::Wait(0.05),
            nav(Nav::Back), // CANCEL
            act(|_| {
                if seat_memory::code().is_none() {
                    fail("CANCEL during REJOIN forgot the token, so the seat is given up for good");
                }
            }),
            Step::Wait(0.5),
            act(move |_| {
                if g.got().iter().any(|m| matches!(m, ClientMsg::Leave)) {
                    fail("CANCEL during REJOIN told the room to give the seat away");
                }
            }),
        ],
    );
}

/// The host has the pause menu open with the focus on LET CY IN. CY gives up waiting, so the
/// menu rebuilds and the same index is now MAKE PUBLIC. Pressing A must not open the table up.
fn pause_focus_shift_doesnt_make_table_public() {
    let fake = Fake::start();
    let table = online_table(&fake, vec![welcome(), seats(Some(0), SeatKind::Bot), knocks(&[7]), deal()]);
    let f = fake.clone();
    let g = fake.clone();
    script(
        table,
        vec![
            Step::Wait(0.2),
            nav(Nav::Menu),
            nav(Nav::Down), // RESUME -> LET CY IN
            act(move |_| f.send(knocks(&[]))),
            Step::Wait(0.4),
            nav(Nav::Confirm),
            Step::Wait(0.4),
            act(move |_| {
                if g.got().iter().any(|m| matches!(m, ClientMsg::SetPublic { public: true })) {
                    fail("the focus stayed on the same index when the menu rebuilt: pressing A on LET CY IN sent SetPublic{true}");
                }
            }),
        ],
    );
}

/// The host has OPTIONS focused when a request to join comes in. The menu grows, and the same
/// index is now SAY NO. Pressing A must not turn a friend away.
fn pause_focus_shift_doesnt_refuse_a_friend() {
    let fake = Fake::start();
    let table = online_table(&fake, vec![welcome(), seats(Some(0), SeatKind::Bot), deal()]);
    let f = fake.clone();
    let g = fake.clone();
    script(
        table,
        vec![
            Step::Wait(0.2),
            nav(Nav::Menu),
            nav(Nav::Down), // RESUME -> MAKE PUBLIC
            nav(Nav::Down), // -> OPTIONS
            act(move |_| f.send(knocks(&[7]))),
            Step::Wait(0.4),
            nav(Nav::Confirm),
            Step::Wait(0.4),
            act(move |_| {
                if g.got().iter().any(|m| matches!(m, ClientMsg::Answer { id: 7, accept: false })) {
                    fail("the focus stayed on the same index when the menu rebuilt: pressing A on OPTIONS sent Answer{accept:false}");
                }
            }),
        ],
    );
}

/// A seat list arriving just after the deal, in the same batch of messages (someone joined as the
/// host dealt). It should still count. Here it makes us the host, so MAKE PUBLIC should be in the menu.
fn seat_update_right_after_the_deal_is_applied() {
    let fake = Fake::start();
    let table = online_table(&fake, vec![welcome(), seats(None, SeatKind::Bot), deal(), seats(Some(0), SeatKind::Human)]);
    let g = fake.clone();
    script(
        table,
        vec![
            Step::Wait(0.5),
            nav(Nav::Menu),
            nav(Nav::Down), // RESUME -> MAKE PUBLIC if we're the host
            nav(Nav::Confirm),
            Step::Wait(0.4),
            act(move |_| {
                if !g.got().iter().any(|m| matches!(m, ClientMsg::SetPublic { .. })) {
                    fail("Table::join left the Seats message after the deal in `pending`, where next_from_room drops it: the table still thinks the old seats are right");
                }
            }),
        ],
    );
}

/// Closing the game from the seated lobby sends Leave, like closing it from the table does. The
/// table also forgets the seat, so the next launch doesn't offer REJOIN for a seat we gave up.
fn closing_from_the_lobby_forgets_the_seat() {
    let fake = Fake::start();
    let mut seated = seats(Some(0), SeatKind::Bot);
    if let ServerMsg::Seats(s) = &mut seated {
        s.playing = false;
    }
    let f = fake.clone();
    let g = fake.clone();
    script(
        |shared| Box::new(Lobby::host(shared)),
        vec![
            act(move |_| {
                f.send(welcome());
                f.send(seated);
            }),
            Step::Until("seat remembered", 5.0, Box::new(|_| seat_memory::code().is_some())),
            Step::Wait(0.2),
            act(|h| {
                let mut scene = h.scene.take().unwrap();
                scene.suspend(&mut h.shared);
                h.scene = Some(scene);
            }),
            Step::Wait(0.3),
            act(move |_| {
                let left = g.got().iter().any(|m| matches!(m, ClientMsg::Leave));
                if left && seat_memory::code().is_some() {
                    fail("Lobby::suspend sent Leave but kept online_seat.txt: the next launch offers REJOIN TABLE for a seat already given up (Table::suspend does forget it)");
                }
            }),
        ],
    );
}

/// title.rs: "A new game leaves the save alone until it's saved over (or this game finishes), so a
/// stray click can't lose it". A stray click on NEW GAME, then the only way back to the title:
/// MENU, SAVE & QUIT. The game you meant to keep should survive that.
fn new_game_keeps_the_earlier_save() {
    a_saved_game();
    script(
        |_| Box::new(Title::new()),
        vec![
            nav(Nav::Down), // CONTINUE -> NEW GAME
            nav(Nav::Confirm),
            nav(Nav::Confirm), // A double press lands on KEEP MY GAME.
            Step::Wait(0.2),
            act(|_| match savegame::load() {
                Some(m) if m.id == 111 => {}
                Some(m) => fail(format!(
                    "the saved game at 8-9 was overwritten by a {}-{} game that was only opened by mistake, without asking",
                    m.scores[0], m.scores[1]
                )),
                None => fail("the saved game is gone"),
            }),
        ],
    );
}

/// settings.rs: "Your name at online tables (empty: the Steam name, else PLAYER)". Clearing the
/// name field and pressing DONE should mean "no name of my own" and bring the store's name back.
fn clearing_your_name_goes_back_to_the_default() {
    script(
        |shared| Box::new(Lobby::new(shared)),
        vec![
            act(|h| h.shared.settings.name = "BOB".into()),
            nav(Nav::Down), // HOST -> JOIN
            nav(Nav::Down), // -> CHANGE NAME
            nav(Nav::Confirm),
            nav(Nav::Back), // Erases one letter at a time.
            nav(Nav::Back),
            nav(Nav::Back),
            nav(Nav::Confirm), // The focus starts on DONE.
            act(|h| {
                if !h.shared.settings.name.is_empty() {
                    fail(format!("an empty name is saved as the literal {:?}, so `player_name` never falls back to the Steam name again", h.shared.settings.name));
                }
            }),
        ],
    );
}

fn wait_for_welcome(c: &mut Connection) -> Option<(String, String)> {
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        while let Some(m) = c.poll() {
            if let ServerMsg::Welcome { room, token, .. } = m {
                return Some((room, token));
            }
        }
        if let Status::Failed(why) = c.status() {
            println!("(setup problem) connection failed: {why}");
            std::process::exit(2);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

/// Two copies of the game on one computer (two people sharing a PC, or anyone testing): the second
/// one types the first one's code. It should get its own seat, and both should settle down.
fn second_window_doesnt_steal_the_seat() {
    real_server_or_skip();
    let mut first = Connection::open(Join::Create, "FIRST", "?target=10&stick=1");
    let (code, token) = wait_for_welcome(&mut first).expect("the first player gets a seat");
    assert_eq!(seat_memory::code().as_deref(), Some(code.as_str()));
    let second = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--proof", "helper_second_window"])
        .env("EUCHRE_PROOF_CODE", &code)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // Meanwhile, see whether anything takes the seat from us.
    let mut takeovers = 0;
    let until = Instant::now() + Duration::from_secs(14);
    while Instant::now() < until {
        while let Some(m) = first.poll() {
            if matches!(m, ServerMsg::Welcome { .. }) {
                takeovers += 1;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = second.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let Some(line) = text.lines().find(|l| l.starts_with("SECOND ")) else {
        println!("(setup problem) the second window said: {text}");
        std::process::exit(2);
    };
    let mut words = line.split_whitespace().skip(1);
    let (second_token, second_takeovers) = (words.next().unwrap(), words.next().unwrap());
    if second_token == token {
        fail("the second window reused the first window's seat token");
    }
    if takeovers != 0 || second_takeovers != "0" {
        fail(format!("the windows took seats from each other: {takeovers} and {second_takeovers} times in 14 s"));
    }
    if first.status() != Status::Connected {
        fail(format!("the first window ended up {:?}", first.status()));
    }
}

/// The other copy of the game, in a process of its own like the real thing: `seat_memory`
/// tells copies apart by a file lock, and a process doesn't conflict with itself.
fn helper_second_window() {
    use_server(&real_server());
    let code = std::env::var("EUCHRE_PROOF_CODE").unwrap();
    if seat_memory::code().is_some() {
        fail("the second window was offered the first window's REJOIN TABLE");
    }
    let mut c = Connection::open(Join::Code(code), "SECOND", "");
    let (_, token) = wait_for_welcome(&mut c).expect("the second player gets a seat");
    let mut takeovers = 0;
    let until = Instant::now() + Duration::from_secs(12);
    while Instant::now() < until {
        while let Some(m) = c.poll() {
            if matches!(m, ServerMsg::Welcome { .. }) {
                takeovers += 1;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    println!("SECOND {token} {takeovers}");
    drop(c);
}

/// The server closes a connection with 4001 "Replaced by a newer connection" when the same token
/// connects again. The old connection should stop, not reconnect and take the seat back.
fn a_replaced_connection_stays_closed() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    use_server(&format!("ws://{}", listener.local_addr().unwrap()));
    let connections = Arc::new(Mutex::new(0));
    let count = connections.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            *count.lock().unwrap() += 1;
            let mut ws = tungstenite::accept(stream).unwrap();
            let _ = ws.read();
            let frame = CloseFrame { code: CloseCode::from(4001), reason: "Replaced by a newer connection".into() };
            let _ = ws.close(Some(frame));
            let _ = ws.flush();
        }
    });
    let mut c = Connection::open(Join::Code("KQJTAB".into()), "OLD", "");
    let until = Instant::now() + Duration::from_secs(6);
    while Instant::now() < until {
        let _ = c.poll();
        std::thread::sleep(Duration::from_millis(50));
    }
    let n = *connections.lock().unwrap();
    if n > 1 {
        fail(format!("after close code 4001 the client connected again {} more time(s) in 6 s, so it takes the seat back from the newer connection", n - 1));
    }
}

/// LEAVE TABLE while the connection is down. The player forgets the token at once, so only the room
/// can free the seat. The client should get the Leave out when it can, not throw it away: otherwise
/// the table waits on a ghost for 10 minutes, 15 s on each of its turns.
fn leaving_while_reconnecting_still_leaves() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    use_server(&format!("ws://{}", listener.local_addr().unwrap()));
    let left = Arc::new(Mutex::new(false));
    let saw = left.clone();
    std::thread::spawn(move || {
        let mut first = true;
        for stream in listener.incoming().flatten() {
            let Ok(mut ws) = tungstenite::accept(stream) else { continue };
            if first {
                // Seats us, then the network "drops".
                first = false;
                let _ = ws.read();
                let _ = ws.send(Message::text(serde_json::to_string(&welcome()).unwrap()));
                std::thread::sleep(Duration::from_millis(300));
                drop(ws);
                continue;
            }
            let saw = saw.clone();
            std::thread::spawn(move || {
                while let Ok(m) = ws.read() {
                    if let Message::Text(t) = m
                        && matches!(serde_json::from_str::<ClientMsg>(t.as_str()), Ok(ClientMsg::Leave))
                    {
                        *saw.lock().unwrap() = true;
                    }
                }
            });
        }
    });
    let mut c = Connection::open(Join::Code("KQJTAB".into()), "ME", "");
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until {
        let _ = c.poll();
        if c.status() == Status::Reconnecting {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(c.status(), Status::Reconnecting, "(setup) the drop should show");
    // What Table::quit_to_title does.
    c.send(ClientMsg::Leave);
    drop(c);
    std::thread::sleep(Duration::from_secs(6));
    if !*left.lock().unwrap() {
        fail("the client threw the Leave away and shut down, though the server was reachable again: the seat stays a ghost for 10 minutes");
    }
}

/// The server restarting (a deploy) cuts every WebSocket. A player who doesn't come back should
/// show as away and lose the seat after 10 minutes, as with any other drop.
///
/// Run by hand, since the room server has to be restarted in the middle: start `wrangler dev` with
/// `--persist-to DIR`, run the test exe with `--proof server_restart_marks_absent_players_away` and
/// EUCHRE_PROOF_RESTART=1, and when it prints RESTART NOW, kill wrangler and start it again.
fn server_restart_marks_absent_players_away() {
    if std::env::var("EUCHRE_PROOF_RESTART").is_err() {
        skip("needs a server restart in the middle: see the comment on this proof");
    }
    use_server(&real_server());
    let mut ann = Connection::open(Join::Create, "ANN", "?target=10&stick=1");
    let (code, _) = wait_for_welcome(&mut ann).expect("Ann sits down");
    // Bo is a plain socket: when the server goes away, Bo doesn't come back.
    let (mut bo, _) = tungstenite::connect(format!("{}/join/{code}", real_server())).expect("Bo connects");
    let hello = ClientMsg::Hello { version: euchre::protocol::PROTOCOL_VERSION, name: "BO".into(), token: None };
    bo.send(Message::text(serde_json::to_string(&hello).unwrap())).unwrap();
    loop {
        if let Message::Text(t) = bo.read().unwrap()
            && matches!(serde_json::from_str::<ServerMsg>(t.as_str()), Ok(ServerMsg::Welcome { .. }))
        {
            break;
        }
    }
    println!("RESTART NOW");
    use std::io::Write;
    std::io::stdout().flush().unwrap();
    // Ann's game reconnects by itself and is told who's where. Bo never returned.
    let until = Instant::now() + Duration::from_secs(240);
    let mut lost_connection = false;
    let mut latest = None;
    let mut settled = until;
    while Instant::now() < until.min(settled) {
        if c_lost(&ann) {
            lost_connection = true;
        }
        while let Some(m) = ann.poll() {
            if let ServerMsg::Seats(s) = m
                && lost_connection
                && ann.status() == Status::Connected
            {
                latest = Some(s);
                settled = Instant::now() + Duration::from_secs(2);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if let Some(s) = latest {
        // Relative to Ann: Bo, who sat second, is across the table.
        let kind = s.seats[2].kind;
        println!("after the restart, Ann sees Bo as {kind:?}, and the host is {:?}", s.host);
        if kind != SeatKind::Away {
            fail(format!(
                "Bo's connection died in the restart but the room still has them as {kind:?} (connected): 60 s turns instead of 15, never released to a bot"
            ));
        }
        if s.seats[0].kind != SeatKind::Human || s.host != Some(0) {
            fail("Ann came back and should still be the host");
        }
        return;
    }
    println!("(setup problem) the server never came back");
    std::process::exit(2);
}

fn c_lost(c: &Connection) -> bool {
    c.status() == Status::Reconnecting
}

/// A connection nobody says anything on: the client pings every 20 s and gives up after 60 s
/// without hearing anything. If the server's answers aren't seen, an idle table reconnects
/// (a full gather-and-deal re-layout, "Welcome back!") every minute.
fn idle_connection_stays_up() {
    real_server_or_skip();
    let mut c = Connection::open(Join::Create, "IDLE", "?target=10&stick=1");
    wait_for_welcome(&mut c).expect("sits down");
    let until = Instant::now() + Duration::from_secs(80);
    let mut welcomes = 0;
    while Instant::now() < until {
        while let Some(m) = c.poll() {
            if matches!(m, ServerMsg::Welcome { .. }) {
                welcomes += 1;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if welcomes > 0 || c.status() != Status::Connected {
        fail(format!("an idle connection reconnected {welcomes} time(s) in 80 s; status {:?}", c.status()));
    }
}

/// A shipped Windows game must not open a console window behind itself. That's the PE header's
/// subsystem field: 2 is a window, 3 is a console.
fn release_exe_has_no_console_window() {
    // This exe is in target/debug/deps/, or wherever CARGO_TARGET_DIR says.
    let exe = std::env::current_exe().unwrap();
    let Some(bytes) = exe.ancestors().find_map(|dir| std::fs::read(dir.join("release/cabin-euchre.exe")).ok()) else {
        skip("no release build found: run `cargo build --release`");
    };
    let pe = u32::from_le_bytes(bytes[0x3c..0x40].try_into().unwrap()) as usize;
    assert_eq!(&bytes[pe..pe + 4], b"PE\0\0");
    let subsystem = u16::from_le_bytes(bytes[pe + 24 + 68..pe + 24 + 70].try_into().unwrap());
    if subsystem != 2 {
        fail(format!(
            "the release .exe has PE subsystem {subsystem} (console), so Windows opens a console window behind the game. main.rs has no #![windows_subsystem = \"windows\"]"
        ));
    }
}
