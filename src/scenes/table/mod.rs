//! The card table. Shows one player's [`Mirror`] of the hand, animates every
//! card and handles that player's bidding and play.
//!
//! The game itself runs in a room (`euchre::room`) behind a [`Link`]: in this
//! process for an offline game against bots, or on the server for an online
//! one. Either way the table sends its own moves and receives everyone
//! else's as messages, which wait in `pending` until the animation reaches
//! them. Other players' cards are guesses until played (see `mirror.rs`);
//! when a guess is corrected, the two face-down sprites trade places.
//!
//! Each card has a *place* (pile, hand, trick, ...) and springs that chase
//! the position the layout gives that place, so motion never snaps. Pacing
//! comes from a queue of timed [`Cue`]s: the deal, trick collection, results
//! and other seats' moves (after a "thinking" pause for bots) are all
//! scheduled through it.
//!
//! Input: mouse clicks, touch (the card under your finger lifts, and lifting
//! the finger plays it), keyboard shortcuts, and gamepad [`Nav`]: left/right
//! walks the focus along your hand or the bid buttons, A plays or bids,
//! Y toggles alone, Start pauses.
//!
//! Saving (offline games): "SAVE & QUIT" in the pause menu (and closing the
//! window mid-game) writes the match to the save slot (`crate::savegame`);
//! [`Table::resume`] lays a saved hand back out on the felt. A finished game
//! deletes its save. Online games live on the server instead: after a
//! dropped connection the room resends the hand (`Sync`) and it's laid out
//! the same way.
//!
//! - `layout.rs` where each place is on screen
//! - `draw.rs`   all rendering (cards, sidebar, prompts, panels)

mod draw;
mod layout;

use std::collections::VecDeque;

use love2drs::prelude::*;

use crate::euchre::protocol::{ClientMsg, HandStart, Move, SeatKind, SeatsInfo, ServerMsg};
use crate::euchre::{
    Action, Card, HandResult, Match, Mirror, Phase, ResultKind, RuleError, Seat, Suit, ai, partner_of, team_of,
};
use crate::juice::Spring;
use crate::net::{Link, LocalTable};
use crate::palette;
use crate::savegame;
use crate::scene::{NO_POINTER, Nav, Scene, Shared, Transition};
use crate::scenes::title::Title;
use crate::scenes::{Overlay, options::OptionsPanel, rules::RulesPanel};
use crate::sfx::Sound;
use crate::ui::{Button, Rect};

use layout::Target;

pub const HUMAN: Seat = 0;
/// Names for an offline game (the room's bots sit in seats 1-3).
pub const NAMES: [&str; 4] = ["YOU", "ROSA", "BENNY", "JULES"];

/// Where a card currently lives on the table.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Place {
    /// Face-down stack in the middle: the deck before the deal, then the kitty.
    Pile,
    /// The turned-up card on top of the pile during bidding.
    UpCard,
    Hand(Seat),
    Trick(Seat),
    /// Collected tricks, stacked by the seat that won them (`n` = trick index).
    Won(Seat, u8),
    /// The kitty, pushed next to the dealer once trump is set.
    Kitty(Seat),
    /// The partner of a lone player: cards tucked away face down.
    SitOut(Seat),
}

struct Sprite {
    x: Spring,
    y: Spring,
    rot: Spring,
    scale: Spring,
    /// 0 = back showing, 1 = face showing. Animated for the flip.
    face: f32,
    /// Brightness multiplier (dims unplayable cards).
    tint: f32,
    /// Bumped whenever the card changes place, so the latest card draws on top.
    z: u32,
}

fn card_spring(v: f32) -> Spring {
    let mut s = Spring::new(v);
    s.stiffness = 190.0;
    s.damping = 21.0;
    s
}

impl Sprite {
    fn new(x: f32, y: f32) -> Self {
        Sprite {
            x: card_spring(x),
            y: card_spring(y),
            rot: card_spring(0.0),
            scale: card_spring(1.0),
            face: 0.0,
            tint: 1.0,
            z: 0,
        }
    }

    fn pos(&self) -> Vec2 {
        vec2(self.x.value, self.y.value)
    }
}

/// A scheduled step. Pacing for everything that isn't instant.
enum Cue {
    Wait(f32),
    Gather,
    Shuffle,
    Deal(Card, Seat),
    RevealUp,
    TurnDown,
    PickUp,
    UpToPile,
    KittyAside,
    SitOut(Seat),
    /// Autoplay (`EUCHRE_AUTOPLAY`): the AI makes your move.
    AiAct(Seat),
    /// Another seat's move, from the room.
    Remote(Move),
    HighlightWinner,
    CollectTrick,
    ShowResult,
    Redeal,
    Say(Seat, String),
    /// Moves cards straight to their places with one sound (laying out a resumed hand).
    Lay(Vec<(Card, Place)>, Sound),
}

/// What the human is being asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prompt {
    None,
    OrderUp,
    NameTrump,
    Discard,
    Play,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bid {
    OrderUp,
    Pass,
    Alone,
    Suit(Suit),
}

struct Bubble {
    text: String,
    age: f32,
    pop: Spring,
}

struct Toast {
    suit: Suit,
    title: String,
    sub: String,
    age: f32,
    pop: Spring,
}

struct Banner {
    result: HandResult,
    title: &'static str,
    color: Color,
    detail: String,
    points: String,
    pop: Spring,
    button: Button,
    age: f32,
}

struct GameOver {
    won: bool,
    pop: Spring,
    again: Button,
    menu: Button,
    age: f32,
    /// Gamepad focus: 0 = play again, 1 = main menu.
    focus: usize,
}

#[derive(Clone, Copy)]
enum PauseChoice {
    Resume,
    Options,
    Rules,
    Quit,
}

struct Pause {
    buttons: Vec<(PauseChoice, Button)>,
    pop: Spring,
    focus: usize,
}

struct Particle {
    pos: Vec2,
    vel: Vec2,
    life: f32,
    max: f32,
    color: Color,
    size: f32,
}

pub struct Table {
    game: Mirror,
    link: Link,
    /// Messages about the hand (deals, moves, resyncs), waiting for the
    /// animation to catch up with them.
    pending: VecDeque<ServerMsg>,
    /// Names on the plates, by seat (you're "YOU").
    names: [String; 4],
    kinds: [SeatKind; 4],
    /// The current hand's result (or misdeal) has been shown: the next deal comes from the room.
    hand_done: bool,
    /// Asked the room for the whole hand again; moves are ignored until it comes.
    resyncing: bool,
    /// Online: the PLAY AGAIN request is out, waiting for the room to deal.
    rematch_asked: bool,
    sprites: Vec<Sprite>,
    places: [Place; 24],
    next_z: u32,
    cues: VecDeque<Cue>,
    wait: f32,
    prompt: Prompt,
    bids: Vec<(Bid, Button)>,
    alone: bool,
    hover: Option<Card>,
    /// Gamepad / keyboard focus: an index into the human's hand (left to right)
    /// while playing or discarding, and into `bids` while bidding.
    card_focus: usize,
    bid_focus: usize,
    bubbles: [Option<Bubble>; 4],
    toast: Option<Toast>,
    hint: Option<(String, f32)>,
    banner: Option<Banner>,
    game_over: Option<GameOver>,
    pause: Option<Pause>,
    overlay: Option<Overlay>,
    menu_button: Button,
    shake: f32,
    particles: Vec<Particle>,
    /// The card that won the trick being shown, and how long it has glowed.
    winner_glow: Option<(Card, f32)>,
    score_shown: [u32; 2],
    score_tick: f32,
    score_pop: [Spring; 2],
    trick_pop: [Spring; 2],
    trump_pop: Spring,
    /// Debug: the computer plays the human seat too (`EUCHRE_AUTOPLAY=1`).
    autoplay: bool,
    /// Seconds the AI waits before acting (scaled by the speed setting).
    delay: f32,
}

impl Table {
    /// A new game with the current options.
    pub fn new(shared: &Shared) -> Self {
        let seed = std::env::var("EUCHRE_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(1)
            });
        if std::env::var("EUCHRE_SEED").is_ok() {
            // Deterministic AI think times and shuffles too, for reproducible screenshots.
            love2drs::math::set_random_seed(seed);
        }
        let s = &shared.settings;
        // Debug: `EUCHRE_TARGET=n` overrides the points needed to win.
        let target = std::env::var("EUCHRE_TARGET")
            .ok()
            .and_then(|t| t.parse().ok())
            .unwrap_or(s.target);
        let link = Link::Local(LocalTable::new(target, s.stick_the_dealer, seed));
        Table::join(shared, link, None, VecDeque::new())
            .unwrap_or_else(|_| panic!("an offline game deals at once"))
    }

    /// Picks a saved offline game back up where it was left.
    pub fn resume(shared: &Shared, game: Match) -> Self {
        let link = Link::Local(LocalTable::resume(game));
        Table::join(shared, link, None, VecDeque::new())
            .unwrap_or_else(|_| panic!("a resumed game sends its hand at once"))
    }

    /// Sits down at a room's table once it has dealt: reads messages until
    /// the first deal (or the hand in progress) and lays it out. `pending`
    /// holds messages already taken from the link, `seats` the latest seat
    /// list. Gives the link back if no hand has arrived yet.
    pub fn join(
        shared: &Shared,
        mut link: Link,
        mut seats: Option<SeatsInfo>,
        mut pending: VecDeque<ServerMsg>,
    ) -> Result<Self, (Link, VecDeque<ServerMsg>)> {
        while let Some(msg) = link.poll() {
            pending.push_back(msg);
        }
        // Start from the latest resync (it holds everything before it), else
        // from the first deal (later ones still need to be played out).
        let start = pending
            .iter()
            .rposition(|m| matches!(m, ServerMsg::Sync { .. }))
            .or_else(|| pending.iter().position(|m| matches!(m, ServerMsg::Deal(_))));
        let Some(start) = start else {
            return Err((link, pending));
        };
        for msg in pending.drain(..start) {
            if let ServerMsg::Seats(s) = msg {
                seats = Some(s);
            }
        }
        let game = match pending.pop_front() {
            Some(ServerMsg::Deal(hand)) => Mirror::new(&hand),
            Some(ServerMsg::Sync { hand, moves }) => Mirror::replay(&hand, &moves),
            _ => unreachable!("found above"),
        };
        let game = match game {
            Ok(g) => g,
            Err(e) => {
                crate::assets::log_warn(&format!("the room sent a hand that doesn't replay: {e:?}"));
                link.send(ClientMsg::Resync);
                return Err((link, pending));
            }
        };
        let mut table = Table::with_mirror(shared, game, link, pending);
        if let Some(s) = seats {
            table.set_seats(&s);
        }
        if table.game.hand_untouched() {
            table.begin_hand();
        } else {
            // A saved game or a reconnect: lay the hand back out.
            table.begin_resume();
        }
        Ok(table)
    }

    fn with_mirror(shared: &Shared, game: Mirror, link: Link, pending: VecDeque<ServerMsg>) -> Self {
        // Before the deal, the cards are still out of sight above the table.
        let sprites = (0..24)
            .map(|i| Sprite::new(layout::CENTER.x + (i as f32 - 12.0) * 2.0, -80.0 - i as f32 * 6.0))
            .collect();
        let score_shown = game.scores;
        Table {
            game,
            link,
            pending,
            names: NAMES.map(String::from),
            kinds: [SeatKind::Human, SeatKind::Bot, SeatKind::Bot, SeatKind::Bot],
            hand_done: false,
            resyncing: false,
            rematch_asked: false,
            sprites,
            places: [Place::Pile; 24],
            next_z: 1,
            cues: VecDeque::new(),
            wait: 0.0,
            prompt: Prompt::None,
            bids: Vec::new(),
            alone: false,
            hover: None,
            card_focus: 0,
            bid_focus: 0,
            bubbles: Default::default(),
            toast: None,
            hint: None,
            banner: None,
            game_over: None,
            pause: None,
            overlay: None,
            menu_button: Button::grey("MENU", Rect::new(578.0, 332.0, 50.0, 16.0)),
            shake: 0.0,
            particles: Vec::new(),
            winner_glow: None,
            score_shown,
            score_tick: 0.0,
            score_pop: [Spring::new(1.0), Spring::new(1.0)],
            trick_pop: [Spring::new(1.0), Spring::new(1.0)],
            trump_pop: Spring::new(1.0),
            autoplay: std::env::var("EUCHRE_AUTOPLAY").is_ok_and(|v| v == "1"),
            delay: shared.settings.delay_factor(),
        }
    }

    pub fn is_online(&self) -> bool {
        self.link.is_online()
    }

    fn set_seats(&mut self, s: &SeatsInfo) {
        for seat in 1..4 {
            let info = &s.seats[seat];
            if self.kinds[seat] != info.kind && self.link.is_online() {
                let line = match info.kind {
                    SeatKind::Human if self.kinds[seat] == SeatKind::Away => Some("I'm back!"),
                    SeatKind::Human => Some("Deal me in!"),
                    SeatKind::Away => Some("(connection lost)"),
                    SeatKind::Bot => None,
                };
                if let Some(line) = line {
                    self.say(seat, line.into());
                }
            }
            self.names[seat] = info.name.clone();
            self.kinds[seat] = info.kind;
        }
    }

    /// Takes everything the room has sent. Seat changes and errors apply at
    /// once; anything about the hand waits in `pending` for the animation.
    fn pump_link(&mut self) {
        while let Some(msg) = self.link.poll() {
            match msg {
                ServerMsg::Seats(s) => self.set_seats(&s),
                ServerMsg::Welcome { .. } => {}
                ServerMsg::Error { message, .. } => self.hint = Some((message, 2.2)),
                ServerMsg::Sync { .. } => {
                    // The whole hand: moves queued before it are already in it.
                    self.pending.retain(|m| !matches!(m, ServerMsg::Acted(_) | ServerMsg::Sync { .. }));
                    self.pending.push_back(msg);
                }
                ServerMsg::Deal(_) | ServerMsg::Acted(_) => self.pending.push_back(msg),
            }
        }
    }

    /// Something didn't add up between this table and the room: ask for the
    /// whole hand again, and ignore moves until it arrives.
    fn request_resync(&mut self) {
        self.resyncing = true;
        self.pending.retain(|m| !matches!(m, ServerMsg::Acted(_)));
        self.link.send(ClientMsg::Resync);
    }

    /// Takes the next message about the hand, once the table is ready for it.
    fn next_from_room(&mut self) {
        let Some(msg) = self.pending.pop_front() else { return };
        match msg {
            ServerMsg::Deal(hand) => self.new_hand(&hand),
            ServerMsg::Sync { hand, moves } => self.lay_out(&hand, &moves),
            ServerMsg::Acted(mv) => {
                if self.resyncing {
                    return;
                }
                let think = if mv.seat == HUMAN {
                    // Your turn timed out and the room moved for you.
                    0.0
                } else if self.kinds[mv.seat] == SeatKind::Human {
                    // A person already took their time deciding.
                    0.15
                } else {
                    let base = match self.game.round.phase {
                        Phase::OrderUp | Phase::NameTrump => 0.8,
                        Phase::DealerDiscard => 0.6,
                        _ => 0.42,
                    };
                    base + random_range(0.0, 0.25)
                };
                self.cue(Cue::Wait(think));
                self.cue(Cue::Remote(mv));
            }
            _ => {}
        }
    }

    fn reset_hand_state(&mut self) {
        self.cues.clear();
        self.prompt = Prompt::None;
        self.bids.clear();
        self.hand_done = false;
        self.resyncing = false;
        self.winner_glow = None;
    }

    /// A new deal from the room.
    fn new_hand(&mut self, hand: &HandStart) {
        match Mirror::new(hand) {
            Ok(m) => {
                self.game = m;
                self.score_shown = self.game.scores;
                self.reset_hand_state();
                self.begin_hand();
            }
            Err(e) => {
                crate::assets::log_warn(&format!("unusable deal from the room: {e:?}"));
                self.request_resync();
            }
        }
    }

    /// The whole hand so far (a resync or a reconnect): lay it back out.
    fn lay_out(&mut self, hand: &HandStart, moves: &[Move]) {
        match Mirror::replay(hand, moves) {
            Ok(m) => {
                self.game = m;
                self.score_shown = self.game.scores;
                self.banner = None;
                self.reset_hand_state();
                if self.game.hand_untouched() {
                    self.begin_hand();
                } else {
                    self.begin_resume();
                }
            }
            Err(e) => crate::assets::log_warn(&format!("the room sent a hand that doesn't replay: {e:?}")),
        }
    }

    /// Hidden cards the mirror traded (see `mirror.rs`): their sprites trade
    /// places too. Both are face down, so nothing visibly changes.
    fn swap_cards(&mut self, a: Card, b: Card) {
        self.sprites.swap(a.id(), b.id());
        self.places.swap(a.id(), b.id());
    }

    // ------------------------------------------------------------------
    // Timeline
    // ------------------------------------------------------------------

    fn cue(&mut self, c: Cue) {
        self.cues.push_back(c);
    }

    fn move_card(&mut self, card: Card, place: Place) {
        self.places[card.id()] = place;
        self.sprites[card.id()].z = self.next_z;
        self.next_z += 1;
    }

    /// Queues the gather, shuffle, deal and up card for the current round.
    fn begin_hand(&mut self) {
        self.cue(Cue::Gather);
        self.cue(Cue::Wait(0.55));
        self.cue(Cue::Shuffle);
        self.cue(Cue::Wait(0.5));
        let r = &self.game.round;
        let mut dealt = [0usize; 4];
        let mut deals = Vec::new();
        // Deal in the traditional 3-2 / 2-3 pattern, starting left of the dealer.
        for pattern in [[3, 2, 3, 2], [2, 3, 2, 3]] {
            for (k, &count) in pattern.iter().enumerate() {
                let seat = (r.dealer + 1 + k) % 4;
                for _ in 0..count {
                    deals.push((r.hands[seat][dealt[seat]], seat));
                    dealt[seat] += 1;
                }
            }
        }
        for (card, seat) in deals {
            self.cue(Cue::Deal(card, seat));
            self.cue(Cue::Wait(0.065));
        }
        self.cue(Cue::Wait(0.3));
        self.cue(Cue::RevealUp);
        self.cue(Cue::Wait(0.45));
    }

    /// Where every card belongs in the round's current state.
    fn settled_places(&self) -> [Place; 24] {
        let r = &self.game.round;
        let mut places = [Place::Pile; 24];
        // Once trump is set the kitty (with any discard) sits beside the dealer.
        if matches!(r.phase, Phase::Play | Phase::TrickDone | Phase::HandOver) {
            for c in &r.kitty {
                places[c.id()] = Place::Kitty(r.dealer);
            }
        }
        if r.phase == Phase::OrderUp {
            places[r.up_card.id()] = Place::UpCard;
        }
        for (seat, hand) in r.hands.iter().enumerate() {
            let place = if r.is_sitting_out(seat) { Place::SitOut(seat) } else { Place::Hand(seat) };
            for c in hand {
                places[c.id()] = place;
            }
        }
        // Collected tricks go to their winners' piles. A finished trick that
        // hasn't been collected yet is still on the felt.
        let per_trick = r.active_players();
        let collected = r.trick_winners.len() - usize::from(r.phase == Phase::TrickDone);
        let mut won = [0u8; 4];
        for (i, &winner) in r.trick_winners[..collected].iter().enumerate() {
            for c in &r.played[i * per_trick..(i + 1) * per_trick] {
                places[c.id()] = Place::Won(winner, won[winner]);
            }
            won[winner] += 1;
        }
        for &(seat, c) in &r.trick {
            places[c.id()] = Place::Trick(seat);
        }
        places
    }

    /// Queues the cards of a resumed hand flying back to where they were:
    /// won tricks and the kitty, then the hands dealt out, then the trick in
    /// play order, then the up card.
    fn begin_resume(&mut self) {
        self.cue(Cue::Gather);
        self.cue(Cue::Wait(0.55));
        let places = self.settled_places();
        let set_aside: Vec<(Card, Place)> = (0..24)
            .map(|id| (Card::from_id(id), places[id]))
            .filter(|(_, p)| matches!(p, Place::Won(..) | Place::Kitty(_) | Place::SitOut(_)))
            .collect();
        if !set_aside.is_empty() {
            self.cue(Cue::Lay(set_aside, Sound::CardSlide));
            self.cue(Cue::Wait(0.35));
        }
        let r = &self.game.round;
        let mut deals = Vec::new();
        for i in 0..6 {
            for k in 1..=4 {
                let seat = (r.dealer + k) % 4;
                if !r.is_sitting_out(seat)
                    && let Some(&card) = r.hands[seat].get(i)
                {
                    deals.push((card, seat));
                }
            }
        }
        let trick = r.trick.clone();
        let up_card = (r.phase == Phase::OrderUp).then_some(r.up_card);
        for (card, seat) in deals {
            self.cue(Cue::Deal(card, seat));
            self.cue(Cue::Wait(0.065));
        }
        self.cue(Cue::Wait(0.25));
        for (seat, card) in trick {
            self.cue(Cue::Lay(vec![(card, Place::Trick(seat))], Sound::CardPlace));
            self.cue(Cue::Wait(0.18));
        }
        if up_card.is_some() {
            self.cue(Cue::RevealUp);
            self.cue(Cue::Wait(0.3));
        }
        self.cue(Cue::Say(partner_of(HUMAN), "Welcome back!".into()));
        self.cue(Cue::Wait(0.6));
    }

    /// Writes an offline game to the save slot, unless it's already over.
    /// (The room's match may be a few bot moves ahead of the animation;
    /// they were already decided, so resuming simply shows them done.)
    fn save(&self) {
        if self.game_over.is_none()
            && let Some(m) = self.link.local_match()
        {
            savegame::write(m);
        }
    }

    fn run(&mut self, cue: Cue, shared: &mut Shared) {
        let sfx = &shared.sfx;
        match cue {
            Cue::Wait(s) => self.wait += s * self.delay,
            Cue::Gather => {
                for id in 0..24 {
                    self.move_card(Card::from_id(id), Place::Pile);
                }
                self.winner_glow = None;
                sfx.play(Sound::CardSlide);
                shared.gather_slide();
            }
            Cue::Shuffle => {
                for s in &mut self.sprites {
                    s.x.impulse(random_range(-90.0, 90.0));
                    s.rot.impulse(random_range(-1.5, 1.5));
                }
                sfx.play(Sound::Shuffle);
                shared.riffle();
            }
            Cue::Deal(card, seat) => {
                self.move_card(card, Place::Hand(seat));
                sfx.play(Sound::Deal);
                shared.deal_flick(seat == HUMAN);
            }
            Cue::RevealUp => {
                let up = self.game.round.up_card;
                self.move_card(up, Place::UpCard);
                self.sprites[up.id()].scale.impulse(2.0);
                sfx.play(Sound::Flip);
            }
            Cue::TurnDown => {
                let up = self.game.round.up_card;
                self.move_card(up, Place::Pile);
                sfx.play(Sound::Flip);
            }
            Cue::PickUp => {
                let r = &self.game.round;
                let (up, dealer) = (r.up_card, r.dealer);
                self.move_card(up, Place::Hand(dealer));
                sfx.play(Sound::CardSlide);
            }
            Cue::UpToPile => {
                let up = self.game.round.up_card;
                self.move_card(up, Place::Pile);
            }
            Cue::KittyAside => {
                let r = &self.game.round;
                let dealer = r.dealer;
                for card in r.kitty.clone() {
                    self.move_card(card, Place::Kitty(dealer));
                }
                sfx.play(Sound::CardSlide);
            }
            Cue::SitOut(seat) => {
                for card in self.game.round.hands[seat].clone() {
                    self.move_card(card, Place::SitOut(seat));
                }
                sfx.play(Sound::CardSlide);
            }
            Cue::AiAct(seat) => {
                // Only if nothing (a timed-out turn, a resync) came in meanwhile.
                if self.game.round.to_act() == Some(seat) && seat == HUMAN && self.pending.is_empty() {
                    // The AI only reads what this seat can know, so it plays from the mirror.
                    let action = ai::choose(&self.game.round, seat);
                    self.perform(seat, action, shared);
                }
            }
            Cue::Remote(mv) => self.perform_remote(mv, shared),
            Cue::HighlightWinner => {
                let r = &self.game.round;
                let winner = r.turn;
                if let Some(&(_, card)) = r.trick.iter().find(|(s, _)| *s == winner) {
                    self.winner_glow = Some((card, 0.0));
                    self.sprites[card.id()].scale.impulse(2.2);
                    let team = team_of(winner);
                    let n = r.team_tricks(team) as f32;
                    if team == 0 {
                        sfx.play_pitched(Sound::Chips, 1.0 + (n - 1.0) * 0.09);
                    } else {
                        sfx.play_pitched(Sound::Chips, 0.72 + (n - 1.0) * 0.05);
                    }
                }
            }
            Cue::CollectTrick => {
                let r = &self.game.round;
                let winner = r.turn;
                let n = r.tricks_won[winner] - 1;
                for (_, card) in r.trick.clone() {
                    self.move_card(card, Place::Won(winner, n));
                }
                self.trick_pop[team_of(winner)].impulse(4.0);
                self.winner_glow = None;
                self.game.advance();
                sfx.play(Sound::CardSlide);
            }
            Cue::ShowResult => self.show_result(shared),
            // The room has already dealt again; `drive` takes the deal next.
            Cue::Redeal => self.hand_done = true,
            Cue::Say(seat, text) => self.say(seat, text),
            Cue::Lay(cards, sound) => {
                for (card, place) in cards {
                    self.move_card(card, place);
                }
                sfx.play(sound);
            }
        }
    }

    /// Decides what happens next when the timeline is idle.
    fn drive(&mut self, shared: &Shared) {
        if self.game_over.is_some() {
            // Online: someone asked for another game and the room dealt it.
            let rematch = match self.pending.front() {
                Some(ServerMsg::Deal(h) | ServerMsg::Sync { hand: h, .. }) => h.game != self.game.game,
                _ => false,
            };
            if rematch {
                self.game_over = None;
                self.rematch_asked = false;
                self.next_from_room();
            }
            return;
        }
        if self.banner.is_some() {
            return;
        }
        let r = &self.game.round;
        match r.phase {
            Phase::TrickDone => {
                self.cue(Cue::Wait(0.35));
                self.cue(Cue::HighlightWinner);
                self.cue(Cue::Wait(0.75));
                self.cue(Cue::CollectTrick);
                self.cue(Cue::Wait(0.3));
            }
            Phase::HandOver if !self.hand_done => {
                self.cue(Cue::Wait(0.2));
                self.cue(Cue::ShowResult);
            }
            Phase::Misdeal if !self.hand_done => {
                let dealer = r.dealer;
                self.cue(Cue::Say(dealer, "Misdeal! Redeal.".into()));
                self.cue(Cue::Wait(1.2));
                self.cue(Cue::Redeal);
            }
            // Waiting for the next deal.
            Phase::HandOver | Phase::Misdeal => self.next_from_room(),
            _ => {
                let Some(seat) = r.to_act() else { return };
                if seat != HUMAN || !self.pending.is_empty() {
                    // Another seat's move (or a move made for you, or a resync) from the room.
                    self.prompt = Prompt::None;
                    self.bids.clear();
                    self.next_from_room();
                } else if !self.autoplay {
                    self.setup_prompt(&shared.cards);
                } else if self.prompt == Prompt::None {
                    // Autoplay: show the prompt for a moment, then let the AI decide.
                    self.setup_prompt(&shared.cards);
                    self.cue(Cue::Wait(1.2));
                    self.cue(Cue::AiAct(seat));
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Actions
    // ------------------------------------------------------------------

    /// Your move: checked by the rules here (exactly as the room will check
    /// it), sent to the room, and shown.
    fn perform(&mut self, seat: Seat, action: Action, shared: &mut Shared) -> bool {
        debug_assert_eq!(seat, HUMAN);
        let before = self.game.round.phase;
        let dealer = self.game.round.dealer;
        if let Err(e) = self.game.act(action) {
            shared.sfx.play(Sound::Deny);
            self.hint = Some((explain(e, &self.game.round), 2.2));
            return false;
        }
        self.link.send(ClientMsg::Act { action });
        self.show_action(seat, action, before, dealer, shared);
        true
    }

    /// Another seat's move from the room (or yours, if your turn timed out).
    fn perform_remote(&mut self, mv: Move, shared: &mut Shared) {
        let before = self.game.round.phase;
        let dealer = self.game.round.dealer;
        match self.game.reveal(mv) {
            Ok((action, swaps)) => {
                for (a, b) in swaps {
                    self.swap_cards(a, b);
                }
                self.show_action(mv.seat, action, before, dealer, shared);
            }
            Err(e) => {
                crate::assets::log_warn(&format!("move {mv:?} doesn't fit this table ({e:?}); resyncing"));
                self.request_resync();
            }
        }
    }

    /// Mirrors a move that was just applied: speech, sounds and card motion.
    fn show_action(&mut self, seat: Seat, action: Action, before: Phase, dealer: Seat, shared: &mut Shared) {
        self.prompt = Prompt::None;
        self.bids.clear();
        self.hint = None;
        let sfx = &shared.sfx;
        let phase = self.game.round.phase;
        match action {
            Action::Pass => {
                let turned_down = before == Phase::OrderUp && phase == Phase::NameTrump;
                self.say(seat, if turned_down { "Turn it down." } else { "Pass" }.into());
                sfx.play(Sound::Pass);
                if turned_down {
                    self.cue(Cue::Wait(0.35));
                    self.cue(Cue::TurnDown);
                }
            }
            Action::OrderUp { alone } => {
                let line = if seat == dealer {
                    "I'll take it!"
                } else if partner_of(seat) == dealer {
                    "Pick it up, partner!"
                } else {
                    "Pick it up!"
                };
                self.say(seat, if alone { format!("{line} Alone!") } else { line.into() });
                self.announce_trump(shared);
                self.cue(Cue::Wait(0.5));
                if phase == Phase::DealerDiscard {
                    self.cue(Cue::PickUp);
                } else {
                    // The dealer is sitting out: nobody picks up.
                    self.cue(Cue::UpToPile);
                    self.cue(Cue::Wait(0.25));
                    self.cue(Cue::KittyAside);
                    self.cue(Cue::Wait(0.6));
                }
                if alone {
                    self.sit_out_partner(seat);
                }
            }
            Action::Call { suit, alone } => {
                let line = format!("{} {}!", suit.symbol(), suit.name());
                self.say(seat, if alone { format!("{line} Alone!") } else { line });
                self.announce_trump(shared);
                self.cue(Cue::Wait(0.45));
                self.cue(Cue::KittyAside);
                self.cue(Cue::Wait(0.7));
                if alone {
                    self.sit_out_partner(seat);
                }
            }
            Action::Discard { card } => {
                self.move_card(card, Place::Pile);
                sfx.play(Sound::CardSlide);
                self.cue(Cue::Wait(0.4));
                self.cue(Cue::KittyAside);
                self.cue(Cue::Wait(0.6));
            }
            Action::Play { card } => {
                self.move_card(card, Place::Trick(seat));
                let n = self.game.round.trick.len() as f32;
                sfx.play_pitched(Sound::CardPlace, 1.0 + n * 0.03);
                if seat == HUMAN {
                    self.sprites[card.id()].scale.impulse(1.0);
                }
            }
        }
        if seat == HUMAN && matches!(action, Action::Play { .. } | Action::Discard { .. }) {
            shared.tap();
        }
    }

    /// How a seat is named in sentences ("You", "Rosa").
    fn display_name(&self, seat: Seat) -> String {
        if seat == HUMAN { "You".to_string() } else { titlecase(&self.names[seat]) }
    }

    fn sit_out_partner(&mut self, maker: Seat) {
        let partner = partner_of(maker);
        self.cue(Cue::Wait(0.3));
        if partner != HUMAN {
            self.cue(Cue::Say(partner, "Go get 'em!".into()));
        }
        self.cue(Cue::SitOut(partner));
    }

    fn announce_trump(&mut self, shared: &Shared) {
        let r = &self.game.round;
        let (Some(trump), Some(maker)) = (r.trump, r.maker) else { return };
        let who = self.names[maker].clone();
        let alone = if r.alone { " ALONE" } else { "" };
        let mut pop = Spring::new(0.4);
        pop.target = 1.0;
        self.toast = Some(Toast {
            suit: trump,
            title: format!("{} ARE TRUMP", trump.name().to_uppercase()),
            sub: format!("CALLED BY {who}{alone}"),
            age: 0.0,
            pop,
        });
        self.trump_pop.impulse(5.0);
        shared.sfx.play(Sound::Trump);
    }

    fn say(&mut self, seat: Seat, text: String) {
        let mut pop = Spring::new(0.3);
        pop.target = 1.0;
        self.bubbles[seat] = Some(Bubble { text, age: 0.0, pop });
    }

    fn show_result(&mut self, shared: &mut Shared) {
        let Some(result) = self.game.score_hand() else { return };
        let r = &self.game.round;
        let maker_name = self.display_name(result.maker);
        let trump = r.trump.map(|t| t.symbol()).unwrap_or('?');
        let we_scored = result.scoring_team == 0;
        let title = match (result.kind, result.alone, we_scored) {
            (ResultKind::Euchred, _, true) => "EUCHRED 'EM!",
            (ResultKind::Euchred, _, false) => "EUCHRED!",
            (ResultKind::March, true, _) => "LONE MARCH!",
            (ResultKind::March, false, _) => "MARCH!",
            (ResultKind::Made, _, true) => "WE MADE IT",
            (ResultKind::Made, _, false) => "THEY MADE IT",
        };
        let color = match (result.kind, we_scored) {
            (ResultKind::Made, true) => palette::US,
            (_, true) => palette::GOLD,
            (_, false) => palette::THEM,
        };
        let verb = if result.alone { "went alone in" } else { "called" };
        let detail = format!("{maker_name} {verb} {trump} and took {} of 5 tricks.", result.maker_tricks);
        let team = if we_scored { "US" } else { "THEM" };
        let s = if result.points == 1 { "" } else { "S" };
        let points = format!("{team} +{} POINT{s}", result.points);
        let label = if self.game.winner().is_some() { "FINISH" } else { "NEXT HAND" };
        let mut pop = Spring::new(0.5);
        pop.target = 1.0;
        self.banner = Some(Banner {
            result,
            title,
            color,
            detail,
            points,
            pop,
            button: Button::brass(label, Rect::centered(layout::CENTER.x, layout::CENTER.y + 46.0, 96.0, 20.0)),
            age: 0.0,
        });
        let big = result.kind != ResultKind::Made;
        if big {
            self.shake = 1.0;
            shared.rumble(0.55, 0.4);
        }
        if we_scored {
            shared.sfx.play(if big { Sound::Fanfare } else { Sound::Pop });
            self.burst(draw::PAD_SCORE_US, if big { 40 } else { 16 });
        } else {
            shared.sfx.play(if big { Sound::Womp } else { Sound::Pop });
        }
    }

    fn continue_after_hand(&mut self, shared: &mut Shared) {
        self.banner = None;
        self.score_shown = self.game.scores;
        if let Some(winner) = self.game.winner() {
            let won = winner == 0;
            // This game is finished, so it can't be continued any more.
            if let Some(m) = self.link.local_match() {
                savegame::forget(m.id);
            }
            if self.autoplay {
                eprintln!(
                    "[euchre] autoplay: game over after {} hands, {}-{}",
                    self.game.hand_number, self.game.scores[0], self.game.scores[1]
                );
            }
            let mut pop = Spring::new(0.4);
            pop.target = 1.0;
            self.game_over = Some(GameOver {
                won,
                pop,
                again: Button::brass("PLAY AGAIN", Rect::centered(layout::CENTER.x - 56.0, 236.0, 100.0, 22.0)),
                menu: Button::grey("MAIN MENU", Rect::centered(layout::CENTER.x + 56.0, 236.0, 100.0, 22.0)),
                age: 0.0,
                focus: 0,
            });
            shared.rumble(if won { 0.7 } else { 0.35 }, if won { 0.6 } else { 0.3 });
            if won {
                shared.sfx.play(Sound::Fanfare);
                self.shake = 0.8;
                for i in 0..5 {
                    self.burst(vec2(150.0 + i as f32 * 85.0, 80.0), 24);
                }
            } else {
                shared.sfx.play(Sound::Womp);
            }
        } else {
            // The room dealt the next hand already; `drive` picks it up.
            self.hand_done = true;
        }
    }

    fn new_game(&mut self, shared: &Shared) {
        if self.link.is_online() {
            // Everyone at the table gets the new deal; `drive` lays it out.
            if !self.rematch_asked {
                self.rematch_asked = true;
                self.link.send(ClientMsg::Start);
            }
            self.hint = Some(("Shuffling up a new game...".into(), 3.0));
        } else {
            *self = Table::new(shared);
        }
    }

    /// Back to the title: an offline game is saved, an online seat goes to a bot.
    fn quit_to_title(&mut self, shared: &mut Shared) -> Transition {
        if self.link.is_online() {
            self.link.send(ClientMsg::Leave);
        } else {
            self.save();
        }
        shared.settings.save();
        Transition::Switch(Box::new(Title::new()))
    }

    fn burst(&mut self, at: Vec2, count: usize) {
        let colors = [palette::GOLD, palette::CREAM, palette::BRASS_LIGHT, palette::US];
        for i in 0..count {
            let a = random_range(0.0, std::f32::consts::TAU);
            let speed = random_range(40.0, 150.0);
            let life = random_range(0.6, 1.3);
            self.particles.push(Particle {
                pos: at,
                vel: vec2(a.cos(), a.sin()) * speed - vec2(0.0, 40.0),
                life,
                max: life,
                color: colors[i % colors.len()],
                size: if i % 3 == 0 { 3.0 } else { 2.0 },
            });
        }
    }

    // ------------------------------------------------------------------
    // Human input
    // ------------------------------------------------------------------

    fn setup_prompt(&mut self, cards: &crate::cards::CardArt) {
        let r = &self.game.round;
        let wanted = match r.phase {
            Phase::OrderUp => Prompt::OrderUp,
            Phase::NameTrump => Prompt::NameTrump,
            Phase::DealerDiscard => Prompt::Discard,
            Phase::Play => Prompt::Play,
            _ => Prompt::None,
        };
        if wanted == self.prompt {
            return;
        }
        if wanted == Prompt::Play {
            // Keep the focus where it was if that card can be played, else jump to the first one that can.
            let hand = layout::hand_cards(self, HUMAN);
            let legal = r.legal_plays(HUMAN);
            if !hand.get(self.card_focus).is_some_and(|c| legal.contains(c)) {
                self.card_focus = hand.iter().position(|c| legal.contains(c)).unwrap_or(0);
            }
        }
        self.prompt = wanted;
        self.alone = false;
        self.bids.clear();
        let mut row: Vec<(Bid, Button)> = Vec::new();
        match wanted {
            Prompt::OrderUp => {
                let label = if r.dealer == HUMAN { "PICK UP" } else { "ORDER UP" };
                row.push((Bid::OrderUp, Button::green(label, Rect::new(0.0, 0.0, 70.0, 18.0))));
                row.push((Bid::Pass, Button::red("PASS", Rect::new(0.0, 0.0, 50.0, 18.0))));
                row.push((Bid::Alone, Button::grey("ALONE", Rect::new(0.0, 0.0, 52.0, 18.0))));
            }
            Prompt::NameTrump => {
                for suit in Suit::ALL {
                    if suit == r.up_card.suit {
                        continue;
                    }
                    let mut b = Button::new(
                        suit.name(),
                        Rect::new(0.0, 0.0, 28.0, 22.0),
                        palette::CREAM,
                        palette::CREAM_SHADE.lerp(palette::INK, 0.3),
                    );
                    b.text_color = crate::cards::suit_color(suit);
                    b.icon = Some((cards.image().clone(), cards.icon(suit)));
                    row.push((Bid::Suit(suit), b));
                }
                let mut pass = Button::red("PASS", Rect::new(0.0, 0.0, 50.0, 18.0));
                pass.enabled = !(r.dealer == HUMAN && r.stick_the_dealer);
                row.push((Bid::Pass, pass));
                row.push((Bid::Alone, Button::grey("ALONE", Rect::new(0.0, 0.0, 52.0, 18.0))));
            }
            _ => {}
        }
        // Focus PASS first (the usual call), unless the dealer is stuck.
        self.bid_focus = row
            .iter()
            .position(|(k, b)| *k == Bid::Pass && b.enabled)
            .or_else(|| row.iter().position(|(_, b)| b.enabled))
            .unwrap_or(0);
        // Center the row above the hand.
        let gap = 6.0;
        let total: f32 = row.iter().map(|(_, b)| b.rect.w).sum::<f32>() + gap * (row.len().max(1) - 1) as f32;
        let mut x = (layout::CENTER.x - total / 2.0).round();
        for (_, b) in &mut row {
            b.rect.x = x;
            b.rect.y = (layout::PROMPT_Y + 21.0 - b.rect.h / 2.0).round();
            x += b.rect.w + gap;
        }
        self.bids = row;
    }

    fn click_bid(&mut self, bid: Bid, shared: &mut Shared) {
        let r = &self.game.round;
        match bid {
            Bid::Alone => {
                self.alone = !self.alone;
                let on = self.alone;
                if let Some((_, b)) = self.bids.iter_mut().find(|(k, _)| *k == Bid::Alone) {
                    if on {
                        b.color = palette::ORANGE;
                        b.lip = palette::ORANGE_DARK;
                        b.label = "ALONE!".into();
                    } else {
                        b.color = palette::GREY_BUTTON;
                        b.lip = palette::GREY_BUTTON_DARK;
                        b.label = "ALONE".into();
                    }
                }
            }
            Bid::Pass => {
                self.perform(HUMAN, Action::Pass, shared);
            }
            Bid::OrderUp => {
                let alone = self.alone;
                self.perform(HUMAN, Action::OrderUp { alone }, shared);
            }
            Bid::Suit(suit) => {
                let _ = r;
                let alone = self.alone;
                self.perform(HUMAN, Action::Call { suit, alone }, shared);
            }
        }
    }

    fn click_card(&mut self, card: Card, shared: &mut Shared) {
        match self.prompt {
            Prompt::Play => {
                if !self.perform(HUMAN, Action::Play { card }, shared) {
                    // Illegal: shake the card "no".
                    let sprite = &mut self.sprites[card.id()];
                    sprite.rot.impulse(2.5);
                    sprite.x.impulse(60.0);
                }
            }
            Prompt::Discard => {
                self.perform(HUMAN, Action::Discard { card }, shared);
            }
            _ => {
                // Not your turn: a little wiggle so the card still feels alive.
                self.sprites[card.id()].rot.impulse(1.2);
                shared.sfx.play(Sound::Flip);
            }
        }
    }

    /// Whether the human may interact with the table right now.
    fn input_open(&self) -> bool {
        self.pause.is_none() && self.overlay.is_none() && self.game_over.is_none() && self.banner.is_none()
    }

    fn open_pause(&mut self, shared: &Shared) {
        let cx = layout::CENTER.x;
        let mut pop = Spring::new(0.85);
        pop.target = 1.0;
        let w = 120.0;
        // Once the game is over there is nothing left to save. Online games
        // aren't saved: leaving hands your seat to a bot.
        let quit_label = if self.link.is_online() {
            "LEAVE TABLE"
        } else if self.game_over.is_some() {
            "MAIN MENU"
        } else {
            "SAVE & QUIT"
        };
        self.pause = Some(Pause {
            buttons: vec![
                (PauseChoice::Resume, Button::brass("RESUME", Rect::centered(cx, 154.0, w, 20.0))),
                (PauseChoice::Options, Button::blue("OPTIONS", Rect::centered(cx, 180.0, w, 20.0))),
                (PauseChoice::Rules, Button::green("HOW TO PLAY", Rect::centered(cx, 206.0, w, 20.0))),
                (PauseChoice::Quit, Button::red(quit_label, Rect::centered(cx, 232.0, w, 20.0))),
            ],
            pop,
            focus: 0,
        });
        shared.sfx.play(Sound::Pop);
    }

    fn pause_choice(&mut self, choice: PauseChoice, shared: &mut Shared) -> Transition {
        match choice {
            PauseChoice::Resume => self.pause = None,
            PauseChoice::Options => self.overlay = Some(Overlay::Options(Box::new(OptionsPanel::new()))),
            PauseChoice::Rules => self.overlay = Some(Overlay::Rules(Box::new(RulesPanel::new()))),
            PauseChoice::Quit => return self.quit_to_title(shared),
        }
        Transition::None
    }

    fn human_hand_order(&self) -> Vec<Card> {
        layout::hand_cards(self, HUMAN)
    }

    /// The hand card the gamepad focus is on, while a card is wanted.
    fn focused_card(&self) -> Option<Card> {
        if !matches!(self.prompt, Prompt::Play | Prompt::Discard) {
            return None;
        }
        let hand = self.human_hand_order();
        hand.get(self.card_focus.min(hand.len().saturating_sub(1))).copied()
    }

    /// Presses the bid button at `i` from the gamepad.
    fn press_bid(&mut self, i: usize, shared: &mut Shared) {
        let Some((bid, b)) = self.bids.get_mut(i) else { return };
        let bid = *bid;
        if b.activate(&shared.sfx) {
            shared.tap();
            self.click_bid(bid, shared);
        }
    }

    // ------------------------------------------------------------------
    // Per-frame animation
    // ------------------------------------------------------------------

    fn animate(&mut self, shared: &mut Shared, dt: f32) {
        // Hover is tested against resting positions, so a lifted card doesn't flicker.
        let hovered = if !self.input_open() {
            None
        } else if shared.focus_visible() {
            self.focused_card()
        } else {
            layout::card_at(self, shared.mouse)
        };
        if hovered != self.hover {
            if let Some(c) = hovered {
                let playable = match self.prompt {
                    Prompt::Play => self.game.round.legal_plays(HUMAN).contains(&c),
                    Prompt::Discard => true,
                    _ => false,
                };
                if playable {
                    shared.sfx.play(Sound::Hover);
                }
            }
            self.hover = hovered;
        }

        let targets: Vec<Target> = layout::targets(self, shared.mouse);
        for (s, t) in self.sprites.iter_mut().zip(&targets) {
            s.x.target = t.pos.x;
            s.y.target = t.pos.y;
            s.rot.target = t.rot;
            s.scale.target = t.scale;
            s.x.update(dt);
            s.y.update(dt);
            s.rot.update(dt);
            s.scale.update(dt);
            let face = if t.face_up { 1.0 } else { 0.0 };
            s.face = approach(s.face, face, 16.0, dt);
            if (s.face - face).abs() < 0.02 {
                s.face = face;
            }
            s.tint = approach(s.tint, t.tint, 10.0, dt);
        }

        for b in self.bubbles.iter_mut() {
            if let Some(bubble) = b {
                bubble.age += dt;
                bubble.pop.update(dt);
                if bubble.age > 1.8 * self.delay.max(0.8) {
                    *b = None;
                }
            }
        }
        if let Some(t) = &mut self.toast {
            t.age += dt;
            t.pop.update(dt);
            if t.age > 1.5 {
                self.toast = None;
            }
        }
        if let Some((_, t)) = &mut self.hint {
            *t -= dt;
            if *t <= 0.0 {
                self.hint = None;
            }
        }
        if let Some((_, t)) = &mut self.winner_glow {
            *t += dt;
        }
        self.shake = (self.shake - dt * 2.5).max(0.0);
        for p in &mut self.particles {
            p.vel.y += 160.0 * dt;
            p.vel *= 1.0 - 1.5 * dt;
            p.pos += p.vel * dt;
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
        for s in self.score_pop.iter_mut().chain(self.trick_pop.iter_mut()) {
            s.update(dt);
        }
        self.trump_pop.update(dt);

        // Count the score up point by point while the result is showing.
        if self.banner.as_ref().is_some_and(|b| b.age > 0.45) {
            self.score_tick -= dt;
            if self.score_tick <= 0.0 {
                for team in 0..2 {
                    if self.score_shown[team] < self.game.scores[team] {
                        self.score_shown[team] += 1;
                        self.score_pop[team].impulse(6.0);
                        shared.sfx.play_pitched(Sound::Tick, 1.0 + self.score_shown[team] as f32 * 0.06);
                        self.score_tick = 0.2;
                    }
                }
            }
        }
    }
}

fn titlecase(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(f) => f.to_string() + &chars.as_str().to_lowercase(),
        None => String::new(),
    }
}

fn explain(e: RuleError, r: &crate::euchre::Round) -> String {
    match e {
        RuleError::MustFollowSuit => match r.led_suit() {
            Some(s) => format!("You must follow {} if you can.", s.symbol()),
            None => "You must follow suit.".into(),
        },
        RuleError::DealerMustCall => "Stick the dealer: you must name trump.".into(),
        RuleError::SuitTurnedDown => "That suit was turned down.".into(),
        RuleError::NotYourTurn | RuleError::WrongPhase => "Not your turn yet.".into(),
        RuleError::CardNotInHand => "That card isn't in your hand.".into(),
    }
}

impl Scene for Table {
    fn update(&mut self, shared: &mut Shared, _ctx: &mut Context, dt: f32) -> Transition {
        self.delay = shared.settings.delay_factor();
        self.pump_link();
        if let Some(o) = &mut self.overlay {
            o.update(shared, dt);
        }
        let focus = shared.focus_visible();
        if let Some(p) = &mut self.pause {
            p.pop.update(dt);
            let mouse = if self.overlay.is_some() { NO_POINTER } else { shared.mouse };
            for (i, (_, b)) in p.buttons.iter_mut().enumerate() {
                b.focused = focus && self.overlay.is_none() && i == p.focus;
                b.update(mouse, dt, &shared.sfx);
            }
        }

        if self.pause.is_none() && self.overlay.is_none() {
            self.wait -= dt;
            let mut guard = 0;
            while self.wait <= 0.0 && guard < 64 {
                guard += 1;
                match self.cues.pop_front() {
                    Some(cue) => self.run(cue, shared),
                    None => {
                        self.drive(shared);
                        if self.cues.is_empty() {
                            break;
                        }
                    }
                }
            }
            if self.wait < 0.0 {
                self.wait = 0.0;
            }
        }

        let open = self.input_open();
        let mouse = if open { shared.mouse } else { NO_POINTER };
        for (i, (_, b)) in self.bids.iter_mut().enumerate() {
            b.focused = focus && open && i == self.bid_focus;
            b.update(mouse, dt, &shared.sfx);
        }
        let menu_mouse = if self.pause.is_none() && self.overlay.is_none() { shared.mouse } else { NO_POINTER };
        self.menu_button.update(menu_mouse, dt, &shared.sfx);

        if let Some(b) = &mut self.banner {
            b.age += dt;
            b.pop.update(dt);
            let m = if self.pause.is_none() { shared.mouse } else { NO_POINTER };
            b.button.focused = focus && self.pause.is_none() && b.age > 0.4;
            b.button.update(m, dt, &shared.sfx);
            // Online, the table moves on without you after a while, so a
            // player reading the result doesn't hold up the next bid.
            let patience = if self.link.is_online() { 7.0 } else { f32::INFINITY };
            if (self.autoplay && b.age > 2.0) || b.age > patience {
                self.continue_after_hand(shared);
            }
        }
        if let Some(g) = &mut self.game_over {
            g.age += dt;
            g.pop.update(dt);
            let m = if self.pause.is_none() { shared.mouse } else { NO_POINTER };
            g.again.focused = focus && self.pause.is_none() && g.focus == 0;
            g.menu.focused = focus && self.pause.is_none() && g.focus == 1;
            g.again.update(m, dt, &shared.sfx);
            g.menu.update(m, dt, &shared.sfx);
            if self.autoplay && g.age > 3.0 {
                self.new_game(shared);
            }
        }

        self.animate(shared, dt);
        Transition::None
    }

    fn draw(&mut self, shared: &Shared, g: &mut Graphics) {
        self.draw_table(shared, g);
    }

    fn suspend(&mut self, _shared: &mut Shared) {
        self.save();
    }

    fn key_pressed(&mut self, shared: &mut Shared, key: Key) -> Transition {
        if let Some(o) = &mut self.overlay {
            if o.key_pressed(shared, key) {
                self.overlay = None;
            }
            return Transition::None;
        }
        if self.pause.is_some() {
            if key == Key::Escape {
                self.pause = None;
                shared.sfx.play(Sound::Select);
            }
            return Transition::None;
        }
        if key == Key::Escape {
            self.open_pause(shared);
            return Transition::None;
        }
        if let Some(go) = &mut self.game_over {
            if matches!(key, Key::Enter | Key::Space) {
                go.again.flash(&shared.sfx);
                self.new_game(shared);
            }
            return Transition::None;
        }
        if let Some(b) = &mut self.banner {
            if matches!(key, Key::Enter | Key::Space) && b.age > 0.4 {
                b.button.flash(&shared.sfx);
                self.continue_after_hand(shared);
            }
            return Transition::None;
        }
        // Bidding shortcuts: O order up, P pass, A alone, C/D/S/H name a suit.
        let bid = match key {
            Key::O => Some(Bid::OrderUp),
            Key::P => Some(Bid::Pass),
            Key::A => Some(Bid::Alone),
            Key::C => Some(Bid::Suit(Suit::Clubs)),
            Key::D => Some(Bid::Suit(Suit::Diamonds)),
            Key::S => Some(Bid::Suit(Suit::Spades)),
            Key::H => Some(Bid::Suit(Suit::Hearts)),
            _ => None,
        };
        if let Some(bid) = bid {
            if let Some((_, b)) = self.bids.iter_mut().find(|(k, _)| *k == bid) {
                if b.enabled {
                    b.flash(&shared.sfx);
                    self.click_bid(bid, shared);
                } else {
                    shared.sfx.play(Sound::Deny);
                }
            }
            return Transition::None;
        }
        let digit = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6]
            .iter()
            .position(|&k| k == key);
        if let Some(i) = digit
            && matches!(self.prompt, Prompt::Play | Prompt::Discard)
                && let Some(&card) = self.human_hand_order().get(i) {
                    self.click_card(card, shared);
                }
        Transition::None
    }

    fn nav(&mut self, shared: &mut Shared, nav: Nav) -> Transition {
        if let Some(o) = &mut self.overlay {
            if o.nav(shared, nav) {
                self.overlay = None;
            }
            return Transition::None;
        }
        if let Some(p) = &mut self.pause {
            match nav {
                Nav::Up | Nav::Down => {
                    let n = p.buttons.len() as i32;
                    p.focus = (p.focus as i32 + nav.dy()).rem_euclid(n) as usize;
                    shared.tick();
                }
                Nav::Confirm => {
                    let (choice, b) = &mut p.buttons[p.focus];
                    let choice = *choice;
                    b.flash(&shared.sfx);
                    shared.tap();
                    return self.pause_choice(choice, shared);
                }
                Nav::Back | Nav::Menu => {
                    self.pause = None;
                    shared.sfx.play(Sound::Select);
                    shared.tap();
                }
                _ => {}
            }
            return Transition::None;
        }
        if nav == Nav::Menu {
            shared.tap();
            self.open_pause(shared);
            return Transition::None;
        }
        if let Some(go) = &mut self.game_over {
            match nav {
                Nav::Left | Nav::Right | Nav::Up | Nav::Down => {
                    go.focus = 1 - go.focus;
                    shared.tick();
                }
                Nav::Confirm if go.focus == 0 => {
                    go.again.flash(&shared.sfx);
                    shared.tap();
                    self.new_game(shared);
                }
                Nav::Confirm => {
                    go.menu.flash(&shared.sfx);
                    shared.tap();
                    return self.quit_to_title(shared);
                }
                _ => {}
            }
            return Transition::None;
        }
        if let Some(b) = &mut self.banner {
            if nav == Nav::Confirm && b.age > 0.4 {
                b.button.flash(&shared.sfx);
                shared.tap();
                self.continue_after_hand(shared);
            }
            return Transition::None;
        }
        match self.prompt {
            Prompt::OrderUp | Prompt::NameTrump => match nav {
                Nav::Left | Nav::Right => {
                    let n = self.bids.len().max(1) as i32;
                    self.bid_focus = (self.bid_focus as i32 + nav.dx()).rem_euclid(n) as usize;
                    shared.tick();
                }
                Nav::Confirm => self.press_bid(self.bid_focus, shared),
                Nav::Alt => {
                    if let Some(i) = self.bids.iter().position(|(k, _)| *k == Bid::Alone) {
                        self.press_bid(i, shared);
                    }
                }
                _ => {}
            },
            Prompt::Play | Prompt::Discard => match nav {
                Nav::Left | Nav::Right => {
                    let n = self.human_hand_order().len().max(1) as i32;
                    let i = (self.card_focus.min(n as usize - 1) as i32 + nav.dx()).rem_euclid(n);
                    self.card_focus = i as usize;
                    shared.tick();
                    // Playable cards tick from the hover logic; unplayable ones get a dull tap.
                    if self.prompt == Prompt::Play
                        && let Some(c) = self.focused_card()
                        && !self.game.round.legal_plays(HUMAN).contains(&c)
                    {
                        shared.sfx.play_pitched(Sound::Tick, 0.6);
                    }
                }
                Nav::Confirm => {
                    if let Some(card) = self.focused_card() {
                        self.click_card(card, shared);
                    }
                }
                _ => {}
            },
            Prompt::None => {}
        }
        Transition::None
    }

    fn touch_pressed(&mut self, shared: &mut Shared, pos: Vec2) -> Transition {
        if let Some(o) = &mut self.overlay {
            o.touch_pressed(shared, pos);
        }
        Transition::None
    }

    fn touch_released(&mut self, shared: &mut Shared, pos: Vec2) -> Transition {
        if let Some(o) = &mut self.overlay {
            if o.touch_released(shared, pos) {
                self.overlay = None;
            }
            return Transition::None;
        }
        self.mouse_pressed(shared, pos, MouseButton::Left)
    }

    fn mouse_pressed(&mut self, shared: &mut Shared, pos: Vec2, button: MouseButton) -> Transition {
        if button != MouseButton::Left {
            return Transition::None;
        }
        if let Some(o) = &mut self.overlay {
            if o.mouse_pressed(shared, pos) {
                self.overlay = None;
            }
            return Transition::None;
        }
        if let Some(p) = &mut self.pause {
            let mut chosen = None;
            for (choice, b) in &mut p.buttons {
                if b.click(pos, &shared.sfx) {
                    chosen = Some(*choice);
                }
            }
            if let Some(c) = chosen {
                return self.pause_choice(c, shared);
            }
            return Transition::None;
        }
        if self.menu_button.click(pos, &shared.sfx) {
            self.open_pause(shared);
            return Transition::None;
        }
        if let Some(go) = &mut self.game_over {
            if go.again.click(pos, &shared.sfx) {
                self.new_game(shared);
            } else if go.menu.click(pos, &shared.sfx) {
                return self.quit_to_title(shared);
            }
            return Transition::None;
        }
        if let Some(b) = &mut self.banner {
            if b.age > 0.3 && b.button.click(pos, &shared.sfx) {
                self.continue_after_hand(shared);
            }
            return Transition::None;
        }
        let mut clicked_bid = None;
        for (bid, b) in &mut self.bids {
            if b.click(pos, &shared.sfx) {
                clicked_bid = Some(*bid);
            }
        }
        if let Some(bid) = clicked_bid {
            self.click_bid(bid, shared);
            return Transition::None;
        }
        if let Some(card) = layout::card_at(self, pos) {
            self.click_card(card, shared);
        }
        Transition::None
    }
}
