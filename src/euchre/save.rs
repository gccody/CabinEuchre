//! Saving a match in progress as a small, human-readable text snapshot.
//!
//! The snapshot holds the score and the RNG state at the start of the current
//! hand, plus every step taken since. Loading re-deals the hand from that RNG
//! state and replays the steps through the real rules, so a resumed game is
//! exactly the one that was left, and a damaged or edited file is rejected
//! instead of producing an impossible table.
//!
//! ```text
//! version=1
//! id=1730419200
//! target=10
//! stick_the_dealer=true
//! scores=3 4
//! hand=6
//! dealer=2
//! rng=11400714819323198485
//! steps=pass order discard:Tc play:Jh play:Ah play:Kh play:Qh next
//! ```

use super::card::{Card, Rank, Suit};
use super::game::{Match, Step};
use super::rng::Rng;
use super::round::{Action, Phase, SEATS};

const VERSION: u32 = 1;

/// Why a save could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadError(pub String);

fn err<T>(msg: impl Into<String>) -> Result<T, LoadError> {
    Err(LoadError(msg.into()))
}

impl Match {
    /// The snapshot text for [`Match::from_save`].
    ///
    /// Points for a hand that was already scored are left out (along with
    /// the hand being "scored"), so a game saved while the result is showing
    /// resumes with that result being counted up again rather than twice.
    pub fn to_save(&self) -> String {
        let mut scores = self.scores;
        if self.scored
            && let Some(result) = self.round.result()
        {
            scores[result.scoring_team] -= result.points;
        }
        let steps: Vec<String> = self.log.iter().map(|&s| step_token(s)).collect();
        format!(
            "version={VERSION}\nid={}\ntarget={}\nstick_the_dealer={}\nscores={} {}\nhand={}\ndealer={}\nrng={}\nsteps={}\n",
            self.id,
            self.target,
            self.stick_the_dealer,
            scores[0],
            scores[1],
            self.hand_number,
            self.round.dealer,
            self.hand_rng.state(),
            steps.join(" "),
        )
    }

    /// Rebuilds a match from [`Match::to_save`] text.
    pub fn from_save(text: &str) -> Result<Match, LoadError> {
        let mut version = None;
        let mut id = None;
        let mut target = None;
        let mut stick = None;
        let mut scores = None;
        let mut hand = None;
        let mut dealer = None;
        let mut rng = None;
        let mut steps = None;
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "version" => version = v.parse::<u32>().ok(),
                "id" => id = v.parse::<u64>().ok(),
                "target" => target = v.parse::<u32>().ok(),
                "stick_the_dealer" => stick = v.parse::<bool>().ok(),
                "scores" => {
                    let mut it = v.split_whitespace().map(|n| n.parse::<u32>().ok());
                    scores = match (it.next(), it.next(), it.next()) {
                        (Some(Some(a)), Some(Some(b)), None) => Some([a, b]),
                        _ => None,
                    };
                }
                "hand" => hand = v.parse::<u32>().ok(),
                "dealer" => dealer = v.parse::<usize>().ok(),
                "rng" => rng = v.parse::<u64>().ok(),
                "steps" => {
                    steps = v.split_whitespace().map(parse_step).collect::<Option<Vec<Step>>>();
                    if steps.is_none() {
                        return err("unreadable step");
                    }
                }
                _ => {}
            }
        }
        if version != Some(VERSION) {
            return err(format!("unsupported version {version:?}"));
        }
        let (Some(id), Some(target), Some(stick), Some(scores), Some(hand), Some(dealer), Some(rng), Some(steps)) =
            (id, target, stick, scores, hand, dealer, rng, steps)
        else {
            return err("missing field");
        };
        if target == 0 || scores.iter().any(|&s| s >= target) || dealer >= SEATS || hand == 0 {
            return err("impossible score, target or dealer");
        }

        let mut m = Match::starting_hand(scores, target, stick, hand, dealer, id, Rng::new(rng));
        for step in steps {
            match step {
                Step::Act(action) => {
                    let Some(seat) = m.round.to_act() else {
                        return err(format!("{action:?} when nobody is due to act"));
                    };
                    if let Err(e) = m.apply(seat, action) {
                        return err(format!("{action:?} by seat {seat}: {e:?}"));
                    }
                }
                Step::Advance => {
                    if m.round.phase != Phase::TrickDone {
                        return err("cleared a trick that wasn't finished");
                    }
                    m.advance();
                }
            }
        }
        Ok(m)
    }
}

fn card_token(c: Card) -> String {
    let rank = ['9', 'T', 'J', 'Q', 'K', 'A'][c.rank.index()];
    format!("{rank}{}", suit_char(c.suit))
}

fn suit_char(s: Suit) -> char {
    ['c', 'd', 's', 'h'][s.index()]
}

fn parse_suit(s: &str) -> Option<Suit> {
    let mut chars = s.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    Suit::ALL.into_iter().find(|&suit| suit_char(suit) == c)
}

fn parse_card(s: &str) -> Option<Card> {
    let mut chars = s.chars();
    let rank = match chars.next()? {
        '9' => Rank::Nine,
        'T' => Rank::Ten,
        'J' => Rank::Jack,
        'Q' => Rank::Queen,
        'K' => Rank::King,
        'A' => Rank::Ace,
        _ => return None,
    };
    Some(Card::new(rank, parse_suit(chars.as_str())?))
}

fn step_token(step: Step) -> String {
    match step {
        Step::Advance => "next".into(),
        Step::Act(Action::Pass) => "pass".into(),
        Step::Act(Action::OrderUp { alone: false }) => "order".into(),
        Step::Act(Action::OrderUp { alone: true }) => "order-alone".into(),
        Step::Act(Action::Call { suit, alone: false }) => format!("call:{}", suit_char(suit)),
        Step::Act(Action::Call { suit, alone: true }) => format!("call-alone:{}", suit_char(suit)),
        Step::Act(Action::Discard(c)) => format!("discard:{}", card_token(c)),
        Step::Act(Action::Play(c)) => format!("play:{}", card_token(c)),
    }
}

fn parse_step(token: &str) -> Option<Step> {
    let (word, arg) = token.split_once(':').unwrap_or((token, ""));
    let action = match (word, arg) {
        ("next", "") => return Some(Step::Advance),
        ("pass", "") => Action::Pass,
        ("order", "") => Action::OrderUp { alone: false },
        ("order-alone", "") => Action::OrderUp { alone: true },
        ("call", s) => Action::Call { suit: parse_suit(s)?, alone: false },
        ("call-alone", s) => Action::Call { suit: parse_suit(s)?, alone: true },
        ("discard", c) => Action::Discard(parse_card(c)?),
        ("play", c) => Action::Play(parse_card(c)?),
        _ => return None,
    };
    Some(Step::Act(action))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::euchre::ai::choose;

    /// Everything that matters about a match, for comparing a loaded copy.
    fn fingerprint(m: &Match) -> String {
        format!("{:?} {} {} {} {:?}", m.scores, m.target, m.stick_the_dealer, m.hand_number, m.round)
    }

    /// Plays AI games, saving and reloading after every single step: the
    /// loaded match must match the live one and keep playing identically.
    #[test]
    fn every_step_round_trips() {
        for seed in 0..30u64 {
            let mut m = Match::new(5, seed % 3 != 0, seed);
            let mut guard = 0;
            while m.winner().is_none() {
                guard += 1;
                assert!(guard < 5_000, "game {seed} did not finish");
                let text = m.to_save();
                let loaded = Match::from_save(&text).unwrap_or_else(|e| panic!("seed {seed}: {e:?}\n{text}"));
                if !m.scored {
                    assert_eq!(fingerprint(&loaded), fingerprint(&m), "seed {seed}\n{text}");
                }
                assert_eq!(loaded.round.trick_winners, m.round.trick_winners);
                assert_eq!(loaded.rng.state(), m.rng.state(), "future deals must match");
                match m.round.phase {
                    Phase::TrickDone => m.advance(),
                    Phase::Misdeal => m.next_hand(),
                    Phase::HandOver => {
                        if m.scored {
                            // Saved after scoring: the loaded copy scores the same hand to the same total.
                            let mut l = loaded;
                            l.score_hand();
                            assert_eq!(l.scores, m.scores);
                            m.next_hand();
                        } else {
                            m.score_hand();
                        }
                    }
                    _ => {
                        let seat = m.round.to_act().unwrap();
                        m.apply(seat, choose(&m.round, seat)).unwrap();
                    }
                }
            }
        }
    }

    #[test]
    fn readable_format() {
        let mut m = Match::new(10, true, 42);
        let seat = m.round.to_act().unwrap();
        m.apply(seat, Action::Pass).unwrap();
        let text = m.to_save();
        assert!(text.starts_with("version=1\nid=42\ntarget=10\n"), "{text}");
        assert!(text.contains("\nscores=0 0\nhand=1\n"), "{text}");
        assert!(text.ends_with("steps=pass\n"), "{text}");
    }

    #[test]
    fn rejects_bad_saves() {
        let mut m = Match::new(10, true, 7);
        let seat = m.round.to_act().unwrap();
        m.apply(seat, Action::Pass).unwrap();
        let good = m.to_save();
        assert!(Match::from_save(&good).is_ok());

        let cases = [
            ("", "empty"),
            (&good.replace("version=1", "version=99")[..], "future version"),
            (&good.replace("scores=0 0", "scores=0 12")[..], "game already over"),
            (&good.replace("scores=0 0", "scores=0")[..], "one score"),
            (&good.replace("\ndealer=", "\ndealer=9")[..], "no such dealer"),
            (&good.replace("steps=pass", "steps=pass play:Ah")[..], "illegal step"),
            (&good.replace("steps=pass", "steps=pass next")[..], "no trick to clear"),
            (&good.replace("steps=pass", "steps=jump")[..], "unknown step"),
            (&good.replace("steps=pass", "steps=play:Zz")[..], "unknown card"),
            (&good.replace("\nrng=", "\nxrng=")[..], "missing rng"),
        ];
        for (text, why) in cases {
            assert!(Match::from_save(text).is_err(), "should reject: {why}");
        }
    }

    #[test]
    fn card_tokens_round_trip() {
        for c in Card::deck() {
            assert_eq!(parse_card(&card_token(c)), Some(c));
        }
    }
}
