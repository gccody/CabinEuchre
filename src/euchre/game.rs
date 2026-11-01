//! A full game: hands are dealt in turn until a team reaches the target score.

use super::rng::Rng;
use super::round::{HandResult, Phase, Round, next_seat};

#[derive(Debug, Clone)]
pub struct Match {
    /// Points per team (team 0 = you and your partner).
    pub scores: [u32; 2],
    pub target: u32,
    pub stick_the_dealer: bool,
    pub round: Round,
    /// 1-based hand counter (misdeals don't count).
    pub hand_number: u32,
    rng: Rng,
    scored: bool,
}

impl Match {
    pub fn new(target: u32, stick_the_dealer: bool, seed: u64) -> Match {
        let mut rng = Rng::new(seed);
        let dealer = rng.below(4);
        let round = Round::deal(dealer, &mut rng, stick_the_dealer);
        Match {
            scores: [0, 0],
            target,
            stick_the_dealer,
            round,
            hand_number: 1,
            rng,
            scored: false,
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
        self.round = Round::deal(dealer, &mut self.rng, self.stick_the_dealer);
        self.scored = false;
    }

    /// The winning team, once someone reaches the target.
    pub fn winner(&self) -> Option<usize> {
        (0..2).find(|&t| self.scores[t] >= self.target)
    }
}
