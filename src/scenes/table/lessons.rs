//! The tutorial's script: what Benny says and asks for, hand by hand. The cards and
//! the bots' plans are in `euchre::tutorial`; `coach.rs` plays this out on the table.
//!
//! The help fades as the match goes on (see [`Level`]):
//! lesson 1 points at every move, lesson 2 asks and corrects, lesson 3 lets you play and
//! comments afterwards, lesson 4 only helps when asked, and lesson 5 is all yours.
//!
//! `*word*` in a line is emphasized. Keep lines short: a note holds three lines at
//! most (`every_line_fits_on_its_note` checks).

use crate::euchre::tutorial::{LESSONS, TARGET, card};
use crate::euchre::{Card, Mirror, ResultKind, Suit};

use super::Bid;
use super::coach::{Among, Beat, Kind, Point, Say, Want, When};

fn talk(when: When, line: &str) -> Beat {
    Beat { when, kind: Kind::Talk, say: Say::Line(line.into()), points: Vec::new() }
}

fn later(when: When, line: fn(&Mirror) -> Option<String>) -> Beat {
    Beat { when, kind: Kind::Talk, say: Say::Later(line), points: Vec::new() }
}

/// Asks for a move. `otherwise` is what Benny says about any other move.
fn task(when: When, want: Want, line: &str, otherwise: &str) -> Beat {
    Beat {
        when,
        kind: Kind::Task { want, wrong: Vec::new(), otherwise: otherwise.into(), praise: None },
        say: Say::Line(line.into()),
        points: Vec::new(),
    }
}

fn quiz(when: When, answer: Card, among: Among, line: &str, otherwise: &str, right: &str) -> Beat {
    Beat {
        when,
        kind: Kind::Quiz { answer, among, wrong: Vec::new(), otherwise: otherwise.into(), right: right.into() },
        say: Say::Line(line.into()),
        points: vec![match among {
            Among::Hand => Point::Hand,
            Among::Trick => Point::Trick,
        }],
    }
}

impl Beat {
    fn at(mut self, points: impl IntoIterator<Item = Point>) -> Beat {
        self.points = points.into_iter().collect();
        self
    }

    /// What Benny says about one particular wrong answer.
    fn but(mut self, wrong_want: Want, line: &str) -> Beat {
        match &mut self.kind {
            Kind::Task { wrong, .. } => wrong.push((wrong_want, line.into())),
            Kind::Quiz { wrong, .. } => {
                if let Want::Card(c) = wrong_want {
                    wrong.push((Want::Card(c), line.into()));
                }
            }
            _ => {}
        }
        self
    }

    /// What Benny says across the table when you get it right.
    fn praise(mut self, line: &'static str) -> Beat {
        if let Kind::Task { praise, .. } = &mut self.kind {
            *praise = Some(line);
        }
        self
    }
}

fn plays(token: &str) -> Want {
    Want::Card(card(token))
}

fn some(tokens: &str) -> Point {
    Point::Cards(tokens.split_whitespace().map(card).collect())
}

/// The lesson's beats, in order. `scores` are the points before this hand.
pub(super) fn beats(lesson: usize, scores: [u32; 2]) -> Vec<Beat> {
    match lesson {
        0 => lesson_one(),
        1 => lesson_two(),
        2 => lesson_three(),
        3 => lesson_four(scores),
        4 => lesson_five(scores),
        _ => extra_hand(scores),
    }
}

fn chapter(lesson: usize, title: &str, line: &str) -> Beat {
    Beat {
        when: When::BeforeDeal,
        kind: Kind::Chapter { tag: format!("LESSON {} OF {LESSONS}", lesson + 1), title: title.into() },
        say: Say::Line(line.into()),
        points: Vec::new(),
    }
}

/// Lesson 1: Benny points at every move and only that move will do.
fn lesson_one() -> Vec<Beat> {
    use When::*;
    let nope = "Hold on, try the one I'm pointing at. You'll see why.";
    vec![
        chapter(0, "PULL UP A CHAIR", "Let's play a hand together. I'll walk you through every move."),
        talk(BeforeDeal, "Hey, glad you made it! I'm *Benny*, your partner tonight. We sit across from each other, and play as a *team*.")
            .at([Point::Plate(0), Point::Plate(2)]),
        talk(BeforeDeal, "*Rosa* and *Jules* are the other team. They're friendly. Mostly.")
            .at([Point::Plate(1), Point::Plate(3)]),
        talk(Dealt, "These five are yours. Euchre only uses 9s and up, so the whole deck is just 24 cards.").at([Point::Hand]),
        talk(Dealt, &format!("A hand is five *tricks*. In each one we all play a card, and the best card wins. Most tricks scores. First team to {TARGET} wins!"))
            .at([Point::Pad]),
        talk(Dealt, "This is the *up card*. Its suit might become *trump*, the power suit: any trump beats any other suit's card.")
            .at([Point::UpCard, Point::Medallion]),
        talk(Dealt, "Starting left of the dealer, each of us can *pass* or *order it up*, which makes ♥ trump. Jules is first.")
            .at([Point::Plate(3)]),
        talk(Move(1), "Your turn. Look at those hearts! The J♦ counts as one too, and I'll show you why soon.")
            .at([some("Jh Jd 9h")]),
        task(Move(1), Want::Bid(Bid::OrderUp), "Hearts are great for us. Press *ORDER UP* to make ♥ trump.", "Not this time. We want hearts, so press ORDER UP.")
            .praise("Pick it up? You bet!"),
        talk(Move(2), "I'm the dealer, so the Q♥ comes to me, and I throw one card away face down. Everyone still has five.")
            .at([Point::Plate(2)]),
        talk(Move(3), "We picked trump, so we're the *makers*, and we need 3 of the 5 tricks. Left of the dealer leads: here comes Jules.")
            .at([Point::Pad, Point::Medallion]),
        task(Move(4), plays("9s"), "Jules led a spade, so you must *follow suit*. Your only spade is the 9♠. Play it.", "You must follow suit: a spade if you have one. That's the 9♠."),
        talk(TrickWon(1), "No trump this time, so the highest card of the suit led wins. Jules's ace takes it, and the winner leads next.")
            .at([Point::Trick]),
        talk(Move(8), "Another spade, and you're out of them. When you can't follow suit, you can play *anything*.").at([Point::Hand]),
        task(Move(8), plays("9h"), "That includes trump! Your smallest will do. Play the 9♥.", nope).praise("Oh, nice!"),
        talk(TrickWon(2), "Your little 9♥ just beat a king! That's trump for you. You won, so you lead next.").at([Point::Trick]),
        talk(Move(11), "The jack of trump is the *right bower*, the best card in the game. It beats even the ace.").at([some("Jh")]),
        task(Move(11), plays("Jh"), "Lead your J♥. Everyone has to play a heart if they can.", nope),
        talk(TrickWon(3), "All hearts, and your bower beat every one of them. That's two tricks for us.").at([Point::Trick, Point::Pad]),
        talk(Move(15), "Now the odd one. The other jack of the same color is the *left bower*, the second best card.").at([some("Jd")]),
        task(Move(15), plays("Jd"), "It counts as trump, not as a diamond! Lead your J♦.", nope),
        talk(TrickWon(4), "Jules threw an *ace* and lost to a jack! Your J♦ is a heart this hand. That's our 3 tricks.").at([Point::Trick, Point::Pad]),
        task(Move(19), plays("Tc"), "One card left. Play your 10♣ and let's see what I've got.", nope),
        talk(TrickWon(5), "My A♣ takes it! A partner's trick counts for you too. So that's four.").at([Point::Trick, Point::Pad]),
        talk(HandDone, "3 or 4 tricks scores *1 point*. All 5 is a *march*, and scores 2. Let's see how we did!").at([Point::Pad]),
    ]
}

/// Lesson 2: you choose, and a wrong choice gets an explanation and another try.
/// A second wrong answer, or a long wait, brings the pointer back.
fn lesson_two() -> Vec<Beat> {
    use When::*;
    let spades = Bid::Suit(Suit::Spades);
    vec![
        chapter(1, "YOU CALL IT", "This time you make the choices. Slip up and I'll tell you why, and you can try again."),
        talk(Dealt, "The deal moves left each hand, so Jules deals now, and you speak first.").at([Point::Plate(3)]),
        task(Move(0), Want::Bid(Bid::Pass), "Do you want hearts as trump? Count yours, then press ORDER UP or PASS.", "One heart is too few, and the 10♥ would go to Jules on the other team. Let's pass."),
        talk(Move(4), "Everybody passed, so the card is turned down. Now we can each *name* any other suit, or pass again.")
            .at([Point::Medallion]),
        task(Move(4), Want::Bid(spades), "Which suit gives you the most trump? Don't forget the jack of the same color!", "Count again. Both of your jacks would be trump in ♠, and so would the 9♠.")
            .but(Want::Bid(Bid::Suit(Suit::Clubs)), "Close! ♣ gives you the two jacks. But ♠ gives you those two and the 9♠ as well.")
            .but(Want::Bid(Bid::Pass), "That hand is too good to pass! Which suit gives you the most trump?")
            .praise("Spades it is!"),
        quiz(Move(5), card("Jc"), Among::Hand, "Pop quiz! ♠ is trump. Which of your cards is the *left bower*?", "The left bower is a jack, the other one of trump's color.", "That's it! Your J♣ is a spade for this hand. Second best card in the game.")
            .but(plays("Js"), "So close! The J♠ is the *right* bower, the best one. The left is the other black jack."),
        task(Move(5), plays("Js"), "You're left of the dealer, so you lead. We made trump: start by pulling theirs out.", "Lead your very best trump. Nothing can beat it, and they have to play theirs.")
            .praise("That's the way!"),
        task(Move(9), plays("Jc"), "You won, so you lead again. Which trump is the strongest one left?", "The left bower is second only to the right, and the right is gone. Lead the J♣."),
        task(Move(13), plays("Qh"), "Keep your last trump for later. Which side card has the better chance?", "The Q♥ outranks your other side card, so try that one.")
            .but(plays("9s"), "Hang on to the 9♠: late in a hand, a little trump can steal a trick."),
        task(Move(20), plays("Td"), "My A♣ is winning this one, and you're out of clubs. What do you play?", "Your 10♦ can go here.")
            .but(plays("9s"), "Don't trump your partner! I've already got this one. Save it and throw your 10♦.")
            .praise("Thanks, partner!"),
        quiz(TrickEnd(5), card("9s"), Among::Trick, "Last trick, and it's a good one. Which card wins it?", "Check for trump first.", "Yes! Even a 9 of trump beats an ace. Four tricks, another point!")
            .but(plays("Ad"), "It's the highest diamond, but somebody played trump. Trump beats any other suit."),
    ]
}

/// Lesson 3: you play as you like. Benny gives a tip, then says something
/// afterwards if a move cost us (`coach.rs` works those out from the cards).
fn lesson_three() -> Vec<Beat> {
    use When::*;
    vec![
        chapter(2, "DEFEND THE TABLE", "Play it however you like now. I'll give you a tip, and tell you afterwards how it went."),
        talk(Dealt, "It's your deal. The up card goes to you if anyone orders it up. Rosa speaks first, and she looks keen.")
            .at([Point::UpCard, Point::Plate(1)]),
        talk(Move(1), "Rosa ordered you up! The 9♦ joins your hand, and now you throw one card away.").at([Point::Hand]),
        talk(Move(2), "Rosa called trump, so *we're defending*. If they win fewer than 3 tricks they're *euchred*, and we score 2!")
            .at([Point::Pad]),
        later(HandDone, |m| {
            let result = m.round.result()?;
            Some(match result.kind {
                ResultKind::Euchred => "*Euchred!* They called trump and couldn't back it up, so we get 2 points. That's the risk of calling.".to_string(),
                _ => "They made it this time. It happens: whoever calls trump usually has the cards for it.".to_string(),
            })
        })
        .at([Point::Pad]),
    ]
}

/// Lesson 4: one last idea, then you're on your own unless you ask.
fn lesson_four(scores: [u32; 2]) -> Vec<Beat> {
    use When::*;
    let alone_wins = scores[0] + 4 >= TARGET;
    let payoff = if alone_wins {
        "Press *ALONE*, then your bid. I sit out, and if you take all five that's *4 points*: enough to win the game!"
    } else {
        "Press *ALONE*, then your bid. I sit out, and if you take all five that's *4 points*."
    };
    vec![
        chapter(3, "THE BIG SWING", "One last new idea, and then I'll keep quiet unless you ask."),
        talk(Dealt, "With a monster hand, you can *go alone*. It's just you against Rosa and Jules.").at([Point::Plate(2)]),
        talk(Dealt, payoff),
        talk(Dealt, "Only do it if you're sure, though. From here on it's your call. Stuck? Press *HINT*.").at([Point::Hint]),
        later(HandDone, |m| {
            let result = m.round.result()?;
            if result.maker != 0 && result.maker != 2 {
                return Some("You held the top four clubs there! Take a look at the jacks next time: they're stronger than they seem.".to_string());
            }
            match (result.alone, result.kind) {
                (true, ResultKind::March) => None,
                (true, _) => Some("Brave! Alone you need all five for the big score, but we still got on the board.".to_string()),
                (false, ResultKind::March) => Some("All five! With cards like that, going alone would have made it 4 points instead of 2.".to_string()),
                (false, _) => Some("We got there! With the top four clubs, you could even have tried it alone.".to_string()),
            }
        })
        .at([Point::Pad]),
    ]
}

/// Lesson 5: not a word from Benny.
fn lesson_five(scores: [u32; 2]) -> Vec<Beat> {
    let need = TARGET.saturating_sub(scores[0]);
    let s = if need == 1 { "" } else { "s" };
    vec![chapter(4, "YOUR TABLE", &format!("No tips, no hints, no help. We need {need} more point{s}. Show me what you've got!"))]
}

/// Any hands the match needs after the lessons.
fn extra_hand(scores: [u32; 2]) -> Vec<Beat> {
    let need = TARGET.saturating_sub(scores[0]);
    let s = if need == 1 { "" } else { "s" };
    vec![Beat {
        when: When::BeforeDeal,
        kind: Kind::Chapter { tag: "ONE MORE HAND".into(), title: "STILL ANYBODY'S GAME".into() },
        say: Say::Line(format!("First to {TARGET} wins, and we need {need} more point{s}. You've got this.")),
        points: Vec::new(),
    }]
}

#[cfg(test)]
mod tests {
    use super::super::coach::{NOTE_TEXT_W, plain_text};
    use super::*;
    use crate::euchre::Action;
    use crate::euchre::protocol::{ClientMsg, Move, ServerMsg};
    use crate::euchre::tutorial::TutorialRoom;

    fn wrap_count(text: &str) -> usize {
        let mut lines = 1;
        let mut line = String::new();
        for word in plain_text(text).split(' ') {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if crate::text::measure(&candidate) > NOTE_TEXT_W && !line.is_empty() {
                lines += 1;
                line = word.to_string();
            } else {
                line = candidate;
            }
        }
        lines
    }

    #[test]
    fn every_line_fits_on_its_note() {
        for lesson in 0..=LESSONS {
            for beat in beats(lesson, [4, 0]) {
                let mut lines: Vec<String> = Vec::new();
                if let Say::Line(l) = &beat.say {
                    lines.push(l.clone());
                }
                let limit = match &beat.kind {
                    Kind::Chapter { .. } => continue,
                    Kind::Talk => 3,
                    Kind::Task { wrong, otherwise, .. } => {
                        lines.push(otherwise.clone());
                        lines.extend(wrong.iter().map(|(_, l)| l.clone()));
                        2
                    }
                    Kind::Quiz { wrong, otherwise, right, .. } => {
                        lines.push(otherwise.clone());
                        lines.push(right.clone());
                        lines.extend(wrong.iter().map(|(_, l)| l.clone()));
                        3
                    }
                };
                for line in lines {
                    let n = wrap_count(&line);
                    assert!(n <= limit, "lesson {lesson}: {n} lines (at most {limit}): {line}");
                }
            }
        }
    }

    #[test]
    fn every_character_benny_says_is_in_the_font() {
        for lesson in 0..=LESSONS {
            for beat in beats(lesson, [4, 0]) {
                if let Say::Line(l) = &beat.say {
                    for ch in plain_text(l).chars() {
                        let known = ch == ' ' || "♣♦♠♥".contains(ch) || crate::text::glyph_rows(ch).is_some() || crate::text::glyph_rows(ch.to_ascii_uppercase()).is_some();
                        assert!(known, "no glyph for {ch:?} in: {l}");
                    }
                }
            }
        }
    }

    /// Plays what lessons 1 and 2 ask for against the real tutorial room, in order,
    /// and checks that each task and quiz is due exactly when the script says.
    #[test]
    fn the_guided_lessons_ask_for_legal_moves_at_the_right_moments() {
        for lesson in [0, 1] {
            let mut room = TutorialRoom::new(lesson, [0, 0], 1);
            let Some(ServerMsg::Deal(hand)) = room.poll() else { panic!("no deal") };
            let mut m = Mirror::new(&hand).unwrap();
            let mut pending: Vec<Move> = Vec::new();
            let take = |room: &mut TutorialRoom, pending: &mut Vec<Move>| {
                while let Some(msg) = room.poll() {
                    if let ServerMsg::Acted(mv) = msg {
                        pending.push(mv);
                    }
                }
            };
            take(&mut room, &mut pending);
            for beat in beats(lesson, [0, 0]) {
                let When::Move(n) = beat.when else { continue };
                // Catch the table up to the moment this beat waits for.
                while m.moves() < n {
                    if m.round.phase == crate::euchre::Phase::TrickDone {
                        m.advance();
                    }
                    assert!(!pending.is_empty(), "lesson {lesson}: nothing more to play before move {n}");
                    m.reveal(pending.remove(0)).unwrap();
                }
                if m.round.phase == crate::euchre::Phase::TrickDone {
                    m.advance();
                }
                match &beat.kind {
                    Kind::Task { want, wrong, .. } => {
                        assert_eq!(m.round.to_act(), Some(0), "lesson {lesson}: a task at move {n} but it's not your turn");
                        let action = match want {
                            Want::Card(card) => {
                                assert!(m.round.legal_plays(0).contains(card), "lesson {lesson}: {} isn't legal at move {n}", card.label());
                                Action::Play { card: *card }
                            }
                            Want::Bid(Bid::Pass) => Action::Pass,
                            Want::Bid(Bid::OrderUp) => Action::OrderUp { alone: false },
                            Want::Bid(Bid::Suit(suit)) => Action::Call { suit: *suit, alone: false },
                            Want::Bid(Bid::Alone) => panic!("alone isn't a move"),
                        };
                        for (w, _) in wrong {
                            if let Want::Card(card) = w {
                                assert!(m.round.hands[0].contains(card), "lesson {lesson}: a wrong answer for a card you don't hold");
                            }
                        }
                        m.act(action).unwrap();
                        room.handle(ClientMsg::Act { action });
                        take(&mut room, &mut pending);
                    }
                    Kind::Quiz { answer, among: Among::Hand, .. } => assert!(m.round.hands[0].contains(answer)),
                    _ => {}
                }
            }
        }
    }

    /// Everything the script asks you to do, in order. Lesson 2's last card isn't
    /// asked for (it's your only one), so it's added here.
    fn intended_moves(lesson: usize) -> Vec<Action> {
        let mut moves: Vec<Action> = beats(lesson, [0, 0])
            .iter()
            .filter_map(|b| match &b.kind {
                Kind::Task { want: Want::Bid(Bid::OrderUp), .. } => Some(Action::OrderUp { alone: false }),
                Kind::Task { want: Want::Bid(Bid::Pass), .. } => Some(Action::Pass),
                Kind::Task { want: Want::Bid(Bid::Suit(suit)), .. } => Some(Action::Call { suit: *suit, alone: false }),
                Kind::Task { want: Want::Card(card), .. } => Some(Action::Play { card: *card }),
                _ => None,
            })
            .collect();
        if lesson == 1 {
            moves.push(Action::Play { card: card("9s") });
        }
        moves
    }

    #[test]
    fn the_trick_quiz_has_the_right_answer() {
        // Lesson 2's last trick, as the room plays it: the answer has to really win.
        let mut room = TutorialRoom::new(1, [0, 0], 1);
        let Some(ServerMsg::Deal(hand)) = room.poll() else { panic!("no deal") };
        let mut m = Mirror::new(&hand).unwrap();
        for action in intended_moves(1) {
            while let Some(msg) = room.poll() {
                if let ServerMsg::Acted(mv) = msg {
                    if m.round.phase == crate::euchre::Phase::TrickDone {
                        m.advance();
                    }
                    m.reveal(mv).unwrap();
                }
            }
            if m.round.phase == crate::euchre::Phase::TrickDone {
                m.advance();
            }
            m.act(action).unwrap();
            room.handle(ClientMsg::Act { action });
        }
        // The last move wasn't yours: Rosa still had a card to play. Then comes the next deal.
        while let Some(ServerMsg::Acted(mv)) = room.poll() {
            m.reveal(mv).unwrap();
        }
        let (winner, card_won) = m.round.trick_leader().unwrap();
        assert_eq!((winner, card_won), (0, card("9s")));
    }
}
