//! What the store the game was launched from can do for online play. Today
//! that's Steam (with the `steam` cargo feature): the player's name, rich
//! presence so friends see "Join Game" in their Steam friends list, the
//! overlay's invite dialog, and join requests from either. Every function has
//! a fallback that does nothing, so mobile and plain desktop builds work the
//! same with codes only.
//!
//! A Steam join request carries the room code as its connect string: Steam
//! starts the game with `+connect KQJTA`, or tells a running game through a
//! callback. Both end up in [`take_join_request`].

#[cfg(feature = "steam")]
mod steam;

use std::sync::Mutex;

use crate::euchre::protocol::clean_code;

/// A room code to join, from the command line or the store's overlay.
static JOIN_REQUEST: Mutex<Option<String>> = Mutex::new(None);

/// Call once at startup: connects to Steam (if built with it) and reads a
/// `+connect CODE` from the command line.
pub fn init() {
    #[cfg(feature = "steam")]
    steam::init();
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "+connect")
        && let Some(code) = args.get(i + 1)
    {
        request_join(code);
    }
}

/// Lets the store library do its work (Steam callbacks). Call every frame.
pub fn update() {
    #[cfg(feature = "steam")]
    steam::update();
}

/// Records a request to join `code` (ignored unless it looks like a room code).
pub fn request_join(code: &str) {
    if let Some(code) = clean_code(code.trim_start_matches("+connect").trim())
        && let Ok(mut slot) = JOIN_REQUEST.lock()
    {
        *slot = Some(code);
    }
}

/// The room a friend invited us to, once.
pub fn take_join_request() -> Option<String> {
    JOIN_REQUEST.lock().ok()?.take()
}

/// The player's name on the store, if there is one.
pub fn player_name() -> Option<String> {
    #[cfg(feature = "steam")]
    if let Some(name) = steam::player_name() {
        return Some(name);
    }
    None
}

/// Whether there's an overlay to invite friends with.
pub fn can_invite() -> bool {
    #[cfg(feature = "steam")]
    if steam::running() {
        return true;
    }
    false
}

/// Opens the store's invite dialog for room `code`.
pub fn invite_friends(code: &str) {
    #[cfg(feature = "steam")]
    steam::invite(code);
    #[cfg(not(feature = "steam"))]
    let _ = code;
}

/// Tells friends what we're up to: seated at room `code` (they can join
/// from their friends list) with `seats` people out of four, or not online.
pub fn set_presence(room: Option<(&str, usize)>) {
    #[cfg(feature = "steam")]
    steam::set_presence(room);
    #[cfg(not(feature = "steam"))]
    let _ = room;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_requests_take_codes_only() {
        request_join("+connect kqjta");
        assert_eq!(take_join_request(), Some("KQJTA".into()));
        assert_eq!(take_join_request(), None, "taken once");
        request_join("not a code");
        assert_eq!(take_join_request(), None);
    }
}
