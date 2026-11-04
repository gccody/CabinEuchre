//! Steam, through the `steamworks` crate (only with `--features steam`).
//!
//! Steam carries nothing but room codes and handles; games always run on the room server,
//! so Steam players and phone players share tables.
//!
//! - Rich presence `connect` is `+knock HANDLE` while you're at a table, so
//!   friends get "Join Game" in their friends list. It only *asks* to join:
//!   the host lets them in, unless the table is public. Accepting starts the
//!   game with those arguments, or reaches a running game as
//!   `GameRichPresenceJoinRequested`. The code is never in rich presence.
//! - INVITE FRIENDS opens the overlay's invite dialog with `+connect CODE`:
//!   an invitation, so the friend sits straight down.
//! - `steam_display` shows "At a Euchre table (2/4)" in the friends list.
//!   It needs the tokens in `steam/rich_presence_english.vdf` uploaded on the
//!   app's Steamworks page (Community > Rich Presence).
//!
//! - Trophies are Steam achievements with the same ids (`docs/achievements.md`). Steam's
//!   overlay pops up for each new one as well as the game's own banner.
//!
//! Without Steam running (or without a `steam_appid.txt` next to a dev
//! build) everything here quietly does nothing.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use steamworks::{CallbackHandle, Client, GameRichPresenceJoinRequested};

use crate::euchre::record::Achievement;

/// How often to try again while Steam can't answer about trophies yet.
const TROPHY_RETRY: Duration = Duration::from_secs(5);

struct Steam {
    client: Client,
    _join: CallbackHandle,
    /// Trophies to send.
    waiting: Mutex<Vec<Achievement>>,
    /// Steam told us what the player has.
    read_store: AtomicBool,
    next_try: Mutex<Instant>,
}

static STEAM: OnceLock<Option<Steam>> = OnceLock::new();

fn steam() -> Option<&'static Steam> {
    STEAM.get()?.as_ref()
}

pub fn init() {
    STEAM.get_or_init(|| match Client::init() {
        Ok(client) => {
            let join = client.register_callback(|r: GameRichPresenceJoinRequested| super::request_join(&r.connect));
            // Stats are synced before a game starts, but this also covers a Steam client that was still catching up.
            let me = client.user().steam_id();
            client.user_stats().request_user_stats(me.raw());
            Some(Steam {
                client,
                _join: join,
                waiting: Mutex::new(Vec::new()),
                read_store: AtomicBool::new(false),
                next_try: Mutex::new(Instant::now()),
            })
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
        s.sync_trophies();
    }
}

/// Sends these trophies now, or as soon as Steam can take them.
pub fn report(list: &[Achievement]) {
    if let Some(s) = steam()
        && let Ok(mut waiting) = s.waiting.lock()
    {
        waiting.extend(list);
    }
}

impl Steam {
    /// Steam says no to everything about achievements until it has the player's stats,
    /// and to any achievement not yet set up on Steamworks, so this keeps trying.
    fn sync_trophies(&self) {
        let (Ok(mut waiting), Ok(mut next_try)) = (self.waiting.lock(), self.next_try.lock()) else { return };
        if (waiting.is_empty() && self.read_store.load(Ordering::Relaxed)) || Instant::now() < *next_try {
            return;
        }
        *next_try = Instant::now() + TROPHY_RETRY;
        let stats = self.client.user_stats();
        if !self.read_store.load(Ordering::Relaxed) {
            let mut answered = false;
            for a in Achievement::ALL {
                if let Ok(has) = stats.achievement(a.id()).get() {
                    answered = true;
                    if has {
                        super::store_has(a);
                    }
                }
            }
            self.read_store.store(answered, Ordering::Relaxed);
        }
        let mut changed = false;
        waiting.retain(|a| {
            let achievement = stats.achievement(a.id());
            match achievement.get() {
                Ok(true) => false,
                Ok(false) if achievement.set().is_ok() => {
                    changed = true;
                    false
                }
                _ => true,
            }
        });
        if changed && stats.store_stats().is_err() {
            crate::assets::log_warn("Steam didn't store the new trophies");
        }
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
        Some((handle, people)) => {
            let connect = format!("+knock {handle}");
            let people = people.to_string();
            friends.set_rich_presence("connect", Some(&connect));
            friends.set_rich_presence("seats", Some(&people));
            friends.set_rich_presence("steam_display", Some("#AtTable"));
            // Groups friends at the same table together in the friends list.
            friends.set_rich_presence("steam_player_group", Some(handle));
            friends.set_rich_presence("steam_player_group_size", Some(&people));
        }
        None => friends.clear_rich_presence(),
    }
}
