//! One player's copy of the hand being played, built only from what the room
//! tells them (see `protocol.rs`).
//!
//! The table needs a whole [`Round`] to lay out and animate, but a player
//! only knows their own cards. So the mirror deals the cards they can't see
//! to the other hands and the kitty as *guesses*, and fixes the guesses up
//! as the truth comes out: when a seat plays a card the mirror had put
//! somewhere else (another hand, the kitty), the two hidden cards trade
//! places first. Both are face down, so the table swaps their sprites and
//! nobody sees a thing.
//!
//! The player's own hand, the up card, trump and every played card are
//! always exact, so the rules give the same answers here as in the room:
//! your legal plays, who won each trick and the score all match.

use crate::card::Card;
use crate::protocol::{HandStart, Move, Shown};
use crate::round::{Action, HandResult, Phase, Round, RuleError, Seat};

/// The player is always seat 0 in their own mirror.
pub const ME: Seat = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorError {
    /// The rules refused a move the room sent.
    Rule(RuleError),
    /// A card turned up that this mirror can't account for.
    Card(Card),
    /// A malformed hand (wrong number of cards, duplicates).
    BadHand,
}

impl From<RuleError> for MirrorError {
    fn from(e: RuleError) -> Self {
        MirrorError::Rule(e)
    }
}

/// Two hidden cards that traded places in the mirror (see the module docs).
pub type Swap = (Card, Card);

#[derive(Debug, Clone)]
pub struct Mirror {
    /// Which game in the room this hand belongs to.
    pub game: u32,
    /// Points per team, yours first.
    pub scores: [u32; 2],
    pub target: u32,
    pub hand_number: u32,
    pub round: Round,
    scored: bool,
    /// Moves applied this hand (a fresh deal has none).
    moves: usize,
}

impl Mirror {
    /// A freshly dealt hand: your cards are real, everyone else's are guesses.
    pub fn new(hand: &HandStart) -> Result<Mirror, MirrorError> {
        let mut known = hand.cards.clone();
        known.push(hand.up_card);
        known.sort();
        known.dedup();
        if hand.cards.len() != 5 || known.len() != 6 || hand.dealer > 3 {
            return Err(MirrorError::BadHand);
        }
        let mut unknown: Vec<Card> = Card::deck().into_iter().filter(|c| !known.contains(c)).collect();
        let mut hands: [Vec<Card>; 4] = Default::default();
        hands[ME] = hand.cards.clone();
        for h in hands.iter_mut().skip(1) {
            *h = unknown.drain(..5).collect();
        }
        let round = Round::with_hands(hand.dealer, hands, hand.up_card, unknown, hand.stick_the_dealer);
        Ok(Mirror {
            game: hand.game,
            scores: hand.scores,
            target: hand.target.max(1),
            hand_number: hand.hand_number,
            round,
            scored: false,
            moves: 0,
        })
    }

    /// A hand in progress: dealt, then every move so far replayed (clearing
    /// finished tricks along the way).
    pub fn replay(hand: &HandStart, moves: &[Move]) -> Result<Mirror, MirrorError> {
        let mut m = Mirror::new(hand)?;
        for &mv in moves {
            if m.round.phase == Phase::TrickDone {
                m.advance();
            }
            m.reveal(mv)?;
        }
        Ok(m)
    }

    /// Your own move, checked exactly as the room will check it.
    pub fn act(&mut self, action: Action) -> Result<(), RuleError> {
        self.round.apply(ME, action)?;
        self.moves += 1;
        Ok(())
    }

    /// Applies a move the room reported. Returns the move with any hidden
    /// card filled in (a guessed discard) and the hidden cards that had to
    /// trade places for it, which the table mirrors on its sprites.
    pub fn reveal(&mut self, mv: Move) -> Result<(Action, Vec<Swap>), MirrorError> {
        let seat = mv.seat;
        if seat > 3 {
            return Err(MirrorError::BadHand);
        }
        let mut swaps = Vec::new();
        let action = match mv.action {
            Shown::Pass => Action::Pass,
            Shown::OrderUp { alone } => Action::OrderUp { alone },
            Shown::Call { suit, alone } => Action::Call { suit, alone },
            Shown::Discard { card: Some(card) } => {
                swaps.extend(self.bring_to_hand(seat, card)?);
                Action::Discard { card }
            }
            Shown::Discard { card: None } => {
                if seat == ME {
                    return Err(MirrorError::BadHand);
                }
                // Any hidden card will do; keeping the up card in hand is the common case.
                let hand = &self.round.hands[seat];
                let card = hand
                    .iter()
                    .copied()
                    .find(|&c| c != self.round.up_card)
                    .or(hand.first().copied())
                    .ok_or(MirrorError::BadHand)?;
                Action::Discard { card }
            }
            Shown::Play { card } => {
                swaps.extend(self.bring_to_hand(seat, card)?);
                Action::Play { card }
            }
        };
        if seat == ME {
            // A move made for you (your turn timed out): your cards are real, so check it fully.
            self.round.apply(ME, action)?;
        } else {
            self.round.apply_revealed(seat, action)?;
        }
        self.moves += 1;
        Ok((action, swaps))
    }

    /// Makes sure `card` is in `seat`'s guessed hand, trading it with one of
    /// their hidden cards if the mirror had guessed it elsewhere.
    fn bring_to_hand(&mut self, seat: Seat, card: Card) -> Result<Option<Swap>, MirrorError> {
        let r = &mut self.round;
        if r.hands[seat].contains(&card) {
            return Ok(None);
        }
        // Only hidden cards can be moved: never yours, a played card, or a
        // card known to be in the kitty (the turned-down up card, your discard).
        let public_kitty = (card == r.up_card && r.discard.is_none()) || (r.dealer == ME && r.discard == Some(card));
        if seat == ME || r.hands[ME].contains(&card) || r.played.contains(&card) || public_kitty {
            return Err(MirrorError::Card(card));
        }
        let up = r.up_card;
        let i = r.hands[seat]
            .iter()
            .position(|&c| c != up)
            .or((!r.hands[seat].is_empty()).then_some(0))
            .ok_or(MirrorError::Card(card))?;
        let other = r.hands[seat][i];
        let found = (1..4)
            .filter(|&t| t != seat)
            .find_map(|t| r.hands[t].iter().position(|&c| c == card).map(|j| (Some(t), j)))
            .or_else(|| r.kitty.iter().position(|&c| c == card).map(|j| (None, j)));
        match found {
            Some((Some(t), j)) => r.hands[t][j] = other,
            Some((None, j)) => {
                r.kitty[j] = other;
                if r.discard == Some(card) {
                    r.discard = Some(other);
                }
            }
            None => return Err(MirrorError::Card(card)),
        }
        r.hands[seat][i] = card;
        Ok(Some((card, other)))
    }

    /// Clears a finished trick, as the room already has.
    pub fn advance(&mut self) {
        self.round.advance();
    }

    /// Adds the finished hand's points (once).
    pub fn score_hand(&mut self) -> Option<HandResult> {
        let result = self.round.result()?;
        if !self.scored {
            self.scores[result.scoring_team] += result.points;
            self.scored = true;
        }
        Some(result)
    }

    pub fn winner(&self) -> Option<usize> {
        (0..2).find(|&t| self.scores[t] >= self.target)
    }

    /// Whether nothing has happened yet this hand.
    pub fn hand_untouched(&self) -> bool {
        self.moves == 0
    }

    /// How many moves have been applied this hand.
    pub fn moves(&self) -> usize {
        self.moves
    }
}
