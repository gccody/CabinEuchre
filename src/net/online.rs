//! The WebSocket connection to an online room, on a background thread so the
//! game loop never waits on the network.
//!
//! The thread connects (TLS for `wss://`), sends `Hello`, then forwards
//! messages both ways over channels. If the connection drops it keeps
//! reconnecting with the seat token it was given, and the room answers with
//! the whole hand (`Sync`), so the table simply lays it back out.
//!
//! Between messages the thread sleeps. An online card game is mostly waiting for
//! someone else's move, and a thread that woke up to look for one would run more
//! often than the game draws. It wakes when the socket has something to read,
//! when the game has something to send ([`Waker`]), and every 20 s to ping.
//!
//! The server address comes from `EUCHRE_SERVER` at run time, else at build
//! time, else [`DEFAULT_SERVER`]: the live server in a release build, and
//! `wrangler dev` on this machine otherwise. `build.rs` refuses a release build
//! aimed anywhere a player couldn't reach.

use std::collections::VecDeque;
use std::io::ErrorKind;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use polling::{Event as Ready, Events, Poller};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use crate::euchre::protocol::{CLOSE_REPLACED, ClientMsg, ErrorCode, PROTOCOL_VERSION, ServerMsg};
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

/// How long a [`ClientMsg::Leave`] made with the connection down keeps trying to get
/// out. Without it the seat stays a ghost until the room gives up on it.
const LEAVE_TRIES_FOR: Duration = Duration::from_secs(30);

enum Command {
    Send(String),
    /// Unlike a move, this can't wait for a resync: the player is gone, and has
    /// forgotten the token, so only this tells the room to free the seat.
    Leave,
    Close,
}

enum Event {
    Status(Status),
    Msg(ServerMsg),
}

/// What the network thread sleeps on: the socket, and a nudge from the game when
/// there's a command for it. `None` if the OS wouldn't give us one, and then the
/// thread looks for commands every [`FALLBACK_POLL`] instead.
type Waker = Option<Arc<Poller>>;

/// The network thread's ends of what joins it to the game.
struct Wires {
    commands: Receiver<Command>,
    events: Sender<Event>,
    waker: Waker,
}

/// The socket's key in the poller. There's only ever the one.
const SOCKET: usize = 1;
/// How often to look for commands when there's no [`Waker`].
const FALLBACK_POLL: Duration = Duration::from_millis(30);
/// How often to ping, which keeps phone networks and home routers from forgetting
/// an idle connection.
const PING_EVERY: Duration = Duration::from_secs(20);
/// How long the server can be silent (it answers pings) before it's taken for gone.
const SILENT_FOR: Duration = Duration::from_secs(60);

pub struct Connection {
    commands: Sender<Command>,
    waker: Waker,
    events: Receiver<Event>,
    status: Status,
    room: Option<String>,
    handle: Option<String>,
    inbox: VecDeque<ServerMsg>,
    thread: JoinHandle<()>,
    rejoining: bool,
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
        let rejoining = token.is_some();
        let waker: Waker = Poller::new().ok().map(Arc::new);
        let thread_waker = waker.clone();
        let thread = std::thread::Builder::new()
            .name("euchre-net".into())
            .spawn(move || run(server, join, token, query, name, Wires { commands: command_rx, events: event_tx, waker: thread_waker }))
            .expect("spawn the network thread");
        Connection { commands, waker, events, status: Status::Connecting, room: None, handle: None, inbox: VecDeque::new(), thread, rejoining }
    }

    /// Hands the network thread a command and wakes it to take it.
    fn command(&self, command: Command) {
        let _ = self.commands.send(command);
        if let Some(waker) = &self.waker {
            let _ = waker.notify();
        }
    }

    /// Connecting with a token from an earlier visit, so a seat here is already ours.
    pub fn rejoining(&self) -> bool {
        self.rejoining
    }

    /// Closes the connection and waits a moment for the thread to finish, so
    /// what was queued (a `Leave`) gets out before the game exits.
    pub fn finish(&mut self, within: Duration) {
        self.command(Command::Close);
        let until = Instant::now() + within;
        while !self.thread.is_finished() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn send(&mut self, msg: ClientMsg) {
        if msg == ClientMsg::Leave {
            self.command(Command::Leave);
        } else if let Ok(text) = serde_json::to_string(&msg) {
            self.command(Command::Send(text));
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
        self.command(Command::Close);
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
    wires: Wires,
) {
    let Wires { commands, events, waker } = wires;
    let mut failures = 0u32;
    let mut was_connected = false;
    // The player left while the connection was down: when to stop trying to say so.
    let mut leaving_until = None;
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
                    if leaving_until.is_some() {
                        // Back only to say goodbye.
                        let _ = socket.send(Message::text(leave_text()));
                        let _ = socket.close(None);
                        let _ = socket.flush();
                        return;
                    }
                    let _ = events.send(Event::Status(Status::Connected));
                    was_connected = true;
                    failures = 0;
                    let ended = match Watch::start(&waker, &socket) {
                        Ok(watch) => serve(&mut socket, &watch, &commands, &events, &mut join, &mut token, &mut leaving_until),
                        Err(e) => {
                            crate::assets::log_warn(&format!("could not watch the connection: {e}"));
                            Ended::Dropped
                        }
                    };
                    match ended {
                        Ended::Closed => {
                            // Back to waiting for the socket, so the goodbye gets out whole.
                            if let Some(tcp) = tcp_of(&socket) {
                                let _ = tcp.set_nonblocking(false);
                                let _ = tcp.set_write_timeout(Some(Duration::from_millis(250)));
                            }
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
                Ok(Command::Leave) => {
                    // Try again right away.
                    leaving_until = Some(Instant::now() + LEAVE_TRIES_FOR);
                    break;
                }
                Ok(Command::Close) | Err(mpsc::RecvTimeoutError::Disconnected) if leaving_until.is_none() => return,
                Err(mpsc::RecvTimeoutError::Disconnected) => std::thread::sleep(Duration::from_millis(100)),
                // Moves made while offline are dropped: the room resyncs us on reconnect.
                Ok(Command::Close | Command::Send(_)) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        // Without a token there's no seat to give back.
        if leaving_until.is_some_and(|by| token.is_none() || Instant::now() > by) {
            return;
        }
    }
}

fn leave_text() -> String {
    serde_json::to_string(&ClientMsg::Leave).expect("leave serializes")
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
                return Ok(socket);
            }
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}

/// The TCP socket under the WebSocket, whether or not TLS is in between.
fn tcp_of(socket: &Socket) -> Option<&TcpStream> {
    match socket.get_ref() {
        MaybeTlsStream::Plain(s) => Some(s),
        MaybeTlsStream::Rustls(s) => Some(s.get_ref()),
        _ => None,
    }
}

/// How the thread waits on one connection: asleep until the socket or the game
/// has something for it, or (with no [`Waker`]) by reads that give up after
/// [`FALLBACK_POLL`]. Stops watching the socket when dropped.
struct Watch<'a> {
    poller: Option<&'a Poller>,
    /// A second handle to the connection's socket, to watch it by while the
    /// WebSocket has the first.
    tcp: TcpStream,
}

impl<'a> Watch<'a> {
    fn start(waker: &'a Waker, socket: &Socket) -> std::io::Result<Watch<'a>> {
        let theirs = tcp_of(socket).ok_or_else(|| std::io::Error::other("no socket under the connection"))?;
        let tcp = theirs.try_clone()?;
        let Some(poller) = waker.as_deref() else {
            theirs.set_read_timeout(Some(FALLBACK_POLL))?;
            return Ok(Watch { poller: None, tcp });
        };
        theirs.set_nonblocking(true)?;
        // SAFETY: `tcp` is taken out of the poller (in `drop`) before it's closed.
        unsafe { poller.add(&tcp, Ready::none(SOCKET))? };
        Ok(Watch { poller: Some(poller), tcp })
    }

    /// Whether reads return at once when there's nothing to read.
    fn sleeps(&self) -> bool {
        self.poller.is_some()
    }

    /// Sleeps until the socket can be read (or, with `unsent` data, written), the
    /// game calls, or `at_most` has passed.
    fn wait(&self, unsent: bool, at_most: Duration, woken: &mut Events) -> std::io::Result<()> {
        let Some(poller) = self.poller else { return Ok(()) };
        let interest = if unsent { Ready::all(SOCKET) } else { Ready::readable(SOCKET) };
        poller.modify(&self.tcp, interest)?;
        woken.clear();
        match poller.wait(woken, Some(at_most)) {
            Err(e) if e.kind() != ErrorKind::Interrupted => Err(e),
            _ => Ok(()),
        }
    }
}

impl Drop for Watch<'_> {
    fn drop(&mut self) {
        if let Some(poller) = self.poller {
            let _ = poller.delete(&self.tcp);
        }
    }
}

/// A send that the socket couldn't take all of yet isn't a failure: the rest
/// goes out with a later `flush`.
fn would_block(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut))
}

fn serve(
    socket: &mut Socket,
    watch: &Watch,
    commands: &Receiver<Command>,
    events: &Sender<Event>,
    join: &mut Join,
    token: &mut Option<String>,
    leaving_until: &mut Option<Instant>,
) -> Ended {
    let mut last_ping = Instant::now();
    let mut last_heard = Instant::now();
    // Part of a message is still waiting for room in the socket.
    let mut unsent = false;
    let mut woken = Events::new();
    loop {
        // Outgoing first, so a move goes out as soon as it's made.
        loop {
            let message = match commands.try_recv() {
                Ok(Command::Send(text)) => Message::text(text),
                Ok(Command::Leave) => {
                    // If this doesn't get out, `run` keeps trying.
                    *leaving_until = Some(Instant::now() + LEAVE_TRIES_FOR);
                    Message::text(leave_text())
                }
                Ok(Command::Close) | Err(TryRecvError::Disconnected) => return Ended::Closed,
                Err(TryRecvError::Empty) => break,
            };
            match socket.send(message) {
                Ok(()) => {}
                Err(e) if would_block(&e) => unsent = true,
                Err(_) => return Ended::Dropped,
            }
        }
        if last_ping.elapsed() >= PING_EVERY {
            last_ping = Instant::now();
            match socket.send(Message::Ping(Vec::new().into())) {
                Ok(()) => {}
                Err(e) if would_block(&e) => unsent = true,
                Err(_) => return Ended::Dropped,
            }
        }
        if unsent {
            match socket.flush() {
                Ok(()) => unsent = false,
                Err(e) if would_block(&e) => {}
                Err(_) => return Ended::Dropped,
            }
        }
        if last_heard.elapsed() > SILENT_FOR {
            return Ended::Dropped;
        }
        // Everything that has arrived. Without a waker one read is the wait, so
        // it's back to the commands after each.
        loop {
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
                Ok(Message::Close(frame)) => {
                    if frame.is_some_and(|f| u16::from(f.code) == CLOSE_REPLACED) {
                        return Ended::Refused("Another window took this seat.".into());
                    }
                    return Ended::Dropped;
                }
                Ok(_) => last_heard = Instant::now(),
                Err(e) if would_block(&e) => break,
                Err(_) => return Ended::Dropped,
            }
            if !watch.sleeps() {
                break;
            }
        }
        // Asleep until the server or the game has something, or it's time to ping.
        let next_ping = PING_EVERY.saturating_sub(last_ping.elapsed());
        let give_up = (SILENT_FOR + Duration::from_millis(50)).saturating_sub(last_heard.elapsed());
        if watch.wait(unsent, next_ping.min(give_up), &mut woken).is_err() {
            return Ended::Dropped;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use super::*;

    /// One end of a connection to a server that this test plays.
    struct Client {
        commands: Sender<Command>,
        events: Receiver<Event>,
        waker: Waker,
        thread: JoinHandle<()>,
    }

    impl Client {
        fn connect(server: String, waker: Waker) -> Client {
            let (commands, command_rx) = mpsc::channel();
            let (event_tx, events) = mpsc::channel();
            let wires = Wires { commands: command_rx, events: event_tx, waker: waker.clone() };
            let thread = std::thread::spawn(move || run(server, Join::Create, None, String::new(), "ME".into(), wires));
            Client { commands, events, waker, thread }
        }

        fn command(&self, command: Command) {
            self.commands.send(command).unwrap();
            if let Some(waker) = &self.waker {
                waker.notify().unwrap();
            }
        }

        /// The next message from the server, and how long it took to come.
        fn message(&self) -> (ServerMsg, Duration) {
            let asked = Instant::now();
            loop {
                match self.events.recv_timeout(Duration::from_secs(5)).expect("a message from the server") {
                    Event::Msg(msg) => return (msg, asked.elapsed()),
                    Event::Status(_) => {}
                }
            }
        }
    }

    fn say(ws: &mut WebSocket<TcpStream>, msg: &ServerMsg) {
        ws.send(Message::text(serde_json::to_string(msg).unwrap())).unwrap();
    }

    /// Plays a hand's worth of talk with the thread, with or without a waker: what
    /// the game sends goes out at once, what the server says comes in at once, and
    /// closing ends the thread and the connection right away.
    fn talks_both_ways(waker: Waker) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server = format!("ws://{}", listener.local_addr().unwrap());
        let client = Client::connect(server, waker);
        let (stream, _) = listener.accept().unwrap();
        let mut ws = tungstenite::accept(stream).unwrap();
        let hello = ws.read().unwrap().into_text().unwrap();
        assert!(hello.contains("hello"), "{hello}");

        // The server speaks first, while the thread is asleep.
        std::thread::sleep(Duration::from_millis(200));
        say(&mut ws, &ServerMsg::Waiting);
        let (msg, took) = client.message();
        assert_eq!(msg, ServerMsg::Waiting);
        assert!(took < Duration::from_millis(150), "the server's message took {took:?}");

        // The game speaks while the thread is asleep, a few times over.
        for _ in 0..3 {
            std::thread::sleep(Duration::from_millis(120));
            let sent = Instant::now();
            client.command(Command::Send("\"start\"".into()));
            assert_eq!(ws.read().unwrap().into_text().unwrap().as_str(), "\"start\"");
            let took = sent.elapsed();
            assert!(took < Duration::from_millis(150), "the game's message took {took:?}");
            say(&mut ws, &ServerMsg::Knocks { requests: Vec::new() });
            assert_eq!(client.message().0, ServerMsg::Knocks { requests: Vec::new() });
        }

        // A burst is kept in order.
        for n in 0..20 {
            client.command(Command::Send(format!("{n}")));
        }
        for n in 0..20 {
            assert_eq!(ws.read().unwrap().into_text().unwrap().as_str(), format!("{n}"));
        }

        // Leaving says so, and closing hangs up without waiting for a ping.
        client.command(Command::Leave);
        assert_eq!(ws.read().unwrap().into_text().unwrap().as_str(), leave_text());
        let closed = Instant::now();
        client.command(Command::Close);
        loop {
            match ws.read() {
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(_) => {}
            }
        }
        client.thread.join().unwrap();
        assert!(closed.elapsed() < Duration::from_secs(1), "closing took {:?}", closed.elapsed());
    }

    #[test]
    fn the_thread_sleeps_until_either_side_speaks() {
        let waker = Poller::new().expect("the OS has a way to wait on a socket");
        talks_both_ways(Some(Arc::new(waker)));
    }

    #[test]
    fn it_still_talks_without_a_waker() {
        talks_both_ways(None);
    }

    /// The same over TLS, which the tests above don't use: sits down at a new table
    /// on the live server, changes it, and leaves.
    /// `cargo test live_server -- --ignored`
    #[test]
    #[ignore = "opens a table on the live server"]
    fn the_live_server_answers_over_tls() {
        let client = Client::connect(LIVE_SERVER.into(), Some(Arc::new(Poller::new().unwrap())));
        let (welcome, _) = client.message();
        assert!(matches!(welcome, ServerMsg::Welcome { .. }), "{welcome:?}");
        // Who sits where, among whatever else a host is told.
        let seats = |client: &Client| loop {
            if let (ServerMsg::Seats(seats), _) = client.message() {
                return seats;
            }
        };
        assert!(!seats(&client).public);
        // Sent while the thread is asleep, and answered while it's asleep again.
        std::thread::sleep(Duration::from_millis(400));
        let text = serde_json::to_string(&ClientMsg::SetPublic { public: true }).unwrap();
        client.command(Command::Send(text));
        assert!(seats(&client).public);
        client.command(Command::Leave);
        client.command(Command::Close);
        client.thread.join().unwrap();
    }

    /// A quiet connection costs nothing: the thread only wakes to ping.
    #[test]
    fn a_quiet_connection_leaves_the_thread_asleep() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server = format!("ws://{}", listener.local_addr().unwrap());
        let waker = Arc::new(Poller::new().unwrap());
        let client = Client::connect(server, Some(waker));
        let (stream, _) = listener.accept().unwrap();
        let mut ws = tungstenite::accept(stream).unwrap();
        ws.read().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let before = thread_cpu(&client.thread);
        std::thread::sleep(Duration::from_millis(1500));
        let used = thread_cpu(&client.thread).saturating_sub(before);
        client.command(Command::Close);
        client.thread.join().unwrap();
        // Fifty wakeups to look for commands would be several milliseconds.
        assert!(used < Duration::from_millis(2), "an idle connection used {used:?} of CPU in 1.5 s");
    }

    /// How much CPU time a thread has used.
    #[cfg(windows)]
    fn thread_cpu(thread: &JoinHandle<()>) -> Duration {
        use std::os::windows::io::AsRawHandle;

        #[repr(C)]
        #[derive(Default)]
        struct FileTime {
            low: u32,
            high: u32,
        }
        unsafe extern "system" {
            fn GetThreadTimes(
                thread: *mut std::ffi::c_void,
                created: *mut FileTime,
                exited: *mut FileTime,
                kernel: *mut FileTime,
                user: *mut FileTime,
            ) -> i32;
        }
        let (mut created, mut exited, mut kernel, mut user) = Default::default();
        // SAFETY: the handle is the live thread's, and the four are out-pointers.
        let ok = unsafe { GetThreadTimes(thread.as_raw_handle().cast(), &mut created, &mut exited, &mut kernel, &mut user) };
        assert_ne!(ok, 0);
        let ticks = |t: &FileTime| (u64::from(t.high) << 32) | u64::from(t.low);
        Duration::from_nanos((ticks(&kernel) + ticks(&user)) * 100)
    }

    /// Only Windows says without more dependencies, so elsewhere this checks nothing.
    #[cfg(not(windows))]
    fn thread_cpu(_thread: &JoinHandle<()>) -> Duration {
        Duration::ZERO
    }
}
