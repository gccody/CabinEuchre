//! A player's record at the table: lifetime stats and trophies (achievements).
//!
//! Everything comes from what the player's own table can see, seat 0 of their
//! [`Mirror`](crate::Mirror), so offline, online and every platform count
//! the same way. Who keeps it and where trophies go after that (Steam, Game
//! Center, Play Games) is the game's business, not this module's.
//!
//! A hand is only counted once. A saved game replayed from its last hand, or a
//! reconnect that shows the result banner again, arrives here with the same
//! game and hand number and is ignored.

use crate::card::Suit;
use crate::round::{HandResult, ResultKind, Round, Seat};

const ME: Seat = 0;
/// The player's team.
const US: usize = 0;
/// How far behind a winner has to have been for the comeback trophy.
pub const COMEBACK_DEFICIT: u32 = 5;

/// Every trophy in the game. The order here is the order on the shelf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Achievement {
    FirstGame,
    FirstWin,
    Tutorial,
    Euchre,
    March,
    LoneMade,
    LoneMarch,
    StopLone,
    PartnerRide,
    Stuck,
    BothBowers,
    AllSuits,
    Shutout,
    Comeback,
    Clean,
    Streak3,
    Streak5,
    Wins10,
    Wins50,
    Euchre25,
    Hands100,
    Hands1000,
    OnlineWin,
    FullTable,
}

impl Achievement {
    pub const ALL: [Achievement; 24] = [
        Achievement::FirstGame,
        Achievement::FirstWin,
        Achievement::Tutorial,
        Achievement::Euchre,
        Achievement::March,
        Achievement::LoneMade,
        Achievement::LoneMarch,
        Achievement::StopLone,
        Achievement::PartnerRide,
        Achievement::Stuck,
        Achievement::BothBowers,
        Achievement::AllSuits,
        Achievement::Shutout,
        Achievement::Comeback,
        Achievement::Clean,
        Achievement::Streak3,
        Achievement::Streak5,
        Achievement::Wins10,
        Achievement::Wins50,
        Achievement::Euchre25,
        Achievement::Hands100,
        Achievement::Hands1000,
        Achievement::OnlineWin,
        Achievement::FullTable,
    ];

    /// The name the stores know it by. These are saved in files and set up on
    /// Steamworks, App Store Connect and the Play Console, so never rename one.
    pub const fn id(self) -> &'static str {
        match self {
            Achievement::FirstGame => "first_game",
            Achievement::FirstWin => "first_win",
            Achievement::Tutorial => "tutorial",
            Achievement::Euchre => "euchre",
            Achievement::March => "march",
            Achievement::LoneMade => "lone_made",
            Achievement::LoneMarch => "lone_march",
            Achievement::StopLone => "stop_lone",
            Achievement::PartnerRide => "partner_ride",
            Achievement::Stuck => "stuck",
            Achievement::BothBowers => "both_bowers",
            Achievement::AllSuits => "all_suits",
            Achievement::Shutout => "shutout",
            Achievement::Comeback => "comeback",
            Achievement::Clean => "clean",
            Achievement::Streak3 => "streak_3",
            Achievement::Streak5 => "streak_5",
            Achievement::Wins10 => "wins_10",
            Achievement::Wins50 => "wins_50",
            Achievement::Euchre25 => "euchre_25",
            Achievement::Hands100 => "hands_100",
            Achievement::Hands1000 => "hands_1000",
            Achievement::OnlineWin => "online_win",
            Achievement::FullTable => "full_table",
        }
    }

    pub fn from_id(id: &str) -> Option<Achievement> {
        Achievement::ALL.into_iter().find(|a| a.id() == id)
    }

    pub const fn title(self) -> &'static str {
        match self {
            Achievement::FirstGame => "PULL UP A CHAIR",
            Achievement::FirstWin => "CABIN CHAMP",
            Achievement::Tutorial => "BENNY'S STAR PUPIL",
            Achievement::Euchre => "EUCHRED 'EM",
            Achievement::March => "MARCHING ORDERS",
            Achievement::LoneMade => "FLYING SOLO",
            Achievement::LoneMarch => "LONE LOON",
            Achievement::StopLone => "NOT SO FAST",
            Achievement::PartnerRide => "ALONG FOR THE RIDE",
            Achievement::Stuck => "STUCK AND MADE IT",
            Achievement::BothBowers => "BOTH BARRELS",
            Achievement::AllSuits => "HOUSE OF CARDS",
            Achievement::Shutout => "SKUNKED",
            Achievement::Comeback => "COMEBACK TRAIL",
            Achievement::Clean => "CLEAN SHEET",
            Achievement::Streak3 => "HOT HAND",
            Achievement::Streak5 => "ON A TEAR",
            Achievement::Wins10 => "TABLE REGULAR",
            Achievement::Wins50 => "LODGE LEGEND",
            Achievement::Euchre25 => "THORN IN THEIR SIDE",
            Achievement::Hands100 => "LATE NIGHT",
            Achievement::Hands1000 => "LAKE SEASON",
            Achievement::OnlineWin => "LONG DISTANCE",
            Achievement::FullTable => "FULL TABLE",
        }
    }

    /// How to earn it, in one short line.
    pub const fn how(self) -> &'static str {
        match self {
            Achievement::FirstGame => "Finish your first game.",
            Achievement::FirstWin => "Win a game.",
            Achievement::Tutorial => "Finish Learn to Play.",
            Achievement::Euchre => "Euchre the other team.",
            Achievement::March => "Take all 5 tricks after your team calls.",
            Achievement::LoneMade => "Go alone and make it.",
            Achievement::LoneMarch => "Go alone and take all 5 tricks.",
            Achievement::StopLone => "Euchre someone who went alone.",
            Achievement::PartnerRide => "Your partner goes alone and marches.",
            Achievement::Stuck => "Get stuck as dealer and make it.",
            Achievement::BothBowers => "Play both bowers in a hand you score.",
            Achievement::AllSuits => "Call each of the four suits as trump.",
            Achievement::Shutout => "Win before they score a point.",
            Achievement::Comeback => "Win after trailing by 5 or more.",
            Achievement::Clean => "Win a game without being euchred.",
            Achievement::Streak3 => "Win 3 games in a row.",
            Achievement::Streak5 => "Win 5 games in a row.",
            Achievement::Wins10 => "Win 10 games.",
            Achievement::Wins50 => "Win 50 games.",
            Achievement::Euchre25 => "Euchre the other team 25 times.",
            Achievement::Hands100 => "Play 100 hands.",
            Achievement::Hands1000 => "Play 1,000 hands.",
            Achievement::OnlineWin => "Win a game online.",
            Achievement::FullTable => "Finish an online game with 4 people.",
        }
    }

    /// The count this trophy is earned at.
    pub const fn goal(self) -> u32 {
        match self {
            Achievement::AllSuits => 4,
            Achievement::Streak3 => 3,
            Achievement::Streak5 => 5,
            Achievement::Wins10 => 10,
            Achievement::Wins50 => 50,
            Achievement::Euchre25 => 25,
            Achievement::Hands100 => 100,
            Achievement::Hands1000 => 1000,
            _ => 1,
        }
    }

    /// How far along `stats` are, which can pass the goal.
    pub fn progress(self, s: &Stats) -> u32 {
        match self {
            Achievement::FirstGame => s.games,
            Achievement::FirstWin | Achievement::Wins10 | Achievement::Wins50 => s.wins,
            Achievement::Tutorial => s.tutorials,
            Achievement::Euchre | Achievement::Euchre25 => s.euchres,
            Achievement::March => s.marches,
            Achievement::LoneMade => s.lones_made,
            Achievement::LoneMarch => s.lone_marches,
            Achievement::StopLone => s.lones_stopped,
            Achievement::PartnerRide => s.partner_lone_marches,
            Achievement::Stuck => s.stuck_made,
            Achievement::BothBowers => s.both_bowers,
            Achievement::AllSuits => s.suits_called.count_ones(),
            Achievement::Shutout => s.shutouts,
            Achievement::Comeback => s.comebacks,
            Achievement::Clean => s.clean_wins,
            Achievement::Streak3 | Achievement::Streak5 => s.best_streak,
            Achievement::Hands100 | Achievement::Hands1000 => s.hands,
            Achievement::OnlineWin => s.online_wins,
            Achievement::FullTable => s.full_tables,
        }
    }
}

/// Lifetime numbers. "Your team" counts both of you; "you" only your seat.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    /// Games played to the end. A game you left doesn't count either way.
    pub games: u32,
    pub wins: u32,
    pub streak: u32,
    pub best_streak: u32,
    pub online_games: u32,
    pub online_wins: u32,
    /// Hands played to the end (misdeals don't count).
    pub hands: u32,
    /// Hands your team scored on.
    pub hands_won: u32,
    /// Tricks your own cards won.
    pub tricks: u32,
    /// Points your team scored, and points scored against it.
    pub points_for: u32,
    pub points_against: u32,
    /// Times you called trump, and how many of those your team made.
    pub calls: u32,
    pub calls_made: u32,
    /// Bit per suit you've called ([`suit_bit`]).
    pub suits_called: u32,
    /// Your team euchred the other team.
    pub euchres: u32,
    /// The other team euchred your team.
    pub euchred: u32,
    /// Your team took all five after calling.
    pub marches: u32,
    /// Times you went alone, made it, and took all five.
    pub lones: u32,
    pub lones_made: u32,
    pub lone_marches: u32,
    pub lones_stopped: u32,
    pub partner_lone_marches: u32,
    pub stuck_made: u32,
    pub both_bowers: u32,
    pub shutouts: u32,
    pub comebacks: u32,
    pub clean_wins: u32,
    pub full_tables: u32,
    /// Times you finished the tutorial.
    pub tutorials: u32,
}

impl Stats {
    /// Every field with its name in the file. New fields just need a line here.
    fn fields(&mut self) -> [(&'static str, &mut u32); 29] {
        [
            ("games", &mut self.games),
            ("wins", &mut self.wins),
            ("streak", &mut self.streak),
            ("best_streak", &mut self.best_streak),
            ("online_games", &mut self.online_games),
            ("online_wins", &mut self.online_wins),
            ("hands", &mut self.hands),
            ("hands_won", &mut self.hands_won),
            ("tricks", &mut self.tricks),
            ("points_for", &mut self.points_for),
            ("points_against", &mut self.points_against),
            ("calls", &mut self.calls),
            ("calls_made", &mut self.calls_made),
            ("suits_called", &mut self.suits_called),
            ("euchres", &mut self.euchres),
            ("euchred", &mut self.euchred),
            ("marches", &mut self.marches),
            ("lones", &mut self.lones),
            ("lones_made", &mut self.lones_made),
            ("lone_marches", &mut self.lone_marches),
            ("lones_stopped", &mut self.lones_stopped),
            ("partner_lone_marches", &mut self.partner_lone_marches),
            ("stuck_made", &mut self.stuck_made),
            ("both_bowers", &mut self.both_bowers),
            ("shutouts", &mut self.shutouts),
            ("comebacks", &mut self.comebacks),
            ("clean_wins", &mut self.clean_wins),
            ("full_tables", &mut self.full_tables),
            ("tutorials", &mut self.tutorials),
        ]
    }
}

/// A finished hand as your table saw it.
#[derive(Debug, Clone, Copy)]
pub struct FinishedHand<'a> {
    /// Names this game. A new name starts a new run of hands.
    pub game: &'a str,
    pub hand_number: u32,
    /// The hand, played to the end.
    pub round: &'a Round,
    /// Yours first, with this hand's points added.
    pub scores: [u32; 2],
    /// The team that reached the target with this hand, if one did.
    pub winner: Option<usize>,
    pub online: bool,
    /// How many of the four seats people are sitting in, you included.
    pub people: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    pub stats: Stats,
    /// In the order they were earned.
    pub unlocked: Vec<Achievement>,
    /// The game being counted, and the last hand of it that was.
    game: String,
    last_hand: u32,
    /// In that game so far: the most your team has trailed by, and whether it was euchred.
    worst_deficit: u32,
    was_euchred: bool,
}

impl Record {
    pub fn is_unlocked(&self, a: Achievement) -> bool {
        self.unlocked.contains(&a)
    }

    /// Counts a finished hand, and the game if that ended it. Returns any
    /// trophies that earned, which is nothing for a hand already counted.
    pub fn finish_hand(&mut self, hand: FinishedHand) -> Vec<Achievement> {
        let Some(result) = hand.round.result() else { return Vec::new() };
        if self.game == hand.game && hand.hand_number <= self.last_hand {
            return Vec::new();
        }
        if self.game != hand.game {
            self.game = hand.game.to_string();
            self.worst_deficit = 0;
            self.was_euchred = false;
        }
        self.last_hand = hand.hand_number;
        self.count_hand(&hand, result);
        self.worst_deficit = self.worst_deficit.max(hand.scores[1].saturating_sub(hand.scores[0]));
        if let Some(winner) = hand.winner {
            self.count_game(&hand, winner);
        }
        self.check()
    }

    fn count_hand(&mut self, hand: &FinishedHand, result: HandResult) {
        let round = hand.round;
        let s = &mut self.stats;
        let ours = result.maker_team == US;
        let scored = result.scoring_team == US;
        let mine = result.maker == ME;
        s.hands += 1;
        s.tricks += u32::from(round.tricks_won[ME]);
        if scored {
            s.hands_won += 1;
            s.points_for += result.points;
        } else {
            s.points_against += result.points;
        }
        match result.kind {
            ResultKind::Euchred if ours => {
                s.euchred += 1;
                self.was_euchred = true;
            }
            ResultKind::Euchred => {
                s.euchres += 1;
                s.lones_stopped += u32::from(result.alone);
            }
            ResultKind::March if ours => {
                s.marches += 1;
                if result.alone && !mine {
                    s.partner_lone_marches += 1;
                }
            }
            _ => {}
        }
        if mine {
            let made = result.kind != ResultKind::Euchred;
            s.calls += 1;
            s.calls_made += u32::from(made);
            if let Some(trump) = round.trump {
                s.suits_called |= suit_bit(trump);
            }
            if result.alone {
                s.lones += 1;
                s.lones_made += u32::from(made);
                s.lone_marches += u32::from(result.kind == ResultKind::March);
            }
            // Only the second round lets the dealer be stuck: everyone else had passed
            // twice, and it's the only way to call a suit that wasn't the up card.
            let stuck = round.stick_the_dealer && round.dealer == ME && round.trump != Some(round.up_card.suit);
            s.stuck_made += u32::from(stuck && made);
        }
        if scored && played_both_bowers(round) {
            s.both_bowers += 1;
        }
    }

    fn count_game(&mut self, hand: &FinishedHand, winner: usize) {
        let s = &mut self.stats;
        let won = winner == US;
        s.games += 1;
        if hand.online {
            s.online_games += 1;
            s.online_wins += u32::from(won);
            s.full_tables += u32::from(hand.people == 4);
        }
        if !won {
            s.streak = 0;
            return;
        }
        s.wins += 1;
        s.streak += 1;
        s.best_streak = s.best_streak.max(s.streak);
        s.shutouts += u32::from(hand.scores[1] == 0);
        s.comebacks += u32::from(self.worst_deficit >= COMEBACK_DEFICIT);
        s.clean_wins += u32::from(!self.was_euchred);
    }

    /// The tutorial was played to the end. Returns any trophy that earned.
    pub fn finish_tutorial(&mut self) -> Vec<Achievement> {
        self.stats.tutorials += 1;
        self.check()
    }

    /// Unlocks whatever the stats have earned that isn't unlocked yet. Returns those.
    pub fn check(&mut self) -> Vec<Achievement> {
        let new: Vec<Achievement> = Achievement::ALL
            .into_iter()
            .filter(|&a| !self.is_unlocked(a) && a.progress(&self.stats) >= a.goal())
            .collect();
        self.unlocked.extend(&new);
        new
    }

    /// A trophy the store says this player already has (earned on another
    /// device, or before a reinstall). Returns whether it was news.
    pub fn merge_from_store(&mut self, a: Achievement) -> bool {
        if self.is_unlocked(a) {
            return false;
        }
        self.unlocked.push(a);
        true
    }

    /// The record as a text file.
    pub fn to_text(&self) -> String {
        let mut stats = self.stats.clone();
        let mut out = String::new();
        for (name, value) in stats.fields() {
            out += &format!("{name}={value}\n");
        }
        let unlocked: Vec<&str> = self.unlocked.iter().map(|a| a.id()).collect();
        out += &format!("unlocked={}\n", unlocked.join(","));
        out += &format!("game={}\n", self.game);
        out += &format!("last_hand={}\n", self.last_hand);
        out += &format!("worst_deficit={}\n", self.worst_deficit);
        out += &format!("was_euchred={}\n", self.was_euchred);
        out
    }

    /// Reads [`Record::to_text`]. It's the player's history, so whatever a
    /// damaged line loses, the rest is kept. Unknown names are ignored.
    pub fn from_text(text: &str) -> Record {
        let mut r = Record::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let number = v.parse::<u32>().ok();
            match k {
                "unlocked" => {
                    for a in v.split(',').filter_map(Achievement::from_id) {
                        r.merge_from_store(a);
                    }
                }
                "game" => r.game = v.to_string(),
                "last_hand" => r.last_hand = number.unwrap_or(0),
                "worst_deficit" => r.worst_deficit = number.unwrap_or(0),
                "was_euchred" => r.was_euchred = v == "true",
                _ => {
                    if let (Some((_, field)), Some(n)) = (r.stats.fields().into_iter().find(|(name, _)| *name == k), number) {
                        *field = n;
                    }
                }
            }
        }
        r
    }
}

/// Whether your own cards this hand included both bowers. A bower you discarded
/// was never played, so it doesn't count.
fn played_both_bowers(round: &Round) -> bool {
    let Some(trump) = round.trump else { return false };
    let mine = round.played_by(ME);
    mine.iter().any(|c| c.is_right_bower(trump)) && mine.iter().any(|c| c.is_left_bower(trump))
}

/// The bit [`Stats::suits_called`] uses for a suit.
pub fn suit_bit(suit: Suit) -> u32 {
    1 << suit.index()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Rank::*;
    use crate::card::Suit::*;
    use crate::card::{Card, Rank};
    use crate::round::{Action, Phase};

    fn c(rank: Rank, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    /// Seat 0 holds both bowers and the ace, king and queen of hearts. The up
    /// card is the nine of `up`, one of the three suits left for it.
    fn strong_hand(dealer: Seat, up: Suit) -> Round {
        let hands = [
            vec![c(Jack, Hearts), c(Jack, Diamonds), c(Ace, Hearts), c(King, Hearts), c(Queen, Hearts)],
            vec![c(Ace, Spades), c(King, Spades), c(Queen, Spades), c(Jack, Spades), c(Ten, Spades)],
            vec![c(Ace, Clubs), c(King, Clubs), c(Queen, Clubs), c(Jack, Clubs), c(Ten, Clubs)],
            vec![c(Ace, Diamonds), c(King, Diamonds), c(Queen, Diamonds), c(Ten, Diamonds), c(Nine, Clubs)],
        ];
        let kitty = vec![c(Ten, Hearts), c(Nine, Hearts), c(Nine, Diamonds), c(Nine, Spades)]
            .into_iter()
            .filter(|&card| card != c(Nine, up))
            .collect();
        Round::with_hands(dealer, hands, c(Nine, up), kitty, true)
    }

    /// Plays out the hand: everyone plays their first legal card.
    fn play_out(round: &mut Round) {
        loop {
            match round.phase {
                Phase::TrickDone => round.advance(),
                Phase::Play => {
                    let seat = round.turn;
                    let card = round.legal_plays(seat)[0];
                    round.apply(seat, Action::Play { card }).unwrap();
                }
                _ => return,
            }
        }
    }

    /// Seat 0 calls hearts in the second round with the hand above, and takes everything.
    fn marched(dealer: Seat, alone: bool) -> Round {
        let mut round = strong_hand(dealer, Spades);
        let mut seat = (dealer + 1) % 4;
        for _ in 0..4 {
            round.apply(seat, Action::Pass).unwrap();
            seat = (seat + 1) % 4;
        }
        while seat != 0 {
            round.apply(seat, Action::Pass).unwrap();
            seat = (seat + 1) % 4;
        }
        round.apply(0, Action::Call { suit: Hearts, alone }).unwrap();
        play_out(&mut round);
        assert_eq!(round.result().unwrap().kind, ResultKind::March);
        round
    }

    fn hand<'a>(game: &'a str, n: u32, round: &'a Round, scores: [u32; 2], winner: Option<usize>) -> FinishedHand<'a> {
        FinishedHand { game, hand_number: n, round, scores, winner, online: false, people: 1 }
    }

    fn online<'a>(game: &'a str, round: &'a Round, people: usize) -> FinishedHand<'a> {
        FinishedHand { online: true, people, ..hand(game, 1, round, [10, 0], Some(0)) }
    }

    #[test]
    fn a_march_is_counted_with_everything_it_earns() {
        let round = marched(3, false);
        let mut r = Record::default();
        let new = r.finish_hand(hand("g", 1, &round, [2, 0], None));
        assert_eq!(r.stats.hands, 1);
        assert_eq!(r.stats.hands_won, 1);
        assert_eq!(r.stats.tricks, 5);
        assert_eq!(r.stats.calls, 1);
        assert_eq!(r.stats.calls_made, 1);
        assert_eq!(r.stats.marches, 1);
        assert_eq!(r.stats.points_for, 2);
        assert_eq!(r.stats.suits_called, suit_bit(Hearts));
        assert_eq!(r.stats.both_bowers, 1);
        assert!(new.contains(&Achievement::March));
        assert!(new.contains(&Achievement::BothBowers));
        assert!(!new.contains(&Achievement::FirstGame), "the game isn't over");
    }

    #[test]
    fn showing_the_same_hand_again_does_not_count_it_twice() {
        let round = marched(3, false);
        let mut r = Record::default();
        r.finish_hand(hand("g", 4, &round, [6, 0], Some(0)));
        let before = r.clone();
        assert!(r.finish_hand(hand("g", 4, &round, [6, 0], Some(0))).is_empty());
        assert!(r.finish_hand(hand("g", 3, &round, [6, 0], None)).is_empty(), "or an older one");
        assert_eq!(r, before);
        r.finish_hand(hand("other", 1, &round, [2, 0], None));
        assert_eq!(r.stats.hands, 2, "another game's hand 1 is news");
    }

    #[test]
    fn the_guard_survives_a_save_and_load() {
        let round = marched(3, false);
        let mut r = Record::default();
        r.finish_hand(hand("g", 4, &round, [10, 0], Some(0)));
        let mut loaded = Record::from_text(&r.to_text());
        assert_eq!(loaded, r);
        assert!(loaded.finish_hand(hand("g", 4, &round, [10, 0], Some(0))).is_empty());
        assert_eq!(loaded.stats.games, 1);
    }

    #[test]
    fn going_alone_counts_for_the_maker_and_the_partner_differently() {
        let round = marched(3, true);
        let mut me = Record::default();
        let new = me.finish_hand(hand("g", 1, &round, [4, 0], None));
        assert_eq!((me.stats.lones, me.stats.lones_made, me.stats.lone_marches), (1, 1, 1));
        assert!(new.contains(&Achievement::LoneMarch) && new.contains(&Achievement::LoneMade));
        assert!(!new.contains(&Achievement::PartnerRide));

        // The same cards with you in the partner's chair, sitting it out.
        let mut round = strong_hand(3, Spades);
        round.hands.swap(0, 2);
        for seat in [0, 1, 2, 3, 0, 1] {
            round.apply(seat, Action::Pass).unwrap();
        }
        round.apply(2, Action::Call { suit: Hearts, alone: true }).unwrap();
        play_out(&mut round);
        let mut partner = Record::default();
        let new = partner.finish_hand(hand("g", 1, &round, [4, 0], None));
        assert_eq!(partner.stats.lones, 0);
        assert_eq!(partner.stats.partner_lone_marches, 1);
        assert_eq!(new, vec![Achievement::March, Achievement::PartnerRide]);
    }

    #[test]
    fn stuck_only_counts_for_a_dealer_naming_in_the_second_round() {
        let stuck = marched(0, false);
        let mut r = Record::default();
        r.finish_hand(hand("g", 1, &stuck, [2, 0], None));
        assert_eq!(r.stats.stuck_made, 1);

        let not_dealer = marched(3, false);
        let mut r = Record::default();
        r.finish_hand(hand("g", 1, &not_dealer, [2, 0], None));
        assert_eq!(r.stats.stuck_made, 0);

        // Picking up the up card yourself isn't being stuck.
        let mut round = strong_hand(0, Hearts);
        for seat in [1, 2, 3] {
            round.apply(seat, Action::Pass).unwrap();
        }
        round.apply(0, Action::OrderUp { alone: false }).unwrap();
        round.apply(0, Action::Discard { card: c(Queen, Hearts) }).unwrap();
        play_out(&mut round);
        let mut r = Record::default();
        r.finish_hand(hand("g", 1, &round, [2, 0], None));
        assert_eq!(r.stats.calls, 1);
        assert_eq!(r.stats.stuck_made, 0);
    }

    /// Seat 1 orders up hearts into seat 0's hand, and seat 0 takes everything.
    fn euchred_them(alone: bool) -> Round {
        let mut round = strong_hand(0, Hearts);
        round.apply(1, Action::OrderUp { alone }).unwrap();
        round.apply(0, Action::Discard { card: c(Queen, Hearts) }).unwrap();
        play_out(&mut round);
        assert_eq!(round.result().unwrap().kind, ResultKind::Euchred);
        round
    }

    #[test]
    fn euchres_land_on_the_right_team() {
        let round = euchred_them(false);
        let mut r = Record::default();
        let new = r.finish_hand(hand("g", 1, &round, [2, 0], None));
        assert_eq!((r.stats.euchres, r.stats.euchred, r.stats.calls), (1, 0, 0));
        assert_eq!(r.stats.points_for, 2);
        assert!(new.contains(&Achievement::Euchre));
        assert!(!new.contains(&Achievement::StopLone));

        let round = euchred_them(true);
        let mut r = Record::default();
        let new = r.finish_hand(hand("g", 1, &round, [2, 0], None));
        assert_eq!(r.stats.lones_stopped, 1);
        assert!(new.contains(&Achievement::StopLone));
    }

    /// Seat 0 orders up on nothing and goes set.
    fn got_euchred() -> Round {
        let hands = [
            vec![c(Nine, Hearts), c(Ten, Clubs), c(Nine, Clubs), c(Ten, Spades), c(Nine, Spades)],
            vec![c(Jack, Hearts), c(Jack, Diamonds), c(Ace, Hearts), c(King, Hearts), c(Queen, Hearts)],
            vec![c(Queen, Clubs), c(Jack, Clubs), c(King, Spades), c(Queen, Spades), c(Jack, Spades)],
            vec![c(Ace, Diamonds), c(King, Diamonds), c(Queen, Diamonds), c(Ace, Clubs), c(King, Clubs)],
        ];
        let kitty = vec![c(Ace, Spades), c(Ten, Diamonds), c(Nine, Diamonds)];
        let mut round = Round::with_hands(3, hands, c(Ten, Hearts), kitty, true);
        round.apply(0, Action::OrderUp { alone: false }).unwrap();
        round.apply(3, Action::Discard { card: c(King, Clubs) }).unwrap();
        play_out(&mut round);
        assert_eq!(round.result().unwrap().kind, ResultKind::Euchred);
        round
    }

    #[test]
    fn being_euchred_costs_the_clean_win_and_earns_the_comeback() {
        let (set, win) = (got_euchred(), marched(3, false));
        let mut r = Record::default();
        r.finish_hand(hand("g", 1, &set, [0, 2], None));
        r.finish_hand(hand("g", 2, &set, [0, 4], None));
        r.finish_hand(hand("g", 3, &set, [0, 6], None));
        assert_eq!(r.stats.euchred, 3);
        assert_eq!(r.stats.calls, 3);
        assert_eq!(r.stats.calls_made, 0);
        assert_eq!(r.stats.points_against, 6);
        assert_eq!(r.stats.suits_called, suit_bit(Hearts));
        let new = r.finish_hand(hand("g", 4, &win, [10, 6], Some(0)));
        assert_eq!(r.stats.comebacks, 1);
        assert_eq!(r.stats.clean_wins, 0);
        assert_eq!(r.stats.shutouts, 0);
        assert!(new.contains(&Achievement::Comeback));
        assert!(new.contains(&Achievement::FirstWin));
        assert!(new.contains(&Achievement::FirstGame));
        assert!(!new.contains(&Achievement::Clean));
    }

    #[test]
    fn a_new_game_starts_a_new_run() {
        let (set, win) = (got_euchred(), marched(3, false));
        let mut r = Record::default();
        r.finish_hand(hand("old", 1, &set, [0, 8], None));
        let new = r.finish_hand(hand("new", 1, &win, [10, 0], Some(0)));
        assert_eq!(r.stats.comebacks, 0, "the deficit was in another game");
        assert_eq!(r.stats.clean_wins, 1);
        assert_eq!(r.stats.shutouts, 1);
        assert!(new.contains(&Achievement::Clean) && new.contains(&Achievement::Shutout));
    }

    #[test]
    fn streaks_end_on_a_loss_but_the_best_one_stays() {
        let win = marched(3, false);
        let mut r = Record::default();
        for (i, won) in [true, true, true, false, true].into_iter().enumerate() {
            let game = i.to_string();
            let winner = if won { 0 } else { 1 };
            r.finish_hand(hand(&game, 1, &win, [2, 0], Some(winner)));
        }
        assert_eq!(r.stats.games, 5);
        assert_eq!(r.stats.wins, 4);
        assert_eq!(r.stats.streak, 1);
        assert_eq!(r.stats.best_streak, 3);
        assert!(r.is_unlocked(Achievement::Streak3));
        assert!(!r.is_unlocked(Achievement::Streak5));
    }

    #[test]
    fn online_games_count_online_and_people_matter() {
        let win = marched(3, false);
        let mut r = Record::default();
        let new = r.finish_hand(online("a", &win, 3));
        assert_eq!((r.stats.online_games, r.stats.online_wins, r.stats.full_tables), (1, 1, 0));
        assert!(new.contains(&Achievement::OnlineWin));
        assert!(r.finish_hand(online("b", &win, 4)).contains(&Achievement::FullTable));
        r.finish_hand(hand("c", 1, &win, [10, 0], Some(0)));
        assert_eq!(r.stats.online_games, 2);
        assert_eq!(r.stats.games, 3);
    }

    #[test]
    fn a_hand_still_being_played_counts_for_nothing() {
        let round = strong_hand(3, Spades);
        let mut r = Record::default();
        assert!(r.finish_hand(hand("g", 1, &round, [0, 0], None)).is_empty());
        assert_eq!(r, Record::default());
    }

    #[test]
    fn the_tutorial_earns_its_trophy_once() {
        let mut r = Record::default();
        assert_eq!(r.finish_tutorial(), vec![Achievement::Tutorial]);
        assert!(r.finish_tutorial().is_empty());
        assert_eq!(r.stats.tutorials, 2);
    }

    #[test]
    fn counters_earn_their_trophies_at_the_goal() {
        let mut r = Record::default();
        r.stats.wins = 9;
        r.stats.hands = 100;
        let new = r.check();
        assert!(new.contains(&Achievement::Hands100));
        assert!(!new.contains(&Achievement::Wins10));
        r.stats.wins = 10;
        assert!(r.check().contains(&Achievement::Wins10));
        assert!(r.check().is_empty());
    }

    #[test]
    fn trophies_from_the_store_are_kept_without_the_stats() {
        let mut r = Record::default();
        assert!(r.merge_from_store(Achievement::Wins50));
        assert!(!r.merge_from_store(Achievement::Wins50));
        assert!(r.check().is_empty(), "having it already isn't news");
        let loaded = Record::from_text(&r.to_text());
        assert!(loaded.is_unlocked(Achievement::Wins50));
        assert_eq!(loaded.stats.wins, 0);
    }

    #[test]
    fn damaged_files_keep_what_they_can() {
        let r = Record::from_text("wins=12\nhands=lots\nnonsense\nunlocked=march,not_a_trophy,euchre,march\nfuture_stat=3\n");
        assert_eq!(r.stats.wins, 12);
        assert_eq!(r.stats.hands, 0);
        assert_eq!(r.unlocked, vec![Achievement::March, Achievement::Euchre]);
    }

    #[test]
    fn every_stat_makes_the_round_trip() {
        let mut r = Record::default();
        let count = r.stats.fields().len() as u32;
        for (i, (_, field)) in r.stats.fields().into_iter().enumerate() {
            *field = i as u32 + 1;
        }
        assert_eq!(r.stats.tutorials, count);
        assert_eq!(Record::from_text(&r.to_text()).stats, r.stats);
    }

    #[test]
    fn played_by_follows_the_lead_through_a_hand() {
        let round = marched(0, false);
        // Seat 1 led the first trick; seat 0 won it with the right bower and led the rest.
        let mine = round.played_by(0);
        assert_eq!(mine.len(), 5);
        assert_eq!(mine[0], c(Jack, Hearts));
        assert_eq!(round.played_by(1).len(), 5);
        assert_eq!(round.played_by(1)[0], c(Ace, Spades));
        // A lone hand skips the partner.
        let round = marched(3, true);
        assert_eq!(round.played_by(2), Vec::new());
        assert_eq!(round.played_by(0).len(), 5);
    }

    #[test]
    fn the_docs_list_every_trophy() {
        let docs = include_str!("../../docs/achievements.md");
        for a in Achievement::ALL {
            let row = format!("| `{}` | {} | {} |", a.id(), a.title(), a.how());
            assert!(docs.contains(&row), "docs/achievements.md should have: {row}");
        }
    }

    #[test]
    fn ids_are_unique_and_plain_enough_for_every_store() {
        let mut ids: Vec<&str> = Achievement::ALL.iter().map(|a| a.id()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), Achievement::ALL.len());
        for a in Achievement::ALL {
            assert_eq!(Achievement::from_id(a.id()), Some(a));
            assert!(a.id().chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'), "{}", a.id());
            assert!(a.goal() > 0);
        }
    }
}
