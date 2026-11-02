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

use serde::{Deserialize, Serialize};

use crate::ai;
use crate::game::Match;
use crate::protocol::{
    ClientMsg, ErrorCode, HandStart, Move, PROTOCOL_VERSION, SeatInfo, SeatKind, SeatsInfo, ServerMsg, Shown,
    clean_name,
};
use crate::round::{Phase, Seat, team_of};
use crate::save::LoadError;

/// Computer players' names, by absolute seat.
pub const BOT_NAMES: [&str; 4] = ["OTTO", "ROSA", "BENNY", "JULES"];

/// Where joining players sit: the first two are partners.
const JOIN_ORDER: [Seat; 4] = [0, 2, 1, 3];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Occupant {
    Bot,
    Human { name: String, token: String, connected: bool },
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
    seats: [Occupant; 4],
    game: Option<Match>,
    /// Games dealt in this room so far.
    game_no: u32,
    target: u32,
    stick_the_dealer: bool,
    /// Counts every move, so a turn timer can tell whether the turn it was
    /// set for is still the one being waited on.
    moves: u64,
    outbox: Vec<Outgoing>,
}

/// Everything a room needs to carry on after the server forgets it (a
/// Durable Object hibernating or restarting). The match is kept in the save
/// format, which is replayed through the rules on load.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomSnapshot {
    pub code: String,
    pub seats: [Occupant; 4],
    pub game: Option<String>,
    pub game_no: u32,
    pub target: u32,
    pub stick_the_dealer: bool,
    pub moves: u64,
}

fn rel(to: Seat, seat: Seat) -> Seat {
    (seat + 4 - to) % 4
}

impl Room {
    pub fn new(code: &str, target: u32, stick_the_dealer: bool) -> Room {
        Room {
            code: code.to_string(),
            seats: std::array::from_fn(|_| Occupant::Bot),
            game: None,
            game_no: 0,
            target: target.max(1),
            stick_the_dealer,
            moves: 0,
            outbox: Vec::new(),
        }
    }

    pub fn snapshot(&self) -> RoomSnapshot {
        RoomSnapshot {
            code: self.code.clone(),
            seats: self.seats.clone(),
            game: self.game.as_ref().map(Match::to_save),
            game_no: self.game_no,
            target: self.target,
            stick_the_dealer: self.stick_the_dealer,
            moves: self.moves,
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
            seats: s.seats,
            game,
            game_no: s.game_no,
            target: s.target.max(1),
            stick_the_dealer: s.stick_the_dealer,
            moves: s.moves,
            outbox: Vec::new(),
        })
    }

    /// The messages produced since the last call, in order.
    pub fn take_outbox(&mut self) -> Vec<Outgoing> {
        std::mem::take(&mut self.outbox)
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

    pub fn seat_of_token(&self, token: &str) -> Option<Seat> {
        self.seats
            .iter()
            .position(|o| matches!(o, Occupant::Human { token: t, .. } if t == token))
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
            self.welcome(seat, &token);
            return Ok(seat);
        }
        let seat = *JOIN_ORDER
            .iter()
            .find(|&&s| self.seats[s] == Occupant::Bot)
            .ok_or(ErrorCode::RoomFull)?;
        self.seats[seat] = Occupant::Human { name, token: fresh_token.to_string(), connected: true };
        self.welcome(seat, fresh_token);
        Ok(seat)
    }

    fn welcome(&mut self, seat: Seat, token: &str) {
        let msg = ServerMsg::Welcome { room: self.code.clone(), token: token.to_string() };
        self.outbox.push(Outgoing { to: seat, msg });
        self.broadcast_seats();
        if self.game.is_some() {
            self.send_sync(seat);
        }
    }

    /// The player's connection closed. Their seat is kept for them.
    pub fn disconnected(&mut self, seat: Seat) {
        if let Occupant::Human { connected, .. } = &mut self.seats[seat]
            && *connected
        {
            *connected = false;
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
            ClientMsg::Leave => {
                self.seats[seat] = Occupant::Bot;
                self.broadcast_seats();
                self.settle();
            }
        }
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
            target: self.target,
            stick_the_dealer: self.stick_the_dealer,
        }
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
