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
//!
//! Trophies go to the store's own achievements as well as being kept in the
//! record: Steam, Game Center on iOS, Play Games on Android. Everywhere else
//! they're only local. The stores ignore what they already have, so
//! [`report_achievements`] can always be called with anything, and each one holds
//! on to reports until the player is signed in. What a store already has comes
//! back through [`take_store_trophies`], for a new device or a reinstall.

#[cfg(target_os = "ios")]
mod game_center;
#[cfg(target_os = "android")]
mod play_games;
#[cfg(feature = "steam")]
mod steam;

use std::sync::Mutex;

use crate::euchre::protocol::{clean_code, clean_handle};
use crate::euchre::record::Achievement;

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
    #[cfg(target_os = "ios")]
    game_center::init();
    #[cfg(target_os = "android")]
    play_games::init();
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
    #[cfg(target_os = "ios")]
    game_center::update();
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

/// Tells the store about trophies. Screenshots and checks that ignore the player's
/// files leave their account alone too (`EUCHRE_DEFAULT_SETTINGS`, `EUCHRE_RECORD=demo`).
pub fn report_achievements(list: &[Achievement]) {
    #[cfg(any(feature = "steam", target_os = "ios", target_os = "android"))]
    if !list.is_empty() && crate::record::is_saved() {
        #[cfg(feature = "steam")]
        steam::report(list);
        #[cfg(target_os = "ios")]
        game_center::report(list);
        #[cfg(target_os = "android")]
        play_games::report(list);
    }
    #[cfg(not(any(feature = "steam", target_os = "ios", target_os = "android")))]
    let _ = list;
}

/// Trophies the store told us the player has, waiting to be taken.
#[cfg(any(feature = "steam", target_os = "ios", target_os = "android"))]
static STORE_TROPHIES: Mutex<Vec<Achievement>> = Mutex::new(Vec::new());

/// A store backend found a trophy the player has.
#[cfg(any(feature = "steam", target_os = "ios", target_os = "android"))]
fn store_has(a: Achievement) {
    if let Ok(mut list) = STORE_TROPHIES.lock() {
        list.push(a);
    }
}

/// What the store says the player has already earned, since the last call.
pub fn take_store_trophies() -> Vec<Achievement> {
    #[cfg(any(feature = "steam", target_os = "ios", target_os = "android"))]
    if let Ok(mut list) = STORE_TROPHIES.lock() {
        return std::mem::take(&mut *list);
    }
    Vec::new()
}

/// The store trophies sync with right now, for the trophy shelf.
pub fn store_name() -> Option<&'static str> {
    #[cfg(feature = "steam")]
    if steam::running() {
        return Some("STEAM");
    }
    #[cfg(target_os = "ios")]
    if game_center::signed_in() {
        return Some("GAME CENTER");
    }
    #[cfg(target_os = "android")]
    if play_games::signed_in() {
        return Some("PLAY GAMES");
    }
    None
}

/// Whether the trophy shelf should offer a button to sign in to the store.
pub fn can_sign_in() -> bool {
    #[cfg(target_os = "android")]
    {
        play_games::can_sign_in()
    }
    #[cfg(not(target_os = "android"))]
    {
        false
    }
}

pub fn sign_in() {
    #[cfg(target_os = "android")]
    play_games::sign_in();
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
