//! The room behind the tutorial: a match played to [`TARGET`] with hands dealt
//! from a script, so each lesson has the cards it needs.
//!
//! It speaks the same protocol as a real room, so the table plays a lesson exactly as
//! it plays any other game: it hears about deals and other seats' moves as
//! [`ServerMsg`]s and only knows its own cards. Nothing about the rules changes.
//!
//! Each seat has a plan: the moves the lesson wants from it, in the order it would
//! like to make them. A bot takes the first planned move the rules allow, and falls
//! back on the normal AI when none fits. So when the player does what a lesson expects
//! the hand goes exactly as written, and when they don't (later lessons let them) the
//! bots just play on.
//!
//! Past the scripted hands the deals are random, so a match always reaches its end.

use std::collections::VecDeque;

use crate::ai;
use crate::card::Card;
use crate::protocol::{ClientMsg, ErrorCode, HandStart, Move, ServerMsg, Shown};
use crate::rng::Rng;
use crate::round::{Action, Phase, Round, Seat, next_seat};

/// Points to win the tutorial's match: a lone march can finish it from four.
pub const TARGET: u32 = 7;
/// How many hands are scripted.
pub const LESSONS: usize = 5;
/// The player's seat.
const YOU: Seat = 0;

/// The dealer of the first hand: your partner. The deal then passes left as usual, so
/// lesson 3 is yours to deal.
const FIRST_DEALER: Seat = 2;

/// One scripted hand.
struct Script {
    /// `[you, Rosa, Benny, Jules]`.
    hands: [&'static str; 4],
    up: &'static str,
    kitty: &'static str,
    /// What each bot would like to do, in order: `P` pass, `O` order up, `Xd9` discard
    /// the 9 of diamonds (`X` then a card), or a card to play.
    plans: [&'static str; 4],
}

const SCRIPTS: [Script; LESSONS] = [
    // Benny deals. You order hearts up and take 4 of 5 tricks.
    Script {
        hands: ["Jh Jd 9h 9s Tc", "Qs Js Th Qd 9c", "Ts Ac 9d Kd Td", "As Ks Kh Ad Kc"],
        up: "Qh",
        kitty: "Ah Jc Qc",
        plans: ["", "Qs Js Th Qd 9c", "XTd Ts 9d Qh Kd Ac", "P As Ks Kh Ad Kc"],
    },
    // Jules deals. Everyone passes, you name spades and take 4 of 5.
    Script {
        hands: ["Js Jc 9s Qh Td", "Ts Ks Ah Kc Qd", "Qs Tc 9h Ac Ad", "As 9d Kh 9c Kd"],
        up: "Th",
        kitty: "Qc Jh Jd",
        plans: ["", "P Ts Ks Ah Kc Qd", "P Qs Tc 9h Ac Ad", "P As 9d Kh 9c Kd"],
    },
    // You deal. Rosa orders up on a hand that isn't good enough, and you can euchre her.
    Script {
        hands: ["Jd Ad As Qc 9h", "Jh Kd Qd Kc Ts", "Td Ah Ks 9s Ac", "Kh Qs 9c Jc Th"],
        up: "9d",
        kitty: "Qh Js Tc",
        plans: ["", "O Jh", "", ""],
    },
    // Rosa deals. You hold the top four clubs: a hand to go alone on.
    Script {
        hands: ["Jc Js Ac Kc Ad", "Tc Kh Qs Jd Ts", "As Ah Kd 9s Th", "Qc Ks Qh 9d Td"],
        up: "9c",
        kitty: "9h Jh Qd",
        plans: ["", "", "P", "P Ks"],
    },
    // Benny deals. A good hand, and nobody to tell you what to do with it.
    Script {
        hands: ["Kh Qh Jd As Tc", "Qs Js Kc Qd Ad", "Jh Th Ac Ks 9d", "9h Kd Td 9s 9c"],
        up: "Ah",
        kitty: "Ts Jc Qc",
        plans: ["", "", "X9d", "P"],
    },
];

/// The card a two-letter token names (see [`Card::token`]).
pub fn card(token: &str) -> Card {
    Card::parse(token).unwrap_or_else(|| panic!("not a card: {token}"))
}

fn cards(tokens: &str) -> Vec<Card> {
    tokens.split_whitespace().map(card).collect()
}

fn plan(tokens: &str) -> VecDeque<Action> {
    tokens
        .split_whitespace()
        .map(|t| match t {
            "P" => Action::Pass,
            "O" => Action::OrderUp { alone: false },
            _ if t.starts_with('X') => Action::Discard { card: card(&t[1..]) },
            _ => Action::Play { card: card(t) },
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct TutorialRoom {
    round: Round,
    scores: [u32; 2],
    /// Which hand this is, from 0.
    hand: usize,
    plans: [VecDeque<Action>; 4],
    rng: Rng,
    outbox: VecDeque<ServerMsg>,
}

impl TutorialRoom {
    /// A match starting at `lesson` (from 0) with the score so far, as when
    /// picking the tutorial back up.
    pub fn new(lesson: usize, scores: [u32; 2], seed: u64) -> TutorialRoom {
        let lesson = lesson.min(LESSONS - 1);
        let dealer = (FIRST_DEALER + lesson) % 4;
        let (round, plans) = deal(lesson, dealer, &mut Rng::new(seed));
        let mut room = TutorialRoom { round, scores, hand: lesson, plans, rng: Rng::new(seed), outbox: VecDeque::new() };
        room.send_deal();
        room.settle();
        room
    }

    pub fn poll(&mut self) -> Option<ServerMsg> {
        self.outbox.pop_front()
    }

    pub fn handle(&mut self, msg: ClientMsg) {
        let ClientMsg::Act { action } = msg else { return };
        match self.round.apply(YOU, action) {
            Ok(()) => self.settle(),
            Err(e) => {
                let code = ErrorCode::Rule(e);
                self.outbox.push_back(ServerMsg::Error { code, message: code.describe().into() });
            }
        }
    }

    fn winner(&self) -> bool {
        self.scores.iter().any(|&s| s >= TARGET)
    }

    /// Runs everything up to the player's next move, as `Room::settle` does.
    fn settle(&mut self) {
        loop {
            if self.winner() {
                return;
            }
            match self.round.phase {
                Phase::TrickDone => self.round.advance(),
                Phase::HandOver => {
                    if let Some(result) = self.round.result() {
                        self.scores[result.scoring_team] += result.points;
                    }
                    if self.winner() {
                        return;
                    }
                    self.next_hand(1);
                }
                // Only without "stick the dealer", which the tutorial always plays with.
                Phase::Misdeal => self.next_hand(0),
                _ => {
                    let Some(seat) = self.round.to_act() else { return };
                    if seat == YOU {
                        return;
                    }
                    let action = self.bot_move(seat);
                    if let Err(e) = self.round.apply(seat, action) {
                        debug_assert!(false, "bot move refused: {e:?}");
                        return;
                    }
                    self.outbox.push_back(ServerMsg::Acted(Move { seat, action: Shown::of(action, false) }));
                }
            }
        }
    }

    fn next_hand(&mut self, by: usize) {
        self.hand += by;
        let dealer = next_seat(self.round.dealer);
        (self.round, self.plans) = deal(self.hand, dealer, &mut self.rng);
        self.send_deal();
    }

    /// The first thing the seat's plan wants that the rules allow, else the AI's choice.
    fn bot_move(&mut self, seat: Seat) -> Action {
        let round = &self.round;
        let fits = |action: &Action| round.clone().apply(seat, *action).is_ok();
        match self.plans[seat].iter().position(fits) {
            Some(i) => self.plans[seat].remove(i).expect("just found"),
            None => ai::choose(round, seat),
        }
    }

    fn send_deal(&mut self) {
        self.outbox.push_back(ServerMsg::Deal(HandStart {
            game: 1,
            target: TARGET,
            stick_the_dealer: true,
            scores: self.scores,
            hand_number: self.hand as u32 + 1,
            dealer: self.round.dealer,
            cards: self.round.hands[YOU].clone(),
            up_card: self.round.up_card,
        }));
    }
}

/// Hand `index`: scripted, or shuffled once the script runs out.
fn deal(index: usize, dealer: Seat, rng: &mut Rng) -> (Round, [VecDeque<Action>; 4]) {
    let Some(s) = SCRIPTS.get(index) else {
        return (Round::deal(dealer, rng, true), Default::default());
    };
    let round = Round::with_hands(dealer, s.hands.map(cards), card(s.up), cards(s.kitty), true);
    (round, s.plans.map(plan))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Mirror;
    use crate::round::ResultKind;

    #[test]
    fn every_script_deals_the_whole_deck_exactly_once() {
        for (i, s) in SCRIPTS.iter().enumerate() {
            let mut all = cards(s.up);
            all.extend(cards(s.kitty));
            for hand in s.hands {
                let hand = cards(hand);
                assert_eq!(hand.len(), 5, "lesson {i}");
                all.extend(hand);
            }
            all.sort();
            all.dedup();
            assert_eq!(all.len(), 24, "lesson {i} doesn't use the whole deck");
        }
    }

    /// One line of a match, played through the mirror the table would hold, so
    /// anything the real table couldn't follow fails here.
    struct Walk {
        room: TutorialRoom,
        mirror: Mirror,
    }

    impl Walk {
        fn start(lesson: usize, scores: [u32; 2]) -> Walk {
            let mut room = TutorialRoom::new(lesson, scores, 1);
            let Some(ServerMsg::Deal(hand)) = room.poll() else { panic!("no deal") };
            let mirror = Mirror::new(&hand).unwrap();
            let mut walk = Walk { room, mirror };
            walk.catch_up();
            walk
        }

        /// Takes in everything the room said, stopping at a new deal.
        fn catch_up(&mut self) -> Option<HandStart> {
            while let Some(msg) = self.room.poll() {
                match msg {
                    ServerMsg::Acted(mv) => {
                        if self.mirror.round.phase == Phase::TrickDone {
                            self.mirror.advance();
                        }
                        self.mirror.reveal(mv).unwrap();
                    }
                    ServerMsg::Deal(hand) => return Some(hand),
                    other => panic!("unexpected {other:?}"),
                }
            }
            None
        }

        fn act(&mut self, action: Action) -> Option<HandStart> {
            self.mirror.act(action).unwrap();
            self.room.handle(ClientMsg::Act { action });
            let next = self.catch_up();
            if self.mirror.round.phase == Phase::TrickDone {
                self.mirror.advance();
            }
            next
        }
    }

    /// The moves a lesson's script expects from you.
    fn intended(lesson: usize) -> Vec<Action> {
        let call = |token: &str| Action::Call { suit: card(token).suit, alone: false };
        let order = Action::OrderUp { alone: false };
        match lesson {
            0 => std::iter::once(order).chain(plan("9s 9h Jh Jd Tc")).collect(),
            1 => [Action::Pass, call("Js")].into_iter().chain(plan("Js Jc Qh Td 9s")).collect(),
            _ => panic!("only the first two lessons are on rails"),
        }
    }

    fn walk_intended(lesson: usize, scores: [u32; 2]) -> (Round, [u32; 2]) {
        let mut walk = Walk::start(lesson, scores);
        let mut next = None;
        for action in intended(lesson) {
            assert!(next.is_none(), "the next hand was dealt before lesson {lesson} finished");
            assert_eq!(walk.mirror.round.to_act(), Some(YOU));
            next = walk.act(action);
        }
        assert!(next.is_some(), "no deal after lesson {lesson}");
        let scored = walk.mirror.score_hand().unwrap();
        assert_eq!(scored.points, 1);
        (walk.mirror.round.clone(), walk.room.scores)
    }

    #[test]
    fn lesson_one_goes_as_written() {
        let (round, scores) = walk_intended(0, [0, 0]);
        assert_eq!(scores, [1, 0]);
        assert_eq!(round.trick_winners, vec![3, 0, 0, 0, 2], "Jules, you three times, then Benny");
        assert_eq!(round.team_tricks(0), 4);
    }

    #[test]
    fn lesson_two_goes_as_written() {
        let (round, scores) = walk_intended(1, [1, 0]);
        assert_eq!(scores, [2, 0]);
        assert_eq!(round.trump, Some(card("Js").suit));
        assert_eq!(round.trick_winners, vec![0, 0, 1, 2, 0], "you twice, Rosa, Benny, then your last trump");
    }

    /// Every decision the player could make from here on, with the bots replying.
    fn every_line(walk: &Walk, visit: &mut impl FnMut(&Walk, Option<&HandStart>)) {
        let r = &walk.mirror.round;
        let moves: Vec<Action> = match r.phase {
            Phase::Play => r.legal_plays(YOU).into_iter().map(|card| Action::Play { card }).collect(),
            Phase::DealerDiscard => r.hands[YOU].iter().map(|&card| Action::Discard { card }).collect(),
            Phase::OrderUp => vec![Action::Pass, Action::OrderUp { alone: false }, Action::OrderUp { alone: true }],
            Phase::NameTrump => {
                let mut bids: Vec<Action> = crate::Suit::ALL
                    .into_iter()
                    .filter(|&s| s != r.up_card.suit)
                    .flat_map(|suit| [Action::Call { suit, alone: false }, Action::Call { suit, alone: true }])
                    .collect();
                if r.dealer != YOU {
                    bids.push(Action::Pass);
                }
                bids
            }
            other => panic!("nothing for the player to do in {other:?}"),
        };
        for action in moves {
            let mut walk = Walk { room: walk.room.clone(), mirror: walk.mirror.clone() };
            let next = walk.act(action);
            if next.is_some() || walk.room.winner() {
                visit(&walk, next.as_ref());
            } else {
                every_line(&walk, visit);
            }
        }
    }

    #[test]
    fn every_line_of_the_free_lessons_finishes_cleanly() {
        for lesson in 2..LESSONS {
            let mut lines = 0;
            let start = Walk::start(lesson, [4, 0]);
            every_line(&start, &mut |walk, next| {
                lines += 1;
                let mut mirror = walk.mirror.clone();
                let result = mirror.score_hand().unwrap_or_else(|| panic!("lesson {lesson}: the hand isn't over"));
                assert_eq!(mirror.scores, walk.room.scores, "lesson {lesson}: the table and the room disagree on the score");
                assert!(result.points > 0);
                if let Some(hand) = next {
                    assert_eq!(hand.hand_number as usize, lesson + 2);
                    assert_eq!(hand.dealer, (mirror.round.dealer + 1) % 4);
                    assert_eq!(hand.scores, walk.room.scores);
                }
            });
            assert!(lines > 10, "lesson {lesson} has only {lines} lines");
        }
    }

    /// Lesson 3 is about euchring Rosa. A bad enough line can still let her make it,
    /// which is fine and Benny has words for it, but it has to be hard to do.
    #[test]
    fn lesson_three_usually_euchres_rosa() {
        let (mut euchres, mut lines) = (0, 0);
        every_line(&Walk::start(2, [2, 0]), &mut |walk, _| {
            lines += 1;
            let mut mirror = walk.mirror.clone();
            assert_eq!(mirror.round.maker, Some(1));
            if mirror.score_hand().unwrap().kind == ResultKind::Euchred {
                euchres += 1;
            }
        });
        println!("lesson 3: {euchres} of {lines} lines euchre Rosa");
        assert!(euchres * 10 >= lines * 6, "only {euchres} of {lines} lines euchre Rosa");
    }

    /// Lesson 4's hand: going alone can win the match on the spot from 4-0, as Benny says.
    #[test]
    fn lesson_four_can_end_the_match_alone() {
        let mut walk = Walk::start(3, [4, 0]);
        assert_eq!(walk.mirror.round.to_act(), Some(YOU));
        assert!(walk.act(Action::OrderUp { alone: true }).is_none());
        for token in ["Kc", "Jc", "Js", "Ac", "Ad"] {
            walk.act(Action::Play { card: card(token) });
        }
        assert_eq!(walk.room.scores, [8, 0]);
        assert!(walk.room.winner());
    }

    #[test]
    fn a_match_always_reaches_its_end_past_the_script() {
        for seed in 0..50 {
            let mut room = TutorialRoom::new(4, [3, 3], seed);
            let mut mirror = None;
            let mut guard = 0;
            loop {
                guard += 1;
                assert!(guard < 2000, "seed {seed} never finished");
                while let Some(msg) = room.poll() {
                    match msg {
                        ServerMsg::Deal(hand) => mirror = Some(Mirror::new(&hand).unwrap()),
                        ServerMsg::Acted(mv) => {
                            let m: &mut Mirror = mirror.as_mut().unwrap();
                            if m.round.phase == Phase::TrickDone {
                                m.advance();
                            }
                            m.reveal(mv).unwrap();
                        }
                        other => panic!("unexpected {other:?}"),
                    }
                }
                if room.winner() {
                    break;
                }
                let m = mirror.as_mut().unwrap();
                if m.round.phase == Phase::TrickDone {
                    m.advance();
                }
                let action = ai::choose(&m.round, YOU);
                m.act(action).unwrap();
                room.handle(ClientMsg::Act { action });
            }
            assert!(room.scores.iter().any(|&s| s >= TARGET));
        }
    }

    #[test]
    fn picking_up_at_a_later_lesson_keeps_the_deal_moving_left() {
        for lesson in 0..LESSONS {
            let room = TutorialRoom::new(lesson, [0, 0], 1);
            assert_eq!(room.round.dealer, (FIRST_DEALER + lesson) % 4, "lesson {lesson}");
        }
    }
}
