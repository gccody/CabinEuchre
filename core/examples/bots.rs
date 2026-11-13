//! How strong each difficulty is, and what it costs to think: plays thousands of
//! bot-only games and prints the win rates. Run it after changing a number in
//! `ai::SKILLS`:
//!
//! ```sh
//! cargo run -p euchre-core --release --example bots          # 4000 deals a line
//! cargo run -p euchre-core --release --example bots -- 20000 # steadier numbers
//! ```
//!
//! Every pairing plays each deal twice with the seats swapped, so the cards favor nobody.
//! With 4000 deals a line is good to about a point either way.

use std::time::Instant;

use euchre_core::Match;
use euchre_core::ai::{self, Difficulty, Skill, Standing};
use euchre_core::round::Phase;

/// Plays one game to 10 with a skill per seat. Returns the winning team and how many moves the bots made.
fn play(seed: u64, skills: [&Skill; 4]) -> (usize, u64) {
    let mut m = Match::new(10, true, seed);
    let mut moves = 0;
    loop {
        if let Some(team) = m.winner() {
            return (team, moves);
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
                let seat = m.round.to_act().expect("someone is due to act");
                let standing = Standing { scores: m.scores, target: m.target };
                let action = ai::choose_with(&m.round, seat, skills[seat], standing);
                m.apply(seat, action).expect("bots play by the rules");
                moves += 1;
            }
        }
    }
}

/// The share of games the team of `a` and `b` wins against two of `them`.
fn win_rate(a: &Skill, b: &Skill, them: &Skill, deals: u64) -> f64 {
    let mut wins = 0;
    for seed in 0..deals {
        wins += u64::from(play(seed, [a, them, b, them]).0 == 0);
        wins += u64::from(play(seed, [them, a, them, b]).0 == 1);
    }
    wins as f64 / (deals * 2) as f64 * 100.0
}

fn main() {
    let deals: u64 = std::env::args().nth(1).and_then(|n| n.parse().ok()).unwrap_or(4000);
    let hard = Difficulty::Hard.skill();

    println!("{deals} deals, each played from both sides.");
    println!("\"You\" are a Hard bot in a person's seat, with the partner the game would give you.\n");
    println!("{:<10} {:>10} {:>10} {:>16} {:>10}", "level", "you win", "partner", "two beat 2 Hard", "per move");
    for level in Difficulty::ALL {
        let skill = level.skill();
        let you = win_rate(hard, level.as_partner().skill(), skill, deals);
        let pair = win_rate(skill, skill, hard, deals);
        let started = Instant::now();
        let mut moves = 0;
        for seed in 0..deals.min(2000) {
            moves += play(seed, [skill; 4]).1;
        }
        let nanos = started.elapsed().as_nanos() as f64 / moves as f64;
        println!(
            "{:<10} {:>9.1}% {:>10} {:>15.1}% {:>7.0} ns",
            level.name(),
            you,
            level.as_partner().name(),
            pair,
            nanos
        );
    }
}
