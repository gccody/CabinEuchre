//! Tests from the pre-release review: each `bug_*` test failed before its fix, and
//! the `stress_*` tests throw random legal histories and damaged saves at the room,
//! the mirror and the save format, checking their invariants after every step.
use euchre_core::Card;
use euchre_core::card::{Rank, Suit};
use euchre_core::Match;
use euchre_core::ai;
use euchre_core::mirror::{ME, Mirror};
use euchre_core::protocol::*;
use euchre_core::rng::Rng;
use euchre_core::room::*;
use euchre_core::round::*;

fn legal_actions(r: &Round, seat: Seat) -> Vec<Action> {
    match r.phase {
        Phase::OrderUp => vec![Action::Pass, Action::OrderUp { alone: false }, Action::OrderUp { alone: true }],
        Phase::NameTrump => {
            let mut v = vec![];
            for s in Suit::ALL {
                if s != r.up_card.suit {
                    v.push(Action::Call { suit: s, alone: false });
                    v.push(Action::Call { suit: s, alone: true });
                }
            }
            if !(seat == r.dealer && r.stick_the_dealer) {
                v.extend([Action::Pass; 4]);
            }
            v
        }
        Phase::DealerDiscard => r.hands[seat].iter().map(|&card| Action::Discard { card }).collect(),
        Phase::Play => r.legal_plays(seat).into_iter().map(|card| Action::Play { card }).collect(),
        _ => vec![],
    }
}

/// Random legal histories with four people, mirrors kept from messages, replay at every step.
#[test]
fn stress_mirror_random_legal_play() {
    for seed in 0..400u64 {
        let mut rng = Rng::new(seed ^ 0xABCDEF);
        let mut room = Room::new("FUZZ", "h", 5, seed % 2 == 0);
        let tokens = ["a", "b", "c", "d"];
        let seats: Vec<Seat> = tokens.iter().map(|t| room.join(PROTOCOL_VERSION, t, None, t).unwrap()).collect();
        room.handle(seats[0], ClientMsg::Start, seed);
        let mut mirrors: [Option<Mirror>; 4] = Default::default();
        for step in 0..3000 {
            for out in room.take_outbox() {
                let m = &mut mirrors[out.to];
                match out.msg {
                    ServerMsg::Deal(h) => *m = Some(Mirror::new(&h).unwrap()),
                    ServerMsg::Sync { hand, moves } => *m = Some(Mirror::replay(&hand, &moves).unwrap()),
                    ServerMsg::Acted(mv) => {
                        let mm = m.as_mut().unwrap();
                        if mm.round.phase == Phase::TrickDone {
                            mm.advance();
                        }
                        mm.reveal(mv).unwrap_or_else(|e| panic!("seed {seed} step {step} seat {} {mv:?}: {e:?}\n{:?}", out.to, mm.round));
                    }
                    ServerMsg::Error { code, .. } => panic!("seed {seed}: {code:?}"),
                    _ => {}
                }
            }
            let Some((seat, _, _)) = room.waiting_on() else { break };
            let snap = room.snapshot();
            let back = Room::restore(serde_json::from_str(&serde_json::to_string(&snap).unwrap()).unwrap()).unwrap();
            assert_eq!(back.snapshot(), snap);
            for s in 0..4 {
                room.handle(s, ClientMsg::Resync, 0);
            }
            for out in room.take_outbox() {
                if let ServerMsg::Sync { hand, moves } = out.msg {
                    Mirror::replay(&hand, &moves).unwrap_or_else(|e| panic!("replay seed {seed} seat {}: {e:?}", out.to));
                }
            }
            let round = &room.game().unwrap().round;
            let acts = legal_actions(round, seat);
            let action = acts[rng.below(acts.len())];
            let m = mirrors[seat].as_mut().unwrap();
            if m.round.phase == Phase::TrickDone {
                m.advance();
            }
            let trust = m.round.clone();
            assert_eq!(trust.to_act(), Some(ME), "seed {seed} step {step}");
            m.act(action).unwrap_or_else(|e| panic!("own move refused {e:?}"));
            // The AI must also survive any state a mirror can be in.
            let _ = ai::choose(&trust, ME);
            room.handle(seat, ClientMsg::Act { action }, 0);
        }
    }
}

/// Everything anyone can do to a room, in random order. Invariants after every step.
#[test]
fn stress_room_random_everything() {
    for seed in 0..800u64 {
        let mut rng = Rng::new(seed);
        let mut room = Room::new("FUZZ", "h", 1 + (seed % 5) as u32, seed % 2 == 0);
        let mut tokens: Vec<String> = vec![];
        let mut knock_id = 0u32;
        for step in 0..600 {
            match rng.below(16) {
                0 | 1 => {
                    let t = format!("t{}", tokens.len());
                    if room.join(PROTOCOL_VERSION, "p", None, &t).is_ok() {
                        tokens.push(t);
                    }
                }
                2 => {
                    knock_id += 1;
                    let t = format!("k{knock_id}");
                    let _ = room.knock(PROTOCOL_VERSION, "k", None, knock_id, &t);
                }
                3 if !tokens.is_empty() => {
                    let t = tokens[rng.below(tokens.len())].clone();
                    let _ = room.join(PROTOCOL_VERSION, "again", Some(&t), "x");
                }
                4 => room.cancel_knock(rng.below(knock_id as usize + 1) as u32),
                5 => room.disconnected(rng.below(4)),
                6 => room.release_seat(rng.below(4)),
                7 => room.handle(rng.below(4), ClientMsg::Leave, 0),
                8 => {
                    let (s, to) = (rng.below(4), rng.below(4));
                    room.handle(s, ClientMsg::Sit { seat: to }, 0);
                }
                9 => {
                    let s = rng.below(4);
                    let seed = rng.next_u64();
                    room.handle(s, ClientMsg::Start, seed);
                }
                10 => {
                    let (s, p) = (rng.below(4), rng.below(2) == 0);
                    room.handle(s, ClientMsg::SetPublic { public: p }, 0);
                }
                11 => {
                    let (s, id, accept) = (rng.below(4), rng.below(knock_id as usize + 1) as u32, rng.below(2) == 0);
                    room.handle(s, ClientMsg::Answer { id, accept }, 0);
                }
                12 => {
                    if let Some((_, _, k)) = room.waiting_on() {
                        room.force_move(k);
                    }
                }
                13 => room.handle(rng.below(4), ClientMsg::Resync, 0),
                _ => {
                    if let Some((seat, _, _)) = room.waiting_on() {
                        let a = legal_actions(&room.game().unwrap().round, seat);
                        let action = a[rng.below(a.len())];
                        room.handle(seat, ClientMsg::Act { action }, 0);
                    }
                }
            }
            // Invariants.
            let snap = room.snapshot();
            let json = serde_json::to_string(&snap).unwrap();
            let back = Room::restore(serde_json::from_str(&json).unwrap())
                .unwrap_or_else(|e| panic!("seed {seed} step {step}: restore failed {e:?}\n{json}"));
            assert_eq!(back.snapshot(), snap, "seed {seed} step {step}");
            if room.connected() > 0 {
                let h = room.host().unwrap_or_else(|| panic!("seed {seed} step {step}: people here but no host"));
                assert!(
                    matches!(room.seats()[h], Occupant::Human { connected: true, .. }),
                    "seed {seed} step {step}: host {h} isn't here {:?}",
                    room.seats()
                );
            } else {
                assert!(room.knocks().is_empty(), "seed {seed} step {step}: knocks with nobody here");
            }
            if let Some(h) = room.host() {
                assert!(matches!(room.seats()[h], Occupant::Human { .. }), "seed {seed} step {step}: host seat {h} is a bot");
            }
            if room.playing() {
                let m = room.game().unwrap();
                let s = m.round.to_act().unwrap_or_else(|| panic!("seed {seed} step {step}: room idle in {:?}", m.round.phase));
                assert!(matches!(room.seats()[s], Occupant::Human { .. }), "seed {seed} step {step}: stuck on a bot at seat {s}");
                assert_eq!(room.waiting_on().map(|w| w.0), Some(s));
            }
            let _ = room.take_declined();
            let _ = room.take_admitted();
            for o in room.take_outbox() {
                if let ServerMsg::Sync { hand, moves } = o.msg {
                    Mirror::replay(&hand, &moves).unwrap_or_else(|e| panic!("seed {seed} step {step}: replay {e:?}"));
                }
            }
        }
    }
}

/// Damaged save text must never panic.
#[test]
fn stress_save_fuzz_never_panics() {
    let mut rng = Rng::new(9);
    let swaps: &[u8] = b"09= \n:-xJh";
    for seed in 0..300u64 {
        let mut m = Match::new(5, seed % 2 == 0, seed);
        for _ in 0..rng.below(40) {
            match m.round.phase {
                Phase::TrickDone => m.advance(),
                Phase::HandOver | Phase::Misdeal => break,
                _ => {
                    let s = m.round.to_act().unwrap();
                    let a = ai::choose(&m.round, s);
                    m.apply(s, a).unwrap();
                }
            }
        }
        let good = m.to_save();
        for _ in 0..60 {
            let mut bytes = good.clone().into_bytes();
            for _ in 0..1 + rng.below(4) {
                let i = rng.below(bytes.len());
                bytes[i] = swaps[rng.below(swaps.len())];
            }
            let text = String::from_utf8_lossy(&bytes).to_string();
            if let Ok(mm) = Match::from_save(&text) {
                // Anything that loads must keep working.
                let _ = mm.history();
                let _ = mm.to_save();
            }
        }
    }
}

// ----------------------------------------------------------------------
// Regressions
// ----------------------------------------------------------------------

/// `ClientMsg::Sit` carries any number the sender likes. `(seat + to)` used to overflow.
#[test]
fn bug_sit_refuses_a_seat_that_doesnt_exist() {
    let mut room = Room::new("SEATS", "h", 10, true);
    room.join(PROTOCOL_VERSION, "ann", None, "a").unwrap();
    let bo = room.join(PROTOCOL_VERSION, "bo", None, "b").unwrap();
    assert_eq!(bo, 2);
    room.take_outbox();
    room.handle(bo, ClientMsg::Sit { seat: usize::MAX }, 0);
    assert_eq!(room.seat_of_token("b"), Some(bo), "there's no seat {}", usize::MAX);
    assert!(room.take_outbox().iter().any(|o| o.to == bo && matches!(o.msg, ServerMsg::Error { .. })));
}

/// The same message without the overflow: seat 5 doesn't exist.
#[test]
fn bug_sit_refuses_seat_five() {
    let mut room = Room::new("SEATS", "h", 10, true);
    room.join(PROTOCOL_VERSION, "ann", None, "a").unwrap();
    room.take_outbox();
    room.handle(0, ClientMsg::Sit { seat: 5 }, 0);
    assert_eq!(room.seat_of_token("a"), Some(0), "relative seats are 0-3, but seat 5 moved Ann");
    // The real seats still work.
    room.handle(0, ClientMsg::Sit { seat: 1 }, 0);
    assert_eq!(room.seat_of_token("a"), Some(1));
}

fn ordered_up_hand() -> (Round, Card) {
    use Rank::*;
    use Suit::*;
    let c = |r, s| Card::new(r, s);
    let up = c(Nine, Hearts);
    let hands = [
        vec![c(Jack, Clubs), c(Ten, Clubs), c(Queen, Clubs), c(King, Clubs), c(Ace, Clubs)],
        vec![c(Nine, Spades), c(Ten, Spades), c(Queen, Spades), c(King, Spades), c(Ace, Spades)],
        vec![c(Nine, Diamonds), c(Ten, Diamonds), c(Queen, Diamonds), c(King, Diamonds), c(Ace, Diamonds)],
        vec![c(Jack, Hearts), c(Jack, Diamonds), c(Ace, Hearts), c(King, Hearts), c(Queen, Hearts)],
    ];
    (Round::with_hands(3, hands, up, vec![c(Ten, Hearts), c(Jack, Spades), c(Nine, Clubs)], true), up)
}

/// ai.rs: "They only look at what their seat could know". When the dealer buries the card they
/// just picked up, only the dealer knows.
#[test]
fn bug_bots_dont_see_the_dealers_discard() {
    let (mut r, up) = ordered_up_hand();
    r.apply(0, Action::OrderUp { alone: false }).unwrap();
    assert!(r.unseen_by(1).contains(&up), "before the discard, seat 1 can't rule out the dealer's 9H");
    r.apply(3, Action::Discard { card: up }).unwrap();
    assert!(r.unseen_by(1).contains(&up), "seat 1 knows the face-down discard was the 9H");
    assert!(!r.unseen_by(3).contains(&up), "the dealer does know");
}

/// A card everyone watched go into the kitty is still known to be out of play.
#[test]
fn a_turned_down_card_is_known_to_be_gone() {
    let (mut r, up) = ordered_up_hand();
    for seat in 0..4 {
        r.apply(seat, Action::Pass).unwrap();
    }
    assert_eq!(r.phase, Phase::NameTrump);
    assert!(!r.unseen_by(1).contains(&up));
    // The dealer's partner goes alone: the dealer never picks it up, in plain sight.
    let (mut r, up) = ordered_up_hand();
    r.apply(0, Action::Pass).unwrap();
    r.apply(1, Action::OrderUp { alone: true }).unwrap();
    assert!(!r.unseen_by(0).contains(&up));
}

/// Leaving and sitting back down used to hand you another seat's whole hand, in the same hand of
/// cards, so anyone could look at a partner's or a bot's cards and go on playing.
#[test]
fn bug_leaving_and_rejoining_doesnt_show_another_hand() {
    let mut room = Room::new("PEEK", "h", 10, true);
    let ann = room.join(PROTOCOL_VERSION, "ann", None, "a").unwrap();
    let bo = room.join(PROTOCOL_VERSION, "bo", None, "b").unwrap();
    room.handle(ann, ClientMsg::Start, 7);
    let hand = room.game().unwrap().hand_number;
    room.handle(ann, ClientMsg::Leave, 0);
    room.take_outbox();
    // Ann's old seat goes to someone else, so she'd come back to a new one.
    room.join(PROTOCOL_VERSION, "cy", None, "c").unwrap();
    room.take_outbox();
    let back = room.join(PROTOCOL_VERSION, "ann", None, "a2").unwrap();
    assert_eq!(room.seat_of_token("b"), Some(bo));
    assert_eq!(room.game().unwrap().hand_number, hand, "(setup) the hand moved on");
    room.handle(back, ClientMsg::Resync, 0);
    let out = room.take_outbox();
    assert!(out.iter().any(|o| o.to == back && matches!(o.msg, ServerMsg::Welcome { .. })), "she does get a seat");
    assert!(
        !out.iter().any(|o| o.to == back && matches!(o.msg, ServerMsg::Sync { .. } | ServerMsg::Deal(_))),
        "same hand, new seat: Ann was shown that seat's cards"
    );
}

/// Someone who sits down mid-hand is dealt in at the next hand, and plays from then on.
#[test]
fn sitting_down_mid_hand_waits_for_the_next_deal() {
    let mut room = Room::new("LATE", "h", 10, true);
    let ann = room.join(PROTOCOL_VERSION, "ann", None, "a").unwrap();
    room.handle(ann, ClientMsg::Start, 11);
    room.take_outbox();
    let cy = room.join(PROTOCOL_VERSION, "cy", None, "c").unwrap();
    assert!(!room.take_outbox().iter().any(|o| o.to == cy && matches!(o.msg, ServerMsg::Sync { .. })));
    // A late player's seat survives a save and a reload, still waiting.
    let mut room = Room::restore(room.snapshot()).unwrap();
    assert!(room.snapshot().waiting[cy]);
    // Nothing waits on them this hand, and they don't hear about its moves.
    for _ in 0..200 {
        let before = room.game().unwrap().hand_number;
        let out = room.take_outbox();
        if before > 1 || out.iter().any(|o| o.to == cy && matches!(o.msg, ServerMsg::Deal(_))) {
            assert!(out.iter().any(|o| o.to == cy && matches!(o.msg, ServerMsg::Deal(_))), "dealt in at the next hand");
            assert!(!room.snapshot().waiting[cy]);
            let seats = room.seats_for(cy);
            assert_eq!(seats.seats[0].kind, SeatKind::Human);
            return;
        }
        assert!(!out.iter().any(|o| o.to == cy && matches!(o.msg, ServerMsg::Acted(_))), "they heard a move from a hand they aren't in");
        let (seat, _, key) = room.waiting_on().expect("the room is waiting on Ann");
        assert_eq!(seat, ann, "the room waited on someone who hasn't been dealt in");
        room.force_move(key);
    }
    panic!("the hand never ended");
}

/// Coming back with your own token is your own seat: you get your hand back at once.
#[test]
fn reconnecting_gets_your_own_hand_back_straight_away() {
    let mut room = Room::new("BACK", "h", 10, true);
    let ann = room.join(PROTOCOL_VERSION, "ann", None, "a").unwrap();
    let bo = room.join(PROTOCOL_VERSION, "bo", None, "b").unwrap();
    room.handle(ann, ClientMsg::Start, 11);
    room.disconnected(bo);
    room.take_outbox();
    room.join(PROTOCOL_VERSION, "bo", Some("b"), "unused").unwrap();
    assert!(room.take_outbox().iter().any(|o| o.to == bo && matches!(o.msg, ServerMsg::Sync { .. })));
}

/// Only the host deals, both from the lobby and after a game ends.
#[test]
fn only_the_host_deals() {
    let mut room = Room::new("DEAL", "h", 1, true);
    let ann = room.join(PROTOCOL_VERSION, "ann", None, "a").unwrap();
    let bo = room.join(PROTOCOL_VERSION, "bo", None, "b").unwrap();
    assert_eq!(room.host(), Some(ann));
    room.take_outbox();
    room.handle(bo, ClientMsg::Start, 1);
    assert!(room.game().is_none(), "a guest dealt");
    assert!(room.take_outbox().iter().any(|o| o.to == bo && matches!(o.msg, ServerMsg::Error { .. })));
    room.handle(ann, ClientMsg::Start, 1);
    assert!(room.game().is_some());
    let mut guard = 0;
    while let Some((_, _, key)) = room.waiting_on() {
        room.force_move(key);
        guard += 1;
        assert!(guard < 500);
    }
    assert!(room.game().unwrap().winner().is_some());
    let game = room.game_no();
    room.handle(bo, ClientMsg::Start, 2);
    assert_eq!(room.game_no(), game, "a guest started a rematch");
    // The job moves on with the host, and the new host can deal.
    room.handle(ann, ClientMsg::Leave, 0);
    assert_eq!(room.host(), Some(bo));
    room.handle(bo, ClientMsg::Start, 3);
    assert_eq!(room.game_no(), game + 1);
}

/// A double press of DEAL or PLAY AGAIN shouldn't be told off over the new deal.
#[test]
fn bug_dealing_twice_isnt_an_error() {
    let mut room = Room::new("AGAIN", "h", 10, true);
    let ann = room.join(PROTOCOL_VERSION, "ann", None, "a").unwrap();
    room.handle(ann, ClientMsg::Start, 4);
    let game = room.game_no();
    room.take_outbox();
    room.handle(ann, ClientMsg::Start, 5);
    assert_eq!(room.game_no(), game);
    let errors: Vec<_> = room.take_outbox().into_iter().filter(|o| matches!(o.msg, ServerMsg::Error { .. })).collect();
    assert!(errors.is_empty(), "the second press got {:?}", errors[0].msg);
}
