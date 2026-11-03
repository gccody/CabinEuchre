//! What a good player would do here and why, in plain words: the tutorial's tips,
//! hints and comments on a move. Pure rules, and it only looks at what the seat can
//! know, so it works from a player's [`Mirror`](crate::Mirror) as well as a real round.
//!
//! It speaks as the player's partner, so "I" is the seat across the table.

use crate::ai;
use crate::card::{Card, Rank, Suit};
use crate::round::{Action, Phase, Round, Seat, partner_of, team_of};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advice {
    pub action: Action,
    /// Why, without saying which card, so it also works as a tip before you've chosen.
    pub why: String,
}

/// How a move looks now that it's been made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Worth a small "nice".
    Good(&'static str),
    /// Worth explaining.
    Costly(String),
    Quiet,
}

fn count_trump(hand: &[Card], trump: Suit) -> usize {
    hand.iter().filter(|c| c.is_trump(trump)).count()
}

/// What the AI would do for `seat` (which must be due to act), and why.
pub fn advise(r: &Round, seat: Seat) -> Advice {
    let action = ai::choose(r, seat);
    let why = match (r.phase, action) {
        (Phase::OrderUp, _) => order_up_reason(r, seat, action),
        (Phase::NameTrump, _) => name_reason(r, seat, action),
        (Phase::DealerDiscard, Action::Discard { card }) => discard_reason(r, seat, card),
        (Phase::Play, Action::Play { card }) => play_reason(r, seat, card),
        _ => String::new(),
    };
    Advice { action, why }
}

fn order_up_reason(r: &Round, seat: Seat, action: Action) -> String {
    let trump = r.up_card.suit;
    let dealer = seat == r.dealer;
    let n = count_trump(&r.hands[seat], trump) + usize::from(dealer);
    let sym = trump.symbol();
    let verb = if dealer { "PICK UP" } else { "ORDER UP" };
    match action {
        Action::OrderUp { alone: true } => {
            format!("That's a monster: {n} {sym} right at the top. Press ALONE, then {verb}, and go for all five.")
        }
        Action::OrderUp { .. } => format!("With {n} {sym} you should win 3 tricks. Press {verb}."),
        _ if dealer => format!("Even with that card you'd only have {n} {sym}. I'd turn it down."),
        _ if partner_of(seat) == r.dealer => format!("Only {n} {sym}, even with me picking it up. I'd pass."),
        _ => format!("Only {n} {sym}, and that card would go to the other team. I'd pass."),
    }
}

fn name_reason(r: &Round, seat: Seat, action: Action) -> String {
    match action {
        Action::Call { suit, alone } => {
            let n = count_trump(&r.hands[seat], suit);
            let sym = suit.symbol();
            let stuck = seat == r.dealer && r.stick_the_dealer;
            match (alone, stuck) {
                (true, _) => format!("{n} {sym} right at the top. Press ALONE, then {sym}, and go for all five."),
                (false, true) => format!("You're stuck with it, so name your best suit: {sym}, with {n} trump."),
                (false, false) => format!("{sym} is your strongest suit: that's {n} trump. Name it."),
            }
        }
        _ => "Nothing here looks strong enough. Passing is fine.".to_string(),
    }
}

fn discard_reason(r: &Round, seat: Seat, card: Card) -> String {
    let Some(trump) = r.trump else { return String::new() };
    if card.is_trump(trump) {
        return "All trump! Lucky you. Throw the smallest one.".to_string();
    }
    let alone_in_suit = r.hands[seat].iter().filter(|c| c.suit_under(trump) == card.suit).count() == 1;
    if alone_in_suit {
        "Throw a low side card. With none of a suit left, you can trump it later.".to_string()
    } else {
        "Keep your trump and aces, and throw your weakest side card.".to_string()
    }
}

fn play_reason(r: &Round, seat: Seat, card: Card) -> String {
    let Some(trump) = r.trump else { return String::new() };
    let our_call = r.maker.is_some_and(|m| team_of(m) == team_of(seat));
    if r.trick.is_empty() {
        return if card.is_trump(trump) && our_call {
            "We made trump, so lead a strong one and pull theirs out.".to_string()
        } else if card.is_trump(trump) {
            "Lead your trump and take what you can.".to_string()
        } else if card.rank == Rank::Ace {
            "Aces are hard to beat. Take your trick while it's there.".to_string()
        } else {
            "Nothing sure to lead, so start low and see who takes it.".to_string()
        };
    }
    let (Some(led), Some((win_seat, win_card))) = (r.led_suit(), r.trick_leader()) else {
        return String::new();
    };
    let wins = card.power(trump, led) > win_card.power(trump, led);
    let last = r.trick.len() + 1 == r.active_players();
    let trumping = card.is_trump(trump) && led != trump;
    if wins && last {
        "You're last to play, so the smallest card that wins will do.".to_string()
    } else if wins && trumping {
        "You can't follow, and a trump would take this. Use a small one if you can.".to_string()
    } else if wins {
        "You can take this one.".to_string()
    } else if win_seat == partner_of(seat) {
        "I'm winning this one. Play low and save your good cards.".to_string()
    } else {
        "You can't win this one, so play your lowest.".to_string()
    }
}

/// How playing `card` looks, given the round just before it was played.
pub fn review_play(before: &Round, seat: Seat, card: Card) -> Verdict {
    let Some(trump) = before.trump else { return Verdict::Quiet };
    let (Some(led), Some((win_seat, win_card))) = (before.led_suit(), before.trick_leader()) else {
        return Verdict::Quiet;
    };
    let legal = before.legal_plays(seat);
    if legal.len() < 2 {
        return Verdict::Quiet;
    }
    let power = |c: Card| c.power(trump, led);
    let wins = power(card) > power(win_card);
    let last = before.trick.len() + 1 == before.active_players();
    if win_seat == partner_of(seat) {
        let spent_trump = card.is_trump(trump) && !win_card.is_trump(trump) && legal.iter().any(|c| !c.is_trump(trump));
        return if spent_trump {
            Verdict::Costly("I had that trick already! Don't trump your partner. Save it for theirs.".into())
        } else if wins && last {
            Verdict::Costly("I had that one won. A low card would have saved your good one.".into())
        } else if last {
            Verdict::Good("Thanks, partner!")
        } else {
            Verdict::Quiet
        };
    }
    if !last {
        return Verdict::Quiet;
    }
    let mut winners: Vec<Card> = legal.iter().copied().filter(|&c| power(c) > power(win_card)).collect();
    winners.sort_by_key(|&c| power(c));
    match winners.first() {
        None => Verdict::Quiet,
        Some(&best) if best == card => Verdict::Good("Just enough. Nice!"),
        Some(&best) if wins => {
            Verdict::Costly(format!("That wins, but your {} would have too. Cheap wins save your big cards.", best.label()))
        }
        Some(&best) => Verdict::Costly(format!("Your {} would have won that. Going last is your chance to pounce.", best.label())),
    }
}

/// How discarding `card` looks, given the round just before it. The dealer holds six cards.
pub fn review_discard(before: &Round, seat: Seat, card: Card) -> Verdict {
    let Some(trump) = before.trump else { return Verdict::Quiet };
    let hand = &before.hands[seat];
    let had_side_card = hand.iter().any(|c| !c.is_trump(trump));
    if card.is_trump(trump) && had_side_card {
        return Verdict::Costly("Careful, that was a trump! Side cards go first.".into());
    }
    let had_lower_side_card = hand.iter().any(|c| !c.is_trump(trump) && c.rank != Rank::Ace);
    if card.rank == Rank::Ace && had_lower_side_card {
        return Verdict::Costly("An ace often wins by itself. A low card is a better throw.".into());
    }
    if hand.iter().filter(|c| c.suit_under(trump) == card.suit_under(trump)).count() == 1 {
        return Verdict::Good("Nice throw!");
    }
    Verdict::Quiet
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tutorial::card;

    fn cards(tokens: &str) -> Vec<Card> {
        tokens.split_whitespace().map(card).collect()
    }

    /// Hearts are trump, Rosa made them. Seat 0 acts after `trick`, holding `hand`; the
    /// other hands get whatever is left.
    fn table(hand: &str, trick: &[(Seat, &str)]) -> Round {
        let mut rest: Vec<Card> = Card::deck();
        let mine = cards(hand);
        let played: Vec<(Seat, Card)> = trick.iter().map(|&(s, t)| (s, card(t))).collect();
        rest.retain(|c| !mine.contains(c) && !played.iter().any(|(_, p)| p == c));
        let mut hands: [Vec<Card>; 4] = Default::default();
        hands[0] = mine;
        for (seat, h) in hands.iter_mut().enumerate().skip(1) {
            let already = played.iter().filter(|(s, _)| *s == seat).count();
            *h = rest.drain(..5 - already).collect();
        }
        let up = rest.remove(0);
        let mut r = Round::with_hands(3, hands, up, rest, true);
        r.trump = Some(Suit::Hearts);
        r.maker = Some(1);
        r.phase = Phase::Play;
        r.trick = played;
        r.turn = 0;
        r
    }

    #[test]
    fn trumping_your_partners_winner_is_explained() {
        let r = table("9h 9c 9s", &[(1, "Qd"), (2, "Ad"), (3, "Td")]);
        assert!(matches!(review_play(&r, 0, card("9h")), Verdict::Costly(s) if s.contains("partner")));
        assert_eq!(review_play(&r, 0, card("9c")), Verdict::Good("Thanks, partner!"));
    }

    #[test]
    fn last_to_play_is_told_about_a_missed_or_a_costly_win() {
        let r = table("Ac Qc 9s", &[(1, "Tc"), (2, "9c"), (3, "Kc")]);
        assert_eq!(review_play(&r, 0, card("Ac")), Verdict::Good("Just enough. Nice!"));
        assert!(matches!(review_play(&r, 0, card("Qc")), Verdict::Costly(s) if s.contains("A♣")));
        let r = table("Ac Qc 9s", &[(1, "Tc"), (2, "9c"), (3, "Jc")]);
        assert!(matches!(review_play(&r, 0, card("Ac")), Verdict::Costly(s) if s.contains("Q♣")));
    }

    #[test]
    fn a_forced_card_or_an_early_seat_gets_no_comment() {
        let r = table("Ac 9h 9s", &[(1, "Tc"), (2, "9c"), (3, "Jc")]);
        assert_eq!(review_play(&r, 0, card("Ac")), Verdict::Quiet, "only one club, so it was forced");
        let r = table("Ac Qc 9s", &[(3, "Tc")]);
        assert_eq!(review_play(&r, 0, card("Qc")), Verdict::Quiet, "two seats still to play");
    }

    #[test]
    fn the_left_bower_counts_as_trump_when_judging_a_play() {
        // Clubs led and your partner's ace is winning: J♦ is a heart here, not a diamond.
        let r = table("Jd 9s Ts", &[(1, "Qc"), (2, "Ac"), (3, "Tc")]);
        assert!(matches!(review_play(&r, 0, card("Jd")), Verdict::Costly(s) if s.contains("partner")));
    }

    #[test]
    fn discards_are_judged() {
        let mut r = table("Jh 9h Ac 9s 9c", &[]);
        r.hands[0] = cards("Jh 9h Ac 9s 9c Qd");
        assert!(matches!(review_discard(&r, 0, card("9h")), Verdict::Costly(s) if s.contains("trump")));
        assert!(matches!(review_discard(&r, 0, card("Ac")), Verdict::Costly(s) if s.contains("ace")));
        assert_eq!(review_discard(&r, 0, card("Qd")), Verdict::Good("Nice throw!"), "the only diamond");
        assert_eq!(review_discard(&r, 0, card("9c")), Verdict::Quiet, "there's another club");
    }

    #[test]
    fn advice_is_always_a_legal_move_with_a_reason() {
        for seed in 0..200u64 {
            let mut m = crate::Match::new(10, true, seed);
            for _ in 0..60 {
                match m.round.phase {
                    Phase::TrickDone => m.advance(),
                    Phase::HandOver => {
                        m.score_hand();
                        if m.winner().is_some() {
                            break;
                        }
                        m.next_hand();
                    }
                    Phase::Misdeal => m.next_hand(),
                    _ => {
                        let seat = m.round.to_act().unwrap();
                        let advice = advise(&m.round, seat);
                        assert!(!advice.why.is_empty(), "no reason for {:?} in {:?}", advice.action, m.round.phase);
                        m.apply(seat, advice.action).unwrap();
                    }
                }
            }
        }
    }

    #[test]
    fn advice_names_the_right_situation() {
        let reason = advise(&table("9c 9d Qd", &[(1, "Qs"), (2, "As"), (3, "Ts")]), 0).why;
        assert!(reason.contains("winning"), "{reason}");
        let reason = advise(&table("9h 9c Qd", &[(1, "Qs"), (2, "Js"), (3, "Ts")]), 0).why;
        assert!(reason.contains("last to play"), "{reason}");
        let reason = advise(&table("9c 9d Qd", &[(1, "As"), (2, "Js"), (3, "Ts")]), 0).why;
        assert!(reason.contains("can't win"), "{reason}");
    }
}
