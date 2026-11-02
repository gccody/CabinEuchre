//! Plays whole online games between a [`Room`] and up to four simulated
//! players, each keeping only a [`Mirror`] built from the messages they're
//! sent, and checks after every step that each mirror agrees with the room
//! on everything that player is allowed to know.
//!
//! The simulated players drop and reconnect, ask to resync, let their turns
//! time out and leave mid-game, so all of the recovery paths are exercised.

use std::collections::VecDeque;

use crate::ai;
use crate::mirror::{ME, Mirror};
use crate::protocol::{ClientMsg, ErrorCode, PROTOCOL_VERSION, SeatKind, SeatsInfo, ServerMsg, Shown};
use crate::rng::Rng;
use crate::room::{Occupant, Room};
use crate::round::{Phase, Seat, team_of};

struct Player {
    seat: Seat,
    token: String,
    online: bool,
    inbox: VecDeque<ServerMsg>,
    mirror: Option<Mirror>,
    seats: Option<SeatsInfo>,
    /// Asked for a resync and is ignoring moves until it arrives.
    resyncing: bool,
}

fn rel(me: Seat, seat: Seat) -> Seat {
    (seat + 4 - me) % 4
}

fn deliver(room: &mut Room, players: &mut [Player]) {
    for out in room.take_outbox() {
        if let Some(p) = players.iter_mut().find(|p| p.seat == out.to && p.online) {
            p.inbox.push_back(out.msg);
        }
    }
}

/// Processes one message. Returns a message to send back, if any.
fn receive(p: &mut Player, msg: ServerMsg) -> Option<ClientMsg> {
    match msg {
        ServerMsg::Welcome { token, .. } => p.token = token,
        ServerMsg::Waiting | ServerMsg::Knocks { .. } => {}
        ServerMsg::Seats(s) => p.seats = Some(s),
        ServerMsg::Deal(hand) => {
            p.mirror = Some(Mirror::new(&hand).expect("deal"));
            p.resyncing = false;
        }
        ServerMsg::Sync { hand, moves } => {
            p.mirror = Some(Mirror::replay(&hand, &moves).expect("sync replays"));
            p.resyncing = false;
        }
        ServerMsg::Acted(mv) => {
            if p.resyncing {
                return None;
            }
            let m = p.mirror.as_mut().expect("a move before any deal");
            if m.round.phase == Phase::TrickDone {
                m.advance();
            }
            let (action, _swaps) = m.reveal(mv).unwrap_or_else(|e| panic!("seat {} reveal {mv:?}: {e:?}", p.seat));
            if let Shown::Discard { card: Some(c) } = mv.action {
                assert_eq!(action, crate::Action::Discard { card: c });
            }
        }
        ServerMsg::Error { code, .. } => panic!("seat {} refused: {code:?}", p.seat),
    }
    None
}

/// The checks: what the player knows matches the room's real hand.
fn check(room: &Room, p: &Player) {
    let (Some(m), Some(truth)) = (&p.mirror, room.game()) else { return };
    if m.game != room.game_no() || truth.hand_number != m.hand_number {
        return;
    }
    let me = p.seat;
    let mut m = m.clone();
    if m.round.phase == Phase::TrickDone && truth.round.phase != Phase::TrickDone {
        // The room clears a finished trick at once; the player's table does it after a pause.
        m.advance();
    }
    let (r, t) = (&m.round, &truth.round);
    let mut mine = r.hands[ME].clone();
    let mut real = t.hands[me].clone();
    mine.sort();
    real.sort();
    // The room may be ahead (bots moved since); compare once this player has caught up.
    if m.moves() == truth.history().len() {
        assert_eq!(mine, real, "seat {me}: own hand");
        assert_eq!(r.played, t.played);
        assert_eq!(r.trump, t.trump);
        assert_eq!(r.maker, t.maker.map(|s| rel(me, s)), "seat {me}: maker");
        assert_eq!(r.turn, rel(me, t.turn), "seat {me}: whose turn");
        for s in 0..4 {
            assert_eq!(r.hands[rel(me, s)].len(), t.hands[s].len(), "seat {me}: hand sizes");
            assert_eq!(r.tricks_won[rel(me, s)], t.tricks_won[s]);
        }
        let trick: Vec<_> = t.trick.iter().map(|&(s, c)| (rel(me, s), c)).collect();
        assert_eq!(r.trick, trick);
        let mut scores = truth.scores_before_hand();
        if team_of(me) == 1 {
            scores.swap(0, 1);
        }
        if !matches!(r.phase, Phase::HandOver) {
            assert_eq!(m.scores, scores, "seat {me}: scores");
        }
    }
}

fn simulate(seed: u64, humans: usize, chaos: bool) -> u32 {
    let mut rng = Rng::new(seed);
    let mut room = Room::new("TESTS", "tests", 5, seed.is_multiple_of(2));
    let mut players: Vec<Player> = Vec::new();
    for i in 0..humans {
        let token = format!("token-{i}");
        let seat = room.join(PROTOCOL_VERSION, &format!("p{i}"), None, &token).unwrap();
        players.push(Player {
            seat,
            token,
            online: true,
            inbox: VecDeque::new(),
            mirror: None,
            seats: None,
            resyncing: false,
        });
    }
    room.handle(players[0].seat, ClientMsg::Start, seed);
    let mut games = 0;
    for step in 0..200_000 {
        deliver(&mut room, &mut players);
        if room.game().is_some_and(|m| m.winner().is_some()) {
            // Everyone catches up, then a rematch.
            let idle = players.iter().all(|p| p.inbox.is_empty());
            if idle {
                for p in players.iter().filter(|p| p.online) {
                    let Some(m) = p.mirror.as_ref() else { continue };
                    let mut m = m.clone();
                    if m.round.phase == Phase::TrickDone {
                        m.advance();
                    }
                    m.score_hand();
                    let truth = room.game().unwrap();
                    let mut scores = truth.scores;
                    if team_of(p.seat) == 1 {
                        scores.swap(0, 1);
                    }
                    assert_eq!(m.scores, scores, "final score for seat {}", p.seat);
                }
                games += 1;
                if games == 2 {
                    return games;
                }
                let starter = players.iter().find(|p| p.online).map(|p| p.seat).unwrap_or(players[0].seat);
                room.handle(starter, ClientMsg::Start, seed + 99);
                continue;
            }
        }

        // Chaos: drop a connection, reconnect, resync, time out.
        if chaos && rng.below(40) == 0 {
            let i = rng.below(players.len());
            let p = &mut players[i];
            if p.online {
                p.online = false;
                p.inbox.clear();
                room.disconnected(p.seat);
            } else {
                p.online = true;
                let seat = room.join(PROTOCOL_VERSION, "back", Some(&p.token.clone()), "unused").unwrap();
                assert_eq!(seat, p.seat, "token gets the seat back");
            }
            continue;
        }
        if chaos && rng.below(60) == 0
            && let Some((_, _, key)) = room.waiting_on()
        {
            room.force_move(key);
            continue;
        }

        // Each online player handles one message, or moves if it's their turn.
        let mut progressed = false;
        #[allow(clippy::needless_range_loop)] // `check` needs the room and the player at once
        for i in 0..players.len() {
            let p = &mut players[i];
            if !p.online {
                continue;
            }
            if let Some(msg) = p.inbox.pop_front() {
                if let Some(reply) = receive(p, msg) {
                    room.handle(p.seat, reply, 0);
                }
                check(&room, &players[i]);
                progressed = true;
                continue;
            }
            let Some(m) = p.mirror.as_mut() else { continue };
            if m.round.phase == Phase::TrickDone {
                m.advance();
            }
            if m.round.phase == Phase::HandOver {
                m.score_hand();
            }
            if chaos && rng.below(200) == 0 {
                p.resyncing = true;
                room.handle(p.seat, ClientMsg::Resync, 0);
                progressed = true;
                continue;
            }
            if m.round.to_act() == Some(ME) && m.winner().is_none() {
                assert_eq!(room.waiting_on().map(|w| w.0), Some(p.seat), "step {step}: room waits on seat {}", p.seat);
                check(&room, &players[i]);
                let p = &mut players[i];
                let m = p.mirror.as_mut().unwrap();
                // The AI only uses what its seat could know, so it plays from a mirror too.
                let action = ai::choose(&m.round, ME);
                m.act(action).unwrap_or_else(|e| panic!("own move refused locally: {e:?}"));
                room.handle(p.seat, ClientMsg::Act { action }, 0);
                progressed = true;
            }
        }
        if !progressed && players.iter().all(|p| !p.online || p.inbox.is_empty()) {
            // Nobody online can move: the room must be waiting on someone offline.
            match room.waiting_on() {
                Some((seat, connected, key)) => {
                    assert!(!connected || players.iter().any(|p| p.seat == seat && !p.online) || chaos);
                    room.force_move(key);
                }
                None => {
                    if !room.game().is_some_and(|m| m.winner().is_some()) {
                        panic!("seed {seed} step {step}: stuck");
                    }
                }
            }
        }
    }
    panic!("seed {seed}: games did not finish");
}

#[test]
fn one_player_and_three_bots() {
    for seed in 0..20 {
        assert_eq!(simulate(seed, 1, false), 2);
    }
}

#[test]
fn full_tables_stay_in_step() {
    for humans in 2..=4 {
        for seed in 0..15 {
            assert_eq!(simulate(seed * 7 + humans as u64, humans, false), 2);
        }
    }
}

#[test]
fn recovers_from_drops_resyncs_and_timeouts() {
    for humans in 1..=4 {
        for seed in 0..25 {
            assert_eq!(simulate(seed * 13 + humans as u64, humans, true), 2);
        }
    }
}

#[test]
fn hidden_cards_never_reach_other_players() {
    let mut room = Room::new("SPIES", "spies", 10, true);
    for i in 0..4 {
        room.join(PROTOCOL_VERSION, "p", None, &format!("t{i}")).unwrap();
    }
    room.handle(0, ClientMsg::Start, 5);
    let truth = room.game().unwrap().round.clone();
    for out in room.take_outbox() {
        if let ServerMsg::Deal(hand) = out.msg {
            let mut cards = hand.cards.clone();
            cards.sort();
            let mut real = truth.hands[out.to].clone();
            real.sort();
            assert_eq!(cards, real, "each player gets exactly their own hand");
        }
    }
    // Play until a discard, checking who sees it.
    for _ in 0..40 {
        let Some((seat, _, _)) = room.waiting_on() else { break };
        let m = room.game().unwrap();
        let action = ai::choose(&m.round, seat);
        let discard = matches!(action, crate::Action::Discard { .. });
        room.handle(seat, ClientMsg::Act { action }, 0);
        for out in room.take_outbox() {
            if let ServerMsg::Acted(mv) = out.msg
                && let Shown::Discard { card } = mv.action
            {
                assert!(discard);
                assert!(card.is_none(), "seat {} saw the dealer's discard", out.to);
            }
        }
    }
}

#[test]
fn seats_and_snapshots() {
    let mut room = Room::new("SEATS", "seats", 7, false);
    assert_eq!(room.join(PROTOCOL_VERSION, "ann", None, "a").unwrap(), 0);
    assert_eq!(room.join(PROTOCOL_VERSION, "bo", None, "b").unwrap(), 2, "second player is the partner");
    assert!(room.join(PROTOCOL_VERSION + 1, "old", None, "c").is_err(), "other protocol versions are refused");
    room.take_outbox();
    // Bo moves to Ann's left.
    room.handle(2, ClientMsg::Sit { seat: 3 }, 0);
    assert!(matches!(room.seats()[1], Occupant::Human { .. }));
    let seats = room
        .take_outbox()
        .into_iter()
        .find_map(|o| match o.msg {
            ServerMsg::Seats(s) if o.to == 0 => Some(s),
            _ => None,
        })
        .unwrap();
    assert_eq!(seats.seats[0].name, "ANN");
    assert_eq!(seats.seats[1].name, "BO");
    assert_eq!(seats.seats[2].kind, SeatKind::Bot);
    room.handle(0, ClientMsg::Start, 11);
    room.handle(1, ClientMsg::Sit { seat: 1 }, 0);
    assert!(
        room.take_outbox().iter().any(|o| matches!(o.msg, ServerMsg::Error { .. })),
        "no seat changes mid-game"
    );
    // Snapshot mid-hand and restore: same state, same future.
    let snap = room.snapshot();
    let json = serde_json::to_string(&snap).unwrap();
    let back = Room::restore(serde_json::from_str(&json).unwrap()).unwrap();
    assert_eq!(back.snapshot(), snap);
    assert_eq!(back.waiting_on(), room.waiting_on());
}

fn knocks_for(room: &mut Room, host: Seat) -> Option<Vec<String>> {
    room.take_outbox().into_iter().rev().find_map(|o| match o.msg {
        ServerMsg::Knocks { requests } if o.to == host => Some(requests.into_iter().map(|k| k.name).collect()),
        _ => None,
    })
}

#[test]
fn private_tables_ask_the_host() {
    let mut room = Room::new("KNOCK", "handle", 10, true);
    let host = room.join(PROTOCOL_VERSION, "ann", None, "a").unwrap();
    assert_eq!(room.host(), Some(host));
    assert!(!room.is_public(), "tables start private");
    // A friend with the code sits straight down (the code is the invitation).
    let bo = room.join(PROTOCOL_VERSION, "bo", None, "b").unwrap();
    room.take_outbox();

    // Someone coming by the handle has to ask.
    assert_eq!(room.knock(PROTOCOL_VERSION, "cy", None, 7, "c").unwrap(), None);
    assert_eq!(knocks_for(&mut room, host), Some(vec!["CY".to_string()]));
    assert_eq!(room.knock(PROTOCOL_VERSION, "dee", None, 8, "d").unwrap(), None);
    room.take_outbox();

    // Only the host answers.
    room.handle(bo, ClientMsg::Answer { id: 7, accept: true }, 0);
    assert!(room.take_admitted().is_empty());
    assert!(room.take_outbox().iter().any(|o| o.to == bo && matches!(o.msg, ServerMsg::Error { .. })));

    room.handle(host, ClientMsg::Answer { id: 7, accept: true }, 0);
    let admitted = room.take_admitted();
    assert_eq!(admitted.len(), 1);
    let (id, cy) = admitted[0];
    assert_eq!(id, 7);
    let out = room.take_outbox();
    assert!(
        out.iter().any(|o| o.to == cy && matches!(o.msg, ServerMsg::Welcome { .. })),
        "the admitted player is welcomed in their new seat"
    );
    room.handle(host, ClientMsg::Answer { id: 8, accept: false }, 0);
    assert_eq!(room.take_declined(), vec![8]);
    assert_eq!(knocks_for(&mut room, host), Some(vec![]));

    // The table is full now (three people and one bot seat left): one more asks, the room fills.
    assert_eq!(room.knock(PROTOCOL_VERSION, "eve", None, 9, "e").unwrap(), None);
    room.join(PROTOCOL_VERSION, "fay", None, "f").unwrap();
    room.handle(host, ClientMsg::Answer { id: 9, accept: true }, 0);
    assert_eq!(room.take_declined(), vec![9], "no seat left for them");
    assert_eq!(room.knock(PROTOCOL_VERSION, "gus", None, 10, "g"), Err(ErrorCode::RoomFull));
}

#[test]
fn public_tables_let_friends_in() {
    let mut room = Room::new("OPEN", "handle", 10, true);
    let host = room.join(PROTOCOL_VERSION, "ann", None, "a").unwrap();
    assert_eq!(room.knock(PROTOCOL_VERSION, "cy", None, 1, "c").unwrap(), None);
    let other = room.join(PROTOCOL_VERSION, "bo", None, "b").unwrap();
    room.handle(other, ClientMsg::SetPublic { public: true }, 0);
    assert!(!room.is_public(), "only the host makes the table public");
    room.take_outbox();

    // Going public lets everyone who was asking in.
    room.handle(host, ClientMsg::SetPublic { public: true }, 0);
    assert!(room.is_public());
    assert_eq!(room.take_admitted().len(), 1);
    let seats = room
        .take_outbox()
        .into_iter()
        .rev()
        .find_map(|o| match o.msg {
            ServerMsg::Seats(s) if o.to == other => Some(s),
            _ => None,
        })
        .unwrap();
    assert!(seats.public);
    assert_eq!(seats.host, Some((host + 4 - other) % 4), "the host, from Bo's point of view");
    // ...and later arrivals sit straight down.
    assert!(room.knock(PROTOCOL_VERSION, "dee", None, 2, "d").unwrap().is_some());

    // Back to private: knocks wait again. A returning player's token always works.
    room.handle(host, ClientMsg::SetPublic { public: false }, 0);
    room.handle(other, ClientMsg::Leave, 0);
    assert_eq!(room.knock(PROTOCOL_VERSION, "eve", None, 3, "e").unwrap(), None);
    assert!(room.knock(PROTOCOL_VERSION, "dee", Some("d"), 4, "x").unwrap().is_some());
}

#[test]
fn the_host_job_moves_on() {
    let mut room = Room::new("HOSTS", "handle", 10, true);
    let ann = room.join(PROTOCOL_VERSION, "ann", None, "a").unwrap();
    let bo = room.join(PROTOCOL_VERSION, "bo", None, "b").unwrap();
    room.knock(PROTOCOL_VERSION, "cy", None, 1, "c").unwrap();
    room.take_outbox();
    room.handle(ann, ClientMsg::Leave, 0);
    assert_eq!(room.host(), Some(bo));
    assert_eq!(knocks_for(&mut room, bo), Some(vec!["CY".to_string()]), "the new host sees who's asking");
    // Moving seats takes the job along.
    room.handle(bo, ClientMsg::Sit { seat: 1 }, 0);
    let moved = room.host().unwrap();
    assert_ne!(moved, bo);
    // Snapshots keep all of it.
    let back = Room::restore(room.snapshot()).unwrap();
    assert_eq!((back.host(), back.is_public(), back.knocks().len()), (Some(moved), false, 1));
    // With nobody left to answer, whoever was asking is turned away.
    room.handle(moved, ClientMsg::Leave, 0);
    assert_eq!(room.host(), None);
    assert_eq!(room.take_declined(), vec![1]);
}
