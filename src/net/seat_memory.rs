//! The online seat this device holds, as `online_seat.txt` next to the settings.
//!
//! A phone can kill the game at any time without warning, and only the seat's
//! token gets you back to it, so the token has to outlive the process. It lets
//! PLAY ONLINE offer REJOIN, and typing the same code again finds the same seat.
//!
//! The memory belongs to one running copy of the game. Two copies on one computer
//! share a save folder, and if the second used the first's token it would take the
//! first's seat instead of getting its own. So the first copy to look holds a lock
//! on `online_seat.lock` until it exits (or dies: the OS lets go then), and any other
//! copy plays without a memory.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::euchre::protocol::clean_code;
use crate::settings::{Settings, config_dir};

fn path() -> Option<PathBuf> {
    if Settings::use_defaults() {
        return None;
    }
    Some(config_dir()?.join("online_seat.txt"))
}

/// The lock on the memory, once this copy of the game has it.
static CLAIM: Mutex<Option<File>> = Mutex::new(None);

/// Whether this copy of the game looks after the memory.
fn ours() -> bool {
    let Some(path) = path().map(|p| p.with_extension("lock")) else { return false };
    let Ok(mut claim) = CLAIM.lock() else { return false };
    if claim.is_some() {
        return true;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = OpenOptions::new().create(true).truncate(false).write(true).open(path);
    match file.map(|f| (f.try_lock(), f)) {
        Ok((Ok(()), file)) => {
            *claim = Some(file);
            true
        }
        Ok((Err(TryLockError::WouldBlock), _)) => false,
        // No locks on this filesystem: carry on as if this were the only copy.
        _ => true,
    }
}

/// The remembered table's code and token.
fn load() -> Option<(String, String)> {
    if !ours() {
        return None;
    }
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
    if !ours() {
        return;
    }
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
    if !ours() {
        return;
    }
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
