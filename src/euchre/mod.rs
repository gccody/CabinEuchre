//! The rules of Euchre, with no rendering or framework code, so everything
//! here can be unit-tested. Scenes only read this state and forward input.
//!
//! - `card`  suits, ranks and trump-aware card power (right/left bower)
//! - `round` one hand: bidding, the dealer's discard, tricks and the result
//! - `game`  a match played to a target score
//! - `ai`    heuristic players for the computer seats
//! - `rng`   a small seeded RNG

pub mod ai;
pub mod card;
pub mod game;
pub mod rng;
pub mod round;

pub use card::{Card, Rank, Suit};
pub use game::Match;
pub use round::{Action, HandResult, Phase, ResultKind, Round, RuleError, Seat, partner_of, team_of};
