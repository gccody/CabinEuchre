//! The player's record on disk: `stats.txt` next to the settings. The rules for
//! what counts live in `euchre::record`; this keeps the file, and notes what
//! was just earned so the app can show it and tell the store.

use std::path::PathBuf;

use crate::euchre::record::{Achievement, FinishedHand, Record};
use crate::settings::{Settings, config_dir};

pub struct PlayerRecord {
    pub data: Record,
    /// Earned and not yet shown or sent to the store.
    fresh: Vec<Achievement>,
}

fn path() -> Option<PathBuf> {
    if Settings::use_defaults() || is_demo() {
        return None;
    }
    Some(config_dir()?.join("stats.txt"))
}

impl PlayerRecord {
    pub fn load() -> PlayerRecord {
        let text = path().and_then(|p| std::fs::read_to_string(p).ok());
        let mut data = text.map_or_else(Record::default, |t| Record::from_text(&t));
        apply_demo(&mut data);
        // An update can add a trophy that the numbers have already earned.
        let fresh = data.check();
        let record = PlayerRecord { data, fresh };
        if !record.fresh.is_empty() {
            record.save();
        }
        record
    }

    /// Written right away: a phone may end the game at any moment.
    fn save(&self) {
        let Some(path) = path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Write beside the file and rename, so a crash mid-write can't lose the record.
        let tmp = path.with_extension("tmp");
        if let Err(e) = std::fs::write(&tmp, self.data.to_text()).and_then(|_| std::fs::rename(&tmp, &path)) {
            crate::assets::log_warn(&format!("could not save your record: {e}"));
        }
    }

    pub fn finish_hand(&mut self, hand: FinishedHand) {
        let new = self.data.finish_hand(hand);
        self.fresh.extend(new);
        self.save();
    }

    pub fn finish_tutorial(&mut self) {
        let new = self.data.finish_tutorial();
        self.fresh.extend(new);
        self.save();
    }

    /// Trophies the store says this player already has. They arrive quietly:
    /// they were celebrated wherever they were earned.
    pub fn merge_from_store(&mut self, list: Vec<Achievement>) {
        let mut changed = false;
        for a in list {
            changed |= self.data.merge_from_store(a);
        }
        if changed {
            self.save();
        }
    }

    /// What was earned since the last call.
    pub fn take_fresh(&mut self) -> Vec<Achievement> {
        std::mem::take(&mut self.fresh)
    }
}

/// Whether this is the player's real record, not one for a screenshot.
#[cfg_attr(not(any(feature = "steam", target_os = "ios", target_os = "android")), allow(dead_code))]
pub fn is_saved() -> bool {
    path().is_some()
}

fn is_demo() -> bool {
    crate::device::var("EUCHRE_RECORD").as_deref() == Some("demo")
}

/// Debug: `EUCHRE_RECORD=demo` starts from a season of play with a few trophies left
/// to earn, for screenshots of the shelf and the banner. Nothing is read or saved.
fn apply_demo(data: &mut Record) {
    if !is_demo() {
        return;
    }
    let s = &mut data.stats;
    (s.games, s.wins, s.streak, s.best_streak) = (41, 27, 2, 4);
    (s.online_games, s.online_wins) = (9, 5);
    (s.hands, s.hands_won, s.tricks) = (388, 221, 612);
    (s.points_for, s.points_against) = (392, 301);
    (s.calls, s.calls_made, s.suits_called) = (142, 118, 0b0111);
    (s.euchres, s.euchred, s.marches) = (19, 24, 31);
    (s.lones, s.lones_made, s.lone_marches) = (12, 9, 2);
    (s.both_bowers, s.tutorials) = (7, 1);
    s.clean_wins = 3;
    data.check();
    // One left to pop up on the title.
    data.unlocked.retain(|&a| a != Achievement::Wins10);
}
