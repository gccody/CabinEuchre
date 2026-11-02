//! The online seat this device holds, as `online_seat.txt` next to the settings.
//!
//! A phone can kill the game at any time without warning, and only the seat's
//! token gets you back to it, so the token has to outlive the process. It lets
//! PLAY ONLINE offer REJOIN, and typing the same code again finds the same seat.

use std::path::PathBuf;

use crate::euchre::protocol::clean_code;
use crate::settings::{Settings, config_dir};

fn path() -> Option<PathBuf> {
    if Settings::use_defaults() {
        return None;
    }
    Some(config_dir()?.join("online_seat.txt"))
}

/// The remembered table's code and token.
fn load() -> Option<(String, String)> {
    let text = std::fs::read_to_string(path()?).ok()?;
    let (code, token) = text.trim().split_once(' ')?;
    Some((clean_code(code)?, token.to_string()))
}

/// The table to offer going back to.
pub fn code() -> Option<String> {
    load().map(|(code, _)| code)
}

/// The token for table `code`, if that's the table we hold a seat at.
pub fn token_for(code: &str) -> Option<String> {
    load().filter(|(c, _)| c == code).map(|(_, token)| token)
}

pub fn remember(code: &str, token: &str) {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("tmp");
    let result = std::fs::write(&tmp, format!("{code} {token}\n")).and_then(|_| std::fs::rename(&tmp, &path));
    if let Err(e) = result {
        crate::assets::log_warn(&format!("could not remember the online seat: {e}"));
    }
}

/// We left the table for good.
pub fn forget() {
    if let Some(path) = path() {
        let _ = std::fs::remove_file(path);
    }
}

/// Table `code` refused us: the seat there is gone (other tables' seats aren't).
pub fn forget_table(code: &str) {
    if self::code().as_deref() == Some(code) {
        forget();
    }
}
