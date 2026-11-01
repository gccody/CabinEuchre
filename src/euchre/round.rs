//! One hand of Euchre: deal, two bidding rounds, the dealer's discard, five
//! tricks and the score. The state machine advances only through
//! [`Round::apply`] and [`Round::advance`], so the table scene can pause
//! between steps to animate.

use super::card::{Card, Suit};
use super::rng::Rng;

pub type Seat = usize;

/// Seats go clockwise: 0 = you (south), 1 = west, 2 = partner (north), 3 = east.
pub const SEATS: usize = 4;
pub const TRICKS_PER_HAND: u8 = 5;

pub fn next_seat(seat: Seat) -> Seat {
    (seat + 1) % SEATS
}

pub fn partner_of(seat: Seat) -> Seat {
    (seat + 2) % SEATS
}

/// Team 0 is seats 0 and 2, team 1 is seats 1 and 3.
pub fn team_of(seat: Seat) -> usize {
    seat % 2
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// First bidding round: each player may order the up card into the dealer's hand.
    OrderUp,
    /// The dealer picked up the up card and must discard one card.
    DealerDiscard,
    /// Second bidding round: name any suit except the turned-down one.
    NameTrump,
    /// Trick play.
    Play,
    /// A trick is complete; call [`Round::advance`] to clear it.
    TrickDone,
    /// All five tricks are played; read [`Round::result`].
    HandOver,
    /// Everyone passed twice (only possible without "stick the dealer").
    Misdeal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Pass,
    /// First round: make the up card's suit trump (the dealer picks it up).
    OrderUp { alone: bool },
    /// Second round: name trump.
    Call { suit: Suit, alone: bool },
    /// The dealer discards after picking up.
    Discard(Card),
    Play(Card),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleError {
    NotYourTurn,
    WrongPhase,
    CardNotInHand,
    MustFollowSuit,
    /// "Stick the dealer": the dealer may not pass in the second round.
    DealerMustCall,
    /// The turned-down suit cannot be named in the second round.
    SuitTurnedDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultKind {
    /// Makers took 3 or 4 tricks.
    Made,
    /// Makers took all 5.
    March,
    /// Makers took fewer than 3: defenders score.
    Euchred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandResult {
    pub maker: Seat,
    pub maker_team: usize,
    pub maker_tricks: u8,
    pub alone: bool,
    pub kind: ResultKind,
    /// The team that scores this hand.
    pub scoring_team: usize,
    pub points: u32,
}

#[derive(Debug, Clone)]
pub struct Round {
    pub dealer: Seat,
    pub hands: [Vec<Card>; SEATS],
    /// The face-down cards nobody plays with. Gains the up card when it is
    /// turned down, or the dealer's discard.
    pub kitty: Vec<Card>,
    pub up_card: Card,
    pub trump: Option<Suit>,
    pub maker: Option<Seat>,
    pub alone: bool,
    pub phase: Phase,
    /// Whose action it is (for `TrickDone`, the trick winner).
    pub turn: Seat,
    /// The trick in progress, in play order.
    pub trick: Vec<(Seat, Card)>,
    pub tricks_won: [u8; SEATS],
    /// Who won each completed trick, in order.
    pub trick_winners: Vec<Seat>,
    /// Every card played so far this hand, in order.
    pub played: Vec<Card>,
    pub discard: Option<Card>,
    pub stick_the_dealer: bool,
}

impl Round {
    /// Shuffles and deals a new hand: five cards each, one card turned up,
    /// three left in the kitty.
    pub fn deal(dealer: Seat, rng: &mut Rng, stick_the_dealer: bool) -> Round {
        let mut deck = Card::deck();
        rng.shuffle(&mut deck);
        let mut hands: [Vec<Card>; SEATS] = Default::default();
        for (i, card) in deck.drain(..20).enumerate() {
            // Deal starts left of the dealer.
            hands[(dealer + 1 + i % SEATS) % SEATS].push(card);
        }
        let up_card = deck.remove(0);
        Round::with_hands(dealer, hands, up_card, deck, stick_the_dealer)
    }

    /// Builds a round from known hands (used by tests and by `deal`).
    pub fn with_hands(
        dealer: Seat,
        hands: [Vec<Card>; SEATS],
        up_card: Card,
        kitty: Vec<Card>,
        stick_the_dealer: bool,
    ) -> Round {
        Round {
            dealer,
            hands,
            kitty,
            up_card,
            trump: None,
            maker: None,
            alone: false,
            phase: Phase::OrderUp,
            turn: next_seat(dealer),
            trick: Vec::new(),
            tricks_won: [0; SEATS],
            trick_winners: Vec::new(),
            played: Vec::new(),
            discard: None,
            stick_the_dealer,
        }
    }

    /// The seat expected to act, or `None` when the round is waiting on
    /// [`Round::advance`] or is over.
    pub fn to_act(&self) -> Option<Seat> {
        match self.phase {
            Phase::OrderUp | Phase::NameTrump | Phase::Play => Some(self.turn),
            Phase::DealerDiscard => Some(self.dealer),
            Phase::TrickDone | Phase::HandOver | Phase::Misdeal => None,
        }
    }

    /// The partner of a lone maker sits the hand out.
    pub fn is_sitting_out(&self, seat: Seat) -> bool {
        self.alone && self.maker.map(partner_of) == Some(seat)
    }

    pub fn active_players(&self) -> usize {
        if self.alone { 3 } else { 4 }
    }

    pub fn next_active(&self, seat: Seat) -> Seat {
        let mut s = next_seat(seat);
        while self.is_sitting_out(s) {
            s = next_seat(s);
        }
        s
    }

    pub fn tricks_played(&self) -> u8 {
        self.tricks_won.iter().sum()
    }

    pub fn team_tricks(&self, team: usize) -> u8 {
        self.tricks_won[team] + self.tricks_won[team + 2]
    }

    /// The effective suit led in the current trick.
    pub fn led_suit(&self) -> Option<Suit> {
        let trump = self.trump?;
        self.trick.first().map(|(_, c)| c.suit_under(trump))
    }

    /// The seat and card currently winning the trick.
    pub fn trick_leader(&self) -> Option<(Seat, Card)> {
        let trump = self.trump?;
        let led = self.led_suit()?;
        self.trick
            .iter()
            .copied()
            .max_by_key(|(_, c)| c.power(trump, led))
    }

    /// Cards `seat` may legally play right now (empty if it is not their turn to play).
    pub fn legal_plays(&self, seat: Seat) -> Vec<Card> {
        if self.phase != Phase::Play || self.turn != seat {
            return Vec::new();
        }
        let hand = &self.hands[seat];
        match self.led_suit() {
            None => hand.clone(),
            Some(led) => {
                let trump = self.trump.expect("trump is set during play");
                let follow: Vec<Card> = hand.iter().copied().filter(|c| c.suit_under(trump) == led).collect();
                if follow.is_empty() { hand.clone() } else { follow }
            }
        }
    }

    pub fn apply(&mut self, seat: Seat, action: Action) -> Result<(), RuleError> {
        if self.to_act() != Some(seat) {
            return Err(RuleError::NotYourTurn);
        }
        match (self.phase, action) {
            (Phase::OrderUp, Action::Pass) => {
                if seat == self.dealer {
                    // Turned down: the up card joins the kitty.
                    self.kitty.push(self.up_card);
                    self.phase = Phase::NameTrump;
                }
                self.turn = next_seat(seat);
            }
            (Phase::OrderUp, Action::OrderUp { alone }) => {
                self.set_trump(self.up_card.suit, seat, alone);
                if self.is_sitting_out(self.dealer) {
                    // The dealer's partner went alone: the dealer does not pick up.
                    self.kitty.push(self.up_card);
                    self.start_play();
                } else {
                    self.hands[self.dealer].push(self.up_card);
                    self.phase = Phase::DealerDiscard;
                    self.turn = self.dealer;
                }
            }
            (Phase::DealerDiscard, Action::Discard(card)) => {
                let hand = &mut self.hands[seat];
                let i = hand.iter().position(|&c| c == card).ok_or(RuleError::CardNotInHand)?;
                hand.remove(i);
                self.kitty.push(card);
                self.discard = Some(card);
                self.start_play();
            }
            (Phase::NameTrump, Action::Pass) => {
                if seat == self.dealer {
                    if self.stick_the_dealer {
                        return Err(RuleError::DealerMustCall);
                    }
                    self.phase = Phase::Misdeal;
                } else {
                    self.turn = next_seat(seat);
                }
            }
            (Phase::NameTrump, Action::Call { suit, alone }) => {
                if suit == self.up_card.suit {
                    return Err(RuleError::SuitTurnedDown);
                }
                self.set_trump(suit, seat, alone);
                self.start_play();
            }
            (Phase::Play, Action::Play(card)) => {
                if !self.hands[seat].contains(&card) {
                    return Err(RuleError::CardNotInHand);
                }
                if !self.legal_plays(seat).contains(&card) {
                    return Err(RuleError::MustFollowSuit);
                }
                self.hands[seat].retain(|&c| c != card);
                self.trick.push((seat, card));
                self.played.push(card);
                if self.trick.len() == self.active_players() {
                    let (winner, _) = self.trick_leader().expect("trick has cards");
                    self.tricks_won[winner] += 1;
                    self.trick_winners.push(winner);
                    self.turn = winner;
                    self.phase = Phase::TrickDone;
                } else {
                    self.turn = self.next_active(seat);
                }
            }
            _ => return Err(RuleError::WrongPhase),
        }
        Ok(())
    }

    /// Clears a finished trick. The winner leads the next one.
    pub fn advance(&mut self) {
        if self.phase != Phase::TrickDone {
            return;
        }
        self.trick.clear();
        if self.tricks_played() >= TRICKS_PER_HAND {
            self.phase = Phase::HandOver;
        } else {
            self.phase = Phase::Play;
        }
    }

    /// The outcome, once the hand is over.
    pub fn result(&self) -> Option<HandResult> {
        if self.phase != Phase::HandOver {
            return None;
        }
        let maker = self.maker?;
        let maker_team = team_of(maker);
        let maker_tricks = self.team_tricks(maker_team);
        let (kind, scoring_team, points) = if maker_tricks < 3 {
            (ResultKind::Euchred, 1 - maker_team, 2)
        } else if maker_tricks == TRICKS_PER_HAND {
            (ResultKind::March, maker_team, if self.alone { 4 } else { 2 })
        } else {
            (ResultKind::Made, maker_team, 1)
        };
        Some(HandResult {
            maker,
            maker_team,
            maker_tricks,
            alone: self.alone,
            kind,
            scoring_team,
            points,
        })
    }

    fn set_trump(&mut self, suit: Suit, maker: Seat, alone: bool) {
        self.trump = Some(suit);
        self.maker = Some(maker);
        self.alone = alone;
    }

    fn start_play(&mut self) {
        self.phase = Phase::Play;
        self.turn = self.next_active(self.dealer);
    }

    /// Cards whose location `seat` cannot know: the other players' hands and
    /// the unknown part of the kitty. Used by the AI for card counting.
    pub fn unseen_by(&self, seat: Seat) -> Vec<Card> {
        let turned_down = self.kitty.contains(&self.up_card);
        Card::deck()
            .into_iter()
            .filter(|c| {
                !self.played.contains(c)
                    && !self.hands[seat].contains(c)
                    && !(turned_down && *c == self.up_card)
                    && !(seat == self.dealer && self.discard == Some(*c))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::euchre::card::Rank;
    use Rank::*;
    use Suit::*;

    fn c(rank: Rank, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    /// Dealer 3 (east), so seat 0 bids and leads first.
    fn sample() -> Round {
        let hands = [
            vec![c(Jack, Hearts), c(Jack, Diamonds), c(Ace, Clubs), c(Nine, Spades), c(Ten, Hearts)],
            vec![c(Ace, Hearts), c(King, Spades), c(Queen, Spades), c(Nine, Clubs), c(Ten, Clubs)],
            vec![c(King, Hearts), c(Ace, Spades), c(Jack, Clubs), c(Queen, Clubs), c(Ten, Diamonds)],
            vec![c(Queen, Hearts), c(Ace, Diamonds), c(King, Diamonds), c(Jack, Spades), c(King, Clubs)],
        ];
        let up = c(Nine, Hearts);
        let kitty = vec![c(Ten, Spades), c(Nine, Diamonds), c(Queen, Diamonds)];
        Round::with_hands(3, hands, up, kitty, true)
    }

    #[test]
    fn deal_gives_five_each() {
        let mut rng = Rng::new(7);
        let r = Round::deal(0, &mut rng, true);
        for h in &r.hands {
            assert_eq!(h.len(), 5);
        }
        assert_eq!(r.kitty.len(), 3);
        let mut all: Vec<Card> = r.hands.iter().flatten().copied().collect();
        all.extend(&r.kitty);
        all.push(r.up_card);
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 24);
        assert_eq!(r.to_act(), Some(1));
    }

    #[test]
    fn order_up_puts_card_in_dealers_hand() {
        let mut r = sample();
        r.apply(0, Action::OrderUp { alone: false }).unwrap();
        assert_eq!(r.trump, Some(Hearts));
        assert_eq!(r.phase, Phase::DealerDiscard);
        assert_eq!(r.to_act(), Some(3));
        assert_eq!(r.hands[3].len(), 6);
        assert_eq!(r.apply(3, Action::Discard(c(Ace, Clubs))), Err(RuleError::CardNotInHand));
        r.apply(3, Action::Discard(c(King, Clubs))).unwrap();
        assert_eq!(r.hands[3].len(), 5);
        assert_eq!(r.phase, Phase::Play);
        assert_eq!(r.to_act(), Some(0), "left of dealer leads");
    }

    #[test]
    fn turn_down_then_name_trump() {
        let mut r = sample();
        for s in [0, 1, 2, 3] {
            r.apply(s, Action::Pass).unwrap();
        }
        assert_eq!(r.phase, Phase::NameTrump);
        assert!(r.kitty.contains(&c(Nine, Hearts)));
        assert_eq!(r.apply(0, Action::Call { suit: Hearts, alone: false }), Err(RuleError::SuitTurnedDown));
        r.apply(0, Action::Pass).unwrap();
        r.apply(1, Action::Pass).unwrap();
        r.apply(2, Action::Pass).unwrap();
        assert_eq!(r.apply(3, Action::Pass), Err(RuleError::DealerMustCall));
        r.apply(3, Action::Call { suit: Spades, alone: false }).unwrap();
        assert_eq!(r.trump, Some(Spades));
        assert_eq!(r.phase, Phase::Play);
    }

    #[test]
    fn misdeal_without_stick_the_dealer() {
        let mut r = sample();
        r.stick_the_dealer = false;
        for s in [0, 1, 2, 3, 0, 1, 2, 3] {
            r.apply(s, Action::Pass).unwrap();
        }
        assert_eq!(r.phase, Phase::Misdeal);
        assert_eq!(r.to_act(), None);
    }

    #[test]
    fn left_bower_must_follow_trump_not_its_printed_suit() {
        let mut r = sample();
        r.apply(0, Action::OrderUp { alone: false }).unwrap();
        r.apply(3, Action::Discard(c(King, Clubs))).unwrap();
        // Hearts are trump, so seat 0's J♦ is the left bower: leading it leads trump.
        r.apply(0, Action::Play(c(Jack, Diamonds))).unwrap();
        assert_eq!(r.led_suit(), Some(Hearts));
        // Seat 1 has A♥ (trump) and must play it.
        assert_eq!(r.legal_plays(1), vec![c(Ace, Hearts)]);
        assert_eq!(r.apply(1, Action::Play(c(King, Spades))), Err(RuleError::MustFollowSuit));
        r.apply(1, Action::Play(c(Ace, Hearts))).unwrap();
        // Seat 2 has K♥.
        assert_eq!(r.legal_plays(2), vec![c(King, Hearts)]);
        r.apply(2, Action::Play(c(King, Hearts))).unwrap();
        // Seat 3 (dealer) picked up 9♥ and holds Q♥.
        let mut legal = r.legal_plays(3);
        legal.sort();
        assert_eq!(legal, vec![c(Nine, Hearts), c(Queen, Hearts)]);
        r.apply(3, Action::Play(c(Queen, Hearts))).unwrap();
        assert_eq!(r.phase, Phase::TrickDone);
        assert_eq!(r.turn, 0, "left bower beats the ace");
        r.advance();
        assert_eq!(r.phase, Phase::Play);
        assert_eq!(r.to_act(), Some(0));
    }

    #[test]
    fn leading_a_suit_does_not_force_the_left_bower() {
        // Diamonds led, hearts trump: J♦ is a heart now and does not follow diamonds.
        let hands = [
            vec![c(Ace, Diamonds)],
            vec![c(Jack, Diamonds), c(Nine, Clubs)],
            vec![c(Nine, Spades)],
            vec![c(Ten, Spades)],
        ];
        let mut r = Round::with_hands(3, hands, c(Nine, Hearts), vec![], true);
        r.trump = Some(Hearts);
        r.maker = Some(0);
        r.phase = Phase::Play;
        r.turn = 0;
        r.apply(0, Action::Play(c(Ace, Diamonds))).unwrap();
        let mut legal = r.legal_plays(1);
        legal.sort();
        assert_eq!(legal, vec![c(Nine, Clubs), c(Jack, Diamonds)], "void in diamonds: anything goes");
    }

    #[test]
    fn going_alone_skips_partner() {
        let mut r = sample();
        r.apply(0, Action::OrderUp { alone: true }).unwrap();
        assert!(r.is_sitting_out(2));
        assert_eq!(r.active_players(), 3);
        r.apply(3, Action::Discard(c(King, Clubs))).unwrap();
        assert_eq!(r.to_act(), Some(0));
        r.apply(0, Action::Play(c(Ace, Clubs))).unwrap();
        assert_eq!(r.to_act(), Some(1));
        r.apply(1, Action::Play(c(Nine, Clubs))).unwrap();
        assert_eq!(r.to_act(), Some(3), "partner (seat 2) is skipped");
        r.apply(3, Action::Play(c(Jack, Spades))).unwrap();
        assert_eq!(r.phase, Phase::TrickDone);
        assert_eq!(r.turn, 0, "A♣ wins: no trump played");
    }

    #[test]
    fn dealer_does_not_pick_up_when_partner_goes_alone() {
        let mut r = sample(); // dealer 3, partner 1
        r.apply(0, Action::Pass).unwrap();
        r.apply(1, Action::OrderUp { alone: true }).unwrap();
        assert!(r.is_sitting_out(3));
        assert_eq!(r.phase, Phase::Play);
        assert_eq!(r.hands[3].len(), 5);
        assert_eq!(r.to_act(), Some(0));
    }

    fn finish(r: &mut Round, maker_tricks: u8, alone: bool) -> HandResult {
        r.trump = Some(Hearts);
        r.maker = Some(0);
        r.alone = alone;
        r.tricks_won = [maker_tricks, 5 - maker_tricks, 0, 0];
        r.phase = Phase::HandOver;
        r.result().unwrap()
    }

    #[test]
    fn scoring() {
        let mut r = sample();
        let res = finish(&mut r, 3, false);
        assert_eq!((res.kind, res.scoring_team, res.points), (ResultKind::Made, 0, 1));
        let res = finish(&mut r, 5, false);
        assert_eq!((res.kind, res.points), (ResultKind::March, 2));
        let res = finish(&mut r, 5, true);
        assert_eq!((res.kind, res.points), (ResultKind::March, 4));
        let res = finish(&mut r, 4, true);
        assert_eq!((res.kind, res.points), (ResultKind::Made, 1));
        let res = finish(&mut r, 2, false);
        assert_eq!((res.kind, res.scoring_team, res.points), (ResultKind::Euchred, 1, 2));
    }

    #[test]
    fn out_of_turn_is_rejected() {
        let mut r = sample();
        assert_eq!(r.apply(2, Action::Pass), Err(RuleError::NotYourTurn));
        assert_eq!(r.apply(0, Action::Play(c(Jack, Hearts))), Err(RuleError::WrongPhase));
    }
}
