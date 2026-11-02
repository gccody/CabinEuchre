//! The online room server: a Cloudflare Worker that hands each WebSocket to
//! the Durable Object for its room code.
//!
//! - `GET /create` (WebSocket upgrade, optional `?target=10&stick=1`): picks a
//!   fresh room code and opens a new room with it.
//! - `GET /join/{CODE}` (WebSocket upgrade): joins or rejoins that room. The
//!   code is the invitation: whoever has it sits straight down.
//! - `GET /knock/{HANDLE}` (WebSocket upgrade): asks to join. The handle is the
//!   room's Durable Object id (64 hex digits), which is what players' Steam
//!   friends see; it can't be turned back into the code. At a private table
//!   (the default) the host has to let them in.
//!
//! The game rules all live in `euchre-core`: a [`Room`] holds the real match
//! and answers each message with an outbox of per-player messages. This file
//! only moves those messages between sockets, keeps the room in storage and
//! runs the turn timer, so it costs almost nothing to run:
//!
//! - WebSockets use the Hibernation API: an idle room is evicted from memory
//!   and not billed. Every change is saved, and the room is reloaded from
//!   storage when the next message arrives.
//! - Bots move instantly; players' games add the "thinking" pauses while
//!   animating, so the object is awake for milliseconds per message.
//! - A single alarm covers both the turn timer (a bot moves for a player who
//!   takes too long or dropped) and deleting rooms nobody has used for a while.

use std::cell::RefCell;

use euchre_core::protocol::{
    ClientMsg, ErrorCode, PROTOCOL_VERSION, ServerMsg, clean_code, clean_handle, make_code,
};
use euchre_core::room::{AWAY_SEAT_MS, Room, RoomSnapshot, TurnClock};
use serde::{Deserialize, Serialize};
use worker::*;

/// A room with nobody connected is deleted after this long.
const EMPTY_ROOM_MS: f64 = 60.0 * 60_000.0;
/// Any room is deleted after this long without a single message.
const IDLE_ROOM_MS: f64 = 24.0 * 60.0 * 60_000.0;

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let url = req.url()?;
    let path = url.path().to_string();
    if path == "/" {
        return Response::ok("Euchre room server. Connect with the game.");
    }
    let upgrade = req.headers().get("Upgrade")?.unwrap_or_default();
    if !upgrade.eq_ignore_ascii_case("websocket") {
        return Response::error("Expected a WebSocket upgrade", 426);
    }
    if !allowed(&req, &env).await {
        return Response::error("Too many tries; wait a minute", 429);
    }
    let rooms = env.durable_object("ROOMS")?;
    if path == "/create" {
        let query = url.query().unwrap_or("").to_string();
        // A code that's already a live room answers 409: try another.
        for _ in 0..8 {
            let code = make_code(random_u32);
            let target = format!("https://room/{code}?create=1&{query}");
            let resp = rooms.id_from_name(&code)?.get_stub()?.fetch_with_request(forward(&req, &target)?).await?;
            if resp.status_code() != 409 {
                return Ok(resp);
            }
        }
        return Response::error("No free room code; try again", 503);
    }
    if let Some(code) = path.strip_prefix("/join/") {
        // A malformed code can't be a room; let a room-less object say so over the
        // socket. Codes never use I, so this one is never anyone's table.
        let code = clean_code(code).unwrap_or_else(|| "IIIII".into());
        let target = format!("https://room/{code}");
        return rooms.id_from_name(&code)?.get_stub()?.fetch_with_request(forward(&req, &target)?).await;
    }
    if let Some(handle) = path.strip_prefix("/knock/") {
        let Some(handle) = clean_handle(handle) else { return Response::error("Not a room handle", 404) };
        let Ok(id) = rooms.id_from_string(&handle) else { return Response::error("Not a room handle", 404) };
        return id.get_stub()?.fetch_with_request(forward(&req, "https://room/knock?knock=1")?).await;
    }
    Response::error("Not found", 404)
}

/// Limits new connections per address (see `[[ratelimits]]` in `wrangler.toml`):
/// room codes are short, so guessing them has to be slow, and creating rooms and
/// asking to join shouldn't be free to flood. If the limiter itself fails, let
/// the player in: a game that works beats a perfect limit.
async fn allowed(req: &Request, env: &Env) -> bool {
    let address = req.headers().get("CF-Connecting-IP").ok().flatten().unwrap_or_else(|| "local".into());
    let outcome = match env.rate_limiter("CONNECTS") {
        Ok(limiter) => limiter.limit(address).await,
        Err(error) => Err(error),
    };
    // Players come first if the limiter itself breaks, but say so: it looks just like no limit.
    outcome.map_or_else(
        |error| {
            console_error!("rate limiter failed: {error}");
            true
        },
        |outcome| outcome.success,
    )
}

/// The same upgrade request, aimed at a room.
fn forward(req: &Request, url: &str) -> Result<Request> {
    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(req.headers().clone());
    Request::new_with_init(url, &init)
}

fn random_bytes(n: u32) -> Vec<u8> {
    let bytes = js_sys::Uint8Array::new_with_length(n);
    let crypto = js_sys::Reflect::get(&js_sys::global(), &"crypto".into()).expect("crypto");
    let get = js_sys::Reflect::get(&crypto, &"getRandomValues".into()).expect("getRandomValues");
    let get: js_sys::Function = get.into();
    get.call1(&crypto, &bytes).expect("random values");
    bytes.to_vec()
}

fn random_u32() -> u32 {
    let b = random_bytes(4);
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn random_u64() -> u64 {
    let b = random_bytes(8);
    u64::from_le_bytes(b.try_into().expect("8 bytes"))
}

/// A seat token: 128 random bits as hex.
fn random_token() -> String {
    random_bytes(16).iter().map(|b| format!("{b:02x}")).collect()
}

/// Kept on each socket (it survives hibernation): whose token it holds, once
/// they've said hello, or which request to join it's waiting on. A token rather
/// than a seat number, because the room is what knows where a token sits: it
/// follows a player who changes seats, and stops working when they leave.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Attachment {
    #[serde(default)]
    token: Option<String>,
    /// Came by the handle: their hello asks to join instead of joining.
    #[serde(default)]
    via_knock: bool,
    /// The request waiting for the host's answer.
    #[serde(default)]
    knock: Option<u32>,
}

impl Attachment {
    fn seated(token: &str) -> Attachment {
        Attachment { token: Some(token.to_string()), via_knock: false, knock: None }
    }
}

/// The room plus the timer bookkeeping, as stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Saved {
    room: RoomSnapshot,
    /// The turn being timed.
    #[serde(default)]
    clock: Option<TurnClock>,
    /// When each disconnected seat's player dropped.
    #[serde(default)]
    away: [Option<f64>; 4],
    /// When anyone last sent a message, and when the last player disconnected.
    last_active: f64,
    empty_since: Option<f64>,
}

#[durable_object]
pub struct EuchreRoom {
    state: State,
    #[allow(dead_code)]
    env: Env,
    /// The room in memory, loaded from storage on first use after waking.
    saved: RefCell<Option<Saved>>,
    loaded: RefCell<bool>,
}

impl EuchreRoom {
    async fn load(&self) -> Result<()> {
        if *self.loaded.borrow() {
            return Ok(());
        }
        let text: Option<String> = self.state.storage().get("room").await?;
        let saved = text.and_then(|t| serde_json::from_str::<Saved>(&t).ok());
        *self.saved.borrow_mut() = saved;
        *self.loaded.borrow_mut() = true;
        Ok(())
    }

    /// Runs `f` on the room (if there is one), then delivers its outbox,
    /// saves it and sets the alarm.
    async fn with_room(&self, f: impl FnOnce(&mut Room, &mut Saved)) -> Result<()> {
        self.load().await?;
        let Some(mut saved) = self.saved.borrow_mut().take() else { return Ok(()) };
        let mut room = match Room::restore(saved.room.clone()) {
            Ok(r) => r,
            Err(e) => {
                console_error!("room snapshot doesn't load: {}", e.0);
                return Ok(());
            }
        };
        f(&mut room, &mut saved);
        // Requests the host answered: seat the waiting sockets before their
        // welcome goes out, or turn them away.
        for (id, seat) in room.take_admitted() {
            let Some(token) = room.token_at(seat) else { continue };
            for ws in self.knocking(id) {
                let _ = ws.serialize_attachment(Attachment::seated(token));
            }
        }
        for id in room.take_declined() {
            for ws in self.knocking(id) {
                EuchreRoom::refuse(&ws, ErrorCode::Declined);
                let _ = ws.serialize_attachment(Attachment::default());
                let _ = ws.close(Some(4003), Some("Not let in"));
            }
        }
        self.deliver(&mut room);
        saved.room = room.snapshot();
        self.schedule(&room, &mut saved).await?;
        self.state.storage().put("room", serde_json::to_string(&saved)?).await?;
        *self.saved.borrow_mut() = Some(saved);
        Ok(())
    }

    fn holding(&self, token: &str) -> Vec<WebSocket> {
        self.state
            .get_websockets()
            .into_iter()
            .filter(|ws| attachment(ws).token.as_deref() == Some(token))
            .collect()
    }

    fn knocking(&self, id: u32) -> Vec<WebSocket> {
        self.state.get_websockets().into_iter().filter(|ws| attachment(ws).knock == Some(id)).collect()
    }

    fn deliver(&self, room: &mut Room) {
        for out in room.take_outbox() {
            let Ok(text) = serde_json::to_string(&out.msg) else { continue };
            let Some(token) = room.token_at(out.to) else { continue };
            for ws in self.holding(token) {
                let _ = ws.send_with_str(&text);
            }
        }
    }

    /// Sets the alarm for whichever comes first: the current turn running
    /// out, or the room expiring.
    async fn schedule(&self, room: &Room, saved: &mut Saved) -> Result<()> {
        let now = Date::now().as_millis() as f64;
        let mut at = saved.last_active + IDLE_ROOM_MS;
        if room.connected() == 0 {
            let since = *saved.empty_since.get_or_insert(now);
            at = at.min(since + EMPTY_ROOM_MS);
        } else {
            saved.empty_since = None;
        }
        for seat in 0..4 {
            saved.away[seat] = room.is_away(seat).then(|| saved.away[seat].unwrap_or(now));
            if let Some(since) = saved.away[seat] {
                at = at.min(since + AWAY_SEAT_MS);
            }
        }
        match room.waiting_on() {
            Some((_, connected, key)) => {
                let clock = TurnClock::tick(saved.clock, key, connected, now);
                saved.clock = Some(clock);
                at = at.min(clock.deadline());
            }
            None => saved.clock = None,
        }
        // A plain number here is a delay from now, not a time of day.
        self.state.storage().set_alarm((at - now).max(50.0) as i64).await
    }

    fn refuse(ws: &WebSocket, code: ErrorCode) {
        let msg = ServerMsg::Error { code, message: code.describe().into() };
        if let Ok(text) = serde_json::to_string(&msg) {
            let _ = ws.send_with_str(&text);
        }
    }
}

fn attachment(ws: &WebSocket) -> Attachment {
    ws.deserialize_attachment().ok().flatten().unwrap_or_default()
}

impl DurableObject for EuchreRoom {
    fn new(state: State, env: Env) -> Self {
        EuchreRoom { state, env, saved: RefCell::new(None), loaded: RefCell::new(false) }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        self.load().await?;
        let url = req.url()?;
        let code = url.path().trim_start_matches('/').to_string();
        let query: Vec<(String, String)> = url.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
        let param = |name: &str| query.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
        if param("create").is_some() {
            if self.saved.borrow().is_some() {
                return Response::error("Room code in use", 409);
            }
            let target = param("target").and_then(|t| t.parse().ok()).unwrap_or(10u32).clamp(1, 99);
            let stick = param("stick").is_none_or(|s| s != "0");
            let now = Date::now().as_millis() as f64;
            // Friends see this object's id, never the code.
            let handle = self.state.id().to_string();
            let saved = Saved {
                room: Room::new(&code, &handle, target, stick).snapshot(),
                clock: None,
                away: [None; 4],
                last_active: now,
                empty_since: Some(now),
            };
            self.state.storage().put("room", serde_json::to_string(&saved)?).await?;
            // Cleans up a room whose host never says hello.
            self.state.storage().set_alarm(EMPTY_ROOM_MS as i64).await?;
            *self.saved.borrow_mut() = Some(saved);
        }

        let pair = WebSocketPair::new()?;
        let ws = pair.server;
        self.state.accept_web_socket(&ws);
        ws.serialize_attachment(Attachment { via_knock: param("knock").is_some(), ..Attachment::default() })?;
        if self.saved.borrow().is_none() {
            EuchreRoom::refuse(&ws, ErrorCode::NoSuchRoom);
            ws.close(Some(4004), Some("no such room"))?;
        }
        Response::from_websocket(pair.client)
    }

    async fn websocket_message(&self, ws: WebSocket, message: WebSocketIncomingMessage) -> Result<()> {
        let WebSocketIncomingMessage::String(text) = message else { return Ok(()) };
        let msg = match serde_json::from_str::<ClientMsg>(&text) {
            Ok(m) => m,
            Err(_) => {
                EuchreRoom::refuse(&ws, ErrorCode::BadMessage);
                return Ok(());
            }
        };
        self.load().await?;
        if self.saved.borrow().is_none() {
            EuchreRoom::refuse(&ws, ErrorCode::NoSuchRoom);
            return Ok(());
        }
        let att = attachment(&ws);
        let mut refused = None;
        let seed = random_u64();
        let fresh = random_token();
        let knock_id = random_u32();
        self.with_room(|room, saved| {
            saved.last_active = Date::now().as_millis() as f64;
            let seat = att.token.as_deref().and_then(|t| room.seat_of_token(t));
            match (seat, msg) {
                (None, ClientMsg::Hello { .. }) if att.knock.is_some() => {}
                (None, ClientMsg::Hello { version, name, token }) => {
                    let joined = if att.via_knock {
                        room.knock(version, &name, token.as_deref(), knock_id, &fresh)
                    } else {
                        room.join(version, &name, token.as_deref(), &fresh).map(Some)
                    };
                    match joined {
                        Ok(Some(s)) => {
                            // An older connection for this seat (a phone that
                            // switched networks) is replaced by this one. Both
                            // happen before the welcome is delivered.
                            let token = room.token_at(s).unwrap_or_default();
                            for old in self.holding(token) {
                                let _ = old.serialize_attachment(Attachment::default());
                                let _ = old.close(Some(4001), Some("Replaced by a newer connection"));
                            }
                            let _ = ws.serialize_attachment(Attachment::seated(token));
                        }
                        Ok(None) => {
                            // A private table: wait for the host.
                            let _ = ws.serialize_attachment(Attachment { knock: Some(knock_id), ..att.clone() });
                            if let Ok(text) = serde_json::to_string(&ServerMsg::Waiting) {
                                let _ = ws.send_with_str(&text);
                            }
                        }
                        Err(code) => refused = Some(code),
                    }
                }
                (Some(s), ClientMsg::Hello { .. }) => room.handle(s, ClientMsg::Resync, seed),
                (Some(s), msg) => room.handle(s, msg, seed),
                (None, _) => refused = Some(ErrorCode::NotAllowed),
            }
        })
        .await?;
        if let Some(code) = refused {
            EuchreRoom::refuse(&ws, code);
            if matches!(code, ErrorCode::RoomFull | ErrorCode::Version | ErrorCode::Declined) {
                ws.close(Some(4000), Some(code.describe()))?;
            }
        }
        Ok(())
    }

    async fn websocket_close(&self, ws: WebSocket, _code: usize, _reason: String, _clean: bool) -> Result<()> {
        self.left(&ws).await
    }

    async fn websocket_error(&self, ws: WebSocket, _error: Error) -> Result<()> {
        self.left(&ws).await
    }

    async fn alarm(&self) -> Result<Response> {
        self.load().await?;
        let now = Date::now().as_millis() as f64;
        let expired = self.saved.borrow().as_ref().is_some_and(|s| {
            s.last_active + IDLE_ROOM_MS <= now || s.empty_since.is_some_and(|t| t + EMPTY_ROOM_MS <= now)
        });
        if expired {
            for ws in self.state.get_websockets() {
                let _ = ws.close(Some(4002), Some("This table has closed"));
            }
            // The alarm first: deleting everything with one still set fails from inside its own handler.
            self.state.storage().delete_alarm().await?;
            self.state.storage().delete_all().await?;
            *self.saved.borrow_mut() = None;
            return Response::ok("closed");
        }
        self.with_room(|room, saved| {
            for seat in 0..4 {
                if saved.away[seat].is_some_and(|since| since + AWAY_SEAT_MS <= now + 100.0) {
                    room.release_seat(seat);
                }
            }
            if let (Some((_, _, key)), Some(clock)) = (room.waiting_on(), saved.clock)
                && clock.key == key
                && clock.deadline() <= now + 100.0
            {
                room.force_move(key);
            }
        })
        .await?;
        Response::ok("ok")
    }
}

impl EuchreRoom {
    /// A socket closed: if it held a seat (and no newer socket does), the seat waits for its player.
    async fn left(&self, ws: &WebSocket) -> Result<()> {
        let att = attachment(ws);
        if let Some(id) = att.knock {
            // Gave up waiting to be let in.
            let _ = ws.serialize_attachment(Attachment::default());
            return self.with_room(|room, _| room.cancel_knock(id)).await;
        }
        let Some(token) = att.token else { return Ok(()) };
        // Forget this socket's token first, so only newer sockets for it count.
        let _ = ws.serialize_attachment(Attachment::default());
        let _ = ws.close(Some(1000), Some("bye"));
        if self.holding(&token).is_empty() {
            self.with_room(|room, _| {
                if let Some(seat) = room.seat_of_token(&token) {
                    room.disconnected(seat);
                }
            })
            .await?;
        }
        Ok(())
    }
}

// Version check at build time: the server and the game share the protocol.
const _: () = assert!(PROTOCOL_VERSION >= 1);
