//! The rules of Euchre, with no rendering or framework code, so everything
//! here can be unit-tested, and the same code runs in the game and on the
//! online server (compiled to wasm). Scenes only read this state and forward input.
//!
//! - `card`  suits, ranks and trump-aware card power (right/left bower)
//! - `round` one hand: bidding, the dealer's discard, tricks and the result
//! - `game`  a match played to a target score
//! - `ai`    heuristic players for the computer seats
//! - `rng`   a small seeded RNG
//! - `save`  a match in progress as a text snapshot (replayed on load)
//! - `protocol` the online messages between a player's game and a room
//! - `room`  four seats around a [`Match`]: runs online rooms on the server, and offline games
//! - `mirror` one player's view of a hand, with the cards they can't see guessed
//! - `tutorial` the room behind the tutorial: a match dealt from a script
//! - `advice` what to do here and why, in words: the tutorial's tips and comments

pub mod advice;
pub mod ai;
pub mod card;
pub mod game;
pub mod mirror;
pub mod protocol;
pub mod rng;
pub mod room;
pub mod round;
pub mod save;
pub mod tutorial;

#[cfg(test)]
mod online_tests;

pub use card::{Card, Rank, Suit};
pub use game::Match;
pub use mirror::Mirror;
pub use round::{Action, HandResult, Phase, ResultKind, Round, RuleError, Seat, partner_of, team_of};
