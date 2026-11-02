//! End-to-end check of a running room server: seats a few AI players in a
//! new room over real WebSockets and plays a whole game, each player keeping
//! only a [`Mirror`] of what it's told (exactly like the game does). One
//! player drops and reconnects mid-game, and a bogus room code is tried.
//! With 3+ players the last one comes by the room's handle (like a Steam
//! friend's "Join Game") and the host lets them in; a stranger knocking the
//! same way is turned away. It also checks that a player who changes seats
//! stays connected to the room, that a connection left open after leaving can't
//! pick up the next player's seat, and that the host's job moves on when the
//! host's connection drops.
//!
//! ```sh
//! cd server && npx wrangler dev        # in one terminal
//! cargo run -p euchre-core --example netcheck -- ws://127.0.0.1:8787 3
//! ```

use std::net::TcpStream;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use euchre_core::ai;
use euchre_core::mirror::{ME, Mirror};
use euchre_core::protocol::{ClientMsg, ErrorCode, PROTOCOL_VERSION, ServerMsg};
use euchre_core::round::Phase;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

fn connect(url: &str) -> Socket {
    let (socket, _) = tungstenite::connect(url).unwrap_or_else(|e| panic!("connect {url}: {e}"));
    if let MaybeTlsStream::Plain(s) = socket.get_ref() {
        s.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
    }
    socket
}

fn send(socket: &mut Socket, msg: &ClientMsg) {
    socket.send(Message::text(serde_json::to_string(msg).unwrap())).unwrap();
}

fn recv(socket: &mut Socket) -> Option<ServerMsg> {
    match socket.read() {
        Ok(Message::Text(t)) => Some(serde_json::from_str(t.as_str()).unwrap_or_else(|e| panic!("bad message {t}: {e}"))),
        Ok(_) => None,
        Err(tungstenite::Error::Io(e))
            if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) =>
        {
            None
        }
        Err(e) => panic!("socket: {e}"),
    }
}

/// How a player arrives.
enum Via {
    Create,
    /// With the code: sits straight down.
    Code(String),
    /// With the handle: asks the host first.
    Knock(String),
}

/// A stranger asks to join by handle, and should be turned away.
fn knock_and_get_declined(server: &str, handle: &str) {
    let mut socket = connect(&format!("{server}/knock/{handle}"));
    send(&mut socket, &ClientMsg::Hello { version: PROTOCOL_VERSION, name: "stranger".into(), token: None });
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut waited = false;
    while Instant::now() < deadline {
        match recv(&mut socket) {
            Some(ServerMsg::Waiting) => waited = true,
            Some(ServerMsg::Error { code: ErrorCode::Declined, .. }) => {
                assert!(waited, "told to wait before being turned away");
                println!("stranger: turned away: ok");
                return;
            }
            Some(other) => panic!("stranger got {other:?}"),
            None => {}
        }
    }
    panic!("stranger never got an answer");
}

/// Reads until `wanted` matches, for at most `secs` seconds.
fn wait_for(socket: &mut Socket, secs: u64, wanted: impl Fn(&ServerMsg) -> bool) -> Option<ServerMsg> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        if let Some(msg) = recv(socket).filter(|m| wanted(m)) {
            return Some(msg);
        }
    }
    None
}

/// Says hello on a new connection. Returns the socket, the room code and the handle.
fn arrive(url: &str, name: &str) -> (Socket, String, String) {
    let mut socket = connect(url);
    send(&mut socket, &ClientMsg::Hello { version: PROTOCOL_VERSION, name: name.into(), token: None });
    match wait_for(&mut socket, 10, |m| matches!(m, ServerMsg::Welcome { .. })) {
        Some(ServerMsg::Welcome { room, handle, .. }) => (socket, room, handle),
        _ => panic!("{name}: no welcome"),
    }
}

/// SIT HERE: the connection follows the player to the new seat.
fn changing_seats_keeps_the_connection(server: &str) {
    let (mut host, _, _) = arrive(&format!("{server}/create?target=5"), "host");
    send(&mut host, &ClientMsg::Sit { seat: 1 });
    assert!(wait_for(&mut host, 10, |m| matches!(m, ServerMsg::Seats(_))).is_some(), "no seats after moving");
    send(&mut host, &ClientMsg::Start);
    assert!(wait_for(&mut host, 10, |m| matches!(m, ServerMsg::Deal(_))).is_some(), "no deal after moving");
    println!("changing seats keeps the connection: ok");
}

/// A connection kept open after `Leave` holds nothing: the next player in that
/// seat keeps their token and their hand to themselves.
fn leaving_gives_up_the_seat(server: &str) {
    let (mut host, code, handle) = arrive(&format!("{server}/create?target=5"), "host");
    let (mut leaver, _, _) = arrive(&format!("{server}/join/{code}"), "leaver");
    send(&mut leaver, &ClientMsg::Leave);
    thread::sleep(Duration::from_millis(500));

    let mut next = connect(&format!("{server}/knock/{handle}"));
    send(&mut next, &ClientMsg::Hello { version: PROTOCOL_VERSION, name: "next".into(), token: None });
    let asked = wait_for(&mut host, 10, |m| matches!(m, ServerMsg::Knocks { requests } if !requests.is_empty()));
    let Some(ServerMsg::Knocks { requests }) = asked else { panic!("the host wasn't asked") };
    send(&mut host, &ClientMsg::Answer { id: requests[0].id, accept: true });
    assert!(wait_for(&mut next, 10, |m| matches!(m, ServerMsg::Welcome { .. })).is_some(), "not let in");
    // Everything the seat is told now goes to them, not to the old connection.
    assert!(wait_for(&mut leaver, 2, |m| !matches!(m, ServerMsg::Seats(_))).is_none(), "the old connection heard about it");
    send(&mut leaver, &ClientMsg::Start);
    match wait_for(&mut leaver, 5, |m| matches!(m, ServerMsg::Error { .. })) {
        Some(ServerMsg::Error { code: ErrorCode::NotAllowed, .. }) => {}
        other => panic!("the old connection could still act: {other:?}"),
    }
    println!("leaving gives up the seat: ok");
}

/// The host's connection drops: the next person answers requests to join.
fn the_host_job_moves_on_a_drop(server: &str) {
    let (mut host, code, handle) = arrive(&format!("{server}/create?target=5"), "host");
    let (mut guest, _, _) = arrive(&format!("{server}/join/{code}"), "guest");
    let _ = host.close(None);
    let _ = host.flush();
    drop(host);
    thread::sleep(Duration::from_millis(800));
    let mut next = connect(&format!("{server}/knock/{handle}"));
    send(&mut next, &ClientMsg::Hello { version: PROTOCOL_VERSION, name: "next".into(), token: None });
    let asked = wait_for(&mut guest, 10, |m| matches!(m, ServerMsg::Knocks { requests } if !requests.is_empty()));
    assert!(asked.is_some(), "the request never reached the next person");
    println!("the host's job moves on a drop: ok");
}

/// Plays one seat until the game ends. Returns the final score (theirs, ours).
/// `start_with`: start the game once this many people are seated (the host only).
/// `code_tx`: where the host passes on the room code and handle.
fn play(
    server: String,
    via: Via,
    name: String,
    start_with: Option<usize>,
    drop_after: Option<usize>,
    code_tx: Option<mpsc::Sender<(String, String)>>,
) -> (String, [u32; 2]) {
    let url = match &via {
        Via::Code(code) => format!("{server}/join/{code}"),
        Via::Knock(handle) => format!("{server}/knock/{handle}"),
        Via::Create => format!("{server}/create?target=5"),
    };
    let mut socket = connect(&url);
    send(&mut socket, &ClientMsg::Hello { version: PROTOCOL_VERSION, name: name.clone(), token: None });
    let mut token = None;
    let mut code = String::new();
    let mut mirror: Option<Mirror> = None;
    let mut moves_made = 0;
    let mut dropped = false;
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut started = start_with.is_none();
    while Instant::now() < deadline {
        let Some(msg) = recv(&mut socket) else {
            // Nothing to read: our move?
            if let Some(m) = mirror.as_mut() {
                if m.round.phase == Phase::TrickDone {
                    m.advance();
                }
                if m.round.phase == Phase::HandOver {
                    m.score_hand();
                    if m.winner().is_some() {
                        return (code, m.scores);
                    }
                }
                if m.round.to_act() == Some(ME) {
                    let action = ai::choose(&m.round, ME);
                    m.act(action).expect("own move is legal");
                    send(&mut socket, &ClientMsg::Act { action });
                    moves_made += 1;
                    if drop_after == Some(moves_made) && !dropped {
                        // Vanish, then come back with the token: the room resends the hand.
                        dropped = true;
                        let _ = socket.close(None);
                        drop(socket);
                        thread::sleep(Duration::from_millis(500));
                        socket = connect(&format!("{server}/join/{code}"));
                        send(&mut socket, &ClientMsg::Hello { version: PROTOCOL_VERSION, name: name.clone(), token: token.clone() });
                        mirror = None;
                        println!("{name}: reconnected");
                    }
                }
            }
            continue;
        };
        match msg {
            ServerMsg::Welcome { room, token: t, handle } => {
                println!("{name}: seated in room {room}");
                if let Some(tx) = &code_tx {
                    let _ = tx.send((room.clone(), handle));
                }
                code = room;
                token = Some(t);
            }
            ServerMsg::Waiting => println!("{name}: waiting for the host"),
            ServerMsg::Knocks { requests } => {
                // The host lets the guests in and turns strangers away.
                for k in requests {
                    let accept = k.name.starts_with("GUEST");
                    println!("{name}: {} asks to join: {}", k.name, if accept { "let in" } else { "no" });
                    send(&mut socket, &ClientMsg::Answer { id: k.id, accept });
                }
            }
            ServerMsg::Seats(s) => {
                let names: Vec<_> = s.seats.iter().map(|x| format!("{}({:?})", x.name, x.kind)).collect();
                println!("{name}: seats {}", names.join(" "));
                let people = s.seats.iter().filter(|x| x.kind != euchre_core::protocol::SeatKind::Bot).count();
                if !started && people >= start_with.unwrap_or(1) {
                    started = true;
                    send(&mut socket, &ClientMsg::Start);
                }
            }
            ServerMsg::Deal(hand) => mirror = Some(Mirror::new(&hand).expect("deal")),
            ServerMsg::Sync { hand, moves } => {
                println!("{name}: sync with {} moves", moves.len());
                mirror = Some(Mirror::replay(&hand, &moves).expect("sync replays"));
            }
            ServerMsg::Acted(mv) => {
                let m = mirror.as_mut().expect("move before deal");
                if m.round.phase == Phase::TrickDone {
                    m.advance();
                }
                m.reveal(mv).unwrap_or_else(|e| panic!("{name}: {mv:?} doesn't fit: {e:?}"));
            }
            ServerMsg::Error { code, message } => panic!("{name}: refused {code:?}: {message}"),
        }
    }
    panic!("{name}: game did not finish in time");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let server = args.next().unwrap_or_else(|| "ws://127.0.0.1:8787".into());
    let players: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(3).clamp(1, 4);
    // With a room code (or handle), only join it (as `players` guests): e.g.
    // a table the game is hosting. A handle asks the host first.
    if let Some(code) = args.next() {
        let guests: Vec<_> = (0..players)
            .map(|i| {
                let server = server.clone();
                let via = match euchre_core::protocol::clean_handle(&code) {
                    Some(handle) => Via::Knock(handle),
                    None => Via::Code(code.clone()),
                };
                thread::spawn(move || play(server, via, format!("guest{i}"), None, None, None))
            })
            .collect();
        for g in guests {
            let (_, s) = g.join().expect("guest finished");
            println!("guest final score {s:?}");
        }
        println!("netcheck: ok");
        return;
    }

    // A code nobody has: refused politely, over the socket.
    let mut bogus = connect(&format!("{server}/join/ZZZZZZ"));
    send(&mut bogus, &ClientMsg::Hello { version: PROTOCOL_VERSION, name: "lost".into(), token: None });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline, "no answer for a bogus code");
        match bogus.read() {
            Ok(Message::Text(t)) => {
                let msg: ServerMsg = serde_json::from_str(t.as_str()).unwrap();
                assert!(matches!(msg, ServerMsg::Error { code: ErrorCode::NoSuchRoom, .. }), "{msg:?}");
                println!("bogus code refused: ok");
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    changing_seats_keeps_the_connection(&server);
    leaving_gives_up_the_seat(&server);
    the_host_job_moves_on_a_drop(&server);

    // The host creates the room; the others join with its code.
    let (code_tx, code_rx) = mpsc::channel::<(String, String)>();
    let host_server = server.clone();
    let host = thread::spawn(move || play(host_server, Via::Create, "host".into(), Some(players), None, Some(code_tx)));
    let (code, handle) = code_rx.recv().unwrap();
    println!("room {code}, handle {}...", &handle[..8.min(handle.len())]);
    knock_and_get_declined(&server, &handle);
    let mut guests = Vec::new();
    for i in 1..players {
        let server = server.clone();
        // With three or more, the last guest asks to join instead of using the code.
        let via = if players >= 3 && i == players - 1 { Via::Knock(handle.clone()) } else { Via::Code(code.clone()) };
        guests.push(thread::spawn(move || play(server, via, format!("guest{i}"), None, (i == 1).then_some(4), None)));
    }
    let (_, host_score) = host.join().expect("host finished");
    println!("host final score {host_score:?}");
    for g in guests {
        let (_, s) = g.join().expect("guest finished");
        println!("guest final score {s:?}");
    }
    println!("netcheck: ok");
}
