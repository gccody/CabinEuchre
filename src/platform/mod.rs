//! What the store the game was launched from can do for online play. Today
//! that's Steam (with the `steam` cargo feature): the player's name, rich
//! presence so friends see "Join Game" in their Steam friends list, the
//! overlay's invite dialog, and join requests from either. Every function has
//! a fallback that does nothing, so mobile and plain desktop builds work the
//! same with codes only.
//!
//! Two kinds of connect strings travel through Steam, and both end up in
//! [`take_join_request`] (Steam starts the game with them as arguments, or
//! tells a running game through a callback):
//!
//! - `+connect CODE` from an invite: the code is the invitation, so the
//!   player sits straight down.
//! - `+knock HANDLE` from rich presence ("Join Game" in the friends list):
//!   only asks to join. The host lets them in, unless the table is public.
//!   Rich presence never carries the code.

#[cfg(feature = "steam")]
mod steam;

use std::sync::Mutex;

use crate::euchre::protocol::{clean_code, clean_handle};

/// Where a friend's invite or "Join Game" leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinRequest {
    /// Invited: join with the code.
    Code(String),
    /// Ask to join by handle.
    Knock(String),
}

/// A pending join, from the command line or the store's overlay.
static JOIN_REQUEST: Mutex<Option<JoinRequest>> = Mutex::new(None);

/// Call once at startup: connects to Steam (if built with it) and reads a
/// `+connect CODE` from the command line.
pub fn init() {
    #[cfg(feature = "steam")]
    steam::init();
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "+connect" || a == "+knock")
        && let Some(arg) = args.get(i + 1)
    {
        request_join(&format!("{} {arg}", args[i]));
    }
}

/// Lets the store library do its work (Steam callbacks). Call every frame.
pub fn update() {
    #[cfg(feature = "steam")]
    steam::update();
}

/// Records a connect string from Steam: `+connect CODE` or `+knock HANDLE`
/// (anything else is ignored).
pub fn request_join(connect: &str) {
    let connect = connect.trim();
    let request = if let Some(code) = connect.strip_prefix("+connect") {
        clean_code(code).map(JoinRequest::Code)
    } else if let Some(handle) = connect.strip_prefix("+knock") {
        clean_handle(handle).map(JoinRequest::Knock)
    } else {
        None
    };
    if let Some(request) = request
        && let Ok(mut slot) = JOIN_REQUEST.lock()
    {
        *slot = Some(request);
    }
}

/// The table a friend invited us to (or that we asked to join), once.
pub fn take_join_request() -> Option<JoinRequest> {
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

/// Opens the store's invite dialog for room `code`: an invite carries the
/// code, so whoever accepts it sits straight down.
pub fn invite_friends(code: &str) {
    #[cfg(feature = "steam")]
    steam::invite(code);
    #[cfg(not(feature = "steam"))]
    let _ = code;
}

/// Tells friends what we're up to: seated at the room with `handle` (they
/// can ask to join from their friends list) with `seats` people out of four,
/// or not online. Never pass the code here: friends who see it could sit down
/// without asking.
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
    fn join_requests() {
        request_join("+connect kqjtab");
        assert_eq!(take_join_request(), Some(JoinRequest::Code("KQJTAB".into())));
        assert_eq!(take_join_request(), None, "taken once");
        let handle = "ab".repeat(32);
        request_join(&format!("+knock {}", handle.to_uppercase()));
        assert_eq!(take_join_request(), Some(JoinRequest::Knock(handle)));
        for junk in ["not a code", "+connect KQJ", "+connect KQJTA", "+knock 1234", "KQJTAB"] {
            request_join(junk);
            assert_eq!(take_join_request(), None, "{junk}");
        }
    }
}
