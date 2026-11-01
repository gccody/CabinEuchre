//! The saved game: one slot holding the match the player last left, as
//! `savegame.txt` next to the settings. The format lives in `euchre::save`;
//! this module only reads, writes and deletes the file.

use std::path::PathBuf;

use crate::euchre::Match;
use crate::settings::{Settings, config_dir};

fn path() -> Option<PathBuf> {
    if Settings::use_defaults() {
        return None;
    }
    Some(config_dir()?.join("savegame.txt"))
}

/// The saved match, if there is one and it still loads.
pub fn load() -> Option<Match> {
    let path = path()?;
    let text = std::fs::read_to_string(&path).ok()?;
    match Match::from_save(&text) {
        Ok(m) => Some(m),
        Err(e) => {
            crate::assets::log_warn(&format!("ignoring saved game {}: {}", path.display(), e.0));
            None
        }
    }
}

/// Saves `game` over whatever was saved before.
pub fn write(game: &Match) {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Write beside the file and rename, so a crash mid-write can't leave half a save.
    let tmp = path.with_extension("tmp");
    let result = std::fs::write(&tmp, game.to_save()).and_then(|_| std::fs::rename(&tmp, &path));
    if let Err(e) = result {
        crate::assets::log_warn(&format!("could not save the game: {e}"));
    }
}

/// Deletes the save if it holds game `id` (it just finished), leaving any
/// other game the player left alone.
pub fn forget(id: u64) {
    let Some(path) = path() else { return };
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let saved_id = text
        .lines()
        .find_map(|l| l.strip_prefix("id="))
        .and_then(|v| v.trim().parse::<u64>().ok());
    if saved_id == Some(id) {
        let _ = std::fs::remove_file(&path);
    }
}
