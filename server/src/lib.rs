//! The online room server: a Cloudflare Worker that hands each WebSocket to
//! the Durable Object for its room code.
//!
//! - `GET /create` (WebSocket upgrade, optional `?target=10&stick=1`): picks a
//!   fresh room code and opens a new room with it.
//! - `GET /join/{CODE}` (WebSocket upgrade): joins or rejoins that room.
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
    ClientMsg, ErrorCode, PROTOCOL_VERSION, ServerMsg, clean_code, make_code,
};
use euchre_core::room::{Room, RoomSnapshot};
use serde::{Deserialize, Serialize};
use worker::*;

/// How long a connected player may think before a bot moves for them.
const TURN_MS: f64 = 45_000.0;
/// How long a disconnected player's turn waits for them to come back.
const AWAY_TURN_MS: f64 = 15_000.0;
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
        // A malformed code can't be a room; let the room-less object say so over the socket.
        let code = clean_code(code).unwrap_or_else(|| "AAAAA".into());
        let target = format!("https://room/{code}");
        return rooms.id_from_name(&code)?.get_stub()?.fetch_with_request(forward(&req, &target)?).await;
    }
    Response::error("Not found", 404)
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

/// Kept on each socket (it survives hibernation): whose seat it is, once they've said hello.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct Attachment {
    seat: Option<u8>,
}

/// The room plus the timer bookkeeping, as stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Saved {
    room: RoomSnapshot,
    /// The turn being timed (its key from `Room::waiting_on`) and when it started.
    turn: Option<(u64, f64)>,
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
        self.deliver(&mut room);
        saved.room = room.snapshot();
        self.schedule(&room, &mut saved).await?;
        self.state.storage().put("room", serde_json::to_string(&saved)?).await?;
        *self.saved.borrow_mut() = Some(saved);
        Ok(())
    }

    fn sockets_for(&self, seat: usize) -> Vec<WebSocket> {
        self.state
            .get_websockets()
            .into_iter()
            .filter(|ws| attachment(ws).seat.map(usize::from) == Some(seat))
            .collect()
    }

    fn deliver(&self, room: &mut Room) {
        for out in room.take_outbox() {
            let Ok(text) = serde_json::to_string(&out.msg) else { continue };
            for ws in self.sockets_for(out.to) {
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
        match room.waiting_on() {
            Some((_, connected, key)) => {
                let started = match saved.turn {
                    Some((k, t)) if k == key => t,
                    _ => now,
                };
                saved.turn = Some((key, started));
                let limit = if connected { TURN_MS } else { AWAY_TURN_MS };
                at = at.min(started + limit);
            }
            None => saved.turn = None,
        }
        self.state.storage().set_alarm(at.max(now + 50.0) as i64).await
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
            let saved = Saved {
                room: Room::new(&code, target, stick).snapshot(),
                turn: None,
                last_active: now,
                empty_since: Some(now),
            };
            self.state.storage().put("room", serde_json::to_string(&saved)?).await?;
            *self.saved.borrow_mut() = Some(saved);
        }

        let pair = WebSocketPair::new()?;
        let ws = pair.server;
        self.state.accept_web_socket(&ws);
        ws.serialize_attachment(Attachment::default())?;
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
        let seat = attachment(&ws).seat;
        let mut refused = None;
        let seed = random_u64();
        let fresh = random_token();
        self.with_room(|room, saved| {
            saved.last_active = Date::now().as_millis() as f64;
            match (seat, msg) {
                (None, ClientMsg::Hello { version, name, token }) => {
                    match room.join(version, &name, token.as_deref(), &fresh) {
                        Ok(s) => {
                            // An older connection for this seat (a phone that
                            // switched networks) is replaced by this one. Both
                            // happen before the welcome is delivered.
                            for old in self.sockets_for(s) {
                                let _ = old.serialize_attachment(Attachment::default());
                                let _ = old.close(Some(4001), Some("Replaced by a newer connection"));
                            }
                            let _ = ws.serialize_attachment(Attachment { seat: Some(s as u8) });
                        }
                        Err(code) => refused = Some(code),
                    }
                }
                (Some(s), ClientMsg::Hello { .. }) => room.handle(s as usize, ClientMsg::Resync, seed),
                (Some(s), msg) => room.handle(s as usize, msg, seed),
                (None, _) => refused = Some(ErrorCode::NotAllowed),
            }
        })
        .await?;
        if let Some(code) = refused {
            EuchreRoom::refuse(&ws, code);
            if matches!(code, ErrorCode::RoomFull | ErrorCode::Version) {
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
            self.state.storage().delete_all().await?;
            *self.saved.borrow_mut() = None;
            return Response::ok("closed");
        }
        self.with_room(|room, saved| {
            if let (Some((_, connected, key)), Some((k, started))) = (room.waiting_on(), saved.turn)
                && k == key
            {
                let limit = if connected { TURN_MS } else { AWAY_TURN_MS };
                if started + limit <= now + 100.0 {
                    room.force_move(key);
                }
            }
        })
        .await?;
        Response::ok("ok")
    }
}

impl EuchreRoom {
    /// A socket closed: if it held a seat (and no newer socket does), the seat waits for its player.
    async fn left(&self, ws: &WebSocket) -> Result<()> {
        let Some(seat) = attachment(ws).seat else { return Ok(()) };
        // Forget this socket's seat first, so only newer sockets for it count.
        let _ = ws.serialize_attachment(Attachment::default());
        let _ = ws.close(Some(1000), Some("bye"));
        if self.sockets_for(seat as usize).is_empty() {
            self.with_room(|room, _| room.disconnected(seat as usize)).await?;
        }
        Ok(())
    }
}

// Version check at build time: the server and the game share the protocol.
const _: () = assert!(PROTOCOL_VERSION >= 1);
