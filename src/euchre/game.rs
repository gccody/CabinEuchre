//! A full game: hands are dealt in turn until a team reaches the target score.
//!
//! A `Match` remembers how the current hand was dealt (the RNG state just
//! before the shuffle) and every step taken since, so it can be saved as a
//! tiny snapshot and rebuilt exactly by replaying those steps (see `save.rs`).

use super::rng::Rng;
use super::round::{Action, HandResult, Phase, Round, RuleError, Seat, next_seat};

/// One thing that happened in the current hand, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// A player's action, by whoever was due to act.
    Act(Action),
    /// A finished trick was cleared ([`Round::advance`]).
    Advance,
}

#[derive(Debug, Clone)]
pub struct Match {
    /// Points per team (team 0 = you and your partner).
    pub scores: [u32; 2],
    pub target: u32,
    pub stick_the_dealer: bool,
    pub round: Round,
    /// 1-based hand counter (misdeals don't count).
    pub hand_number: u32,
    /// Identifies this game (its seed), so a save file can tell whether it
    /// belongs to the game being played.
    pub id: u64,
    pub(super) rng: Rng,
    /// The RNG just before the current hand was shuffled.
    pub(super) hand_rng: Rng,
    /// Everything done in the current hand, so it can be replayed.
    pub(super) log: Vec<Step>,
    pub(super) scored: bool,
}

impl Match {
    pub fn new(target: u32, stick_the_dealer: bool, seed: u64) -> Match {
        let mut rng = Rng::new(seed);
        let dealer = rng.below(4);
        Match::starting_hand([0, 0], target, stick_the_dealer, 1, dealer, seed, rng)
    }

    /// A match at the start of a hand: `rng` is about to shuffle it.
    pub(super) fn starting_hand(
        scores: [u32; 2],
        target: u32,
        stick_the_dealer: bool,
        hand_number: u32,
        dealer: Seat,
        id: u64,
        mut rng: Rng,
    ) -> Match {
        let hand_rng = rng.clone();
        let round = Round::deal(dealer, &mut rng, stick_the_dealer);
        Match {
            scores,
            target,
            stick_the_dealer,
            round,
            hand_number,
            id,
            rng,
            hand_rng,
            log: Vec::new(),
            scored: false,
        }
    }

    /// Applies an action to the current hand (see [`Round::apply`]) and
    /// remembers it for saving.
    pub fn apply(&mut self, seat: Seat, action: Action) -> Result<(), RuleError> {
        self.round.apply(seat, action)?;
        self.log.push(Step::Act(action));
        Ok(())
    }

    /// Clears a finished trick (see [`Round::advance`]).
    pub fn advance(&mut self) {
        if self.round.phase == Phase::TrickDone {
            self.round.advance();
            self.log.push(Step::Advance);
        }
    }

    /// Adds the finished hand's points to the score. Safe to call more than
    /// once; points are only added the first time.
    pub fn score_hand(&mut self) -> Option<HandResult> {
        let result = self.round.result()?;
        if !self.scored {
            self.scores[result.scoring_team] += result.points;
            self.scored = true;
        }
        Some(result)
    }

    /// The deal passes to the left and a new hand is dealt. Call after
    /// [`Match::score_hand`], or after a misdeal.
    pub fn next_hand(&mut self) {
        if self.round.phase != Phase::Misdeal {
            self.hand_number += 1;
        }
        let dealer = next_seat(self.round.dealer);
        self.hand_rng = self.rng.clone();
        self.round = Round::deal(dealer, &mut self.rng, self.stick_the_dealer);
        self.log.clear();
        self.scored = false;
    }

    /// Whether nothing has happened yet in the current hand.
    pub fn hand_untouched(&self) -> bool {
        self.log.is_empty()
    }

    /// The winning team, once someone reaches the target.
    pub fn winner(&self) -> Option<usize> {
        (0..2).find(|&t| self.scores[t] >= self.target)
    }
}
