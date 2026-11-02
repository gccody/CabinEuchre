//! Steam, through the `steamworks` crate (only with `--features steam`).
//!
//! Steam carries nothing but room codes; games always run on the room server,
//! so Steam players and phone players share tables.
//!
//! - Rich presence `connect` is `+connect CODE` while you're at a table, so
//!   friends get "Join Game" in their friends list. Accepting starts the game
//!   with those arguments, or reaches a running game as
//!   `GameRichPresenceJoinRequested`.
//! - INVITE FRIENDS opens the overlay's invite dialog with the same string.
//! - `steam_display` shows "At a Euchre table (2/4)" in the friends list.
//!   It needs the tokens in `steam/rich_presence_english.vdf` uploaded on the
//!   app's Steamworks page (Community > Rich Presence).
//!
//! Without Steam running (or without a `steam_appid.txt` next to a dev
//! build) everything here quietly does nothing.

use std::sync::OnceLock;

use steamworks::{CallbackHandle, Client, GameRichPresenceJoinRequested};

struct Steam {
    client: Client,
    _join: CallbackHandle,
}

static STEAM: OnceLock<Option<Steam>> = OnceLock::new();

fn steam() -> Option<&'static Steam> {
    STEAM.get()?.as_ref()
}

pub fn init() {
    STEAM.get_or_init(|| match Client::init() {
        Ok(client) => {
            let join = client.register_callback(|r: GameRichPresenceJoinRequested| super::request_join(&r.connect));
            Some(Steam { client, _join: join })
        }
        Err(e) => {
            crate::assets::log_warn(&format!("Steam isn't available ({e}); invites are off"));
            None
        }
    });
}

pub fn update() {
    if let Some(s) = steam() {
        s.client.run_callbacks();
    }
}

pub fn running() -> bool {
    steam().is_some()
}

pub fn player_name() -> Option<String> {
    Some(steam()?.client.friends().name())
}

pub fn invite(code: &str) {
    if let Some(s) = steam() {
        s.client.friends().activate_invite_dialog_connect_string(&format!("+connect {code}"));
    }
}

pub fn set_presence(room: Option<(&str, usize)>) {
    let Some(s) = steam() else { return };
    let friends = s.client.friends();
    match room {
        Some((code, people)) => {
            let connect = format!("+connect {code}");
            let people = people.to_string();
            friends.set_rich_presence("connect", Some(&connect));
            friends.set_rich_presence("seats", Some(&people));
            friends.set_rich_presence("steam_display", Some("#AtTable"));
            // Groups friends at the same table together in the friends list.
            friends.set_rich_presence("steam_player_group", Some(code));
            friends.set_rich_presence("steam_player_group_size", Some(&people));
        }
        None => friends.clear_rich_presence(),
    }
}
