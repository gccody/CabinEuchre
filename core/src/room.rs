//! A table of four seats: the authority for one online game, and also what
//! runs an offline game against the computer, so both play by the same code.
//!
//! A `Room` holds the real [`Match`] (every hand, the deck), seats humans and
//! bots, applies moves, lets bots act at once, and writes the messages each
//! player should receive into an outbox (see [`Room::take_outbox`]). It does
//! no I/O and keeps no clock: the server (`server/`) delivers the outbox over
//! WebSockets and handles turn timers through [`Room::waiting_on`] and
//! [`Room::force_move`].
//!
//! Bots move instantly here. The players' games pace them with "thinking"
//! pauses while animating, so the server only runs for a moment per message.
//!
//! Seats in this file are absolute (0-3 around the table). Messages are
//! turned into each player's point of view (they are seat 0) as they go out.
//!
//! Getting in (see `protocol.rs`): [`Room::join`] seats anyone who came with
//! the code. [`Room::knock`] is for players who came by the handle (a Steam
//! friend's "Join Game"): at a public table they sit straight down, at a
//! private one (the default) they wait until the host answers. Admitted and
//! declined requests are picked up by the server with
//! [`Room::take_admitted`] / [`Room::take_declined`], since those players
//! aren't seated yet and have no seat to address messages to.

use serde::{Deserialize, Serialize};

use crate::ai;
use crate::game::Match;
use crate::protocol::{
    ClientMsg, ErrorCode, HandStart, KnockInfo, Move, PROTOCOL_VERSION, SeatInfo, SeatKind, SeatsInfo, ServerMsg,
    Shown, clean_name,
};
use crate::round::{Phase, Seat, team_of};
use crate::save::LoadError;

/// Computer players' names, by absolute seat.
pub const BOT_NAMES: [&str; 4] = ["OTTO", "ROSA", "BENNY", "JULES"];

/// Where joining players sit: the first two are partners.
const JOIN_ORDER: [Seat; 4] = [0, 2, 1, 3];

/// How many requests to join may wait at once, so nobody can bury the host in them.
const MAX_KNOCKS: usize = 8;

/// How long a connected player may think before a bot moves for them. This
/// starts when the room is ready, a little before the player's game has finished
/// showing the deal and the bots' moves, so it allows for that too.
pub const TURN_MS: f64 = 60_000.0;
/// How long a turn waits for a disconnected player, from when they dropped.
/// Also how long someone who just came back gets, at the least.
pub const AWAY_TURN_MS: f64 = 15_000.0;
/// No turn runs past this, however often its player drops and comes back.
pub const LONGEST_TURN_MS: f64 = 120_000.0;
/// A person who stays disconnected this long loses their seat to a bot.
pub const AWAY_SEAT_MS: f64 = 10.0 * 60_000.0;

/// When the current turn runs out. A pure calculation over times the server
/// passes in (milliseconds), since a room keeps no clock of its own.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TurnClock {
    pub key: u64,
    started: f64,
    away_since: Option<f64>,
    back_at: Option<f64>,
}

impl TurnClock {
    /// The clock for turn `key` now, carrying on from `previous` if it's the same turn.
    pub fn tick(previous: Option<TurnClock>, key: u64, connected: bool, now: f64) -> TurnClock {
        let mut clock = previous.filter(|c| c.key == key).unwrap_or(TurnClock {
            key,
            started: now,
            away_since: None,
            back_at: None,
        });
        match (connected, clock.away_since) {
            (false, None) => clock.away_since = Some(now),
            (true, Some(_)) => {
                clock.away_since = None;
                clock.back_at = Some(now);
            }
            _ => {}
        }
        clock
    }

    pub fn deadline(&self) -> f64 {
        let deadline = match self.away_since {
            Some(since) => since + AWAY_TURN_MS,
            None => (self.started + TURN_MS).max(self.back_at.map_or(0.0, |t| t + AWAY_TURN_MS)),
        };
        deadline.min(self.started + LONGEST_TURN_MS)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Occupant {
    Bot,
    Human { name: String, token: String, connected: bool },
}

/// A player asking to join, waiting for the host's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Knock {
    pub id: u32,
    pub name: String,
    /// The seat token they get if they're let in.
    pub token: String,
}

/// A message for the player in absolute seat `to`.
#[derive(Debug, Clone, PartialEq)]
pub struct Outgoing {
    pub to: Seat,
    pub msg: ServerMsg,
}

#[derive(Debug, Clone)]
pub struct Room {
    pub code: String,
    /// What friends see instead of the code (see `protocol.rs`).
    pub handle: String,
    seats: [Occupant; 4],
    game: Option<Match>,
    /// Games dealt in this room so far.
    game_no: u32,
    target: u32,
    stick_the_dealer: bool,
    /// Counts every move, so a turn timer can tell whether the turn it was
    /// set for is still the one being waited on.
    moves: u64,
    /// Players coming by the handle sit down without asking.
    public: bool,
    /// Who answers requests and sets `public`: the first person to sit down,
    /// then whoever is next when they leave.
    host: Option<Seat>,
    knocks: Vec<Knock>,
    outbox: Vec<Outgoing>,
    admitted: Vec<(u32, Seat)>,
    declined: Vec<u32>,
}

/// Everything a room needs to carry on after the server forgets it (a
/// Durable Object hibernating or restarting). The match is kept in the save
/// format, which is replayed through the rules on load.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomSnapshot {
    pub code: String,
    #[serde(default)]
    pub handle: String,
    pub seats: [Occupant; 4],
    pub game: Option<String>,
    pub game_no: u32,
    pub target: u32,
    pub stick_the_dealer: bool,
    pub moves: u64,
    #[serde(default)]
    pub public: bool,
    #[serde(default)]
    pub host: Option<Seat>,
    #[serde(default)]
    pub knocks: Vec<Knock>,
}

fn rel(to: Seat, seat: Seat) -> Seat {
    (seat + 4 - to) % 4
}

impl Room {
    /// A new, private room. `handle` is what friends will see (any string
    /// for a room nobody can reach by handle, like an offline game).
    pub fn new(code: &str, handle: &str, target: u32, stick_the_dealer: bool) -> Room {
        Room {
            code: code.to_string(),
            handle: handle.to_string(),
            seats: std::array::from_fn(|_| Occupant::Bot),
            game: None,
            game_no: 0,
            target: target.max(1),
            stick_the_dealer,
            moves: 0,
            public: false,
            host: None,
            knocks: Vec::new(),
            outbox: Vec::new(),
            admitted: Vec::new(),
            declined: Vec::new(),
        }
    }

    pub fn snapshot(&self) -> RoomSnapshot {
        RoomSnapshot {
            code: self.code.clone(),
            handle: self.handle.clone(),
            seats: self.seats.clone(),
            game: self.game.as_ref().map(Match::to_save),
            game_no: self.game_no,
            target: self.target,
            stick_the_dealer: self.stick_the_dealer,
            moves: self.moves,
            public: self.public,
            host: self.host,
            knocks: self.knocks.clone(),
        }
    }

    pub fn restore(s: RoomSnapshot) -> Result<Room, LoadError> {
        let game = match &s.game {
            Some(text) => {
                let mut m = Match::from_save(text)?;
                // Saves leave out a scored hand's points; a room only stops on
                // a finished hand when the game is over, so count it again.
                if m.round.phase == Phase::HandOver {
                    m.score_hand();
                }
                Some(m)
            }
            None => None,
        };
        Ok(Room {
            code: s.code,
            handle: s.handle,
            seats: s.seats,
            game,
            game_no: s.game_no,
            target: s.target.max(1),
            stick_the_dealer: s.stick_the_dealer,
            moves: s.moves,
            public: s.public,
            host: s.host,
            knocks: s.knocks,
            outbox: Vec::new(),
            admitted: Vec::new(),
            declined: Vec::new(),
        })
    }

    /// The messages produced since the last call, in order.
    pub fn take_outbox(&mut self) -> Vec<Outgoing> {
        std::mem::take(&mut self.outbox)
    }

    /// Requests the host accepted since the last call: (request id, the seat
    /// they now hold). Attach their connection to the seat before delivering
    /// the outbox, which already holds their welcome.
    pub fn take_admitted(&mut self) -> Vec<(u32, Seat)> {
        std::mem::take(&mut self.admitted)
    }

    /// Requests the host turned down (or that no longer fit) since the last call.
    pub fn take_declined(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.declined)
    }

    pub fn is_public(&self) -> bool {
        self.public
    }

    pub fn host(&self) -> Option<Seat> {
        self.host
    }

    pub fn knocks(&self) -> &[Knock] {
        &self.knocks
    }

    pub fn seats(&self) -> &[Occupant; 4] {
        &self.seats
    }

    pub fn game(&self) -> Option<&Match> {
        self.game.as_ref()
    }

    /// Games dealt in this room so far (the current one's number).
    pub fn game_no(&self) -> u32 {
        self.game_no
    }

    /// A game is under way (dealt and not yet won).
    pub fn playing(&self) -> bool {
        self.game.as_ref().is_some_and(|m| m.winner().is_none())
    }

    /// Seats held by people, connected or not.
    pub fn humans(&self) -> usize {
        self.seats.iter().filter(|o| matches!(o, Occupant::Human { .. })).count()
    }

    pub fn connected(&self) -> usize {
        self.seats.iter().filter(|o| matches!(o, Occupant::Human { connected: true, .. })).count()
    }

    /// A person holds this seat, but their connection is down.
    pub fn is_away(&self, seat: Seat) -> bool {
        matches!(self.seats[seat], Occupant::Human { connected: false, .. })
    }

    /// Someone has been gone too long: a bot takes the seat, so a table can't
    /// fill up with people who are never coming back.
    pub fn release_seat(&mut self, seat: Seat) {
        if self.is_away(seat) {
            self.vacate(seat);
        }
    }

    pub fn seat_of_token(&self, token: &str) -> Option<Seat> {
        self.seats
            .iter()
            .position(|o| matches!(o, Occupant::Human { token: t, .. } if t == token))
    }

    /// The token of the person in `seat`. A connection belongs to a token, not
    /// a seat number: the token moves when its player changes seats, and is
    /// gone when they leave, so a connection can't outlive its claim to a seat.
    pub fn token_at(&self, seat: Seat) -> Option<&str> {
        match &self.seats[seat] {
            Occupant::Human { token, .. } => Some(token),
            Occupant::Bot => None,
        }
    }

    /// Seats a player who sent [`ClientMsg::Hello`]. A known `token` gets its
    /// old seat back; anyone else takes the next bot's seat and `fresh_token`.
    /// Returns the absolute seat.
    pub fn join(&mut self, version: u32, name: &str, token: Option<&str>, fresh_token: &str) -> Result<Seat, ErrorCode> {
        if version != PROTOCOL_VERSION {
            return Err(ErrorCode::Version);
        }
        let name = clean_name(name);
        if let Some(seat) = token.and_then(|t| self.seat_of_token(t)) {
            let token = token.unwrap_or_default().to_string();
            self.seats[seat] = Occupant::Human { name, token: token.clone(), connected: true };
            self.check_host();
            self.welcome(seat, &token);
            return Ok(seat);
        }
        let seat = self.free_seat().ok_or(ErrorCode::RoomFull)?;
        self.seat_player(seat, name, fresh_token);
        Ok(seat)
    }

    fn free_seat(&self) -> Option<Seat> {
        JOIN_ORDER.iter().copied().find(|&s| self.seats[s] == Occupant::Bot)
    }

    fn seat_player(&mut self, seat: Seat, name: String, token: &str) {
        self.seats[seat] = Occupant::Human { name, token: token.to_string(), connected: true };
        self.check_host();
        self.welcome(seat, token);
    }

    /// A player who came by the handle (a Steam friend's "Join Game"). At a
    /// public table, or with a token for a seat they already hold, they sit
    /// down and get `Some(seat)`. Otherwise their request, `id`, waits for
    /// the host (`None`): tell them [`ServerMsg::Waiting`].
    pub fn knock(
        &mut self,
        version: u32,
        name: &str,
        token: Option<&str>,
        id: u32,
        fresh_token: &str,
    ) -> Result<Option<Seat>, ErrorCode> {
        if version != PROTOCOL_VERSION {
            return Err(ErrorCode::Version);
        }
        let returning = token.is_some_and(|t| self.seat_of_token(t).is_some());
        if self.public || returning {
            return self.join(version, name, token, fresh_token).map(Some);
        }
        // Nobody is here to answer, and a private table stays private when it's empty.
        if self.connected() == 0 {
            return Err(ErrorCode::Declined);
        }
        if self.free_seat().is_none() || self.knocks.len() >= MAX_KNOCKS {
            return Err(ErrorCode::RoomFull);
        }
        self.knocks.retain(|k| k.id != id);
        self.knocks.push(Knock { id, name: clean_name(name), token: fresh_token.to_string() });
        self.send_knocks();
        Ok(None)
    }

    /// A player waiting to be let in gave up (their connection closed).
    pub fn cancel_knock(&mut self, id: u32) {
        let before = self.knocks.len();
        self.knocks.retain(|k| k.id != id);
        if self.knocks.len() != before {
            self.send_knocks();
        }
    }

    /// Lets request `id` in, if there's still a seat for them.
    fn admit(&mut self, id: u32) {
        let Some(i) = self.knocks.iter().position(|k| k.id == id) else { return };
        let knock = self.knocks.remove(i);
        match self.free_seat() {
            Some(seat) => {
                self.admitted.push((id, seat));
                self.seat_player(seat, knock.name, &knock.token);
            }
            None => self.declined.push(id),
        }
    }

    fn welcome(&mut self, seat: Seat, token: &str) {
        let msg = ServerMsg::Welcome { room: self.code.clone(), token: token.to_string(), handle: self.handle.clone() };
        self.outbox.push(Outgoing { to: seat, msg });
        self.broadcast_seats();
        if self.game.is_some() {
            self.send_sync(seat);
        }
        if self.host == Some(seat) && !self.knocks.is_empty() {
            self.send_knocks();
        }
    }

    /// Keeps the host's job with someone who is actually here, so requests to
    /// join always reach a person. With nobody here, the waiting players are
    /// turned away, and the job stays with whoever may come back first.
    fn check_host(&mut self) {
        let here = |o: &Occupant| matches!(o, Occupant::Human { connected: true, .. });
        if self.host.is_some_and(|h| here(&self.seats[h])) {
            return;
        }
        match JOIN_ORDER.iter().copied().find(|&s| here(&self.seats[s])) {
            Some(seat) => {
                self.host = Some(seat);
                if !self.knocks.is_empty() {
                    self.send_knocks();
                }
            }
            None => {
                let seated = |s: &Seat| matches!(self.seats[*s], Occupant::Human { .. });
                self.host = self.host.filter(seated).or_else(|| JOIN_ORDER.iter().copied().find(seated));
                self.declined.extend(self.knocks.drain(..).map(|k| k.id));
            }
        }
    }

    /// The player's connection closed. Their seat is kept for them, but someone
    /// else at the table takes over answering requests to join.
    pub fn disconnected(&mut self, seat: Seat) {
        if let Occupant::Human { connected, .. } = &mut self.seats[seat]
            && *connected
        {
            *connected = false;
            self.check_host();
            self.broadcast_seats();
        }
    }

    /// Handles a message from the player in absolute seat `seat`. `seed`
    /// shuffles a new game if this message starts one.
    pub fn handle(&mut self, seat: Seat, msg: ClientMsg, seed: u64) {
        if !matches!(self.seats[seat], Occupant::Human { .. }) {
            return;
        }
        match msg {
            ClientMsg::Hello { .. } => self.refuse(seat, ErrorCode::NotAllowed),
            ClientMsg::Sit { seat: to } => {
                let to = (seat + to) % 4;
                if self.playing() || self.seats[to] != Occupant::Bot {
                    self.refuse(seat, ErrorCode::NotAllowed);
                    return;
                }
                self.seats.swap(seat, to);
                if self.host == Some(seat) {
                    self.host = Some(to);
                }
                self.broadcast_seats();
            }
            ClientMsg::Start => {
                if self.playing() {
                    self.refuse(seat, ErrorCode::NotAllowed);
                    return;
                }
                self.game = Some(Match::new(self.target, self.stick_the_dealer, seed));
                self.game_no += 1;
                self.broadcast_seats();
                self.broadcast_deal();
                self.settle();
            }
            ClientMsg::Act { action } => {
                let Some(m) = self.game.as_mut().filter(|m| m.winner().is_none()) else {
                    self.refuse(seat, ErrorCode::NotAllowed);
                    return;
                };
                if let Err(e) = m.apply(seat, action) {
                    // Their game thought this was fine, so it's out of step: resend the hand.
                    self.refuse(seat, ErrorCode::Rule(e));
                    self.send_sync(seat);
                    return;
                }
                self.moves += 1;
                self.broadcast_move(seat, action, Some(seat));
                self.settle();
            }
            ClientMsg::Resync => {
                if self.game.is_some() {
                    self.send_sync(seat);
                }
            }
            ClientMsg::Leave => self.vacate(seat),
            ClientMsg::SetPublic { public } => {
                if self.host != Some(seat) {
                    self.refuse(seat, ErrorCode::NotAllowed);
                    return;
                }
                self.public = public;
                if public {
                    // Everyone already asking gets in (while there are seats).
                    for id in self.knocks.iter().map(|k| k.id).collect::<Vec<_>>() {
                        self.admit(id);
                    }
                    self.send_knocks();
                }
                self.broadcast_seats();
            }
            ClientMsg::Answer { id, accept } => {
                if self.host != Some(seat) {
                    self.refuse(seat, ErrorCode::NotAllowed);
                    return;
                }
                if accept {
                    self.admit(id);
                } else if let Some(i) = self.knocks.iter().position(|k| k.id == id) {
                    self.knocks.remove(i);
                    self.declined.push(id);
                }
                self.send_knocks();
            }
        }
    }

    /// A bot takes the seat, and its token stops working.
    fn vacate(&mut self, seat: Seat) {
        self.seats[seat] = Occupant::Bot;
        self.check_host();
        self.broadcast_seats();
        self.settle();
    }

    /// The human seat the game is waiting on, and a key for this turn: pass
    /// it to [`Room::force_move`] when their time runs out.
    pub fn waiting_on(&self) -> Option<(Seat, bool, u64)> {
        let m = self.game.as_ref().filter(|m| m.winner().is_none())?;
        let seat = m.round.to_act()?;
        match &self.seats[seat] {
            Occupant::Human { connected, .. } => Some((seat, *connected, self.moves)),
            Occupant::Bot => None,
        }
    }

    /// A bot makes the move for a human who ran out of time, if the turn
    /// `key` is still the one being waited on. Everyone, including that
    /// player, is told about the move.
    pub fn force_move(&mut self, key: u64) -> bool {
        let Some((seat, _, k)) = self.waiting_on() else { return false };
        if k != key {
            return false;
        }
        let m = self.game.as_mut().expect("waiting on a move");
        let action = ai::choose(&m.round, seat);
        if m.apply(seat, action).is_err() {
            return false;
        }
        self.moves += 1;
        self.broadcast_move(seat, action, None);
        self.settle();
        true
    }

    /// Replaces the game (an offline game resumed from its save) and tells the players.
    pub fn set_game(&mut self, mut m: Match) {
        if m.round.phase == Phase::HandOver {
            m.score_hand();
        }
        self.game = Some(m);
        self.game_no += 1;
        for seat in 0..4 {
            if matches!(self.seats[seat], Occupant::Human { .. }) {
                self.send_sync(seat);
            }
        }
        self.settle();
    }

    /// Runs everything that needs nobody's input: bots' moves, clearing
    /// tricks, scoring and dealing the next hand. Stops when a person is due
    /// to act or the game is over.
    fn settle(&mut self) {
        for _ in 0..10_000 {
            let Some(m) = self.game.as_mut() else { return };
            if m.winner().is_some() {
                return;
            }
            match m.round.phase {
                Phase::TrickDone => m.advance(),
                Phase::HandOver => {
                    m.score_hand();
                    if m.winner().is_some() {
                        self.broadcast_seats();
                        return;
                    }
                    m.next_hand();
                    self.broadcast_deal();
                }
                Phase::Misdeal => {
                    m.next_hand();
                    self.broadcast_deal();
                }
                _ => {
                    let Some(seat) = m.round.to_act() else { return };
                    if self.seats[seat] != Occupant::Bot {
                        return;
                    }
                    let action = ai::choose(&m.round, seat);
                    if let Err(e) = m.apply(seat, action) {
                        debug_assert!(false, "bot move refused: {e:?}");
                        return;
                    }
                    self.moves += 1;
                    self.broadcast_move(seat, action, None);
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Outgoing messages, each from its receiver's point of view
    // ------------------------------------------------------------------

    fn people(&self) -> Vec<Seat> {
        (0..4).filter(|&s| matches!(self.seats[s], Occupant::Human { .. })).collect()
    }

    fn refuse(&mut self, to: Seat, code: ErrorCode) {
        let msg = ServerMsg::Error { code, message: code.describe().into() };
        self.outbox.push(Outgoing { to, msg });
    }

    fn seat_info(&self, seat: Seat) -> SeatInfo {
        match &self.seats[seat] {
            Occupant::Bot => SeatInfo { name: BOT_NAMES[seat].into(), kind: SeatKind::Bot },
            Occupant::Human { name, connected, .. } => SeatInfo {
                name: name.clone(),
                kind: if *connected { SeatKind::Human } else { SeatKind::Away },
            },
        }
    }

    pub fn seats_for(&self, to: Seat) -> SeatsInfo {
        SeatsInfo {
            seats: std::array::from_fn(|r| self.seat_info((to + r) % 4)),
            playing: self.playing(),
            public: self.public,
            host: self.host.map(|h| rel(to, h)),
            target: self.target,
            stick_the_dealer: self.stick_the_dealer,
        }
    }

    /// Tells the host who is asking to join.
    fn send_knocks(&mut self) {
        let Some(host) = self.host else { return };
        let requests = self.knocks.iter().map(|k| KnockInfo { id: k.id, name: k.name.clone() }).collect();
        self.outbox.push(Outgoing { to: host, msg: ServerMsg::Knocks { requests } });
    }

    fn broadcast_seats(&mut self) {
        for to in self.people() {
            let msg = ServerMsg::Seats(self.seats_for(to));
            self.outbox.push(Outgoing { to, msg });
        }
    }

    /// The current hand as dealt, from `to`'s point of view.
    fn hand_for(&self, to: Seat) -> Option<HandStart> {
        let m = self.game.as_ref()?;
        let dealt = m.dealt_round();
        let mut scores = m.scores_before_hand();
        if team_of(to) == 1 {
            scores.swap(0, 1);
        }
        Some(HandStart {
            game: self.game_no,
            target: m.target,
            stick_the_dealer: m.stick_the_dealer,
            scores,
            hand_number: m.hand_number,
            dealer: rel(to, dealt.dealer),
            cards: dealt.hands[to].clone(),
            up_card: dealt.up_card,
        })
    }

    fn send_sync(&mut self, to: Seat) {
        let (Some(hand), Some(m)) = (self.hand_for(to), self.game.as_ref()) else { return };
        let moves = m
            .history()
            .into_iter()
            .map(|(seat, action)| Move { seat: rel(to, seat), action: Shown::of(action, seat == to) })
            .collect();
        self.outbox.push(Outgoing { to, msg: ServerMsg::Sync { hand, moves } });
    }

    fn broadcast_deal(&mut self) {
        for to in self.people() {
            if let Some(hand) = self.hand_for(to) {
                self.outbox.push(Outgoing { to, msg: ServerMsg::Deal(hand) });
            }
        }
    }

    /// Tells everyone but `except` (who made the move themselves) about it.
    fn broadcast_move(&mut self, seat: Seat, action: crate::round::Action, except: Option<Seat>) {
        for to in self.people() {
            if Some(to) == except {
                continue;
            }
            let m = Move { seat: rel(to, seat), action: Shown::of(action, seat == to) };
            self.outbox.push(Outgoing { to, msg: ServerMsg::Acted(m) });
        }
    }
}
