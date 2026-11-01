//! Rendering for the table scene. Nothing here changes game state.

use std::f32::consts::PI;

use love2drs::prelude::*;

use super::layout::{self, CENTER, PROMPT_Y};
use super::{HUMAN, NAMES, Place, Prompt, Table};
use crate::cards::{CARD_H, CARD_W, Pose};
use crate::euchre::{Card, Phase, team_of};
use crate::palette;
use crate::scene::Shared;
use crate::text::Text;
use crate::ui::{self, Rect};

const SIDEBAR: Rect = Rect::new(6.0, 6.0, 138.0, 348.0);

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
        self.draw_sidebar(shared, g);
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
            ui::panel(g, Rect::centered(c.x, c.y, 160.0, 150.0), palette::PANEL);
            Text::new("PAUSED", c.x, 116.0).scale(2.0).center().draw(g, &shared.font);
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

    fn draw_sidebar(&self, shared: &Shared, g: &mut Graphics) {
        let font = &shared.font;
        let r = &self.game.round;
        ui::panel(g, SIDEBAR, palette::PANEL);
        let cx = SIDEBAR.center().x;

        Text::new("EUCHRE", cx, 20.0).scale(1.5).color(palette::GOLD).center().draw(g, font);
        let info = format!("HAND {}  ·  TO {}", self.game.hand_number, self.game.target);
        Text::new(&info, cx, 36.0).color(palette::MUTED).center().draw(g, font);

        // Scores.
        for team in 0..2 {
            let x = 12.0 + team as f32 * 66.0;
            let rect = Rect::new(x, 46.0, 60.0, 62.0);
            ui::inset(g, rect, palette::PANEL_DARK);
            g.set_color(team_color(team));
            g.rounded_rectangle(Fill, rect.x, rect.y, rect.w, 13.0, 3.0);
            let label = if team == 0 { "US" } else { "THEM" };
            Text::new(label, rect.center().x, rect.y + 7.0).center().draw(g, font);
            let pop = self.score_pop[team].value;
            Text::new(&self.score_shown[team].to_string(), rect.center().x, rect.y + 37.0)
                .scale(3.0 * pop)
                .center()
                .draw(g, font);
            // Progress pips toward the target score.
            let target = self.game.target.max(1);
            let filled = self.score_shown[team].min(target);
            let pip_w = ((rect.w - 8.0) / target as f32).floor();
            let start = rect.center().x - pip_w * target as f32 / 2.0;
            for i in 0..target {
                let c = if i < filled { team_color(team) } else { palette::PANEL_LIGHT };
                g.set_color(c);
                g.rectangle(Fill, start + i as f32 * pip_w, rect.y + rect.h - 7.0, pip_w - 1.0, 3.0);
            }
        }

        // Trump.
        let trump_rect = Rect::new(12.0, 114.0, 126.0, 82.0);
        ui::inset(g, trump_rect, palette::PANEL_DARK);
        Text::new("TRUMP", trump_rect.center().x, trump_rect.y + 8.0)
            .color(palette::MUTED)
            .center()
            .draw(g, font);
        let tc = trump_rect.center().x;
        match (r.trump, r.maker) {
            (Some(trump), Some(maker)) => {
                let pop = self.trump_pop.value;
                let wob = (shared.time * 2.0).sin() * 0.05;
                let color = crate::cards::suit_color_on_dark(trump);
                shared.cards.draw_suit(g, trump, vec2(tc, trump_rect.y + 36.0), 3.0 * pop, wob, color);
                let who = if maker == HUMAN { "YOU".to_string() } else { NAMES[maker].to_string() };
                Text::new(&format!("CALLED BY {who}"), tc, trump_rect.y + 62.0)
                    .center()
                    .draw(g, font);
                if r.alone {
                    let a = 0.75 + 0.25 * (shared.time * 5.0).sin();
                    Text::new("GOING ALONE", tc, trump_rect.y + 73.0)
                        .color(palette::ORANGE.with_alpha(a))
                        .center()
                        .draw(g, font);
                }
            }
            _ => {
                Text::new("?", tc, trump_rect.y + 34.0)
                    .scale(4.0)
                    .color(palette::DIM)
                    .center()
                    .draw(g, font);
                let line = if r.phase == Phase::NameTrump {
                    format!("{} turned down", r.up_card.suit.symbol())
                } else {
                    "deciding...".to_string()
                };
                Text::new(&line, tc, trump_rect.y + 64.0)
                    .color(palette::MUTED)
                    .center()
                    .draw(g, font);
            }
        }

        // Tricks this hand.
        let tr = Rect::new(12.0, 200.0, 126.0, 52.0);
        ui::inset(g, tr, palette::PANEL_DARK);
        Text::new("TRICKS", tr.center().x, tr.y + 8.0).color(palette::MUTED).center().draw(g, font);
        for team in 0..2 {
            let y = tr.y + 22.0 + team as f32 * 16.0;
            let label = if team == 0 { "US" } else { "THEM" };
            Text::new(label, tr.x + 8.0, y).color(team_color(team)).anchor(0.0, 0.5).draw(g, font);
            let won = r.team_tricks(team);
            let pop = self.trick_pop[team].value;
            for i in 0..5u8 {
                let px = tr.x + 48.0 + i as f32 * 14.0;
                let filled = i < won;
                let s = if filled && i + 1 == won { 10.0 * pop } else { 10.0 };
                g.set_color(palette::INK);
                g.rounded_rectangle(Fill, px - s / 2.0, y - s / 2.0 + 1.0, s, s, 2.0);
                g.set_color(if filled { team_color(team) } else { palette::PANEL_LIGHT });
                g.rounded_rectangle(Fill, px - s / 2.0, y - s / 2.0, s, s - 1.0, 2.0);
            }
        }
        if r.maker.is_some() && r.phase != Phase::NameTrump {
            let need = r.maker.map(team_of).unwrap_or(0);
            let msg = if need == 0 { "We need 3 tricks" } else { "Hold them to 2!" };
            Text::new(msg, tr.center().x, tr.y + tr.h + 8.0).color(palette::MUTED).center().draw(g, font);
        }

        // Dealer and teams.
        let dr = Rect::new(12.0, 272.0, 126.0, 50.0);
        ui::inset(g, dr, palette::PANEL_DARK);
        let dealer = if r.dealer == HUMAN { "YOU".to_string() } else { NAMES[r.dealer].to_string() };
        Text::new("DEALER", dr.x + 8.0, dr.y + 10.0).color(palette::MUTED).anchor(0.0, 0.5).draw(g, font);
        Text::new(&dealer, dr.x + dr.w - 8.0, dr.y + 10.0).color(palette::GOLD).anchor(1.0, 0.5).draw(g, font);
        let cx = dr.center().x;
        Text::new("YOU & BENNY", cx, dr.y + 25.0).color(palette::US).center().draw(g, font);
        Text::new("ROSA & JULES", cx, dr.y + 39.0).color(palette::THEM).center().draw(g, font);

        self.menu_button.draw(g, font);
    }

    fn draw_plates(&self, shared: &Shared, g: &mut Graphics) {
        let font = &shared.font;
        let r = &self.game.round;
        // Light up a seat only when its action is the next real step (not during the deal).
        let next_step = self.cues.iter().find(|c| !matches!(c, super::Cue::Wait(_)));
        let waiting_on = match next_step {
            None | Some(super::Cue::AiAct(_)) => r.to_act(),
            _ => None,
        };
        for (seat, &name) in NAMES.iter().enumerate() {
            let pos = layout::plate_pos(seat);
            let out = r.is_sitting_out(seat);
            let w = font.width(name) + 18.0;
            let rect = Rect::centered(pos.x, pos.y, w, 13.0);
            let active = waiting_on == Some(seat) && self.banner.is_none();
            if active {
                let pulse = 0.55 + 0.45 * (shared.time * 6.0).sin();
                g.set_color(palette::GOLD.with_alpha(pulse));
                g.rounded_rectangle(Fill, rect.x - 2.0, rect.y - 2.0, rect.w + 4.0, rect.h + 4.0, 4.0);
            }
            g.set_color(palette::INK.with_alpha(0.5));
            g.rounded_rectangle(Fill, rect.x + 1.0, rect.y + 2.0, rect.w, rect.h, 3.0);
            g.set_color(palette::PANEL_DARK);
            g.rounded_rectangle(Fill, rect.x, rect.y, rect.w, rect.h, 3.0);
            g.set_color(team_color(team_of(seat)));
            g.rounded_rectangle(Fill, rect.x + 3.0, rect.y + 3.0, 5.0, rect.h - 6.0, 1.0);
            let color = if out { palette::DIM } else { palette::CREAM };
            Text::new(name, rect.x + 12.0, rect.center().y).color(color).anchor(0.0, 0.5).draw(g, font);

            // Badges beside the plate (on the left for the east seat, which hugs the edge).
            let dir = if seat == 3 { -1.0 } else { 1.0 };
            let mut bx = if seat == 3 { rect.x - 8.0 } else { rect.x + rect.w + 8.0 };
            if r.dealer == seat {
                g.set_color(palette::INK);
                g.circle(Fill, bx, rect.center().y + 1.0, 6.0);
                g.set_color(palette::GOLD);
                g.circle(Fill, bx, rect.center().y, 6.0);
                Text::new("D", bx + 0.5, rect.center().y)
                    .color(palette::INK)
                    .no_shadow()
                    .center()
                    .draw(g, font);
                bx += 15.0 * dir;
            }
            if r.maker == Some(seat)
                && let Some(trump) = r.trump {
                    g.set_color(palette::CREAM);
                    g.rounded_rectangle(Fill, bx - 6.0, rect.y, 12.0, 13.0, 2.0);
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
            g.set_color(palette::INK.with_alpha(0.5 * fade));
            g.rounded_rectangle(Fill, rect.x + 1.0, rect.y + 2.0, rect.w, rect.h, 4.0);
            g.set_color(palette::INK.with_alpha(fade));
            g.rounded_rectangle(Fill, rect.x - 1.0, rect.y - 1.0, rect.w + 2.0, rect.h + 2.0, 5.0);
            g.set_color(palette::CREAM.with_alpha(fade));
            g.rounded_rectangle(Fill, rect.x, rect.y, rect.w, rect.h, 4.0);
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
            g.rounded_rectangle(Fill, (CENTER.x + wob - w / 2.0).round(), PROMPT_Y - 7.0, w, 15.0, 4.0);
            Text::new(hint, CENTER.x + wob, PROMPT_Y).color(palette::SUIT_RED_LIGHT).center().draw(g, font);
        } else {
            let text = match self.prompt {
                Prompt::OrderUp => {
                    let up = r.up_card;
                    if r.dealer == HUMAN {
                        format!("Pick up the {} and make {} trump?", up.label(), up.suit.symbol())
                    } else {
                        let who = if r.dealer == 2 { "your partner".to_string() } else { super::titlecase(NAMES[r.dealer]) };
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
                g.set_color(palette::INK.with_alpha(0.55));
                g.rounded_rectangle(Fill, (CENTER.x - w / 2.0).round(), PROMPT_Y - 7.0 + bob, w, 15.0, 4.0);
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
        g.rounded_rectangle(Fill, rect.x + 2.0, rect.y + 3.0, rect.w, rect.h, 5.0);
        g.set_color(palette::GOLD.with_alpha(fade));
        g.rounded_rectangle(Fill, rect.x - 1.0, rect.y - 1.0, rect.w + 2.0, rect.h + 2.0, 6.0);
        g.set_color(palette::PANEL.with_alpha(fade));
        g.rounded_rectangle(Fill, rect.x, rect.y, rect.w, rect.h, 5.0);
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
        g.set_color(palette::INK.with_alpha((b.age * 3.0).min(1.0) * 0.35));
        g.rectangle(Fill, 150.0, 0.0, 490.0, 360.0);
        g.push();
        let c = CENTER + vec2(0.0, 8.0);
        g.translate(c.x, c.y);
        g.scale(b.pop.value, b.pop.value);
        g.translate(-c.x, -c.y);
        // Grow to fit the widest line (the detail names the maker and can run long).
        let text_w = (font.width(b.title) * 2.5).max(font.width(&b.detail)).max(font.width(&b.points) * 1.5);
        let rect = Rect::centered(c.x, c.y, (text_w + 32.0).clamp(260.0, 470.0), 112.0);
        ui::panel(g, rect, palette::PANEL);
        g.set_color(b.color);
        g.rounded_rectangle(Fill, rect.x, rect.y, rect.w, 4.0, 2.0);
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
