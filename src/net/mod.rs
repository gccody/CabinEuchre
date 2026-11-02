//! Where the table's moves go and where the other seats' moves come from.
//!
//! Every game, offline or online, runs through a [`Room`](crate::euchre::room::Room)
//! speaking the protocol in `euchre::protocol`. The table only ever sees
//! [`ServerMsg`]s from its own seat's point of view and sends [`ClientMsg`]s:
//!
//! - [`Link::Local`]: the room runs right here, with three bots (offline play).
//! - [`Link::Online`]: the room is on the server, over a WebSocket (`online.rs`).

pub mod local;
pub mod online;

use crate::euchre::Match;
use crate::euchre::protocol::{ClientMsg, ServerMsg};

pub use local::LocalTable;
pub use online::{Connection, Status};

pub enum Link {
    Local(LocalTable),
    Online(Connection),
}

impl Link {
    pub fn send(&mut self, msg: ClientMsg) {
        match self {
            Link::Local(l) => l.send(msg),
            Link::Online(c) => c.send(msg),
        }
    }

    /// The next message for this player, if one has arrived.
    pub fn poll(&mut self) -> Option<ServerMsg> {
        match self {
            Link::Local(l) => l.poll(),
            Link::Online(c) => c.poll(),
        }
    }

    pub fn is_online(&self) -> bool {
        matches!(self, Link::Online(_))
    }

    /// The real match, for an offline game (what gets saved).
    pub fn local_match(&self) -> Option<&Match> {
        match self {
            Link::Local(l) => l.game(),
            Link::Online(_) => None,
        }
    }

    pub fn status(&self) -> Status {
        match self {
            Link::Local(_) => Status::Connected,
            Link::Online(c) => c.status(),
        }
    }

    /// The room code, for an online game.
    pub fn room_code(&self) -> Option<String> {
        match self {
            Link::Local(_) => None,
            Link::Online(c) => c.room_code(),
        }
    }
}
