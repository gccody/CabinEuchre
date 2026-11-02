//! Where everything sits on the table, in design space. Seats go clockwise
//! from the bottom: you (0), west (1), partner/north (2), east (3).
//!
//! The table is centered on the felt. The score pad sits in the top-left
//! corner, the trump medallion in the top-right, so the side seats keep their
//! won tricks and kitty below their name plates.
//!
//! On a phone the seats follow the edges of the safe area ([`render::bounds`]),
//! your hand is bigger, and the bidding buttons get more room above it.

use std::cmp::Reverse;
use std::f32::consts::PI;

use love2drs::prelude::*;

use super::{HUMAN, Place, Prompt, Table};
use crate::cards::{CARD_H, CARD_W};
use crate::device;
use crate::euchre::{Card, Seat, Suit};
use crate::render;
use crate::scene::NO_POINTER;

/// Center of the playing area (the pile and the trick).
pub const CENTER: Vec2 = Vec2::new(320.0, 152.0);
/// Baseline of the human's prompt text; bid buttons sit just below it.
pub const PROMPT_Y: f32 = 232.0;

const HUMAN_SCALE: f32 = 1.0;
/// Your cards on a phone: big enough to read, and to hit with a finger.
const MOBILE_HUMAN_SCALE: f32 = 1.3;
const OTHER_SCALE: f32 = 0.78;

/// The prompt's baseline. On a phone the bidding buttons need the space under
/// the pile, and there are no trick cards in the way until the bidding is over.
pub fn prompt_y(bidding: bool) -> f32 {
    match (device::mobile(), bidding) {
        (false, _) => PROMPT_Y,
        (true, true) => 196.0,
        (true, false) => 240.0,
    }
}

/// The phone's table: x of the west and east seats, and the top and bottom of the safe area.
fn mobile_edges() -> (f32, f32, f32, f32) {
    let b = render::bounds();
    (b.x + 62.0, b.x + b.w - 62.0, b.y, b.y + b.h)
}

/// Where a card wants to be this frame.
#[derive(Debug, Clone, Copy)]
pub struct Target {
    pub pos: Vec2,
    pub rot: f32,
    pub scale: f32,
    pub face_up: bool,
    pub tint: f32,
    /// Draw layer (higher draws on top); `order` sorts within a layer.
    pub layer: u8,
    pub order: f32,
}

pub fn seat_angle(seat: Seat) -> f32 {
    [0.0, PI / 2.0, PI, -PI / 2.0][seat]
}

pub fn hand_anchor(seat: Seat) -> Vec2 {
    if device::mobile() {
        let (west, east, top, bottom) = mobile_edges();
        return [vec2(320.0, bottom - 44.0), vec2(west, 156.0), vec2(320.0, top + 20.0), vec2(east, 156.0)][seat];
    }
    [vec2(320.0, 314.0), vec2(98.0, 156.0), vec2(320.0, 20.0), vec2(542.0, 156.0)][seat]
}

/// Center of each player's name plate.
pub fn plate_pos(seat: Seat) -> Vec2 {
    if device::mobile() {
        let (west, east, top, bottom) = mobile_edges();
        return [vec2(118.0, bottom - 20.0), vec2(west, 228.0), vec2(425.0, top + 16.0), vec2(east, 228.0)][seat];
    }
    [vec2(170.0, 336.0), vec2(98.0, 228.0), vec2(425.0, 16.0), vec2(542.0, 228.0)][seat]
}

/// Where each seat stacks the tricks it wins.
pub fn won_spot(seat: Seat) -> Vec2 {
    if device::mobile() {
        let (west, east, top, bottom) = mobile_edges();
        return [vec2(510.0, bottom - 38.0), vec2(west, 270.0), vec2(215.0, top + 20.0), vec2(east, 270.0)][seat];
    }
    [vec2(457.0, 322.0), vec2(98.0, 270.0), vec2(215.0, 20.0), vec2(542.0, 270.0)][seat]
}

/// Where the kitty goes once trump is set (beside the dealer).
pub fn kitty_spot(seat: Seat) -> Vec2 {
    if device::mobile() {
        let (west, east, top, bottom) = mobile_edges();
        return [vec2(118.0, bottom - 56.0), vec2(west, 318.0), vec2(522.0, top + 30.0), vec2(east, 318.0)][seat];
    }
    [vec2(170.0, 304.0), vec2(98.0, 318.0), vec2(522.0, 30.0), vec2(542.0, 318.0)][seat]
}

/// Anchor for speech bubbles, plus which part of the bubble sits on it
/// (0 = left edge, 0.5 = center, 1 = right edge) so side bubbles grow toward the table.
pub fn bubble_pos(seat: Seat) -> (Vec2, f32) {
    if device::mobile() {
        let (west, east, top, bottom) = mobile_edges();
        return [
            (vec2(118.0, bottom - 40.0), 0.5),
            (vec2(west + 32.0, 120.0), 0.0),
            (vec2(420.0, top + 56.0), 0.5),
            (vec2(east - 32.0, 120.0), 1.0),
        ][seat];
    }
    [
        (vec2(170.0, 314.0), 0.5),
        (vec2(130.0, 120.0), 0.0),
        (vec2(420.0, 56.0), 0.5),
        (vec2(510.0, 120.0), 1.0),
    ][seat]
}

fn trick_offset(seat: Seat) -> Vec2 {
    [vec2(0.0, 32.0), vec2(-46.0, 3.0), vec2(0.0, -28.0), vec2(46.0, 3.0)][seat]
}

fn rotate(v: Vec2, a: f32) -> Vec2 {
    Vec2::from_angle(a).rotate(v)
}

/// A small, stable "hand placed" tilt for a card.
fn jitter(card: Card) -> f32 {
    ((card.id() * 37 % 9) as f32 - 4.0) * 0.016
}

/// Sort key for the human's hand: suits grouped with alternating colors,
/// trump first (left bower included) once trump is known, high cards left.
fn human_key(card: Card, trump: Option<Suit>) -> (usize, Reverse<u8>) {
    match trump {
        Some(t) => {
            let s = card.suit_under(t);
            ((s.index() + 4 - t.index()) % 4, Reverse(card.power(t, s)))
        }
        None => (card.suit.index(), Reverse(card.rank.index() as u8)),
    }
}

/// The cards currently in `seat`'s hand, in display order (left to right).
pub fn hand_cards(t: &Table, seat: Seat) -> Vec<Card> {
    let mut cards: Vec<Card> = (0..24)
        .filter(|&id| t.places[id] == Place::Hand(seat))
        .map(Card::from_id)
        .collect();
    if seat == HUMAN {
        let trump = t.game.round.trump;
        cards.sort_by_key(|&c| human_key(c, trump));
    } else {
        // Hidden hands are kept in an arbitrary but stable order, so card
        // positions never leak information.
        let salt = t.game.hand_number as usize * 7 + seat;
        cards.sort_by_key(|c| (c.id() * 11 + salt * 5) % 24);
    }
    cards
}

/// Resting slot of card `i` of `n` in `seat`'s fan.
fn hand_slot(seat: Seat, i: usize, n: usize) -> (Vec2, f32, f32) {
    let human = seat == HUMAN;
    let k = i as f32 - (n as f32 - 1.0) / 2.0;
    if human && device::mobile() {
        // Spread out, so each card shows enough to tap.
        let spacing = if n == 6 { 44.0 } else { 52.0 };
        let local = vec2(k * spacing, k * k * 2.2);
        return (hand_anchor(seat) + local, k * 0.035, MOBILE_HUMAN_SCALE);
    }
    let spacing = match (human, n) {
        (true, 6) => 31.0,
        (true, _) => 35.0,
        (false, _) => 15.0,
    };
    let local = vec2(k * spacing, k * k * if human { 1.8 } else { 1.2 });
    let rot = k * if human { 0.045 } else { 0.06 };
    let a = seat_angle(seat);
    let scale = if human { HUMAN_SCALE } else { OTHER_SCALE };
    (hand_anchor(seat) + rotate(local, a), a + rot, scale)
}

pub fn targets(t: &Table, mouse: Vec2) -> Vec<Target> {
    let r = &t.game.round;
    let mut out: Vec<Target> = vec![
        Target {
            pos: CENTER,
            rot: 0.0,
            scale: 1.0,
            face_up: false,
            tint: 1.0,
            layer: 0,
            order: 0.0,
        };
        24
    ];

    // Hands.
    let legal = if t.prompt == Prompt::Play { r.legal_plays(HUMAN) } else { Vec::new() };
    for seat in 0..4 {
        let cards = hand_cards(t, seat);
        let n = cards.len();
        for (i, &card) in cards.iter().enumerate() {
            let (mut pos, mut rot, mut scale) = hand_slot(seat, i, n);
            let mut tint = 1.0;
            if seat == HUMAN {
                let mut lift = 0.0;
                if t.prompt == Prompt::Play {
                    if legal.contains(&card) {
                        lift += 4.0;
                    } else {
                        lift -= 5.0;
                        tint = 0.55;
                    }
                }
                if t.hover == Some(card) {
                    lift += 8.0;
                    scale *= 1.05;
                    if mouse != NO_POINTER {
                        rot += ((mouse.x - pos.x) / 22.0).clamp(-1.0, 1.0) * 0.05;
                    }
                }
                pos.y -= lift;
            }
            out[card.id()] = Target {
                pos,
                rot,
                scale,
                face_up: seat == HUMAN,
                tint,
                layer: 4,
                order: i as f32,
            };
        }
    }

    // Everything else, ordered by when it last moved.
    let mut others: Vec<usize> = (0..24).filter(|&id| !matches!(t.places[id], Place::Hand(_))).collect();
    others.sort_by_key(|&id| t.sprites[id].z);
    let mut pile_i = 0.0;
    let mut kitty_i = 0.0;
    let mut sit_i = [0.0f32; 4];
    for id in others {
        let card = Card::from_id(id);
        let z = t.sprites[id].z as f32;
        let target = match t.places[id] {
            Place::Pile => {
                pile_i += 1.0;
                Target {
                    pos: CENTER + vec2(0.0, -pile_i * 0.35),
                    rot: jitter(card) * 0.4,
                    scale: 1.0,
                    face_up: false,
                    tint: 1.0,
                    layer: 0,
                    order: z,
                }
            }
            Place::UpCard => Target {
                pos: CENTER + vec2(5.0, -9.0),
                rot: -0.07,
                scale: 1.0,
                face_up: true,
                tint: 1.0,
                layer: 1,
                order: z,
            },
            Place::Trick(seat) => {
                let glow = t.winner_glow.is_some_and(|(c, _)| c == card);
                Target {
                    pos: CENTER + trick_offset(seat),
                    rot: jitter(card),
                    scale: if glow { 1.1 } else { 1.0 },
                    face_up: true,
                    tint: 1.0,
                    layer: 3,
                    order: z,
                }
            }
            Place::Won(seat, n) => {
                let a = seat_angle(seat);
                let along = n as f32 * 7.0 - 14.0;
                let cross = if n % 2 == 1 { PI / 2.0 } else { 0.0 };
                Target {
                    pos: won_spot(seat) + rotate(vec2(along, 0.0), a),
                    rot: a + cross + jitter(card) * 0.5,
                    scale: 0.42,
                    face_up: false,
                    tint: 1.0,
                    layer: 1,
                    order: z,
                }
            }
            Place::Kitty(seat) => {
                kitty_i += 1.0;
                Target {
                    pos: kitty_spot(seat) + vec2(kitty_i * 1.5, -kitty_i),
                    rot: seat_angle(seat) + 0.25 + jitter(card),
                    scale: 0.45,
                    face_up: false,
                    tint: 0.8,
                    layer: 1,
                    order: z,
                }
            }
            Place::SitOut(seat) => {
                sit_i[seat] += 1.0;
                let a = seat_angle(seat);
                Target {
                    pos: hand_anchor(seat) + rotate(vec2(sit_i[seat] * 4.0 - 12.0, 6.0), a),
                    rot: a + jitter(card),
                    scale: 0.62,
                    face_up: false,
                    tint: 0.55,
                    layer: 2,
                    order: z,
                }
            }
            Place::Hand(_) => unreachable!(),
        };
        out[id] = target;
    }
    out
}

/// The human's card under `p` (testing resting positions, topmost first).
pub fn card_at(t: &Table, p: Vec2) -> Option<Card> {
    let cards = hand_cards(t, HUMAN);
    let n = cards.len();
    let (hw, hh) = (CARD_W / 2.0, CARD_H / 2.0);
    // Below the cards there's only the edge of the screen, so on a phone a
    // finger that's slipped off the bottom still means the card above it.
    let below = if device::mobile() { 30.0 } else { 0.0 };
    cards.iter().enumerate().rev().find_map(|(i, &card)| {
        let (pos, rot, scale) = hand_slot(HUMAN, i, n);
        let local = rotate(p - pos, -rot) / scale;
        // A little extra reach upward so a lifted card stays hovered.
        (local.x.abs() <= hw && local.y >= -hh - 10.0 && local.y <= hh + below).then_some(card)
    })
}
