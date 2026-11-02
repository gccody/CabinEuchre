//! The online protocol: JSON messages between a player's game and a room on
//! the server (`server/`, a Cloudflare Durable Object), one per WebSocket
//! text frame.
//!
//! Every message is written from the receiving player's point of view: they
//! are always seat 0 (south) and their team is team 0, so the table scene
//! draws an online game exactly like an offline one. The room turns absolute
//! seats into relative ones for each player (see `room.rs`).
//!
//! Hidden information never leaves the room: a player is only ever sent their
//! own cards, plus each card as it is played. The dealer's discard is shown
//! only to the dealer.

use serde::{Deserialize, Serialize};

use crate::card::{Card, Suit};
use crate::round::{Action, RuleError, Seat};

/// Bumped whenever a message changes shape. A room refuses other versions.
pub const PROTOCOL_VERSION: u32 = 1;

/// Longest player name, in characters.
pub const MAX_NAME_LEN: usize = 10;

/// Letters a room code is made from: no I or O, which read like 1 and 0.
pub const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
pub const CODE_LEN: usize = 5;

/// From a player's game to their room.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    /// First message on every connection. `token` reclaims a seat this
    /// player held before (after a dropped connection or a restart).
    Hello { version: u32, name: String, token: Option<String> },
    /// Move to another seat (relative), while no game is being played.
    Sit { seat: Seat },
    /// Deal a new game, from the lobby or after a game ends.
    Start,
    /// Your bid, discard or card.
    Act { action: Action },
    /// Something didn't add up: send me the whole hand again.
    Resync,
    /// Leaving for good: a bot takes the seat.
    Leave,
}

/// From a room to one player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    /// You're in. Keep `token` to get your seat back if you reconnect.
    Welcome { room: String, token: String },
    /// Who sits where (relative seats), sent whenever that changes.
    Seats(SeatsInfo),
    /// A new hand was dealt.
    Deal(HandStart),
    /// The hand so far, for a player who just (re)joined or asked to resync.
    Sync { hand: HandStart, moves: Vec<Move> },
    /// Another seat bid, discarded or played.
    Acted(Move),
    /// A request was refused.
    Error { code: ErrorCode, message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatsInfo {
    pub seats: [SeatInfo; 4],
    /// A game is being played (seats are fixed until it ends).
    pub playing: bool,
    pub target: u32,
    pub stick_the_dealer: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatInfo {
    pub name: String,
    pub kind: SeatKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeatKind {
    /// The computer plays this seat. Anyone joining takes a bot's seat.
    Bot,
    Human,
    /// A player whose connection dropped. Their seat waits for them; if
    /// they're due to act for too long, a bot moves for them.
    Away,
}

/// The start of a hand, from one player's point of view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandStart {
    /// Counts games played in this room, so a new game can be told apart from a new hand.
    pub game: u32,
    pub target: u32,
    pub stick_the_dealer: bool,
    /// Points before this hand: yours first.
    pub scores: [u32; 2],
    pub hand_number: u32,
    pub dealer: Seat,
    /// Your five cards, as dealt.
    pub cards: Vec<Card>,
    pub up_card: Card,
}

/// One seat's action, as another player sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Move {
    pub seat: Seat,
    pub action: Shown,
}

/// An action with hidden parts left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shown {
    Pass,
    OrderUp { alone: bool },
    Call { suit: Suit, alone: bool },
    /// The dealer's discard: the card is only shown to the dealer.
    Discard { card: Option<Card> },
    Play { card: Card },
}

impl Shown {
    /// `action` as seen by a player who may (`visible`) or may not see a discard.
    pub fn of(action: Action, visible: bool) -> Shown {
        match action {
            Action::Pass => Shown::Pass,
            Action::OrderUp { alone } => Shown::OrderUp { alone },
            Action::Call { suit, alone } => Shown::Call { suit, alone },
            Action::Discard { card } => Shown::Discard { card: visible.then_some(card) },
            Action::Play { card } => Shown::Play { card },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// No room has that code.
    NoSuchRoom,
    /// Four players are already seated.
    RoomFull,
    /// The game and the server speak different protocol versions.
    Version,
    /// Not possible right now (moving seats mid-game, starting twice...).
    NotAllowed,
    /// A bid or card the rules refuse.
    Rule(RuleError),
    /// A message that couldn't be read.
    BadMessage,
}

impl ErrorCode {
    /// A short explanation for players.
    pub fn describe(self) -> &'static str {
        match self {
            ErrorCode::NoSuchRoom => "No table has that code.",
            ErrorCode::RoomFull => "That table is full.",
            ErrorCode::Version => "Please update the game to play online.",
            ErrorCode::NotAllowed => "You can't do that right now.",
            ErrorCode::Rule(_) => "That move isn't allowed.",
            ErrorCode::BadMessage => "The server didn't understand that.",
        }
    }
}

/// A player name as the room will show it: the characters the pixel font
/// draws, upper case, trimmed to [`MAX_NAME_LEN`]. Empty names become "PLAYER".
pub fn clean_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| c.to_ascii_uppercase())
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '.' | '\''))
        .take(MAX_NAME_LEN)
        .collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() { "PLAYER".into() } else { cleaned }
}

/// Upper-cases `code` and checks it could be a room code.
pub fn clean_code(code: &str) -> Option<String> {
    let code = code.trim().to_ascii_uppercase();
    (code.len() == CODE_LEN && code.bytes().all(|b| CODE_ALPHABET.contains(&b))).then_some(code)
}

/// A room code from random numbers (one per letter).
pub fn make_code(mut random: impl FnMut() -> u32) -> String {
    (0..CODE_LEN)
        .map(|_| CODE_ALPHABET[random() as usize % CODE_ALPHABET.len()] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Rank;

    #[test]
    fn messages_round_trip_as_json() {
        let card = Card::new(Rank::Jack, Suit::Hearts);
        let msgs = vec![
            ServerMsg::Acted(Move { seat: 1, action: Shown::Play { card } }),
            ServerMsg::Acted(Move { seat: 3, action: Shown::Discard { card: None } }),
            ServerMsg::Error { code: ErrorCode::Rule(RuleError::MustFollowSuit), message: "x".into() },
            ServerMsg::Deal(HandStart {
                game: 1,
                target: 10,
                stick_the_dealer: true,
                scores: [0, 3],
                hand_number: 4,
                dealer: 2,
                cards: vec![card],
                up_card: card,
            }),
        ];
        for m in msgs {
            let json = serde_json::to_string(&m).unwrap();
            assert_eq!(serde_json::from_str::<ServerMsg>(&json).unwrap(), m, "{json}");
        }
        let json = serde_json::to_string(&ClientMsg::Act { action: Action::Play { card } }).unwrap();
        assert_eq!(json, r#"{"type":"act","action":{"kind":"play","card":"Jh"}}"#);
        let back: ClientMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ClientMsg::Act { action: Action::Play { card } });
    }

    #[test]
    fn names_and_codes() {
        assert_eq!(clean_name("  rosa♥ <b>  "), "ROSA B");
        assert_eq!(clean_name("a very long name indeed"), "A VERY LON");
        assert_eq!(clean_name("   "), "PLAYER");
        assert_eq!(clean_code(" kqjta "), Some("KQJTA".into()));
        assert_eq!(clean_code("KQJT0"), None);
        assert_eq!(clean_code("KQJ"), None);
        let mut n = 0;
        let code = make_code(|| {
            n += 7;
            n
        });
        assert!(clean_code(&code).is_some(), "{code}");
    }
}
