//! Heuristic computer players. They only look at what their seat could know:
//! their own hand, the up card, trump, who made it, and the cards played so far.

use super::card::{Card, Rank, Suit};
use super::round::{Action, Phase, Round, Seat, partner_of, team_of};

/// Points needed (see [`hand_strength`]) to make trump.
const CALL_THRESHOLD: f32 = 6.3;
/// Points needed to go alone.
const ALONE_THRESHOLD: f32 = 11.0;

/// Picks an action for `seat`, which must be the seat to act.
pub fn choose(round: &Round, seat: Seat) -> Action {
    match round.phase {
        Phase::OrderUp => order_up(round, seat),
        Phase::NameTrump => name_trump(round, seat),
        Phase::DealerDiscard => {
            let trump = round.trump.expect("trump set before discard");
            Action::Discard(best_discard(&round.hands[seat], trump))
        }
        Phase::Play => Action::Play(choose_play(round, seat)),
        Phase::TrickDone | Phase::HandOver | Phase::Misdeal => Action::Pass,
    }
}

/// A rough "how many tricks can I take" score for a hand with `trump`.
/// A typical good calling hand (a bower, two more trump and an off ace) is around 7-8.
pub fn hand_strength(cards: &[Card], trump: Suit) -> f32 {
    let mut score = 0.0;
    let mut trumps = 0;
    for &c in cards {
        if c.is_right_bower(trump) {
            score += 3.1;
            trumps += 1;
        } else if c.is_left_bower(trump) {
            score += 2.7;
            trumps += 1;
        } else if c.suit == trump {
            trumps += 1;
            score += match c.rank {
                Rank::Ace => 2.2,
                Rank::King => 1.9,
                Rank::Queen => 1.7,
                Rank::Ten => 1.5,
                _ => 1.4,
            };
        } else if c.rank == Rank::Ace {
            score += 1.1;
        } else if c.rank == Rank::King {
            score += 0.4;
        }
    }
    if trumps >= 2 {
        for suit in Suit::ALL {
            if suit != trump && !cards.iter().any(|c| c.suit_under(trump) == suit) {
                score += 0.6;
            }
        }
    }
    score
}

/// How much one trump card is worth to whoever holds it.
fn card_value(card: Card, trump: Suit) -> f32 {
    hand_strength(&[card], trump)
}

fn order_up(round: &Round, seat: Seat) -> Action {
    let up = round.up_card;
    let trump = up.suit;
    let hand = &round.hands[seat];
    let strength = if seat == round.dealer {
        let mut six = hand.clone();
        six.push(up);
        let discard = best_discard(&six, trump);
        six.retain(|&c| c != discard);
        hand_strength(&six, trump)
    } else {
        let base = hand_strength(hand, trump);
        // The up card goes to the dealer's team.
        let swing = card_value(up, trump) * 0.6;
        if partner_of(seat) == round.dealer { base + swing } else { base - swing }
    };
    if strength >= CALL_THRESHOLD {
        Action::OrderUp { alone: strength >= ALONE_THRESHOLD }
    } else {
        Action::Pass
    }
}

fn name_trump(round: &Round, seat: Seat) -> Action {
    let turned_down = round.up_card.suit;
    let hand = &round.hands[seat];
    let (suit, strength) = Suit::ALL
        .iter()
        .filter(|&&s| s != turned_down)
        .map(|&s| {
            let mut v = hand_strength(hand, s);
            // "Next": the suit of the same color as the turned-down card is a
            // little better for the player left of the dealer.
            if s == turned_down.same_color() && seat == (round.dealer + 1) % 4 {
                v += 0.4;
            }
            (s, v)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .expect("three suits to choose from");
    let stuck = seat == round.dealer && round.stick_the_dealer;
    if strength >= CALL_THRESHOLD || stuck {
        Action::Call { suit, alone: strength >= ALONE_THRESHOLD }
    } else {
        Action::Pass
    }
}

/// The dealer's discard: dump a lone low off-suit card to create a void, or
/// otherwise the lowest off-suit card. Never an ace if avoidable.
pub fn best_discard(hand: &[Card], trump: Suit) -> Card {
    let off: Vec<Card> = hand.iter().copied().filter(|c| !c.is_trump(trump)).collect();
    if off.is_empty() {
        return *hand
            .iter()
            .min_by_key(|c| c.power(trump, trump))
            .expect("hand is not empty");
    }
    let count = |s: Suit| off.iter().filter(|c| c.suit == s).count();
    *off.iter()
        .min_by_key(|c| {
            let ace = c.rank == Rank::Ace;
            let singleton = count(c.suit) == 1;
            // Lower key = better discard.
            (ace as u8, !singleton as u8, c.rank.index())
        })
        .expect("off-suit cards exist")
}

fn choose_play(round: &Round, seat: Seat) -> Card {
    let legal = round.legal_plays(seat);
    if legal.len() == 1 {
        return legal[0];
    }
    let trump = round.trump.expect("trump set during play");
    if round.trick.is_empty() {
        return lead(round, seat, &legal, trump);
    }
    let led = round.led_suit().expect("trick has a lead");
    let (win_seat, win_card) = round.trick_leader().expect("trick has a leader");
    let win_power = win_card.power(trump, led);
    let last = round.trick.len() + 1 == round.active_players();
    let unseen = round.unseen_by(seat);
    let is_boss = |c: Card| {
        let p = c.power(trump, led);
        !unseen.iter().any(|u| u.power(trump, led) > p)
    };

    if win_seat == partner_of(seat) && (last || is_boss(win_card) || win_card.is_trump(trump)) {
        return lowest(&legal, trump);
    }
    let mut winners: Vec<Card> = legal.iter().copied().filter(|c| c.power(trump, led) > win_power).collect();
    winners.sort_by_key(|c| c.power(trump, led));
    if win_seat == partner_of(seat) {
        // Partner is winning with a beatable card: only overtake with a sure thing.
        return winners.iter().copied().find(|&c| is_boss(c)).unwrap_or_else(|| lowest(&legal, trump));
    }
    if winners.is_empty() {
        return lowest(&legal, trump);
    }
    if last {
        return winners[0];
    }
    // Not last: a guaranteed winner if we have one, otherwise the cheapest card that wins now.
    winners.iter().copied().find(|&c| is_boss(c)).unwrap_or(winners[0])
}

fn lead(round: &Round, seat: Seat, hand: &[Card], trump: Suit) -> Card {
    let maker = round.maker.expect("maker set during play");
    let our_call = team_of(maker) == team_of(seat);
    let unseen = round.unseen_by(seat);
    let boss_in_own_suit = |c: Card| {
        let s = c.suit_under(trump);
        let p = c.power(trump, s);
        !unseen.iter().any(|u| u.power(trump, s) > p)
    };
    let mut trumps: Vec<Card> = hand.iter().copied().filter(|c| c.is_trump(trump)).collect();
    trumps.sort_by_key(|c| std::cmp::Reverse(c.power(trump, trump)));
    let trump_outstanding = unseen.iter().any(|c| c.is_trump(trump));

    // Makers pull trump with the top trump while the opponents might still hold some.
    if our_call && trump_outstanding && !trumps.is_empty() {
        let top = trumps[0];
        if boss_in_own_suit(top) && (maker == seat || trumps.len() >= 2) {
            return top;
        }
        // Leading trump to a partner who called it.
        if maker == partner_of(seat) && trumps.len() >= 2 {
            return *trumps.last().unwrap();
        }
    }
    // Cash a winner in a side suit.
    let side: Vec<Card> = hand.iter().copied().filter(|c| !c.is_trump(trump)).collect();
    if let Some(&ace) = side
        .iter()
        .filter(|&&c| boss_in_own_suit(c))
        .max_by_key(|c| c.rank)
    {
        return ace;
    }
    if side.is_empty() {
        return trumps[0];
    }
    // Otherwise lead low from the shortest side suit, hoping to make a void.
    let count = |s: Suit| side.iter().filter(|c| c.suit == s).count();
    *side
        .iter()
        .min_by_key(|c| (count(c.suit), c.rank.index()))
        .unwrap()
}

/// The card we least mind giving up: low off-suit first, trump last.
fn lowest(cards: &[Card], trump: Suit) -> Card {
    *cards
        .iter()
        .min_by_key(|c| {
            if c.is_trump(trump) {
                100 + c.power(trump, trump) as usize
            } else {
                c.rank.index() * 2 + (c.rank == Rank::Ace) as usize * 10
            }
        })
        .expect("at least one legal card")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::euchre::game::Match;
    use crate::euchre::round::ResultKind;

    fn c(rank: Rank, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    #[test]
    fn strong_hand_orders_up_weak_hand_passes() {
        use Rank::*;
        use Suit::*;
        let strong = vec![c(Jack, Hearts), c(Jack, Diamonds), c(Ace, Hearts), c(Ace, Clubs), c(Nine, Spades)];
        let weak = vec![c(Nine, Clubs), c(Ten, Spades), c(Queen, Diamonds), c(Nine, Spades), c(Ten, Clubs)];
        assert!(hand_strength(&strong, Hearts) >= CALL_THRESHOLD);
        assert!(hand_strength(&weak, Hearts) < CALL_THRESHOLD);
    }

    #[test]
    fn discard_makes_a_void_and_keeps_aces() {
        use Rank::*;
        use Suit::*;
        let hand = [
            c(Jack, Hearts),
            c(Ace, Hearts),
            c(Ace, Clubs),
            c(Nine, Clubs),
            c(King, Spades),
            c(Ten, Hearts),
        ];
        assert_eq!(best_discard(&hand, Hearts), c(King, Spades), "singleton creates a void");
        let hand = [c(Jack, Hearts), c(Ace, Hearts), c(Ace, Clubs), c(Nine, Clubs), c(Ten, Clubs), c(Ten, Hearts)];
        assert_eq!(best_discard(&hand, Hearts), c(Nine, Clubs));
    }

    /// Plays thousands of AI-only games: no rule errors, every game finishes,
    /// and the bidding is sensible (makers usually succeed).
    #[test]
    fn simulated_games_are_sane() {
        let mut hands = 0u32;
        let mut euchres = 0u32;
        let mut marches = 0u32;
        let mut alones = 0u32;
        let mut misdeals = 0u32;
        let mut team_wins = [0u32; 2];
        for seed in 0..400u64 {
            let mut m = Match::new(10, seed % 2 == 0, seed);
            let mut guard = 0;
            while m.winner().is_none() {
                guard += 1;
                assert!(guard < 10_000, "game {seed} did not finish");
                let r = &m.round;
                match r.phase {
                    Phase::TrickDone => m.round.advance(),
                    Phase::Misdeal => {
                        misdeals += 1;
                        m.next_hand();
                    }
                    Phase::HandOver => {
                        let res = m.score_hand().unwrap();
                        hands += 1;
                        if res.kind == ResultKind::Euchred {
                            euchres += 1;
                        }
                        if res.kind == ResultKind::March {
                            marches += 1;
                        }
                        if res.alone {
                            alones += 1;
                        }
                        for seat in 0..4 {
                            if !m.round.is_sitting_out(seat) {
                                assert!(m.round.hands[seat].is_empty());
                            }
                        }
                        assert_eq!(m.round.played.len(), m.round.active_players() * 5);
                        if m.winner().is_none() {
                            m.next_hand();
                        }
                    }
                    _ => {
                        let seat = r.to_act().unwrap();
                        let action = choose(r, seat);
                        m.round.apply(seat, action).unwrap_or_else(|e| {
                            panic!("seed {seed}: {action:?} by {seat} rejected: {e:?}")
                        });
                    }
                }
            }
            team_wins[m.winner().unwrap()] += 1;
            assert!(m.scores.iter().any(|&s| s >= 10));
        }
        let euchre_rate = euchres as f32 / hands as f32;
        let march_rate = marches as f32 / hands as f32;
        println!(
            "hands {hands}, euchre {euchre_rate:.2}, march {march_rate:.2}, alone {alones}, misdeals {misdeals}, wins {team_wins:?}"
        );
        assert!((0.08..0.40).contains(&euchre_rate), "euchre rate {euchre_rate}");
        assert!(march_rate > 0.05, "march rate {march_rate}");
        assert!(alones > 0, "AI should sometimes go alone");
    }
}
