//! The tutorial's coach: Benny, your partner, teaching across the table.
//!
//! A lesson is a queue of [`Beat`]s from `lessons.rs`, each waiting for its moment in
//! the hand ([`When`]). The table asks for the next one whenever it goes idle, so the
//! real game keeps its own pacing and nothing here touches the rules.
//!
//! - A **talk** stops the table until you press on, and can spotlight things.
//! - A **task** waits for your move, and only that move will do. Anything else gets an
//!   explanation instead of being played.
//! - A **quiz** stops the table and asks you to point at a card.
//! - Tips, hints and comments come from `euchre::advice`, worked out from the cards.
//!
//! How much of this you get depends on the lesson's [`Level`]. Without a coach (every
//! other game) none of these paths run.

use std::collections::VecDeque;

use love2drs::prelude::*;

use crate::device;
use crate::euchre::advice::{self, Verdict};
use crate::euchre::{Action, Card, Mirror, Phase, Round, Seat, team_of};
use crate::juice::Spring;
use crate::palette;
use crate::scene::{Nav, Shared, Transition};
use crate::scenes::title::Title;
use crate::settings::Settings;
use crate::sfx::Sound;
use crate::ui::{Button, Rect};

use super::layout::{self, Target};
use super::{Bid, Cue, HUMAN, Place, Prompt, Table, draw, lessons};

/// How much help a lesson gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd)]
pub(super) enum Level {
    /// Points at every move, and only that move will do.
    Show,
    /// Asks. A wrong answer gets an explanation and another try.
    Ask,
    /// Any legal move goes. Tips before it, comments after it.
    Review,
    /// Only helps when you press HINT.
    Hint,
    /// Not a word.
    Quiet,
}

impl Level {
    fn of(lesson: usize) -> Level {
        match lesson {
            0 => Level::Show,
            1 => Level::Ask,
            2 => Level::Review,
            3 => Level::Hint,
            _ => Level::Quiet,
        }
    }
}

/// When a beat is due.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum When {
    /// As soon as the table is idle.
    Now,
    /// The deck is gathered, before the shuffle.
    BeforeDeal,
    /// Dealt, up card showing, nobody has moved.
    Dealt,
    /// Just before move `n` of the hand (from 0, counting everyone).
    Move(usize),
    /// Trick `k` (from 1) is complete, before showing who won it.
    TrickEnd(u8),
    /// Trick `k`'s winner is glowing, before the cards are collected.
    TrickWon(u8),
    /// All tricks collected, before the result.
    HandDone,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Want {
    Bid(Bid),
    Card(Card),
}

impl Want {
    fn point(self) -> Point {
        match self {
            Want::Bid(b) => Point::Bid(b),
            Want::Card(c) => Point::Cards(vec![c]),
        }
    }

    fn action(self, alone: bool) -> Option<Action> {
        match self {
            Want::Bid(Bid::Pass) => Some(Action::Pass),
            Want::Bid(Bid::OrderUp) => Some(Action::OrderUp { alone }),
            Want::Bid(Bid::Suit(suit)) => Some(Action::Call { suit, alone }),
            Want::Bid(Bid::Alone) => None,
            Want::Card(card) => Some(Action::Play { card }),
        }
    }
}

/// Something on the table Benny is talking about.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Point {
    Plate(Seat),
    Hand,
    Cards(Vec<Card>),
    UpCard,
    Trick,
    Pad,
    Medallion,
    Hint,
    Bid(Bid),
}

/// Where a quiz's answer is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Among {
    Hand,
    Trick,
}

pub(super) enum Say {
    Line(String),
    /// Worked out when it's due, from how the hand went. `None` skips the beat.
    Later(fn(&Mirror) -> Option<String>),
}

pub(super) enum Kind {
    Chapter { tag: String, title: String },
    Talk,
    Task { want: Want, wrong: Vec<(Want, String)>, otherwise: String, praise: Option<&'static str> },
    Quiz { answer: Card, among: Among, wrong: Vec<(Want, String)>, otherwise: String, right: String },
}

pub(super) struct Beat {
    pub when: When,
    pub kind: Kind,
    pub say: Say,
    pub points: Vec<Point>,
}

/// What's on the table now.
pub(super) enum Current {
    Idle,
    Chapter,
    Talk,
    Task { want: Want, wrong: Vec<(Want, String)>, otherwise: String, praise: Option<&'static str> },
    Quiz { answer: Card, among: Among, wrong: Vec<(Want, String)>, otherwise: String, right: String },
    /// A tip or a hint: says its piece and doesn't stop anything.
    Tip,
}

pub(super) struct Chapter {
    pub tag: String,
    pub title: String,
    pub line: String,
    pub pop: Spring,
    pub age: f32,
}

/// The paper note: Benny's words, typed out as he says them.
pub(super) struct Note {
    /// The words with `*emphasis*` marks taken out.
    pub text: String,
    /// Per non-space character: emphasized?
    pub loud: Vec<bool>,
    pub shown: f32,
    pub pop: Spring,
    pub age: f32,
    /// Whether it stops the table and waits for you to press on.
    pub next: bool,
    /// Above the pile, clear of the bid buttons, instead of below the trick.
    pub high: bool,
    /// Set when it's a wrong answer's explanation: the note shakes.
    pub shake: f32,
    /// Fading out.
    pub closing: bool,
    pub fade: f32,
}

/// How you did, for the report at the end.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Stats {
    /// Cards the rules refused after the first lesson.
    pub refused: u32,
    pub quiz_wrong: u32,
    pub bid_wrong: u32,
    /// Trump spent on a trick your partner was already winning.
    pub trumped_partner: u32,
    pub hints: u32,
    /// Called trump in a lesson nobody told you to.
    pub called_alone: bool,
    /// The last lesson's call: anything but passing up that monster.
    pub passed_monster: bool,
}

pub(super) struct Coach {
    pub lesson: usize,
    pub level: Level,
    beats: VecDeque<Beat>,
    pub now: Current,
    pub note: Option<Note>,
    pub note_y: Spring,
    pub chapter: Option<Chapter>,
    /// What the current beat is about, for the spotlight.
    pub points: Vec<Point>,
    /// The spotlight's holes as they're drawn, easing toward where they belong.
    pub holes: Vec<Rect>,
    /// How dark the table is around them, 0..1.
    pub dim: f32,
    /// What the pointing glove is on, and how visible it is.
    pub pointing: Option<Point>,
    pub glove: (Spring, Spring),
    pub glove_alpha: f32,
    /// How long you've been looking at this decision.
    pub idle: f32,
    pub wrongs: u32,
    /// A hint was asked for (or came by itself): what it suggests.
    pub hinted: Option<Action>,
    pub prompt_seen: Prompt,
    pub quiz_focus: usize,
    pub stats: Stats,
    pub hint_button: Button,
    pub hint_focused: bool,
    /// Where the current beats are being looked for from, so the next one with the
    /// same moment follows on.
    at: Option<When>,
}

/// Width of a note.
pub(super) const NOTE_W: f32 = 350.0;
/// Room for the words on it, inside the margins and past the portrait.
pub(crate) const NOTE_TEXT_W: f32 = NOTE_W - 44.0;
/// Characters typed per second.
const TYPE_RATE: f32 = 70.0;
/// How long a decision sits before lesson 3 suggests something.
const REVIEW_PATIENCE: f32 = 10.0;
/// How long lesson 2 waits before pointing at the answer anyway.
const ASK_PATIENCE: f32 = 18.0;

/// A line with its `*emphasis*` marks taken out.
pub(crate) fn plain_text(line: &str) -> String {
    line.replace('*', "")
}

fn emphasis(line: &str) -> Vec<bool> {
    let mut loud = false;
    let mut out = Vec::new();
    for ch in line.chars() {
        match ch {
            '*' => loud = !loud,
            ' ' | '\n' => {}
            _ => out.push(loud),
        }
    }
    out
}

impl Note {
    fn new(line: &str, next: bool, high: bool) -> Note {
        let mut pop = Spring::new(0.86);
        pop.target = 1.0;
        Note {
            text: plain_text(line),
            loud: emphasis(line),
            shown: 0.0,
            pop,
            age: 0.0,
            next,
            high,
            shake: 0.0,
            closing: false,
            fade: 1.0,
        }
    }

    fn typed(&self) -> bool {
        self.shown as usize >= self.text.chars().count()
    }
}

impl Coach {
    pub(super) fn new(lesson: usize, scores: [u32; 2]) -> Coach {
        let mut coach = Coach {
            lesson,
            level: Level::of(lesson),
            beats: lessons::beats(lesson, scores).into(),
            now: Current::Idle,
            note: None,
            note_y: Spring::new(layout::note_low_bottom(true)),
            chapter: None,
            points: Vec::new(),
            holes: Vec::new(),
            dim: 0.0,
            pointing: None,
            glove: (Spring::new(320.0), Spring::new(400.0)),
            glove_alpha: 0.0,
            idle: 0.0,
            wrongs: 0,
            hinted: None,
            prompt_seen: Prompt::None,
            quiz_focus: 0,
            stats: Stats::default(),
            hint_button: hint_button(),
            hint_focused: false,
            at: None,
        };
        coach.glove.0.stiffness = 120.0;
        coach.glove.1.stiffness = 120.0;
        coach
    }

    pub(super) fn enter_hand(&mut self, lesson: usize, scores: [u32; 2]) {
        self.lesson = lesson;
        self.level = Level::of(lesson);
        self.beats = lessons::beats(lesson, scores).into();
        self.now = Current::Idle;
        self.points.clear();
        self.pointing = None;
        self.hinted = None;
        self.prompt_seen = Prompt::None;
        self.hint_focused = false;
        self.close_note();
    }

    /// Whether the table has to wait for you.
    pub(super) fn holding(&self) -> bool {
        matches!(self.now, Current::Chapter | Current::Talk | Current::Quiz { .. })
    }

    /// A note with words on it is showing, so the usual prompt line stays out of its way.
    pub(super) fn speaking(&self) -> bool {
        self.note.as_ref().is_some_and(|n| !n.closing)
    }

    fn say(&mut self, line: &str, next: bool, high: bool) {
        self.note = Some(Note::new(line, next, high));
    }

    fn close_note(&mut self) {
        if let Some(n) = &mut self.note {
            n.closing = true;
        }
    }

    /// Replaces the note with an explanation, and gives it a shake.
    fn scold(&mut self, line: &str) {
        let high = self.note.as_ref().is_some_and(|n| n.high);
        let next = self.note.as_ref().is_some_and(|n| n.next);
        let mut note = Note::new(line, next, high);
        note.shake = 1.0;
        note.shown = 8.0;
        self.note = Some(note);
    }

    fn cleared_turn(&mut self) {
        self.idle = 0.0;
        self.wrongs = 0;
        self.hinted = None;
        self.prompt_seen = Prompt::None;
        self.hint_focused = false;
        self.pointing = None;
    }

    /// What the glove and the gold outlines are on, for what's asked right now.
    fn wanted_point(&self, alone_on: bool) -> Option<Point> {
        match &self.now {
            Current::Task { want, .. } => {
                let stuck = self.wrongs >= 2 || self.idle > ASK_PATIENCE;
                (self.level == Level::Show || stuck).then(|| want.point())
            }
            Current::Quiz { answer, .. } => (self.wrongs >= 2).then(|| Point::Cards(vec![*answer])),
            _ => match self.hinted? {
                Action::Pass => Some(Point::Bid(Bid::Pass)),
                Action::OrderUp { alone } => Some(Point::Bid(if alone && !alone_on { Bid::Alone } else { Bid::OrderUp })),
                Action::Call { suit, alone } => Some(Point::Bid(if alone && !alone_on { Bid::Alone } else { Bid::Suit(suit) })),
                Action::Discard { card } | Action::Play { card } => Some(Point::Cards(vec![card])),
            },
        }
    }
}

/// HINT stays in the corner opposite MENU.
fn hint_rect() -> Rect {
    if device::mobile() {
        let b = crate::render::bounds();
        return Rect::new(b.x + 6.0, b.y + b.h - 40.0, 70.0, 36.0);
    }
    Rect::new(12.0, 332.0, 50.0, 16.0)
}

fn hint_button() -> Button {
    let b = Button::new("HINT", hint_rect(), palette::ORANGE, palette::ORANGE_DARK);
    if device::mobile() { b.scaled(1.2) } else { b }
}

impl Table {
    /// The tutorial's match, from `lesson` (0 is the start) with the score so far.
    pub fn tutorial(shared: &Shared, lesson: usize, scores: [u32; 2]) -> Self {
        let seed = crate::device::var("EUCHRE_SEED").and_then(|s| s.parse().ok()).unwrap_or(1);
        if crate::device::var("EUCHRE_SEED").is_some() {
            love2drs::math::set_random_seed(seed);
        }
        let room = crate::euchre::tutorial::TutorialRoom::new(lesson, scores, seed);
        let link = crate::net::Link::Tutorial(Box::new(room));
        let Ok(mut table) = Table::join(shared, link, None, VecDeque::new()) else {
            panic!("the tutorial deals at once");
        };
        table.coach = Some(Box::new(Coach::new(lesson, scores)));
        table
    }

    pub(super) fn coach_holding(&self) -> bool {
        self.coach.as_ref().is_some_and(|c| c.holding())
    }

    /// Another seat just moved: in the lessons that explain things, give it time to
    /// land before Benny says anything about it.
    pub(super) fn coach_let_it_land(&mut self) {
        if self.coach.as_ref().is_some_and(|c| c.level <= Level::Review) {
            self.cue(Cue::Wait(0.45));
        }
    }

    /// A new hand was dealt in the tutorial.
    pub(super) fn coach_new_hand(&mut self) {
        let (lesson, scores) = (self.game.hand_number as usize - 1, self.game.scores);
        if let Some(c) = &mut self.coach {
            c.enter_hand(lesson, scores);
        }
    }

    // ------------------------------------------------------------------
    // Beats
    // ------------------------------------------------------------------

    /// Whether `when` has come, is still to come, or has gone by. `Some(at)` means the
    /// timeline reached that moment; `None` means the table is idle.
    fn beat_due(&self, when: When, at: Option<When>) -> std::cmp::Ordering {
        use std::cmp::Ordering::{Equal, Greater, Less};
        let r = &self.game.round;
        let tricks = r.tricks_played();
        match when {
            When::Now => Equal,
            When::BeforeDeal if at == Some(When::BeforeDeal) => Equal,
            When::BeforeDeal => {
                if self.game.moves() > 0 { Less } else { Greater }
            }
            _ if at == Some(when) => Equal,
            When::TrickWon(k) => {
                if tricks > k || (tricks == k && r.phase != Phase::TrickDone) { Less } else { Greater }
            }
            _ if at.is_some() => Greater,
            When::Dealt => {
                if self.game.moves() == 0 && r.phase == Phase::OrderUp { Equal } else { Less }
            }
            When::Move(n) => match self.game.moves().cmp(&n) {
                Less => Greater,
                Greater => Less,
                Equal => match r.phase {
                    Phase::TrickDone => Greater,
                    Phase::HandOver => Less,
                    _ => Equal,
                },
            },
            When::TrickEnd(k) => {
                if tricks > k || (tricks == k && r.phase != Phase::TrickDone) {
                    Less
                } else if tricks == k {
                    Equal
                } else {
                    Greater
                }
            }
            When::HandDone => {
                if self.hand_done {
                    Less
                } else if r.phase == Phase::HandOver {
                    Equal
                } else {
                    Greater
                }
            }
        }
    }

    /// Opens whatever is due. Returns true if the table now has to wait.
    pub(super) fn coach_gate(&mut self, at: Option<When>, shared: &Shared) -> bool {
        let Some(c) = &mut self.coach else { return false };
        if c.holding() {
            return true;
        }
        c.at = at;
        while let Some(when) = self.coach.as_ref().and_then(|c| c.beats.front().map(|b| b.when)) {
            match self.beat_due(when, at) {
                std::cmp::Ordering::Greater => break,
                std::cmp::Ordering::Less => {
                    if let Some(c) = &mut self.coach {
                        c.beats.pop_front();
                    }
                }
                std::cmp::Ordering::Equal => {
                    // Not over the trump announcement: try again once it's gone.
                    if self.toast.is_some() {
                        return true;
                    }
                    let Some(beat) = self.coach.as_mut().and_then(|c| c.beats.pop_front()) else { break };
                    if self.open_beat(beat, shared) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Puts a beat on the table. Returns true if it holds the table.
    fn open_beat(&mut self, beat: Beat, shared: &Shared) -> bool {
        let line = match beat.say {
            Say::Line(l) => l,
            Say::Later(f) => match f(&self.game) {
                Some(l) => l,
                None => return false,
            },
        };
        let bidding = matches!(self.game.round.phase, Phase::OrderUp | Phase::NameTrump) && self.game.round.to_act() == Some(HUMAN);
        let Some(c) = &mut self.coach else { return false };
        c.points = beat.points;
        match beat.kind {
            Kind::Chapter { tag, title } => {
                let mut pop = Spring::new(0.7);
                pop.target = 1.0;
                c.chapter = Some(Chapter { tag, title, line, pop, age: 0.0 });
                c.now = Current::Chapter;
                c.close_note();
                shared.sfx.play(Sound::Flip);
                true
            }
            Kind::Talk => {
                c.say(&line, true, false);
                c.now = Current::Talk;
                true
            }
            Kind::Task { want, wrong, otherwise, praise } => {
                let high = matches!(want, Want::Bid(_)) || bidding;
                c.say(&line, false, high);
                c.now = Current::Task { want, wrong, otherwise, praise };
                c.idle = 0.0;
                c.wrongs = 0;
                c.points.clear();
                false
            }
            Kind::Quiz { answer, among, wrong, otherwise, right } => {
                c.say(&line, false, false);
                c.now = Current::Quiz { answer, among, wrong, otherwise, right };
                c.wrongs = 0;
                c.quiz_focus = 0;
                shared.sfx.play(Sound::Pop);
                true
            }
        }
    }

    /// You pressed on past a talk. The first press finishes typing it.
    pub(super) fn coach_press(&mut self, shared: &Shared) {
        let Some(c) = &mut self.coach else { return };
        match c.now {
            Current::Chapter => {
                if c.chapter.as_ref().is_none_or(|ch| ch.age < 0.25) {
                    return;
                }
                c.chapter = None;
            }
            Current::Talk => {
                let Some(n) = &mut c.note else { return };
                if n.age < 0.12 {
                    return;
                }
                if !n.typed() {
                    n.shown = n.text.chars().count() as f32;
                    return;
                }
                c.close_note();
            }
            _ => return,
        }
        shared.sfx.play(Sound::Tick);
        c.now = Current::Idle;
        c.points.clear();
        let at = c.at;
        self.coach_gate(at, shared);
    }

    // ------------------------------------------------------------------
    // Your moves
    // ------------------------------------------------------------------

    fn coach_wrong(&mut self, want: Want, shared: &Shared) {
        let Some(c) = &mut self.coach else { return };
        let line = match &c.now {
            Current::Task { wrong, otherwise, .. } => wrong.iter().find(|(w, _)| *w == want).map_or(otherwise.clone(), |(_, l)| l.clone()),
            _ => return,
        };
        c.scold(&line);
        c.wrongs += 1;
        if matches!(want, Want::Bid(_)) && c.lesson >= 1 {
            c.stats.bid_wrong += 1;
        }
        shared.sfx.play(Sound::Deny);
    }

    /// Whether this bid button may do its thing. A wrong one gets a word from Benny.
    pub(super) fn coach_allows_bid(&mut self, bid: Bid, shared: &Shared) -> bool {
        let Some(Current::Task { want, .. }) = self.coach.as_ref().map(|c| &c.now) else { return true };
        if *want == Want::Bid(bid) {
            return true;
        }
        self.coach_wrong(Want::Bid(bid), shared);
        false
    }

    /// Whether this card may be played or thrown. The rules refuse cards they don't
    /// allow by themselves, with their own explanation; this is for legal cards.
    pub(super) fn coach_allows_card(&mut self, card: Card, shared: &Shared) -> bool {
        let Some(Current::Task { want, .. }) = self.coach.as_ref().map(|c| &c.now) else { return true };
        if *want == Want::Card(card) || !self.game.round.legal_plays(HUMAN).contains(&card) {
            return true;
        }
        self.coach_wrong(Want::Card(card), shared);
        false
    }

    /// The rules refused your move. Returns true if Benny explains it (the note is up).
    pub(super) fn coach_refused(&mut self, line: &str) -> bool {
        let Some(c) = &mut self.coach else { return false };
        if c.lesson >= 1 {
            c.stats.refused += 1;
        }
        if c.level > Level::Review || !c.speaking() {
            return false;
        }
        c.scold(line);
        true
    }

    /// Your move went through. `before` is the round just before it.
    pub(super) fn coach_after_move(&mut self, before: &Round, action: Action) {
        let (level, praise) = match &mut self.coach {
            Some(c) => {
                let praise = match &c.now {
                    Current::Task { praise, .. } => *praise,
                    _ => None,
                };
                if matches!(c.now, Current::Task { .. } | Current::Tip) {
                    c.now = Current::Idle;
                    c.close_note();
                }
                c.cleared_turn();
                if c.level == Level::Hint && matches!(action, Action::Pass) && before.phase == Phase::OrderUp {
                    c.stats.passed_monster = true;
                }
                if matches!(action, Action::OrderUp { alone: true } | Action::Call { alone: true, .. }) {
                    c.stats.called_alone = true;
                }
                (c.level, praise)
            }
            None => return,
        };
        if let Some(line) = praise {
            self.say(2, line.into());
        }
        let verdict = match action {
            Action::Play { card } => {
                self.note_trumped_partner(before, card);
                advice::review_play(before, HUMAN, card)
            }
            Action::Discard { card } => advice::review_discard(before, HUMAN, card),
            _ => Verdict::Quiet,
        };
        if level != Level::Review {
            return;
        }
        match verdict {
            Verdict::Good(line) => self.say(2, line.into()),
            Verdict::Costly(line) => {
                if let Some(c) = &mut self.coach {
                    c.beats.push_front(Beat { when: When::Now, kind: Kind::Talk, say: Say::Line(line), points: Vec::new() });
                }
                // Let the card land first.
                self.cue(Cue::Wait(0.6));
            }
            Verdict::Quiet => {}
        }
    }

    fn note_trumped_partner(&mut self, before: &Round, card: Card) {
        let (Some(trump), Some((seat, winning))) = (before.trump, before.trick_leader()) else { return };
        let spent = team_of(seat) == 0 && seat != HUMAN && card.is_trump(trump) && !winning.is_trump(trump);
        if spent
            && before.legal_plays(HUMAN).iter().any(|c| !c.is_trump(trump))
            && let Some(c) = &mut self.coach
            && c.lesson >= 2
        {
            c.stats.trumped_partner += 1;
        }
    }

    /// You're being asked for something: tips, the glove and which bid buttons are live.
    pub(super) fn coach_on_prompt(&mut self) {
        let prompt = self.prompt;
        let hand_size = match prompt {
            Prompt::Play => self.game.round.legal_plays(HUMAN).len(),
            Prompt::Discard => 6,
            _ => 0,
        };
        let tip = (hand_size > 1).then(|| advice::advise(&self.game.round, HUMAN).why);
        let Some(c) = &mut self.coach else { return };
        if c.prompt_seen == prompt {
            return;
        }
        c.prompt_seen = prompt;
        if let Current::Task { want, .. } = c.now {
            if c.level == Level::Show {
                // Only the button asked for looks pressable, and the focus starts on it.
                if let Want::Bid(bid) = want {
                    for (i, (b, button)) in self.bids.iter_mut().enumerate() {
                        button.enabled = *b == bid;
                        if *b == bid {
                            self.bid_focus = i;
                        }
                    }
                }
                if let Want::Card(card) = want
                    && let Some(i) = layout::hand_cards(self, HUMAN).iter().position(|&x| x == card)
                {
                    self.card_focus = i;
                }
            }
            return;
        }
        if c.level == Level::Review
            && let Some(tip) = tip
        {
            c.say(&tip, false, false);
            c.now = Current::Tip;
        }
    }

    /// HINT: Benny says what he'd do, and why.
    fn coach_hint(&mut self, shared: &mut Shared) {
        if self.prompt == Prompt::None {
            return;
        }
        let advice = advice::advise(&self.game.round, HUMAN);
        let bidding = matches!(self.prompt, Prompt::OrderUp | Prompt::NameTrump);
        let Some(c) = &mut self.coach else { return };
        if c.hinted.is_none() {
            c.stats.hints += 1;
        }
        c.hinted = Some(advice.action);
        c.say(&advice.why, false, bidding);
        c.now = Current::Tip;
        c.hint_button.flash(&shared.sfx);
        shared.tap();
    }

    /// The move autoplay should make instead of asking the AI (`EUCHRE_AUTOPLAY`).
    pub(super) fn coach_auto_action(&self) -> Option<Action> {
        match &self.coach.as_ref()?.now {
            Current::Task { want, .. } => want.action(self.alone),
            _ => None,
        }
    }

    // ------------------------------------------------------------------
    // Quizzes
    // ------------------------------------------------------------------

    /// The cards a quiz is asking you to choose from, left to right.
    pub(super) fn quiz_cards(&self) -> Vec<Card> {
        match self.coach.as_ref().map(|c| &c.now) {
            Some(Current::Quiz { among: Among::Hand, .. }) => layout::hand_cards(self, HUMAN),
            Some(Current::Quiz { among: Among::Trick, .. }) => {
                let mut cards: Vec<(f32, Card)> =
                    self.game.round.trick.iter().map(|&(_, c)| (self.sprites[c.id()].x.value, c)).collect();
                cards.sort_by(|a, b| a.0.total_cmp(&b.0));
                cards.into_iter().map(|(_, c)| c).collect()
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn quiz_card_at(&self, p: Vec2) -> Option<Card> {
        match self.coach.as_ref().map(|c| &c.now) {
            Some(Current::Quiz { among: Among::Hand, .. }) => layout::card_at(self, p),
            Some(Current::Quiz { among: Among::Trick, .. }) => layout::trick_card_at(self, p),
            _ => None,
        }
    }

    fn quiz_answer(&mut self, card: Card, shared: &mut Shared) {
        let Some(c) = &mut self.coach else { return };
        let Current::Quiz { answer, wrong, otherwise, right, .. } = &c.now else { return };
        if card == *answer {
            let right = right.clone();
            c.say(&right, true, false);
            c.now = Current::Talk;
            c.points.clear();
            c.wrongs = 0;
            shared.sfx.play(Sound::Chips);
            shared.tap();
            let at = self.sprites[card.id()].pos();
            self.sprites[card.id()].scale.impulse(2.2);
            self.burst(at, 18);
            return;
        }
        let line = wrong.iter().find(|(w, _)| *w == Want::Card(card)).map_or(otherwise.clone(), |(_, l)| l.clone());
        c.scold(&line);
        c.wrongs += 1;
        c.stats.quiz_wrong += 1;
        shared.sfx.play(Sound::Deny);
        let sprite = &mut self.sprites[card.id()];
        sprite.rot.impulse(2.5);
        sprite.x.impulse(60.0);
    }

    // ------------------------------------------------------------------
    // Input. Each returns true if the coach used it.
    // ------------------------------------------------------------------

    pub(super) fn coach_mouse(&mut self, pos: Vec2, shared: &mut Shared) -> bool {
        if self.coach.is_none() {
            return false;
        }
        if self.coach.as_ref().is_some_and(|c| matches!(c.now, Current::Quiz { .. })) {
            if let Some(card) = self.quiz_card_at(pos) {
                self.quiz_answer(card, shared);
            }
            return true;
        }
        if self.coach_holding() {
            self.coach_press(shared);
            return true;
        }
        let level = self.coach.as_ref().map(|c| c.level);
        if level == Some(Level::Hint)
            && self.input_open()
            && let Some(c) = &mut self.coach
            && c.hint_button.click(pos, &shared.sfx)
        {
            self.coach_hint(shared);
            return true;
        }
        false
    }

    pub(super) fn coach_key(&mut self, key: Key, shared: &mut Shared) -> bool {
        if !self.coach_holding() {
            return false;
        }
        let quiz = self.coach.as_ref().is_some_and(|c| matches!(c.now, Current::Quiz { .. }));
        if quiz {
            let digit = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5].iter().position(|&k| k == key);
            if let Some(&card) = digit.and_then(|i| self.quiz_cards().get(i).copied()).as_ref() {
                self.quiz_answer(card, shared);
            }
        } else if matches!(key, Key::Enter | Key::KpEnter | Key::Space) {
            self.coach_press(shared);
        }
        true
    }

    pub(super) fn coach_nav(&mut self, nav: Nav, shared: &mut Shared) -> bool {
        if self.coach.is_none() {
            return false;
        }
        if self.coach.as_ref().is_some_and(|c| matches!(c.now, Current::Quiz { .. })) {
            let cards = self.quiz_cards();
            match nav {
                Nav::Left | Nav::Right => {
                    if let Some(c) = &mut self.coach {
                        let n = cards.len().max(1) as i32;
                        c.quiz_focus = (c.quiz_focus as i32 + nav.dx()).rem_euclid(n) as usize;
                        shared.tick();
                    }
                }
                Nav::Confirm => {
                    let i = self.coach.as_ref().map_or(0, |c| c.quiz_focus);
                    if let Some(&card) = cards.get(i) {
                        self.quiz_answer(card, shared);
                    }
                }
                _ => {}
            }
            return true;
        }
        if self.coach_holding() {
            if nav == Nav::Confirm {
                shared.tap();
                self.coach_press(shared);
            }
            return true;
        }
        // HINT sits above the hand and the bid buttons: up to reach it, down to leave.
        let can_hint = self.coach.as_ref().is_some_and(|c| c.level == Level::Hint) && self.prompt != Prompt::None;
        if !can_hint {
            return false;
        }
        let focused = self.coach.as_ref().is_some_and(|c| c.hint_focused);
        match nav {
            Nav::Up if !focused => {
                if let Some(c) = &mut self.coach {
                    c.hint_focused = true;
                    shared.tick();
                }
                true
            }
            Nav::Confirm if focused => {
                self.coach_hint(shared);
                true
            }
            Nav::Down | Nav::Left | Nav::Right | Nav::Back if focused => {
                if let Some(c) = &mut self.coach {
                    c.hint_focused = false;
                    shared.tick();
                }
                true
            }
            _ => false,
        }
    }

    /// Clicking on a bid button that's greyed out: it was a wrong answer.
    pub(super) fn coach_disabled_bid(&mut self, pos: Vec2, shared: &Shared) {
        let clicked = self.bids.iter().find(|(_, b)| !b.enabled && b.rect.contains(pos)).map(|(bid, _)| *bid);
        if let Some(bid) = clicked {
            self.coach_allows_bid(bid, shared);
        }
    }

    /// A key or the gamepad pressed a bid that's greyed out.
    pub(super) fn coach_disabled_key(&mut self, bid: Bid, shared: &Shared) {
        self.coach_allows_bid(bid, shared);
    }

    // ------------------------------------------------------------------
    // Per frame
    // ------------------------------------------------------------------

    pub(super) fn coach_update(&mut self, shared: &mut Shared, dt: f32) {
        if self.coach.is_none() {
            return;
        }
        let paused = self.pause.is_some() || self.overlay.is_some();
        let alone = self.alone;
        let prompting = self.prompt != Prompt::None && self.input_open();
        let focus = shared.focus_visible();
        let mouse = if paused { crate::scene::NO_POINTER } else { shared.mouse };
        let rects = self.coach_spot_rects(shared);
        let pointed = {
            let c = self.coach.as_ref().expect("checked above");
            c.wanted_point(alone)
        };
        let tip = pointed.as_ref().and_then(|p| self.point_tip(p, shared));
        let autoplay = self.autoplay;
        let c = self.coach.as_mut().expect("checked above");

        if !paused && prompting {
            c.idle += dt;
        }
        if c.level == Level::Review
            && c.idle > REVIEW_PATIENCE
            && c.hinted.is_none()
            && matches!(c.now, Current::Tip | Current::Idle)
            && prompting
        {
            c.hinted = Some(advice::advise(&self.game.round, HUMAN).action);
            shared.sfx.play(Sound::Tick);
        }

        // The note.
        if let Some(n) = &mut c.note {
            n.age += dt;
            n.pop.update(dt);
            n.shake = (n.shake - dt * 3.0).max(0.0);
            if n.closing {
                n.fade = (n.fade - dt * 7.0).max(0.0);
            } else if !paused {
                let before = n.shown as usize;
                n.shown += dt * TYPE_RATE;
                // A soft blip now and then, as he talks.
                if (n.shown as usize) / 5 != before / 5 && !n.typed() {
                    shared.sfx.play_pitched(Sound::Hover, 1.25);
                }
            }
            if n.closing && n.fade <= 0.0 {
                c.note = None;
            }
        }
        if let Some(n) = &c.note {
            c.note_y.target = if n.high { layout::note_high_bottom() } else { layout::note_low_bottom(n.next) };
            c.note_y.update(dt);
        }
        if let Some(ch) = &mut c.chapter {
            ch.age += dt;
            ch.pop.update(dt);
        }

        // The spotlight.
        let spotting = !rects.is_empty() && matches!(c.now, Current::Talk | Current::Quiz { .. });
        let want_dim = if matches!(c.now, Current::Chapter) || spotting { 1.0 } else { 0.0 };
        c.dim = approach(c.dim, want_dim, 9.0, dt);
        if spotting {
            ease_holes(&mut c.holes, &rects, dt);
        } else if c.dim < 0.02 {
            c.holes.clear();
        }

        // The glove glides to whatever it's on.
        if let Some(tip) = tip {
            if c.glove_alpha <= 0.01 {
                c.glove.0.value = tip.x;
                c.glove.1.value = tip.y + 30.0;
            }
            c.glove.0.target = tip.x;
            c.glove.1.target = tip.y;
        }
        c.glove_alpha = approach(c.glove_alpha, if tip.is_some() { 1.0 } else { 0.0 }, 10.0, dt);
        c.glove.0.update(dt);
        c.glove.1.update(dt);
        c.pointing = pointed;

        // HINT.
        if device::mobile() {
            c.hint_button.rect = hint_rect();
        }
        c.hint_button.enabled = prompting || c.points.contains(&Point::Hint);
        c.hint_button.focused = focus && c.hint_focused;
        c.hint_button.update(mouse, dt, &shared.sfx);

        // Debug: autoplay gets through the talking by itself.
        if autoplay && !paused {
            match &c.now {
                Current::Chapter | Current::Talk => self.coach_press(shared),
                Current::Quiz { answer, .. } => {
                    let answer = *answer;
                    self.quiz_answer(answer, shared);
                }
                _ => {}
            }
        }
    }

    /// The bid button Benny is pointing at: it pulses the way a focused one does.
    pub(super) fn coach_pointed_bid(&self) -> Option<Bid> {
        match self.coach.as_ref()?.pointing {
            Some(Point::Bid(bid)) => Some(bid),
            _ => None,
        }
    }

    /// Rects of what the current beat is about.
    fn coach_spot_rects(&self, shared: &Shared) -> Vec<Rect> {
        let Some(c) = &self.coach else { return Vec::new() };
        c.points.iter().flat_map(|p| self.point_rects(p, shared)).collect()
    }

    fn card_rect(&self, card: Card, grow: f32) -> Rect {
        let s = &self.sprites[card.id()];
        let scale = s.scale.value;
        // Wide enough for a hand card's tilt.
        Rect::centered(s.x.value, s.y.value, crate::cards::CARD_W * scale, crate::cards::CARD_H * scale).grown(grow)
    }

    fn point_rects(&self, p: &Point, shared: &Shared) -> Vec<Rect> {
        let union = |cards: Vec<Card>| -> Vec<Rect> {
            let mut rects = cards.into_iter().map(|c| self.card_rect(c, 3.0));
            let Some(first) = rects.next() else { return Vec::new() };
            let joined = rects.fold(first, |a, b| {
                let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
                let (x1, y1) = ((a.x + a.w).max(b.x + b.w), (a.y + a.h).max(b.y + b.h));
                Rect::new(x0, y0, x1 - x0, y1 - y0)
            });
            vec![joined]
        };
        match p {
            Point::Plate(seat) => {
                let pos = layout::plate_pos(*seat);
                let w = shared.font.width(&self.names[*seat]) + 18.0;
                vec![Rect::centered(pos.x, pos.y, w, 13.0).grown(4.0)]
            }
            Point::Hand => union(layout::hand_cards(self, HUMAN)),
            Point::Cards(cards) => cards.iter().map(|&c| self.card_rect(c, 2.0)).collect(),
            Point::UpCard => vec![self.card_rect(self.game.round.up_card, 3.0)],
            Point::Trick => union((0..24).map(Card::from_id).filter(|c| matches!(self.places[c.id()], Place::Trick(_))).collect()),
            Point::Pad => vec![draw::pad_rect().grown(4.0)],
            Point::Medallion => vec![draw::medallion_rect()],
            Point::Hint => self.coach.as_ref().map(|c| vec![c.hint_button.rect.grown(5.0)]).unwrap_or_default(),
            Point::Bid(bid) => self.bids.iter().filter(|(b, _)| b == bid).map(|(_, b)| b.rect.grown(3.0)).collect(),
        }
    }

    /// Where the glove's fingertip goes for this point.
    fn point_tip(&self, p: &Point, shared: &Shared) -> Option<Vec2> {
        let r = *self.point_rects(p, shared).first()?;
        Some(match p {
            Point::Cards(_) => vec2(r.center().x, r.center().y + 6.0),
            _ => vec2(r.center().x, r.y + r.h - 3.0),
        })
    }

    /// Cards the glove or a hint is on, to outline in gold.
    pub(super) fn coach_glow(&self) -> Vec<Card> {
        match self.coach.as_ref().and_then(|c| c.pointing.as_ref()) {
            Some(Point::Cards(cards)) => cards.clone(),
            _ => Vec::new(),
        }
    }

    /// A little extra lift for the card being pointed at.
    pub(super) fn coach_adjust(&self, card: Card, target: &mut Target) {
        if self.coach_glow().contains(&card) {
            target.pos.y -= 5.0;
        }
    }

    /// What the coach is up to, for the checks that play the tutorial through
    /// (`tests/scene_checks.rs`): `kind|lesson|focus now|focus wanted|words`.
    pub fn coach_state(&self) -> String {
        if self.game_over.is_some() {
            return "report|||".into();
        }
        if let Some(b) = &self.banner {
            return format!("banner|||{}|{}", b.age > 0.4, b.title);
        }
        let Some(c) = &self.coach else { return String::new() };
        let lesson = c.lesson;
        let words = c.note.as_ref().filter(|n| !n.closing).map_or("", |n| n.text.as_str());
        let hand = layout::hand_cards(self, HUMAN);
        match &c.now {
            Current::Chapter => format!("chapter|{lesson}|||{}", c.chapter.as_ref().map_or("", |ch| ch.title.as_str())),
            Current::Talk => {
                let typed = c.note.as_ref().is_some_and(|n| n.typed());
                format!("talk|{lesson}|{typed}||{words}")
            }
            Current::Quiz { answer, .. } => {
                let cards = self.quiz_cards();
                let goal = cards.iter().position(|x| x == answer).unwrap_or(0);
                format!("quiz|{lesson}|{}|{goal}|{words}", c.quiz_focus)
            }
            _ if self.prompt == Prompt::None => format!("wait|{lesson}|||{words}"),
            Current::Task { want: Want::Card(card), .. } => {
                let goal = hand.iter().position(|x| x == card).unwrap_or(0);
                format!("task|{lesson}|{}|{goal}|{words}", self.card_focus)
            }
            Current::Task { want: Want::Bid(bid), .. } => {
                let goal = self.bids.iter().position(|(b, _)| b == bid).unwrap_or(0);
                format!("task|{lesson}|{}|{goal}|{words}", self.bid_focus)
            }
            _ if matches!(self.prompt, Prompt::OrderUp | Prompt::NameTrump) => format!("bid|{lesson}|{}||{words}", self.bid_focus),
            _ => format!("card|{lesson}|{}||{words}", self.card_focus),
        }
    }

    // ------------------------------------------------------------------
    // The end
    // ------------------------------------------------------------------

    /// The match is over: remember that, and lay out the buttons for the report.
    pub(super) fn coach_finished(&mut self, shared: &mut Shared) {
        shared.settings.tutorial_done = true;
        shared.settings.lesson = None;
        shared.settings.save();
    }

    /// Picks up the deck for a new lesson: where to come back to if you leave now.
    pub(super) fn coach_save_progress(&self, shared: &mut Shared) {
        if let Some(c) = &self.coach {
            shared.settings.lesson = (c.lesson > 0).then_some((c.lesson.min(crate::euchre::tutorial::LESSONS - 1), self.game.scores));
            shared.settings.save();
        }
    }

    /// PLAY FOR REAL. With a game in the save slot, the title asks about it first.
    pub(super) fn coach_play_for_real(&mut self, shared: &mut Shared) -> Transition {
        if Settings::use_defaults() || crate::savegame::load().is_none() {
            return Transition::Switch(Box::new(Table::new(shared)));
        }
        Transition::Switch(Box::new(Title::new()))
    }
}

/// Moves each drawn hole toward its target. New ones grow out of their target's center.
fn ease_holes(holes: &mut Vec<Rect>, targets: &[Rect], dt: f32) {
    holes.truncate(targets.len());
    while holes.len() < targets.len() {
        let t = targets[holes.len()];
        let start = holes.last().copied().unwrap_or(Rect::new(t.center().x, t.center().y, 0.0, 0.0));
        holes.push(start);
    }
    let k = 1.0 - (-dt * 14.0).exp();
    for (h, t) in holes.iter_mut().zip(targets) {
        h.x += (t.x - h.x) * k;
        h.y += (t.y - h.y) * k;
        h.w += (t.w - h.w) * k;
        h.h += (t.h - h.h) * k;
    }
}
