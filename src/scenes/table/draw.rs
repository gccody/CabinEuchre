//! Rendering for the table scene. Nothing here changes game state.

use std::f32::consts::PI;

use love2drs::prelude::*;

use super::layout::{self, CENTER, PROMPT_Y};
use super::{HUMAN, Place, Prompt, Table};
use crate::euchre::protocol::SeatKind;
use crate::cards::{CARD_H, CARD_W, Pose};
use crate::euchre::{Card, Phase, team_of};
use crate::palette;
use crate::scene::Shared;
use crate::text::Text;
use crate::ui::{self, Rect};

/// The paper score pad in the top-left corner (before its slight tilt).
const PAD: Rect = Rect::new(12.0, 11.0, 124.0, 86.0);
const PAD_TILT: f32 = -0.035;
/// Where score bursts fly from: the "WE" column of the pad.
pub(super) const PAD_SCORE_US: Vec2 = Vec2::new(43.0, 44.0);
/// Center of the brass trump medallion in the top-right corner.
const MEDALLION: Vec2 = Vec2::new(578.0, 40.0);

fn team_color(team: usize) -> Color {
    if team == 0 { palette::US } else { palette::THEM }
}

impl Table {
    pub(super) fn draw_table(&self, shared: &Shared, g: &mut Graphics) {
        g.push();
        if self.shake > 0.0 {
            let a = self.shake * self.shake * 3.0;
            let t = shared.time * 60.0;
            g.translate((t * 1.7).sin() * a, (t * 2.3).cos() * a);
        }
        self.draw_scorepad(shared, g);
        self.draw_trump(shared, g);
        self.menu_button.draw(g, &shared.font);
        self.draw_plates(shared, g);
        self.draw_cards(shared, g);
        self.draw_particles(g);
        self.draw_bubbles(shared, g);
        self.draw_prompt(shared, g);
        self.draw_toast(shared, g);
        g.pop();

        if let Some(b) = &self.banner {
            self.draw_banner(shared, g, b);
        }
        if self.game_over.is_some() {
            self.draw_game_over(shared, g);
        }
        if let Some(p) = &self.pause {
            ui::dim_screen(g, 0.6);
            g.push();
            let c = vec2(CENTER.x, 180.0);
            g.translate(c.x, c.y);
            g.scale(p.pop.value, p.pop.value);
            g.translate(-c.x, -c.y);
            ui::panel(g, Rect::centered(c.x, c.y, 160.0, 166.0), palette::PANEL);
            Text::new("PAUSED", c.x, 122.0).scale(2.0).color(palette::BRASS_LIGHT).center().draw(g, &shared.font);
            for (_, b) in &p.buttons {
                b.draw(g, &shared.font);
            }
            g.pop();
        }
        if let Some(o) = &self.overlay {
            o.draw(shared, g);
        }
    }

    fn draw_cards(&self, shared: &Shared, g: &mut Graphics) {
        let targets = layout::targets(self, shared.mouse);
        let mut order: Vec<usize> = (0..24).collect();
        let in_flight = |id: usize| {
            let s = &self.sprites[id];
            s.pos().distance(targets[id].pos) > 12.0
        };
        order.sort_by(|&a, &b| {
            let la = if in_flight(a) { 9 } else { targets[a].layer };
            let lb = if in_flight(b) { 9 } else { targets[b].layer };
            // Cards in flight draw in the order they were sent.
            let (oa, ob) = if la == 9 && lb == 9 {
                (self.sprites[a].z as f32, self.sprites[b].z as f32)
            } else {
                (targets[a].order, targets[b].order)
            };
            la.cmp(&lb).then(oa.total_cmp(&ob))
        });

        let playable = match self.prompt {
            Prompt::Play => self.game.round.legal_plays(HUMAN),
            Prompt::Discard => self.game.round.hands[HUMAN].clone(),
            _ => Vec::new(),
        };
        for id in order {
            let s = &self.sprites[id];
            let card = Card::from_id(id);
            let flip_arc = (s.face * PI).sin();
            let flip = (1.0 - 2.0 * s.face).abs().max(0.06);
            let scale = s.scale.value;
            let (sx, sy) = (scale * flip, scale * (1.0 + flip_arc * 0.06));
            let (x, y, rot) = (s.x.value, s.y.value - flip_arc * 6.0, s.rot.value);
            let face = (s.face >= 0.5).then_some(card);
            let hovered = self.hover == Some(card) && self.places[id] == Place::Hand(HUMAN);
            let height = if hovered { 3.0 } else { 0.0 } + flip_arc * 3.0;

            let pose = Pose::new(x, y, rot, sx, sy);

            // Shadow first: the back silhouette, tinted.
            let shadow = pose.offset(2.0 * scale + height * 0.5, 3.0 * scale + height);
            shared.cards.draw(g, None, shadow, palette::INK.with_alpha(0.42));
            let tint = Color::rgb(s.tint, s.tint, s.tint);
            shared.cards.draw(g, face, pose, tint);

            if let Some((glow, age)) = self.winner_glow
                && glow == card
            {
                let pulse = 0.6 + 0.4 * (age * 9.0).sin();
                outline(g, pose, palette::GOLD.with_alpha(pulse), 2.0);
            }
            if hovered && playable.contains(&card) {
                let color = if self.prompt == Prompt::Discard { palette::RED } else { palette::GOLD };
                outline(g, pose, color, 1.0);
            }
        }
    }

    /// The score pad: a tilted sheet of ruled paper with WE / THEY columns,
    /// the score in pencil (a number plus tally marks toward the target), the
    /// tricks taken this hand as pencil dots, and a note at the bottom.
    pub(super) fn draw_scorepad(&self, shared: &Shared, g: &mut Graphics) {
        let font = &shared.font;
        let r = &self.game.round;
        let (w, h) = (PAD.w, PAD.h);
        g.push();
        let c = PAD.center();
        g.translate(c.x, c.y);
        g.rotate(PAD_TILT);
        g.translate(-w / 2.0, -h / 2.0);

        // Shadow, a sheet peeking out underneath, then the top sheet.
        g.set_color(palette::INK.with_alpha(0.45));
        g.rectangle(Fill, 3.0, 4.0, w, h);
        g.push();
        g.rotate(0.03);
        g.set_color(palette::INK);
        g.rectangle(Fill, 1.0, -1.0, w + 2.0, h + 2.0);
        g.set_color(palette::PAPER_SHADE);
        g.rectangle(Fill, 2.0, 0.0, w, h);
        g.pop();
        g.set_color(palette::INK);
        g.rectangle(Fill, -1.0, -1.0, w + 2.0, h + 2.0);
        g.set_color(palette::PAPER);
        g.rectangle(Fill, 0.0, 0.0, w, h);
        g.set_color(palette::PAPER_RULE.with_alpha(0.7));
        let mut y = 20.0;
        while y < h - 3.0 {
            g.rectangle(Fill, 2.0, y, w - 4.0, 1.0);
            y += 11.0;
        }
        // Gummed binding strip along the top.
        g.set_color(palette::RED_DARK);
        g.rectangle(Fill, 0.0, 0.0, w, 6.0);
        g.set_color(palette::RED);
        g.rectangle(Fill, 0.0, 0.0, w, 1.0);
        g.set_color(palette::INK.with_alpha(0.5));
        g.rectangle(Fill, 0.0, 6.0, w, 1.0);
        // Column divider, drawn in pencil.
        g.set_color(palette::PENCIL.with_alpha(0.55));
        g.rectangle(Fill, (w / 2.0).round(), 10.0, 1.0, h - 24.0);

        let target = self.game.target.max(1);
        for team in 0..2 {
            let cx = (w * (0.25 + 0.5 * team as f32)).round();
            let (label, ink) = if team == 0 { ("WE", palette::US_INK) } else { ("THEY", palette::THEM_INK) };
            Text::new(label, cx, 14.0).color(ink).no_shadow().center().draw(g, font);
            let pop = self.score_pop[team].value;
            Text::new(&self.score_shown[team].to_string(), cx, 31.0)
                .scale(2.0 * pop)
                .color(palette::PENCIL)
                .no_shadow()
                .center()
                .draw(g, font);
            tallies(g, cx, 44.0, self.score_shown[team].min(target), target);

            // Tricks this hand: a filled pencil dot per trick, faint rings for the rest.
            let won = r.team_tricks(team);
            let tpop = self.trick_pop[team].value;
            for i in 0..5u8 {
                let x = cx + (i as f32 - 2.0) * 9.0;
                if i < won {
                    let rad = if i + 1 == won { 3.0 * tpop } else { 3.0 };
                    g.set_color(ink);
                    g.circle(Fill, x, 64.0, rad);
                } else {
                    g.set_color(palette::PENCIL_LIGHT.with_alpha(0.6));
                    g.set_line_width(1.0);
                    g.circle(Line, x, 64.0, 2.5);
                }
            }
        }

        let note = if r.maker.is_some() && r.phase != Phase::NameTrump {
            if r.maker.map(team_of) == Some(0) { "we need 3 tricks".to_string() } else { "hold them to 2!".to_string() }
        } else {
            format!("hand {}  ·  to {}", self.game.hand_number, self.game.target)
        };
        Text::new(&note, w / 2.0, 78.0).color(palette::PENCIL_LIGHT).no_shadow().center().draw(g, font);
        g.pop();
    }

    /// The brass trump medallion with a ribbon, and who called it beneath.
    fn draw_trump(&self, shared: &Shared, g: &mut Graphics) {
        let font = &shared.font;
        let r = &self.game.round;
        let m = MEDALLION;
        let rad = 23.0;
        g.set_color(palette::INK.with_alpha(0.5));
        g.circle(Fill, m.x + 2.0, m.y + 3.0, rad + 3.0);
        g.set_color(palette::INK);
        g.circle(Fill, m.x, m.y, rad + 3.0);
        g.set_color(palette::BRASS_DARK);
        g.circle(Fill, m.x, m.y, rad + 2.0);
        g.set_color(palette::BRASS);
        g.circle(Fill, m.x, m.y - 1.0, rad + 1.0);
        g.set_color(palette::BRASS_LIGHT.with_alpha(0.8));
        g.set_line_width(1.0);
        g.arc(Line, m.x, m.y - 1.0, rad, PI * 1.1, PI * 1.6);
        g.set_color(palette::INK);
        g.circle(Fill, m.x, m.y, rad - 3.0);
        g.set_color(palette::PANEL_DARK);
        g.circle(Fill, m.x, m.y, rad - 4.0);

        let (ribbon, ribbon_color) = match (r.trump, r.alone) {
            (Some(_), true) => ("ALONE!", palette::ORANGE),
            _ => ("TRUMP", palette::RED),
        };
        match (r.trump, r.maker) {
            (Some(trump), Some(maker)) => {
                let pop = self.trump_pop.value;
                let wob = (shared.time * 2.0).sin() * 0.05;
                let color = crate::cards::suit_color_on_dark(trump);
                shared.cards.draw_suit(g, trump, vec2(m.x, m.y - 2.0), 2.2 * pop, wob, color);
                let who = &self.names[maker];
                Text::new(&format!("BY {who}"), m.x, m.y + rad + 18.0).center().draw(g, font);
            }
            _ => {
                Text::new("?", m.x, m.y - 2.0).scale(3.0).color(palette::DIM).center().draw(g, font);
                let line = if r.phase == Phase::NameTrump {
                    format!("{} turned down", r.up_card.suit.symbol())
                } else {
                    "deciding...".to_string()
                };
                Text::new(&line, m.x, m.y + rad + 18.0).color(palette::MUTED).center().draw(g, font);
            }
        }
        // A ribbon across the bottom of the medallion, with folded tails.
        let rw = font.width(ribbon) + 12.0;
        let rr = Rect::centered(m.x, m.y + rad + 1.0, rw, 11.0);
        let pulse = if r.alone { 0.15 + 0.15 * (shared.time * 5.0).sin() } else { 0.0 };
        let dark = ribbon_color.lerp(palette::INK, 0.45);
        for (x, dir) in [(rr.x, -1.0), (rr.x + rr.w, 1.0)] {
            let (x0, x1) = (x - dir * 2.0, x + dir * 7.0);
            let (top, bot) = (rr.y + 2.0, rr.y + rr.h + 2.0);
            let notch = x1 - dir * 3.0;
            g.set_color(palette::INK);
            g.polygon(Fill, &[x0, top - 1.0, x1 + dir, top - 1.0, notch + dir, (top + bot) / 2.0, x1 + dir, bot + 1.0, x0, bot + 1.0]);
            g.set_color(dark);
            g.polygon(Fill, &[x0, top, x1, top, notch, (top + bot) / 2.0, x1, bot, x0, bot]);
        }
        g.set_color(palette::INK);
        g.rectangle(Fill, rr.x - 1.0, rr.y - 1.0, rr.w + 2.0, rr.h + 2.0);
        g.set_color(ribbon_color.lerp(palette::WHITE, pulse));
        g.rectangle(Fill, rr.x, rr.y, rr.w, rr.h);
        g.set_color(ribbon_color.lerp(palette::WHITE, 0.3));
        g.rectangle(Fill, rr.x, rr.y, rr.w, 1.0);
        Text::new(ribbon, m.x, rr.center().y).center().draw(g, font);
    }

    fn draw_plates(&self, shared: &Shared, g: &mut Graphics) {
        let font = &shared.font;
        let r = &self.game.round;
        // Light up a seat only when its action is the next real step (not during the deal).
        let next_step = self.cues.iter().find(|c| !matches!(c, super::Cue::Wait(_)));
        let waiting_on = match next_step {
            None | Some(super::Cue::AiAct(_) | super::Cue::Remote(_)) => r.to_act(),
            _ => None,
        };
        for (seat, name) in self.names.iter().enumerate() {
            let name = name.as_str();
            let pos = layout::plate_pos(seat);
            // Sitting out, or (online) a player whose connection dropped.
            let out = r.is_sitting_out(seat) || self.kinds[seat] == SeatKind::Away;
            let w = font.width(name) + 18.0;
            let rect = Rect::centered(pos.x, pos.y, w, 13.0);
            let active = waiting_on == Some(seat) && self.banner.is_none();
            g.set_color(palette::INK.with_alpha(0.5));
            ui::chamfer(g, Fill, rect.x + 1.0, rect.y + 2.0, rect.w, rect.h, 3.0);
            if active {
                let pulse = 0.55 + 0.45 * (shared.time * 6.0).sin();
                g.set_color(palette::BRASS_LIGHT.with_alpha(pulse));
                ui::chamfer(g, Fill, rect.x - 3.0, rect.y - 3.0, rect.w + 6.0, rect.h + 6.0, 5.0);
            }
            g.set_color(palette::INK);
            ui::chamfer(g, Fill, rect.x - 1.0, rect.y - 1.0, rect.w + 2.0, rect.h + 2.0, 4.0);
            g.set_color(palette::PANEL);
            ui::chamfer(g, Fill, rect.x, rect.y, rect.w, rect.h, 3.0);
            // Team color as an arrow-shaped notch on the left edge.
            let (l, t, b, cy) = (rect.x + 3.0, rect.y + 3.0, rect.y + rect.h - 3.0, rect.center().y);
            g.set_color(team_color(team_of(seat)));
            g.polygon(Fill, &[l, t, l + 4.0, t, l + 6.0, cy, l + 4.0, b, l, b]);
            let color = if out { palette::DIM } else { palette::CREAM };
            Text::new(name, rect.x + 12.0, rect.center().y).color(color).anchor(0.0, 0.5).draw(g, font);

            // Badges beside the plate (on the left for the east seat, which hugs the edge).
            let dir = if seat == 3 { -1.0 } else { 1.0 };
            let mut bx = if seat == 3 { rect.x - 8.0 } else { rect.x + rect.w + 8.0 };
            if r.dealer == seat {
                g.set_color(palette::INK);
                g.circle(Fill, bx, rect.center().y + 0.5, 7.0);
                g.set_color(palette::BRASS_DARK);
                g.circle(Fill, bx, rect.center().y, 6.0);
                g.set_color(palette::BRASS);
                g.circle(Fill, bx, rect.center().y - 0.5, 5.0);
                Text::new("D", bx + 0.5, rect.center().y)
                    .color(palette::INK)
                    .no_shadow()
                    .center()
                    .draw(g, font);
                bx += 15.0 * dir;
            }
            if r.maker == Some(seat)
                && let Some(trump) = r.trump {
                    g.set_color(palette::INK);
                    ui::chamfer(g, Fill, bx - 7.0, rect.y - 1.0, 14.0, 15.0, 3.0);
                    g.set_color(palette::CREAM);
                    ui::chamfer(g, Fill, bx - 6.0, rect.y, 12.0, 13.0, 2.0);
                    Text::new(&trump.symbol().to_string(), bx, rect.center().y)
                        .color(crate::cards::suit_color(trump))
                        .plain_suits()
                        .no_shadow()
                        .center()
                        .draw(g, font);
                }
            if out {
                Text::new("sitting out", pos.x, pos.y + 12.0).color(palette::MUTED).center().draw(g, font);
            }
        }
    }

    fn draw_bubbles(&self, shared: &Shared, g: &mut Graphics) {
        let font = &shared.font;
        for (seat, bubble) in self.bubbles.iter().enumerate() {
            let Some(b) = bubble else { continue };
            let life = 1.8 * self.delay.max(0.8);
            let fade = ((life - b.age) / 0.25).clamp(0.0, 1.0);
            let (anchor, ax) = layout::bubble_pos(seat);
            let w = font.width(&b.text) + 12.0;
            let h = 15.0;
            g.push();
            g.translate(anchor.x, anchor.y);
            g.scale(b.pop.value, b.pop.value);
            let rect = Rect::new(-w * ax, -h / 2.0, w, h);
            // A tail pointing back at the speaker: down for you, left for the
            // west seat, up for your partner, right for the east seat.
            let (hh, r0, r1) = (h / 2.0, rect.x, rect.x + rect.w);
            let tail: [f32; 6] = match seat {
                0 => [-4.0, hh - 1.0, 4.0, hh - 1.0, 0.0, hh + 5.0],
                1 => [r0 + 1.0, -4.0, r0 + 1.0, 4.0, r0 - 5.0, 0.0],
                2 => [-4.0, -hh + 1.0, 4.0, -hh + 1.0, 0.0, -hh - 5.0],
                _ => [r1 - 1.0, -4.0, r1 - 1.0, 4.0, r1 + 5.0, 0.0],
            };
            // The same triangle grown by a pixel, for its outline.
            let (mx, my) = ((tail[0] + tail[2] + tail[4]) / 3.0, (tail[1] + tail[3] + tail[5]) / 3.0);
            let mut outline_tail = tail;
            for i in 0..3 {
                outline_tail[i * 2] = mx + (tail[i * 2] - mx) * 1.6;
                outline_tail[i * 2 + 1] = my + (tail[i * 2 + 1] - my) * 1.6;
            }
            g.set_color(palette::INK.with_alpha(0.5 * fade));
            ui::chamfer(g, Fill, rect.x + 1.0, rect.y + 2.0, rect.w, rect.h, 3.0);
            g.set_color(palette::INK.with_alpha(fade));
            ui::chamfer(g, Fill, rect.x - 1.0, rect.y - 1.0, rect.w + 2.0, rect.h + 2.0, 4.0);
            g.polygon(Fill, &outline_tail);
            g.set_color(palette::CREAM.with_alpha(fade));
            ui::chamfer(g, Fill, rect.x, rect.y, rect.w, rect.h, 3.0);
            g.polygon(Fill, &tail);
            Text::new(&b.text, rect.center().x, 0.0)
                .color(palette::INK.with_alpha(fade))
                .no_shadow()
                .plain_suits()
                .center()
                .draw(g, font);
            g.pop();
        }
    }

    fn draw_prompt(&self, shared: &Shared, g: &mut Graphics) {
        let font = &shared.font;
        let r = &self.game.round;
        if let Some((hint, t)) = &self.hint {
            let wob = (t * 30.0).sin() * (t - 1.9).max(0.0) * 6.0;
            let w = font.width(hint) + 16.0;
            g.set_color(palette::INK.with_alpha(0.7));
            ui::chamfer(g, Fill, (CENTER.x + wob - w / 2.0).round(), PROMPT_Y - 7.0, w, 15.0, 3.0);
            Text::new(hint, CENTER.x + wob, PROMPT_Y).color(palette::SUIT_RED_LIGHT).center().draw(g, font);
        } else {
            let text = match self.prompt {
                Prompt::OrderUp => {
                    let up = r.up_card;
                    if r.dealer == HUMAN {
                        format!("Pick up the {} and make {} trump?", up.label(), up.suit.symbol())
                    } else {
                        let who = if r.dealer == 2 { "your partner".to_string() } else { self.display_name(r.dealer) };
                        format!("Order up the {}? {who} would pick it up.", up.label())
                    }
                }
                Prompt::NameTrump => {
                    if r.dealer == HUMAN && r.stick_the_dealer {
                        "Stuck with the deal: you must name trump!".to_string()
                    } else {
                        "Name a trump suit, or pass.".to_string()
                    }
                }
                Prompt::Discard => "Choose a card to discard.".to_string(),
                Prompt::Play => {
                    match r.led_suit() {
                        None => "Your lead. Play any card.".to_string(),
                        Some(led) => {
                            let can_follow = r.hands[HUMAN].iter().any(|c| r.trump.is_some_and(|t| c.suit_under(t) == led));
                            if can_follow {
                                format!("Your turn: follow {}.", led.symbol())
                            } else {
                                format!("Your turn: no {}, so play anything.", led.symbol())
                            }
                        }
                    }
                }
                Prompt::None => {
                    if r.is_sitting_out(HUMAN) && r.phase == Phase::Play {
                        "Your partner is going alone. Sit back and watch!".to_string()
                    } else {
                        String::new()
                    }
                }
            };
            if !text.is_empty() {
                let bob = ((shared.time * 3.0).sin() * 0.6).round();
                let w = font.width(&text) + 16.0;
                g.set_color(palette::INK.with_alpha(0.6));
                ui::chamfer(g, Fill, (CENTER.x - w / 2.0).round(), PROMPT_Y - 7.0 + bob, w, 15.0, 3.0);
                Text::new(&text, CENTER.x, PROMPT_Y + bob).color(palette::CREAM).center().draw(g, font);
            }
        }
        for (_, b) in &self.bids {
            b.draw(g, font);
        }
    }

    fn draw_toast(&self, shared: &Shared, g: &mut Graphics) {
        let Some(t) = &self.toast else { return };
        let font = &shared.font;
        let fade = ((1.5 - t.age) / 0.3).clamp(0.0, 1.0);
        let pos = CENTER + vec2(0.0, -2.0);
        g.push();
        g.translate(pos.x, pos.y);
        g.scale(t.pop.value, t.pop.value);
        // Suit icon on the left, text in the column to its right; grow to fit the text.
        let text_w = font.width(&t.title).max(font.width(&t.sub));
        let rect = Rect::centered(0.0, 0.0, (text_w + 62.0).max(196.0), 46.0);
        let text_x = (rect.x + 45.0 + rect.x + rect.w - 12.0) / 2.0;
        g.set_color(palette::INK.with_alpha(0.5 * fade));
        ui::chamfer(g, Fill, rect.x + 2.0, rect.y + 3.0, rect.w, rect.h, 5.0);
        g.set_color(palette::INK.with_alpha(fade));
        ui::chamfer(g, Fill, rect.x - 3.0, rect.y - 3.0, rect.w + 6.0, rect.h + 6.0, 7.0);
        g.set_color(palette::BRASS.with_alpha(fade));
        ui::chamfer(g, Fill, rect.x - 2.0, rect.y - 2.0, rect.w + 4.0, rect.h + 4.0, 6.0);
        g.set_color(palette::PANEL.with_alpha(fade));
        ui::chamfer(g, Fill, rect.x, rect.y, rect.w, rect.h, 5.0);
        // The suit sits in its own dark well on the left.
        g.set_color(palette::PANEL_DARK.with_alpha(fade));
        ui::chamfer(g, Fill, rect.x + 4.0, rect.y + 4.0, 36.0, rect.h - 8.0, 3.0);
        let color = crate::cards::suit_color_on_dark(t.suit).with_alpha(fade);
        let wob = (shared.time * 3.0).sin() * 0.08;
        shared.cards.draw_suit(g, t.suit, vec2(rect.x + 22.0, -1.0), 2.0, wob, color);
        Text::new(&t.title, text_x, -7.0)
            .scale(1.0)
            .color(palette::CREAM.with_alpha(fade))
            .wave(shared.time, 1.0)
            .center()
            .draw(g, font);
        Text::new(&t.sub, text_x, 9.0).color(palette::GOLD.with_alpha(fade)).center().draw(g, font);
        g.pop();
    }

    fn draw_particles(&self, g: &mut Graphics) {
        for p in &self.particles {
            let a = (p.life / p.max).clamp(0.0, 1.0);
            g.set_color(p.color.with_alpha(a));
            g.rectangle(Fill, p.pos.x.round(), p.pos.y.round(), p.size, p.size);
        }
    }

    fn draw_banner(&self, shared: &Shared, g: &mut Graphics, b: &super::Banner) {
        let font = &shared.font;
        ui::dim_screen(g, (b.age * 3.0).min(1.0) * 0.4);
        // Keep the score pad lit above the dim so the points visibly tick up.
        self.draw_scorepad(shared, g);
        g.push();
        let c = CENTER + vec2(0.0, 8.0);
        g.translate(c.x, c.y);
        g.scale(b.pop.value, b.pop.value);
        g.translate(-c.x, -c.y);
        // Grow to fit the widest line (the detail names the maker and can run long).
        let text_w = (font.width(b.title) * 2.5).max(font.width(&b.detail)).max(font.width(&b.points) * 1.5);
        let rect = Rect::centered(c.x, c.y, (text_w + 32.0).clamp(260.0, 470.0), 112.0);
        ui::panel(g, rect, palette::PANEL);
        Text::new(b.title, c.x, rect.y + 23.0)
            .scale(2.5)
            .color(b.color)
            .wave(shared.time, 1.5)
            .center()
            .draw(g, font);
        Text::new(&b.detail, c.x, rect.y + 46.0).color(palette::CREAM).center().draw(g, font);
        let team_color = team_color(b.result.scoring_team);
        Text::new(&b.points, c.x, rect.y + 65.0).scale(1.5).color(team_color).center().draw(g, font);
        b.button.draw(g, font);
        g.pop();
    }

    fn draw_game_over(&self, shared: &Shared, g: &mut Graphics) {
        let Some(go) = &self.game_over else { return };
        let font = &shared.font;
        ui::dim_screen(g, (go.age * 2.0).min(1.0) * 0.55);
        g.push();
        let c = vec2(CENTER.x, 180.0);
        g.translate(c.x, c.y);
        g.scale(go.pop.value, go.pop.value);
        g.translate(-c.x, -c.y);
        let rect = Rect::centered(c.x, c.y, 290.0, 150.0);
        ui::panel(g, rect, palette::PANEL);
        let (title, color, line) = if go.won {
            ("VICTORY!", palette::GOLD, "You and Benny win the game!")
        } else {
            ("DEFEAT", palette::RED, "Rosa and Jules take this one.")
        };
        Text::new(title, c.x, rect.y + 29.0)
            .scale(3.5)
            .color(color)
            .wave(shared.time, 2.0)
            .center()
            .draw(g, font);
        Text::new(line, c.x, rect.y + 60.0).center().draw(g, font);
        let s = self.game.scores;
        Text::new(&format!("{}", s[0]), c.x - 40.0, rect.y + 87.0)
            .scale(3.0)
            .color(palette::US)
            .center()
            .draw(g, font);
        Text::new("-", c.x, rect.y + 87.0).scale(2.0).color(palette::MUTED).center().draw(g, font);
        Text::new(&format!("{}", s[1]), c.x + 40.0, rect.y + 87.0)
            .scale(3.0)
            .color(palette::THEM)
            .center()
            .draw(g, font);
        go.again.draw(g, font);
        go.menu.draw(g, font);
        g.pop();
    }
}

/// A rounded outline around a card drawn with the same transform as the card.
fn outline(g: &mut Graphics, pose: Pose, color: Color, width: f32) {
    let (w, h) = (CARD_W, CARD_H);
    g.push();
    g.translate(pose.x, pose.y);
    g.rotate(pose.rot);
    g.set_color(color);
    g.set_line_width(width);
    let (hw, hh) = (w * pose.sx / 2.0, h * pose.sy / 2.0);
    g.rounded_rectangle(Line, -hw - 1.0, -hh - 1.0, hw * 2.0 + 2.0, hh * 2.0 + 2.0, 3.0);
    g.set_line_width(1.0);
    g.pop();
}

/// Pencil tally marks centered on `cx`: groups of four strokes crossed by a
/// fifth, with the rest of the way to `target` sketched in very faintly.
fn tallies(g: &mut Graphics, cx: f32, y: f32, n: u32, target: u32) {
    let group_w = 17.0;
    let groups = target.div_ceil(5) as f32;
    let start = (cx - (groups * group_w - 5.0) / 2.0).round();
    g.set_line_width(1.0);
    for i in 0..target {
        let (grp, k) = ((i / 5) as f32, i % 5);
        let x0 = start + grp * group_w;
        // A little hand-drawn unevenness per stroke.
        let wob = ((i * 7 % 3) as f32 - 1.0) * 0.5;
        let color = if i < n { palette::PENCIL } else { palette::PENCIL_LIGHT.with_alpha(0.14) };
        g.set_color(color);
        if k < 4 {
            let x = x0 + 1.0 + k as f32 * 3.0;
            g.rectangle(Fill, x, y + wob.max(0.0), 1.0, 9.0 - wob.abs());
        } else {
            g.line(x0 - 1.0, y + 7.5, x0 + 12.0, y + 1.5);
        }
    }
}
