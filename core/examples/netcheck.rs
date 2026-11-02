//! End-to-end check of a running room server: seats a few AI players in a
//! new room over real WebSockets and plays a whole game, each player keeping
//! only a [`Mirror`] of what it's told (exactly like the game does). One
//! player drops and reconnects mid-game, and a bogus room code is tried.
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

/// Plays one seat until the game ends. Returns the final score (theirs, ours).
/// `start_with`: start the game once this many people are seated (the host only).
/// `code_tx`: where the host passes on the room code.
fn play(
    server: String,
    room: Option<String>,
    name: String,
    start_with: Option<usize>,
    drop_after: Option<usize>,
    code_tx: Option<mpsc::Sender<String>>,
) -> (String, [u32; 2]) {
    let url = match &room {
        Some(code) => format!("{server}/join/{code}"),
        None => format!("{server}/create?target=5"),
    };
    let mut socket = connect(&url);
    send(&mut socket, &ClientMsg::Hello { version: PROTOCOL_VERSION, name: name.clone(), token: None });
    let mut token = None;
    let mut code = room.clone().unwrap_or_default();
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
            ServerMsg::Welcome { room, token: t } => {
                println!("{name}: seated in room {room}");
                if let Some(tx) = &code_tx {
                    let _ = tx.send(room.clone());
                }
                code = room;
                token = Some(t);
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

    // A code nobody has: refused politely, over the socket.
    let mut bogus = connect(&format!("{server}/join/ZZZZZ"));
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

    // The host creates the room; the others join with its code.
    let (code_tx, code_rx) = mpsc::channel::<String>();
    let host_server = server.clone();
    let host = thread::spawn(move || play(host_server, None, "host".into(), Some(players), None, Some(code_tx)));
    let code = code_rx.recv().unwrap();
    println!("room {code}");
    let mut guests = Vec::new();
    for i in 1..players {
        let server = server.clone();
        let code = code.clone();
        guests.push(thread::spawn(move || play(server, Some(code), format!("guest{i}"), None, (i == 1).then_some(4), None)));
    }
    let (_, host_score) = host.join().expect("host finished");
    println!("host final score {host_score:?}");
    for g in guests {
        let (_, s) = g.join().expect("guest finished");
        println!("guest final score {s:?}");
    }
    println!("netcheck: ok");
}
