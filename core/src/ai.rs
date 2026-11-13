//! Heuristic computer players. They only look at what their seat could know:
//! their own hand, the up card, trump, who made it, and the cards played so far.
//!
//! How well they play is a [`Difficulty`], and each one is a row of numbers in
//! [`SKILLS`]: change a number there to make a level easier or harder, then run
//! `cargo run -p euchre-core --release --example bots` to see what it did to the
//! win rates. Every level is a handful of comparisons per move, with no search
//! and no dice: a weaker bot's mistakes come from a hash of what it can see
//! ([`chance`]), so a game replays the same from its seed, on a phone as on the server.

use serde::{Deserialize, Serialize};

use crate::card::{Card, Rank, Suit};
use crate::rng::Rng;
use crate::round::{Action, Phase, Round, SEATS, Seat, partner_of, team_of};

/// How well the computer players play. A table has one level, and a bot whose
/// partner is a person may play above it (see [`PARTNER_FLOOR`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Difficulty {
    Rookie,
    Easy,
    Medium,
    /// How the bots played before there were levels, and what the tutorial and
    /// its advice still use ([`choose`]).
    #[default]
    Hard,
    Expert,
}

impl Difficulty {
    pub const ALL: [Difficulty; 5] =
        [Difficulty::Rookie, Difficulty::Easy, Difficulty::Medium, Difficulty::Hard, Difficulty::Expert];

    pub fn index(self) -> usize {
        self as usize
    }

    /// What the options call it.
    pub fn name(self) -> &'static str {
        ["Rookie", "Easy", "Medium", "Hard", "Expert"][self.index()]
    }

    /// How it's written in the settings file and in a new table's address.
    pub fn key(self) -> &'static str {
        ["rookie", "easy", "medium", "hard", "expert"][self.index()]
    }

    /// Reads [`Difficulty::key`] text.
    pub fn parse(s: &str) -> Option<Difficulty> {
        Difficulty::ALL.into_iter().find(|d| d.key() == s)
    }

    pub fn skill(self) -> &'static Skill {
        &SKILLS[self.index()]
    }

    /// The level of a bot at this table that sits across from a person.
    pub fn as_partner(self) -> Difficulty {
        self.max(PARTNER_FLOOR)
    }
}

/// A person's partner never plays below this. An easy table is easy because
/// the other team is weak, not because your partner trumps your ace: with
/// everyone at `Rookie`, a decent player's partner loses about as many
/// tricks as the other team gives away. Set it to `Rookie` and every bot at a
/// table plays alike.
pub const PARTNER_FLOOR: Difficulty = Difficulty::Hard;

/// What makes one level play better than another.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Skill {
    /// Points needed (see [`hand_strength`]) to make trump. Lower calls thinner hands.
    pub call: f32,
    /// Points needed to go alone.
    pub alone: f32,
    /// How far off its count of its own hand can be, in points either way. This
    /// is what makes a weak bot call on nothing and pass a good hand.
    pub misjudge: f32,
    /// How much it minds whose hand the up card ends up in (0 not at all).
    pub up_card: f32,
    /// Points added for "next" (the turned-down card's color) left of the dealer.
    pub next: f32,
    /// The chance, at each card, that it plays any legal card rather than the right one.
    pub slip: f32,
    /// Remembers which cards have gone, so it knows when a king has become the
    /// best of its suit. Without this it only sees its hand and the trick.
    pub counts_cards: bool,
    /// Notices who couldn't follow suit, where the up card went and who is
    /// still to play: it stops pulling trump nobody against it holds, and
    /// doesn't lead into a suit that will be trumped.
    pub reads_table: bool,
    /// Leads a side ace the first time its suit is led, before anyone has run out of it.
    pub leads_aces: bool,
    /// Knows the score: it won't go alone when two points would win anyway.
    pub reads_score: bool,
}

/// The levels, by [`Difficulty::index`]. `Hard` must stay as it is: the
/// tutorial's script, its advice and the screenshots all count on those moves.
pub const SKILLS: [Skill; 5] = [
    // Rookie
    Skill {
        call: 6.3,
        alone: 11.0,
        misjudge: 3.0,
        up_card: 0.0,
        next: 0.0,
        slip: 0.35,
        counts_cards: false,
        reads_table: false,
        leads_aces: false,
        reads_score: false,
    },
    // Easy
    Skill {
        call: 6.3,
        alone: 11.0,
        misjudge: 2.25,
        up_card: 0.3,
        next: 0.0,
        slip: 0.20,
        counts_cards: false,
        reads_table: false,
        leads_aces: false,
        reads_score: false,
    },
    // Medium
    Skill {
        call: 6.3,
        alone: 11.0,
        misjudge: 1.5,
        up_card: 0.6,
        next: 0.4,
        slip: 0.10,
        counts_cards: true,
        reads_table: false,
        leads_aces: false,
        reads_score: false,
    },
    // Hard
    Skill {
        call: 6.3,
        alone: 11.0,
        misjudge: 0.0,
        up_card: 0.6,
        next: 0.4,
        slip: 0.0,
        counts_cards: true,
        reads_table: false,
        leads_aces: false,
        reads_score: false,
    },
    // Expert. Against two Hard bots, leading aces is worth about 3 games in 100 and
    // going alone this readily another 3. The other two are within the noise.
    Skill {
        call: 6.3,
        alone: 9.0,
        misjudge: 0.0,
        up_card: 0.6,
        next: 0.4,
        slip: 0.0,
        counts_cards: true,
        reads_table: true,
        leads_aces: true,
        reads_score: true,
    },
];

/// The score a bot plays to, for the levels that look at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Standing {
    /// Points per team (team 0 is seats 0 and 2).
    pub scores: [u32; 2],
    pub target: u32,
}

impl Standing {
    /// Nothing known, so nothing to play to.
    pub const NONE: Standing = Standing { scores: [0, 0], target: u32::MAX };

    /// Points `seat`'s team still needs to win.
    fn needs(&self, seat: Seat) -> u32 {
        self.target.saturating_sub(self.scores[team_of(seat)])
    }
}

/// Picks an action for `seat`, which must be the seat to act, playing at `Hard`.
pub fn choose(round: &Round, seat: Seat) -> Action {
    choose_with(round, seat, Difficulty::Hard.skill(), Standing::NONE)
}

/// Picks an action for `seat`, which must be the seat to act.
pub fn choose_at(round: &Round, seat: Seat, level: Difficulty, standing: Standing) -> Action {
    choose_with(round, seat, level.skill(), standing)
}

/// [`choose_at`] with any skill, for trying out numbers that aren't in [`SKILLS`].
pub fn choose_with(round: &Round, seat: Seat, skill: &Skill, standing: Standing) -> Action {
    match round.phase {
        Phase::OrderUp => order_up(round, seat, skill, standing),
        Phase::NameTrump => name_trump(round, seat, skill, standing),
        Phase::DealerDiscard => {
            let trump = round.trump.expect("trump set before discard");
            Action::Discard { card: best_discard(&round.hands[seat], trump) }
        }
        Phase::Play => Action::Play { card: choose_play(round, seat, skill) },
        Phase::TrickDone | Phase::HandOver | Phase::Misdeal => Action::Pass,
    }
}

/// A number from 0 up to 1 that depends only on what `seat` can see, and on
/// `salt` to tell one question from another. It stands in for dice.
fn chance(round: &Round, seat: Seat, salt: u64) -> f32 {
    let phase = match round.phase {
        Phase::OrderUp => 0,
        Phase::NameTrump => 1,
        _ => 2,
    };
    let mut h = salt
        ^ (seat as u64) << 8
        ^ (round.dealer as u64) << 12
        ^ phase << 16
        ^ (round.up_card.id() as u64) << 20
        ^ (round.played.len() as u64) << 28;
    for card in &round.hands[seat] {
        h = Rng::new(h ^ card.id() as u64).next_u64();
    }
    (Rng::new(h).next_u64() >> 40) as f32 / (1u64 << 24) as f32
}

/// How far off this bot's count of its hand is, with `suit` as trump.
fn misjudgement(round: &Round, seat: Seat, skill: &Skill, suit: Suit) -> f32 {
    if skill.misjudge == 0.0 {
        return 0.0;
    }
    (chance(round, seat, suit.index() as u64) * 2.0 - 1.0) * skill.misjudge
}

fn goes_alone(strength: f32, seat: Seat, skill: &Skill, standing: Standing) -> bool {
    // A march with a partner is worth two, so alone only gives up their help.
    strength >= skill.alone && !(skill.reads_score && standing.needs(seat) <= 2)
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

fn order_up(round: &Round, seat: Seat, skill: &Skill, standing: Standing) -> Action {
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
        let swing = card_value(up, trump) * skill.up_card;
        if partner_of(seat) == round.dealer { base + swing } else { base - swing }
    };
    let strength = strength + misjudgement(round, seat, skill, trump);
    if strength >= skill.call {
        Action::OrderUp { alone: goes_alone(strength, seat, skill, standing) }
    } else {
        Action::Pass
    }
}

fn name_trump(round: &Round, seat: Seat, skill: &Skill, standing: Standing) -> Action {
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
                v += skill.next;
            }
            (s, v + misjudgement(round, seat, skill, s))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .expect("three suits to choose from");
    let stuck = seat == round.dealer && round.stick_the_dealer;
    if strength >= skill.call || stuck {
        Action::Call { suit, alone: goes_alone(strength, seat, skill, standing) }
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

/// What the cards played so far give away to a seat that reads the table.
struct Tells {
    /// `void[seat][suit]`: the seat couldn't follow that suit, so it holds none.
    void: [[bool; 4]; SEATS],
    /// The up card, while the dealer who picked it up hasn't played it.
    picked_up: Option<Card>,
    dealer: Seat,
}

impl Tells {
    fn read(round: &Round, trump: Suit) -> Tells {
        let mut void = [[false; 4]; SEATS];
        let mut leader = round.next_active(round.dealer);
        for (i, trick) in round.played.chunks(round.active_players()).enumerate() {
            let led = trick[0].suit_under(trump);
            let mut at = leader;
            for &card in trick {
                if card.suit_under(trump) != led {
                    void[at][led.index()] = true;
                }
                at = round.next_active(at);
            }
            let Some(&winner) = round.trick_winners.get(i) else { break };
            leader = winner;
        }
        // Everyone saw the dealer pick it up. What they threw away is their secret,
        // and it's almost never this.
        let picked_up = (round.discard.is_some() && !round.played.contains(&round.up_card)).then_some(round.up_card);
        Tells { void, picked_up, dealer: round.dealer }
    }

    /// Whether `who` could be holding `card`, which this seat hasn't seen.
    fn might_hold(&self, who: Seat, card: Card, trump: Suit) -> bool {
        !self.void[who][card.suit_under(trump).index()] && (self.picked_up != Some(card) || who == self.dealer)
    }
}

/// What a seat goes on when it picks a card.
struct Seen {
    trump: Suit,
    /// Cards that could be in another hand, as far as this seat remembers.
    unseen: Vec<Card>,
    /// Only for a bot that reads the table.
    tells: Option<Tells>,
}

impl Seen {
    fn new(round: &Round, seat: Seat, trump: Suit, skill: &Skill) -> Seen {
        let unseen = if skill.counts_cards {
            round.unseen_by(seat)
        } else {
            // No memory for earlier tricks: anything not in hand or on the table could be out there.
            Card::deck()
                .into_iter()
                .filter(|c| !round.hands[seat].contains(c) && !round.trick.iter().any(|(_, t)| t == c))
                .collect()
        };
        Seen { trump, unseen, tells: skill.reads_table.then(|| Tells::read(round, trump)) }
    }

    /// Whether one of `seats` could hold `card`.
    fn among(&self, seats: [bool; SEATS], card: Card) -> bool {
        match &self.tells {
            None => true,
            Some(t) => (0..SEATS).any(|s| seats[s] && t.might_hold(s, card, self.trump)),
        }
    }

    /// Whether `card` could still lose a trick in which `led` was led. A bot
    /// that reads the table only worries about `threats`, the seats against it
    /// that have yet to play; the others worry about every card they haven't seen.
    fn beatable(&self, card: Card, led: Suit, threats: [bool; SEATS]) -> bool {
        let p = card.power(self.trump, led);
        self.unseen.iter().any(|&u| u.power(self.trump, led) > p && self.among(threats, u))
    }
}

fn choose_play(round: &Round, seat: Seat, skill: &Skill) -> Card {
    let legal = round.legal_plays(seat);
    if legal.len() == 1 {
        return legal[0];
    }
    if skill.slip > 0.0 && chance(round, seat, 8) < skill.slip {
        return legal[(chance(round, seat, 9) * legal.len() as f32) as usize % legal.len()];
    }
    let trump = round.trump.expect("trump set during play");
    let seen = Seen::new(round, seat, trump, skill);
    if round.trick.is_empty() {
        return lead(round, seat, &legal, &seen, skill);
    }
    let led = round.led_suit().expect("trick has a lead");
    let (win_seat, win_card) = round.trick_leader().expect("trick has a leader");
    let win_power = win_card.power(trump, led);
    let to_come = round.active_players() - round.trick.len() - 1;
    let last = to_come == 0;
    // The other team's seats that play after this one.
    let mut threats = [false; SEATS];
    let mut at = seat;
    for _ in 0..to_come {
        at = round.next_active(at);
        threats[at] = team_of(at) != team_of(seat);
    }
    let is_boss = |c: Card| !seen.beatable(c, led, threats);

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

fn lead(round: &Round, seat: Seat, hand: &[Card], seen: &Seen, skill: &Skill) -> Card {
    let trump = seen.trump;
    let maker = round.maker.expect("maker set during play");
    let our_call = team_of(maker) == team_of(seat);
    // Both seats of the other team play after a lead (a lone maker's partner is on ours).
    let threats: [bool; SEATS] = std::array::from_fn(|s| team_of(s) != team_of(seat));
    let boss_in_own_suit = |c: Card| !seen.beatable(c, c.suit_under(trump), threats);
    let mut trumps: Vec<Card> = hand.iter().copied().filter(|c| c.is_trump(trump)).collect();
    trumps.sort_by_key(|c| std::cmp::Reverse(c.power(trump, trump)));
    let trump_outstanding = seen.unseen.iter().any(|&c| c.is_trump(trump) && seen.among(threats, c));

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
    let count = |s: Suit| side.iter().filter(|c| c.suit == s).count();
    // Whether one of `seats` has shown it's out of `suit` and could still trump it.
    let ruffs = |seats: [bool; SEATS], suit: Suit| {
        seen.tells.as_ref().is_some_and(|t| {
            (0..SEATS).any(|s| {
                seats[s]
                    && t.void[s][suit.index()]
                    && seen.unseen.iter().any(|&u| u.is_trump(trump) && t.might_hold(s, u, trump))
            })
        })
    };
    if skill.leads_aces {
        // The best card of a suit nobody has led yet: on the first round everyone
        // usually still has one to follow with, so it walks more often than not.
        let fresh = |c: &&Card| {
            !round.played.iter().any(|p| p.suit_under(trump) == c.suit)
                && !seen.unseen.iter().any(|u| u.suit_under(trump) == c.suit && u.rank > c.rank)
                && !ruffs(threats, c.suit)
        };
        if let Some(&ace) = side.iter().filter(fresh).min_by_key(|c| count(c.suit)) {
            return ace;
        }
    }
    // Otherwise lead low from the shortest side suit, hoping to make a void. A bot
    // that reads the table would sooner lead what its partner can trump, and
    // not what the other team can.
    let partner: [bool; SEATS] = std::array::from_fn(|s| s == partner_of(seat) && !round.is_sitting_out(s));
    *side
        .iter()
        .min_by_key(|c| {
            let (ours, theirs) = (ruffs(partner, c.suit), ruffs(threats, c.suit));
            (theirs && !ours, !ours && seen.tells.is_some(), count(c.suit), c.rank.index())
        })
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
    use crate::game::Match;
    use crate::round::ResultKind;

    fn c(rank: Rank, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    #[test]
    fn strong_hand_orders_up_weak_hand_passes() {
        use Rank::*;
        use Suit::*;
        let strong = vec![c(Jack, Hearts), c(Jack, Diamonds), c(Ace, Hearts), c(Ace, Clubs), c(Nine, Spades)];
        let weak = vec![c(Nine, Clubs), c(Ten, Spades), c(Queen, Diamonds), c(Nine, Spades), c(Ten, Clubs)];
        assert!(hand_strength(&strong, Hearts) >= Difficulty::Hard.skill().call);
        assert!(hand_strength(&weak, Hearts) < Difficulty::Hard.skill().call);
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
                    Phase::TrickDone => m.advance(),
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
                        m.apply(seat, action).unwrap_or_else(|e| {
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

    /// Plays a game to 10 with a skill per seat, and returns the winning team.
    /// `seen` is told every move.
    fn play(seed: u64, skills: [&Skill; 4], mut seen: impl FnMut(Seat, Action)) -> usize {
        let mut m = Match::new(10, true, seed);
        loop {
            if let Some(team) = m.winner() {
                return team;
            }
            match m.round.phase {
                Phase::TrickDone => m.advance(),
                Phase::Misdeal => m.next_hand(),
                Phase::HandOver => {
                    m.score_hand();
                    if m.winner().is_none() {
                        m.next_hand();
                    }
                }
                _ => {
                    let seat = m.round.to_act().unwrap();
                    let standing = Standing { scores: m.scores, target: m.target };
                    let action = choose_with(&m.round, seat, skills[seat], standing);
                    m.apply(seat, action).unwrap_or_else(|e| panic!("seed {seed}: {action:?} by {seat} rejected: {e:?}"));
                    seen(seat, action);
                }
            }
        }
    }

    /// `Hard` is what the tutorial's bots fall back on, what its advice says and
    /// what the README's screenshots show. If this fails, a change meant for
    /// another level has changed `Hard`'s moves too.
    #[test]
    fn hard_plays_as_it_always_has() {
        let hard = Difficulty::Hard.skill();
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for seed in 0..150 {
            play(seed, [hard; 4], |seat, action| {
                for b in format!("{seat}{action:?}").bytes() {
                    hash = (hash ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
                }
            });
        }
        assert_eq!(hash, GOLDEN_HARD, "{hash:#018x}");
    }
    const GOLDEN_HARD: u64 = 0xa81a_9b53_31a5_bd90;

    /// Two bots of each level beat two of the level below, over the same deals
    /// played from both sides. `cargo run -p euchre-core --release --example bots` shows by how much.
    #[test]
    fn each_level_beats_the_one_below() {
        const DEALS: u64 = 300;
        for pair in Difficulty::ALL.windows(2) {
            let (low, high) = (pair[0].skill(), pair[1].skill());
            let mut wins = 0;
            for seed in 0..DEALS {
                wins += u64::from(play(seed, [high, low, high, low], |_, _| {}) == 0);
                wins += u64::from(play(seed, [low, high, low, high], |_, _| {}) == 1);
            }
            let rate = wins as f32 / (DEALS * 2) as f32;
            println!("{:?} beats {:?} in {:.0}% of games", pair[1], pair[0], rate * 100.0);
            assert!(rate > 0.52, "{:?} only wins {rate:.2} against {:?}", pair[1], pair[0]);
        }
    }

    #[test]
    fn levels_are_written_and_read_back() {
        for level in Difficulty::ALL {
            assert_eq!(Difficulty::parse(level.key()), Some(level));
            assert_eq!(serde_json::to_string(&level).unwrap(), format!("\"{}\"", level.key()));
        }
        assert_eq!(Difficulty::parse("godlike"), None);
        assert_eq!(Difficulty::Rookie.as_partner(), PARTNER_FLOOR);
        assert_eq!(Difficulty::Expert.as_partner(), Difficulty::Expert);
    }

    /// Dealer 3 with the 9♥ up, and seat 0 holding the top four hearts and an ace.
    fn monster() -> Round {
        use Rank::*;
        use Suit::*;
        let hands = [
            vec![c(Jack, Hearts), c(Jack, Diamonds), c(Ace, Hearts), c(King, Hearts), c(Ace, Clubs)],
            vec![c(Nine, Clubs), c(Ten, Clubs), c(Nine, Spades), c(Ten, Spades), c(Nine, Diamonds)],
            vec![c(Queen, Clubs), c(King, Clubs), c(Queen, Spades), c(King, Spades), c(Ten, Diamonds)],
            vec![c(Queen, Hearts), c(Ten, Hearts), c(Ace, Spades), c(Queen, Diamonds), c(King, Diamonds)],
        ];
        Round::with_hands(3, hands, c(Nine, Hearts), vec![c(Jack, Clubs), c(Jack, Spades), c(Ace, Diamonds)], true)
    }

    #[test]
    fn expert_keeps_its_partner_when_two_points_win() {
        let r = monster();
        let at = |scores| choose_at(&r, 0, Difficulty::Expert, Standing { scores, target: 10 });
        assert_eq!(at([7, 0]), Action::OrderUp { alone: true }, "three to go: four points are worth the risk");
        assert_eq!(at([8, 0]), Action::OrderUp { alone: false }, "two to go: a march with a partner wins too");
        assert_eq!(choose(&r, 0), Action::OrderUp { alone: true }, "Hard doesn't look at the score");
    }

    #[test]
    fn expert_leads_an_ace_nobody_has_led() {
        use Rank::*;
        use Suit::*;
        let hands = [
            vec![c(Ace, Clubs), c(Nine, Clubs), c(King, Spades), c(Queen, Spades), c(Ten, Diamonds)],
            vec![c(Jack, Hearts), c(Ace, Hearts), c(King, Hearts), c(Ace, Spades), c(Nine, Spades)],
            vec![c(Queen, Clubs), c(King, Clubs), c(Ten, Spades), c(Nine, Diamonds), c(Queen, Diamonds)],
            vec![c(Queen, Hearts), c(Ten, Hearts), c(Ten, Clubs), c(King, Diamonds), c(Ace, Diamonds)],
        ];
        let mut r = Round::with_hands(3, hands, c(Nine, Hearts), vec![], true);
        r.trump = Some(Hearts);
        r.maker = Some(1);
        r.phase = Phase::Play;
        r.turn = 0;
        let lead = |level| choose_at(&r, 0, level, Standing::NONE);
        assert_eq!(lead(Difficulty::Expert), Action::Play { card: c(Ace, Clubs) });
        assert_eq!(lead(Difficulty::Hard), Action::Play { card: c(Ten, Diamonds) }, "Hard starts a void");
    }

    #[test]
    fn a_seat_that_cannot_follow_is_marked_void() {
        use Rank::*;
        use Suit::*;
        let hands = [
            vec![c(Ace, Diamonds), c(Nine, Clubs)],
            vec![c(Nine, Spades), c(Ten, Clubs)],
            vec![c(King, Diamonds), c(Queen, Clubs)],
            vec![c(Queen, Diamonds), c(King, Clubs)],
        ];
        let mut r = Round::with_hands(3, hands, c(Nine, Hearts), vec![], true);
        r.trump = Some(Hearts);
        r.maker = Some(0);
        r.phase = Phase::Play;
        r.turn = 0;
        for (seat, card) in [(0, c(Ace, Diamonds)), (1, c(Nine, Spades)), (2, c(King, Diamonds)), (3, c(Queen, Diamonds))] {
            r.apply(seat, Action::Play { card }).unwrap();
        }
        r.advance();
        let tells = Tells::read(&r, Hearts);
        for seat in 0..SEATS {
            for suit in Suit::ALL {
                assert_eq!(tells.void[seat][suit.index()], seat == 1 && suit == Diamonds, "seat {seat}, {suit:?}");
            }
        }
        assert!(!tells.might_hold(1, c(Ten, Diamonds), Hearts));
        assert!(tells.might_hold(3, c(Ten, Diamonds), Hearts));
        // Hearts are trump, so the jack of diamonds is a heart: seat 1 could still have it.
        assert!(tells.might_hold(1, c(Jack, Diamonds), Hearts));
    }

    /// A weak bot's mistakes come from what it can see, not from dice, so the
    /// same game plays out the same way every time.
    #[test]
    fn a_rookie_makes_the_same_mistakes_twice() {
        let rookie = Difficulty::Rookie.skill();
        let moves = || {
            let mut moves = Vec::new();
            play(7, [rookie; 4], |seat, action| moves.push((seat, action)));
            moves
        };
        assert_eq!(moves(), moves());
    }
}
