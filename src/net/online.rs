//! The WebSocket connection to an online room, on a background thread so the
//! game loop never waits on the network.
//!
//! The thread connects (TLS for `wss://`), sends `Hello`, then forwards
//! messages both ways over channels. If the connection drops it keeps
//! reconnecting with the seat token it was given, and the room answers with
//! the whole hand (`Sync`), so the table simply lays it back out.
//!
//! The server address comes from `EUCHRE_SERVER` at run time, else at build
//! time, else [`DEFAULT_SERVER`]: the live server in a release build, and
//! `wrangler dev` on this machine otherwise. `build.rs` refuses a release build
//! aimed anywhere a player couldn't reach.

use std::collections::VecDeque;
use std::io::ErrorKind;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use crate::euchre::protocol::{ClientMsg, ErrorCode, PROTOCOL_VERSION, ServerMsg};
use crate::net::seat_memory;

/// Where `wrangler dev` serves the room server (see `server/README.md`).
const LOCAL_SERVER: &str = "ws://127.0.0.1:8787";
/// The deployed room server.
const LIVE_SERVER: &str = "wss://euchre.gccody2010.workers.dev";
/// Where a game connects when nothing says otherwise.
pub const DEFAULT_SERVER: &str = if cfg!(debug_assertions) { LOCAL_SERVER } else { LIVE_SERVER };

/// The room server's base URL (`ws://` or `wss://`, no trailing slash).
pub fn server_url() -> String {
    std::env::var("EUCHRE_SERVER")
        .ok()
        .or_else(|| crate::device::var("EUCHRE_SERVER"))
        .or(option_env!("EUCHRE_SERVER").map(str::to_string))
        .unwrap_or_else(|| DEFAULT_SERVER.to_string())
        .trim_end_matches('/')
        .to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Connecting,
    Connected,
    /// The connection dropped; trying again.
    Reconnecting,
    /// Gave up, with a reason for the player.
    Failed(String),
}

/// Which room to connect to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Join {
    /// A new room with a fresh code.
    Create,
    /// The room with this code (the code is the invitation).
    Code(String),
    /// Ask to join the room with this handle (a Steam friend's "Join Game").
    Knock(String),
}

enum Command {
    Send(String),
    Close,
}

enum Event {
    Status(Status),
    Msg(ServerMsg),
}

pub struct Connection {
    commands: Sender<Command>,
    events: Receiver<Event>,
    status: Status,
    room: Option<String>,
    handle: Option<String>,
    inbox: VecDeque<ServerMsg>,
    thread: JoinHandle<()>,
}

impl Connection {
    /// Starts connecting in the background. `create_query` (such as
    /// `?target=10&stick=1`) sets up a new room's game.
    pub fn open(join: Join, name: &str, create_query: &str) -> Connection {
        let (commands, command_rx) = mpsc::channel();
        let (event_tx, events) = mpsc::channel();
        let name = name.to_string();
        let server = server_url();
        let query = create_query.to_string();
        // Back to a table we hold a seat at, even after the game was killed.
        let token = match &join {
            Join::Code(code) => seat_memory::token_for(code),
            _ => None,
        };
        let thread = std::thread::Builder::new()
            .name("euchre-net".into())
            .spawn(move || run(server, join, token, query, name, command_rx, event_tx))
            .expect("spawn the network thread");
        Connection { commands, events, status: Status::Connecting, room: None, handle: None, inbox: VecDeque::new(), thread }
    }

    /// Closes the connection and waits a moment for the thread to finish, so
    /// what was queued (a `Leave`) gets out before the game exits.
    pub fn finish(&mut self, within: Duration) {
        let _ = self.commands.send(Command::Close);
        let until = Instant::now() + within;
        while !self.thread.is_finished() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn send(&mut self, msg: ClientMsg) {
        if let Ok(text) = serde_json::to_string(&msg) {
            let _ = self.commands.send(Command::Send(text));
        }
    }

    fn pump(&mut self) {
        loop {
            match self.events.try_recv() {
                Ok(Event::Status(s)) => self.status = s,
                Ok(Event::Msg(msg)) => {
                    if let ServerMsg::Welcome { room, handle, token } = &msg {
                        self.room = Some(room.clone());
                        self.handle = Some(handle.clone());
                        seat_memory::remember(room, token);
                    }
                    self.inbox.push_back(msg);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !matches!(self.status, Status::Failed(_)) {
                        self.status = Status::Failed("Lost the connection.".into());
                    }
                    break;
                }
            }
        }
    }

    pub fn poll(&mut self) -> Option<ServerMsg> {
        self.pump();
        self.inbox.pop_front()
    }

    pub fn status(&self) -> Status {
        self.status.clone()
    }

    pub fn room_code(&self) -> Option<String> {
        self.room.clone()
    }

    /// What friends may see (never the code): see `euchre::protocol`.
    pub fn room_handle(&self) -> Option<String> {
        self.handle.clone()
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Close);
    }
}

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

/// Why a connection ended.
enum Ended {
    /// The game closed the connection.
    Closed,
    /// The room refused us for good (no such room, full, old version).
    Refused(String),
    /// The network let us down: try again.
    Dropped,
}

fn run(
    server: String,
    mut join: Join,
    mut token: Option<String>,
    query: String,
    name: String,
    commands: Receiver<Command>,
    events: Sender<Event>,
) {
    let mut failures = 0u32;
    let mut was_connected = false;
    loop {
        let url = match &join {
            Join::Create => format!("{server}/create{query}"),
            Join::Code(code) => format!("{server}/join/{code}"),
            Join::Knock(handle) => format!("{server}/knock/{handle}"),
        };
        match connect(&url) {
            Ok(mut socket) => {
                let hello = ClientMsg::Hello { version: PROTOCOL_VERSION, name: name.clone(), token: token.clone() };
                let hello = serde_json::to_string(&hello).expect("hello serializes");
                if socket.send(Message::text(hello)).is_ok() {
                    let _ = events.send(Event::Status(Status::Connected));
                    was_connected = true;
                    failures = 0;
                    match serve(&mut socket, &commands, &events, &mut join, &mut token) {
                        Ended::Closed => {
                            let _ = socket.close(None);
                            let _ = socket.flush();
                            return;
                        }
                        Ended::Refused(why) => {
                            let _ = events.send(Event::Status(Status::Failed(why)));
                            return;
                        }
                        Ended::Dropped => {}
                    }
                }
            }
            Err(e) => {
                // Not the whole address: it holds the table's code.
                crate::assets::log_warn(&format!("could not reach {server}: {e}"));
            }
        }
        failures += 1;
        if !was_connected && failures >= 3 {
            let _ = events.send(Event::Status(Status::Failed("Couldn't reach the server.".into())));
            return;
        }
        let _ = events.send(Event::Status(Status::Reconnecting));
        // Back off, but keep listening for the game closing us.
        let wait = Duration::from_millis((500 * 2u64.pow(failures.min(4))).min(8_000));
        let until = Instant::now() + wait;
        while Instant::now() < until {
            match commands.recv_timeout(Duration::from_millis(100)) {
                Ok(Command::Close) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                // Moves made while offline are dropped: the room resyncs us on reconnect.
                Ok(Command::Send(_)) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
}

fn connect(url: &str) -> Result<Socket, String> {
    let rest = url
        .strip_prefix("wss://")
        .map(|r| (r, 443))
        .or_else(|| url.strip_prefix("ws://").map(|r| (r, 80)));
    let Some((rest, default_port)) = rest else { return Err(format!("not a ws:// or wss:// address: {url}")) };
    let host_port = rest.split('/').next().unwrap_or(rest);
    let addr = if host_port.contains(':') { host_port.to_string() } else { format!("{host_port}:{default_port}") };
    let addrs: Vec<_> = addr.to_socket_addrs().map_err(|e| e.to_string())?.collect();
    let mut last = String::from("no address");
    for a in addrs {
        match TcpStream::connect_timeout(&a, Duration::from_secs(5)) {
            Ok(stream) => {
                stream.set_read_timeout(Some(Duration::from_secs(8))).map_err(|e| e.to_string())?;
                let _ = stream.set_nodelay(true);
                let (socket, _) = tungstenite::client_tls(url, stream).map_err(|e| e.to_string())?;
                set_read_timeout(&socket, Duration::from_millis(30));
                return Ok(socket);
            }
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}

fn set_read_timeout(socket: &Socket, t: Duration) {
    let tcp = match socket.get_ref() {
        MaybeTlsStream::Plain(s) => s,
        MaybeTlsStream::Rustls(s) => s.get_ref(),
        _ => return,
    };
    let _ = tcp.set_read_timeout(Some(t));
}

fn serve(
    socket: &mut Socket,
    commands: &Receiver<Command>,
    events: &Sender<Event>,
    join: &mut Join,
    token: &mut Option<String>,
) -> Ended {
    let mut last_ping = Instant::now();
    let mut last_heard = Instant::now();
    loop {
        // Outgoing first, so a move goes out as soon as it's made.
        loop {
            match commands.try_recv() {
                Ok(Command::Send(text)) => {
                    if socket.send(Message::text(text)).is_err() {
                        return Ended::Dropped;
                    }
                }
                Ok(Command::Close) | Err(TryRecvError::Disconnected) => return Ended::Closed,
                Err(TryRecvError::Empty) => break,
            }
        }
        // Keeps phone networks and home routers from forgetting an idle connection.
        if last_ping.elapsed() > Duration::from_secs(20) {
            last_ping = Instant::now();
            if socket.send(Message::Ping(Vec::new().into())).is_err() {
                return Ended::Dropped;
            }
        }
        if last_heard.elapsed() > Duration::from_secs(60) {
            return Ended::Dropped;
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                last_heard = Instant::now();
                let msg: ServerMsg = match serde_json::from_str(text.as_str()) {
                    Ok(m) => m,
                    Err(e) => {
                        crate::assets::log_warn(&format!("unreadable message from the server: {e}"));
                        continue;
                    }
                };
                match &msg {
                    ServerMsg::Welcome { room, token: t, .. } => {
                        // Reconnect to this room, as this player (with the code: we're in now).
                        *join = Join::Code(room.clone());
                        *token = Some(t.clone());
                    }
                    ServerMsg::Error {
                        code: code @ (ErrorCode::NoSuchRoom | ErrorCode::RoomFull | ErrorCode::Version | ErrorCode::Declined),
                        ..
                    } => {
                        if let (Join::Code(table), false) = (&*join, *code == ErrorCode::Version) {
                            seat_memory::forget_table(table);
                        }
                        return Ended::Refused(code.describe().into());
                    }
                    _ => {}
                }
                if events.send(Event::Msg(msg)).is_err() {
                    return Ended::Closed;
                }
            }
            Ok(Message::Close(_)) => return Ended::Dropped,
            Ok(_) => last_heard = Instant::now(),
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return Ended::Dropped,
        }
    }
}
