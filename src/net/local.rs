//! An offline game: a [`Room`] running in-process with you in seat 0 and
//! three bots, so playing against the computer goes through exactly the same
//! rules, messages and table code as playing online.

use std::collections::VecDeque;

use crate::euchre::Match;
use crate::euchre::ai::Difficulty;
use crate::euchre::protocol::{ClientMsg, PROTOCOL_VERSION, ServerMsg};
use crate::euchre::room::Room;

pub struct LocalTable {
    room: Room,
    inbox: VecDeque<ServerMsg>,
    seed: u64,
}

impl LocalTable {
    /// A new game, shuffled from `seed`, against bots of this `difficulty`.
    pub fn new(target: u32, stick_the_dealer: bool, difficulty: Difficulty, seed: u64) -> LocalTable {
        let mut t = LocalTable::seated(target, stick_the_dealer, difficulty, seed);
        t.send(ClientMsg::Start);
        t
    }

    /// Picks a saved game back up. The bots play at today's `difficulty`: the save doesn't keep one.
    pub fn resume(game: Match, difficulty: Difficulty) -> LocalTable {
        let mut t = LocalTable::seated(game.target, game.stick_the_dealer, difficulty, game.id);
        t.room.set_game(game);
        t.pump();
        t
    }

    /// The Difficulty option changed: the bots play that way from their next move.
    pub fn set_difficulty(&mut self, difficulty: Difficulty) {
        self.room.set_difficulty(difficulty);
    }

    fn seated(target: u32, stick_the_dealer: bool, difficulty: Difficulty, seed: u64) -> LocalTable {
        let mut room = Room::new("LOCAL", "", target, stick_the_dealer);
        room.set_difficulty(difficulty);
        room.join(PROTOCOL_VERSION, "YOU", None, "local").expect("an empty room has seats");
        let mut t = LocalTable { room, inbox: VecDeque::new(), seed };
        t.pump();
        t
    }

    fn pump(&mut self) {
        for out in self.room.take_outbox() {
            if out.to == 0 {
                self.inbox.push_back(out.msg);
            }
        }
    }

    pub fn send(&mut self, msg: ClientMsg) {
        self.room.handle(0, msg, self.seed);
        // A rematch from the same table shuffles differently.
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.pump();
    }

    pub fn poll(&mut self) -> Option<ServerMsg> {
        self.inbox.pop_front()
    }

    pub fn game(&self) -> Option<&Match> {
        self.room.game()
    }
}
