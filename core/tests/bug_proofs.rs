//! Bug proofs for the rules core, and the stress tests that found nothing wrong.
//! Not part of a plain `cargo test`, since every proof FAILS while its bug is there:
//!
//! ```sh
//! cargo test -p euchre-core --test bug_proofs
//! ```
//!
//! Each `bug_*` test asserts the CORRECT behaviour. The `stress_*` tests pass today.
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
// Bug proofs
// ----------------------------------------------------------------------

/// `ClientMsg::Sit` carries any number the sender likes. In a debug build `(seat + to)` overflows
/// and panics. The room should refuse a seat that doesn't exist.
#[test]
fn bug_sit_refuses_a_seat_that_doesnt_exist() {
    let mut room = Room::new("SEATS", "h", 10, true);
    room.join(PROTOCOL_VERSION, "ann", None, "a").unwrap();
    let bo = room.join(PROTOCOL_VERSION, "bo", None, "b").unwrap();
    assert_eq!(bo, 2);
    room.take_outbox();
    room.handle(bo, ClientMsg::Sit { seat: usize::MAX }, 0);
    assert_eq!(room.seat_of_token("b"), Some(bo), "there's no seat {}", usize::MAX);
}

/// The same message without the overflow: seat 5 doesn't exist, and it still moves the player.
#[test]
fn bug_sit_refuses_seat_five() {
    let mut room = Room::new("SEATS", "h", 10, true);
    room.join(PROTOCOL_VERSION, "ann", None, "a").unwrap();
    room.take_outbox();
    room.handle(0, ClientMsg::Sit { seat: 5 }, 0);
    assert_eq!(room.seat_of_token("a"), Some(0), "relative seats are 0-3, but seat 5 moved Ann");
}

/// ai.rs: "They only look at what their seat could know". When the dealer buries the card they just
/// picked up, nobody else can know it. `unseen_by` works it out from the real kitty, so the bots know.
#[test]
fn bug_bots_dont_see_the_dealers_discard() {
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
    let mut r = Round::with_hands(3, hands, up, vec![c(Ten, Hearts), c(Jack, Spades), c(Nine, Clubs)], true);
    r.apply(0, Action::OrderUp { alone: false }).unwrap();
    assert!(r.unseen_by(1).contains(&up), "before the discard, seat 1 can't rule out the dealer's 9H");
    r.apply(3, Action::Discard { card: up }).unwrap();
    assert!(r.unseen_by(1).contains(&up), "seat 1 knows the face-down discard was the 9H");
}

/// The discard stays hidden from people: that's the rule in protocol.rs. But leaving and sitting back
/// down hands you another seat's whole hand, in the same hand of cards, so any player can peek at their
/// partner's cards (or a bot's) and go on playing.
#[test]
fn bug_leaving_and_rejoining_doesnt_show_another_hand() {
    let mut room = Room::new("PEEK", "h", 10, true);
    let ann = room.join(PROTOCOL_VERSION, "ann", None, "a").unwrap();
    let bo = room.join(PROTOCOL_VERSION, "bo", None, "b").unwrap();
    room.handle(ann, ClientMsg::Start, 7);
    let mut seen: Vec<Card> = vec![];
    for o in room.take_outbox() {
        if o.to == ann
            && let ServerMsg::Deal(h) = o.msg
        {
            seen = h.cards;
        }
    }
    assert_eq!(seen.len(), 5);
    let hand = room.game().unwrap().hand_number;
    // Bo stays, and whoever is due to act: the room must still be on this hand when Ann is back.
    room.handle(ann, ClientMsg::Leave, 0);
    room.take_outbox();
    // Seat 0 is free again, so she'd get her own cards back. Bo's old seat isn't, so take the next one.
    room.join(PROTOCOL_VERSION, "cy", None, "c").unwrap();
    room.take_outbox();
    let back = room.join(PROTOCOL_VERSION, "ann", None, "a2").unwrap();
    assert_eq!(room.seat_of_token("b"), Some(bo));
    let mut checked = false;
    for o in room.take_outbox() {
        if o.to == back
            && let ServerMsg::Sync { hand: h, .. } = o.msg
            && room.game().unwrap().hand_number == hand
        {
            checked = true;
            let new: Vec<_> = h.cards.iter().filter(|c| !seen.contains(c)).collect();
            assert!(new.is_empty(), "same hand, new seat: Ann now also knows {new:?}");
        }
    }
    assert!(checked, "(setup) the hand moved on while Ann was away");
}

/// Two people press PLAY AGAIN at about the same time. Both asked for the same thing, and the game
/// starts. The second shouldn't be told "You can't do that right now." over the new deal.
#[test]
fn bug_two_people_pressing_play_again_isnt_an_error() {
    let mut room = Room::new("AGAIN", "h", 1, true);
    let ann = room.join(PROTOCOL_VERSION, "ann", None, "a").unwrap();
    let bo = room.join(PROTOCOL_VERSION, "bo", None, "b").unwrap();
    room.handle(ann, ClientMsg::Start, 3);
    let mut guard = 0;
    while let Some((_, _, key)) = room.waiting_on() {
        room.force_move(key);
        guard += 1;
        assert!(guard < 500);
    }
    assert!(room.game().unwrap().winner().is_some());
    room.take_outbox();
    room.handle(ann, ClientMsg::Start, 4);
    room.handle(bo, ClientMsg::Start, 5);
    let errors: Vec<_> = room.take_outbox().into_iter().filter(|o| matches!(o.msg, ServerMsg::Error { .. })).collect();
    assert!(errors.is_empty(), "the second PLAY AGAIN got {:?}", errors[0].msg);
}
