//! What kind of device the game is on. Phones and tablets get the same game
//! with their own layouts: the tabletop fills the whole screen, everything
//! stays inside the OS's safe area, and controls are big enough for fingers.
//!
//! Every mobile branch in the game goes through [`mobile`], which is always
//! false on a desktop build (unless `EUCHRE_MOBILE=1` asks to preview it), so
//! the desktop game is untouched by them.

use std::sync::OnceLock;

/// Built for Android or iOS.
pub const NATIVE_MOBILE: bool = cfg!(any(target_os = "android", target_os = "ios"));

/// Whether to use the phone and tablet layouts.
pub fn mobile() -> bool {
    static MOBILE: OnceLock<bool> = OnceLock::new();
    *MOBILE.get_or_init(|| NATIVE_MOBILE || std::env::var("EUCHRE_MOBILE").is_ok_and(|v| v == "1"))
}

/// Whether the back gesture (`Key::Escape` on the title screen) should quit.
/// Android's does. iOS has none, and Apple doesn't want apps quitting themselves.
pub fn back_quits() -> bool {
    !cfg!(target_os = "ios")
}

/// A debug switch (see the table in `CLAUDE.md`): the environment, or on a
/// debug build for a phone, where nothing can set one, a `KEY=value` line in
/// `euchre-debug.txt` in the save directory.
pub fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().or_else(|| debug_file(name))
}

#[cfg(all(debug_assertions, any(target_os = "android", target_os = "ios")))]
fn debug_file(name: &str) -> Option<String> {
    static FILE: OnceLock<String> = OnceLock::new();
    let text = FILE.get_or_init(|| std::fs::read_to_string(love2drs::filesystem::save_dir().join("euchre-debug.txt")).unwrap_or_default());
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim() == name).then(|| v.trim().to_string())
    })
}

#[cfg(not(all(debug_assertions, any(target_os = "android", target_os = "ios"))))]
fn debug_file(_name: &str) -> Option<String> {
    None
}
