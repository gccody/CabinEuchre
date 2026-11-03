//! Where the table's moves go and where the other seats' moves come from.
//!
//! Every game, offline or online, runs through a [`Room`](crate::euchre::room::Room)
//! speaking the protocol in `euchre::protocol`. The table only ever sees
//! [`ServerMsg`]s from its own seat's point of view and sends [`ClientMsg`]s:
//!
//! - [`Link::Local`]: the room runs right here, with three bots (offline play).
//! - [`Link::Online`]: the room is on the server, over a WebSocket (`online.rs`).
//! - [`Link::Tutorial`]: the tutorial's scripted room, right here too.

pub mod local;
pub mod online;
pub mod seat_memory;

use std::time::Duration;

use crate::euchre::Match;
use crate::euchre::protocol::{ClientMsg, ServerMsg};
use crate::euchre::tutorial::TutorialRoom;

pub use local::LocalTable;
pub use online::{Connection, Status};

pub enum Link {
    Local(Box<LocalTable>),
    Online(Connection),
    Tutorial(Box<TutorialRoom>),
}

impl Link {
    pub fn send(&mut self, msg: ClientMsg) {
        match self {
            Link::Local(l) => l.send(msg),
            Link::Online(c) => c.send(msg),
            Link::Tutorial(t) => t.handle(msg),
        }
    }

    /// The next message for this player, if one has arrived.
    pub fn poll(&mut self) -> Option<ServerMsg> {
        match self {
            Link::Local(l) => l.poll(),
            Link::Online(c) => c.poll(),
            Link::Tutorial(t) => t.poll(),
        }
    }

    pub fn is_online(&self) -> bool {
        matches!(self, Link::Online(_))
    }

    /// The tutorial's match: a coach at the table, and nothing to save.
    pub fn is_tutorial(&self) -> bool {
        matches!(self, Link::Tutorial(_))
    }

    /// Going back to a seat this player already held at this table.
    pub fn rejoining(&self) -> bool {
        matches!(self, Link::Online(c) if c.rejoining())
    }

    /// The real match, for an offline game (what gets saved).
    pub fn local_match(&self) -> Option<&Match> {
        match self {
            Link::Local(l) => l.game(),
            Link::Online(_) | Link::Tutorial(_) => None,
        }
    }

    pub fn status(&self) -> Status {
        match self {
            Link::Local(_) | Link::Tutorial(_) => Status::Connected,
            Link::Online(c) => c.status(),
        }
    }

    /// Gives the connection a moment to send what's queued, for when the game is
    /// about to exit and a thread still working would be cut off.
    pub fn finish(&mut self, within: Duration) {
        if let Link::Online(c) = self {
            c.finish(within);
        }
    }

    /// The room code, for an online game.
    pub fn room_code(&self) -> Option<String> {
        match self {
            Link::Local(_) | Link::Tutorial(_) => None,
            Link::Online(c) => c.room_code(),
        }
    }

    /// The room handle (what friends see instead of the code), for an online game.
    pub fn room_handle(&self) -> Option<String> {
        match self {
            Link::Local(_) | Link::Tutorial(_) => None,
            Link::Online(c) => c.room_handle(),
        }
    }
}
