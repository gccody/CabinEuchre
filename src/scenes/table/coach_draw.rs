//! How Benny looks: the paper note with his portrait, the chapter cards between
//! lessons, the spotlight, the pointing glove and the report at the end.

use love2drs::prelude::*;

use crate::device;
use crate::euchre::tutorial::LESSONS;
use crate::palette;
use crate::scene::{InputMode, Shared};
use crate::text::{PixelFont, Text};
use crate::ui::{self, Rect};

use super::Table;
use super::coach::{Chapter, Coach, Level, NOTE_TEXT_W, NOTE_W, Note};
use super::layout;

/// Between lines on a note.
const LINE: f32 = 13.0;
/// How far the portrait's frame sits in from the note's left edge (it pokes out a little).
const FRAME: f32 = 38.0;

impl Table {
    /// The spotlight, Benny's note and the glove, over the table.
    pub(super) fn draw_coach(&self, shared: &Shared, g: &mut Graphics) {
        let Some(c) = &self.coach else { return };
        if c.dim > 0.01 {
            let alpha = c.dim * if c.chapter.is_some() { 0.5 } else { 0.45 };
            ui::dim_around(g, alpha, &c.holes);
            let pulse = 0.65 + 0.35 * (shared.time * 5.0).sin();
            for h in &c.holes {
                if h.w > 2.0 {
                    bracket(g, *h, palette::GOLD.with_alpha(pulse * c.dim));
                }
            }
        }
        if c.level == Level::Hint {
            c.hint_button.draw(g, &shared.font);
        }
        if let Some(n) = &c.note {
            draw_note(c, n, shared, g);
        }
        if let Some(ch) = &c.chapter {
            draw_chapter(ch, c, shared, g);
        }
        if c.glove_alpha > 0.01 {
            draw_glove(c, shared, g);
        }
    }

    /// The end of the match: how it finished and Benny's notes on how you played.
    pub(super) fn draw_report(&self, shared: &Shared, g: &mut Graphics) {
        let (Some(go), Some(c)) = (&self.game_over, &self.coach) else { return };
        let font = &shared.font;
        ui::dim_screen(g, (go.age * 2.0).min(1.0) * 0.55);
        g.push();
        let cx = layout::panel_x();
        let rect = Rect::centered(cx, 180.0, layout::panel_max_width(360.0), if device::mobile() { 240.0 } else { 214.0 });
        let center = rect.center();
        g.translate(center.x, center.y);
        g.scale(go.pop.value, go.pop.value);
        g.translate(-center.x, -center.y);
        ui::panel(g, rect, palette::PANEL);

        let (title, color, quote) = if go.won {
            ("WELL PLAYED, PARTNER!", palette::GOLD, "You're ready for\na real table.\nDeal me in anytime!")
        } else {
            ("GOOD GAME, PARTNER!", palette::BRASS_LIGHT, "They got us, but you\nknow the game now.\nRematch?")
        };
        Text::new(title, rect.center().x, rect.y + 22.0).scale(if rect.w < 340.0 { 1.5 } else { 2.0 }).color(color).wave(shared.time, 1.0).center().draw(g, font);
        let s = self.game.scores;
        Text::new("WE", rect.center().x - 62.0, rect.y + 46.0).color(palette::US).center().draw(g, font);
        Text::new(&s[0].to_string(), rect.center().x - 30.0, rect.y + 46.0).scale(2.0).color(palette::US).center().draw(g, font);
        Text::new("-", rect.center().x, rect.y + 46.0).color(palette::MUTED).center().draw(g, font);
        Text::new(&s[1].to_string(), rect.center().x + 30.0, rect.y + 46.0).scale(2.0).color(palette::THEM).center().draw(g, font);
        Text::new("THEY", rect.center().x + 68.0, rect.y + 46.0).color(palette::THEM).center().draw(g, font);

        // Benny's notes on a sheet of the score pad.
        // Beside a fold there's only room for the notes, in the middle.
        let room_for_benny = rect.w >= 350.0;
        let paper_x = if room_for_benny { rect.x + 10.0 } else { rect.center().x - 100.0 };
        let paper = Rect::new(paper_x, rect.y + 60.0, 200.0, 96.0);
        g.set_color(palette::INK.with_alpha(0.45));
        g.rectangle(Fill, paper.x + 3.0, paper.y + 4.0, paper.w, paper.h);
        g.set_color(palette::INK);
        g.rectangle(Fill, paper.x - 1.0, paper.y - 1.0, paper.w + 2.0, paper.h + 2.0);
        g.set_color(palette::PAPER);
        g.rectangle(Fill, paper.x, paper.y, paper.w, paper.h);
        g.set_color(palette::RED_DARK);
        g.rectangle(Fill, paper.x, paper.y, paper.w, 5.0);
        g.set_color(palette::RED);
        g.rectangle(Fill, paper.x, paper.y, paper.w, 1.0);
        Text::new("BENNY'S NOTES", paper.x + 8.0, paper.y + 15.0).color(palette::US_INK).no_shadow().anchor(0.0, 0.5).draw(g, font);
        let st = &c.stats;
        let rows = [
            ("Following suit", st.refused == 0),
            ("Trump and bowers", st.quiz_wrong == 0),
            ("Calling trump", st.bid_wrong == 0 && !st.passed_monster),
            ("Helping your partner", st.trumped_partner == 0),
        ];
        for (i, (label, good)) in rows.iter().enumerate() {
            let y = paper.y + 30.0 + i as f32 * 15.0;
            g.set_color(palette::PAPER_RULE.with_alpha(0.7));
            g.rectangle(Fill, paper.x + 3.0, y + 7.0, paper.w - 6.0, 1.0);
            if *good {
                check(g, paper.x + 12.0, y, palette::US_INK);
            } else {
                Text::new("*", paper.x + 12.0, y).color(palette::THEM_INK).no_shadow().center().draw(g, font);
            }
            Text::new(label, paper.x + 24.0, y).color(palette::PENCIL).no_shadow().anchor(0.0, 0.5).draw(g, font);
        }

        // Benny himself.
        if room_for_benny {
            let face = vec2(rect.x + rect.w - 75.0, paper.y + 22.0);
            draw_portrait(shared, g, face, false, 1.1);
            for (i, line) in quote.lines().enumerate() {
                Text::new(line, face.x, paper.y + 56.0 + i as f32 * LINE).color(palette::CREAM).center().draw(g, font);
            }
        }
        go.again.draw(g, font);
        go.menu.draw(g, font);
        g.pop();
    }
}

/// Word-wraps a note's text, returning the lines.
fn note_lines(n: &Note, font: &PixelFont) -> Vec<String> {
    let extra = layout::panel_max_width(NOTE_W) - NOTE_W;
    font.wrap(&n.text, NOTE_TEXT_W + extra)
}

fn draw_note(c: &Coach, n: &Note, shared: &Shared, g: &mut Graphics) {
    let font = &shared.font;
    let lines = note_lines(n, font);
    let w = layout::panel_max_width(NOTE_W);
    let h = (lines.len() as f32 * LINE + 19.0).max(47.0);
    let top = c.note_y.value - h;
    let shake = (n.shake * n.shake) * (n.age * 44.0).sin() * 5.0;
    let x = (layout::note_x() - w / 2.0 + FRAME / 4.0).round();
    let rect = Rect::new(x + shake, top.round(), w, h);
    let alpha = n.fade;
    let center = rect.center();
    g.push();
    g.translate(center.x, center.y);
    g.rotate(-0.008);
    let s = n.pop.value * (0.94 + 0.06 * alpha);
    g.scale(s, s);
    g.translate(-center.x, -center.y);

    // Shadow, outline, sheet, ruled lines and the gummed strip of the score pad.
    g.set_color(palette::INK.with_alpha(0.45 * alpha));
    g.rectangle(Fill, rect.x + 3.0, rect.y + 4.0, rect.w, rect.h);
    g.set_color(palette::INK.with_alpha(alpha));
    g.rectangle(Fill, rect.x - 1.0, rect.y - 1.0, rect.w + 2.0, rect.h + 2.0);
    g.set_color(palette::PAPER.with_alpha(alpha));
    g.rectangle(Fill, rect.x, rect.y, rect.w, rect.h);
    g.set_color(palette::PAPER_RULE.with_alpha(0.55 * alpha));
    let mut y = rect.y + 18.0;
    while y < rect.y + rect.h - 3.0 {
        g.rectangle(Fill, rect.x + 2.0, y, rect.w - 4.0, 1.0);
        y += LINE;
    }
    g.set_color(palette::RED_DARK.with_alpha(alpha));
    g.rectangle(Fill, rect.x, rect.y, rect.w, 5.0);
    g.set_color(palette::RED.with_alpha(alpha));
    g.rectangle(Fill, rect.x, rect.y, rect.w, 1.0);

    // The words, as many of them as he's said.
    let mut left = n.shown as usize;
    let mut loud_index = 0;
    let text_x = rect.x + FRAME - 2.0;
    let text_top = rect.y + 13.0 + ((rect.h - 13.0 - LINE * lines.len() as f32) / 2.0).max(0.0).floor();
    for (i, line) in lines.iter().enumerate() {
        let take = left.min(line.chars().count());
        left -= take;
        let y = text_top + i as f32 * LINE;
        draw_line(g, font, line, take, text_x, y, &n.loud, &mut loud_index, alpha);
        left = left.saturating_sub(1); // The space the wrap took out.
    }

    // Benny, looking pleased with himself. His mouth moves while he's talking.
    let talking = !n.closing && (n.shown as usize) < n.text.chars().count();
    let mouth_open = talking && (shared.time * 11.0).sin() > 0.0;
    let bob = (shared.time * 2.2).sin() * 0.6;
    let face = vec2(rect.x + 8.0, rect.y + rect.h / 2.0 + 1.0 + bob);
    if alpha > 0.5 {
        draw_portrait(shared, g, face, mouth_open, 1.0);
    }

    // NEXT, on a brass tab hanging off the bottom corner.
    if n.next && !n.closing && n.shown as usize >= n.text.chars().count() {
        let label = match shared.input {
            InputMode::Touch => "TAP",
            _ => "NEXT",
        };
        let tw = font.width(label) + font.width(" >") + 12.0;
        let bounce = ((shared.time * 5.0).sin() * 1.2).round();
        let tab = Rect::new(rect.x + rect.w - tw - 8.0, rect.y + rect.h - 2.0 + bounce, tw, 12.0);
        g.set_color(palette::INK);
        ui::chamfer(g, Fill, tab.x - 1.0, tab.y - 1.0, tab.w + 2.0, tab.h + 2.0, 3.0);
        g.set_color(palette::BRASS);
        ui::chamfer(g, Fill, tab.x, tab.y, tab.w, tab.h, 2.0);
        g.set_color(palette::BRASS_LIGHT);
        g.rectangle(Fill, tab.x + 2.0, tab.y, tab.w - 4.0, 1.0);
        Text::new(&format!("{label} >"), tab.center().x, tab.center().y + 0.5)
            .color(palette::INK)
            .shadow(palette::BRASS_LIGHT.with_alpha(0.5))
            .center()
            .draw(g, font);
    }
    g.pop();
}

/// One line of a note: `take` characters of it, in pencil with the emphasized words in
/// ink and red suits red. `loud` has an entry per non-space character of the whole note.
#[allow(clippy::too_many_arguments)]
fn draw_line(g: &mut Graphics, font: &PixelFont, line: &str, take: usize, x: f32, y: f32, loud: &[bool], index: &mut usize, alpha: f32) {
    let color_of = |ch: char, is_loud: bool| match ch {
        '♥' | '♦' => palette::SUIT_RED,
        _ if is_loud => palette::US_INK,
        _ => palette::PENCIL,
    };
    let chars: Vec<char> = line.chars().collect();
    let mut start = 0;
    let mut i = 0;
    let mut run_color = None;
    let flush = |g: &mut Graphics, from: usize, to: usize, color: Color| {
        if from >= to {
            return;
        }
        let run: String = chars[from..to].iter().collect();
        let before: String = chars[..from].iter().collect();
        let dx = if before.is_empty() { 0.0 } else { font.line_width(&before) + crate::text::UNIT };
        Text::new(&run, x + dx, y).color(color.with_alpha(alpha)).no_shadow().plain_suits().anchor(0.0, 0.5).draw(g, font);
    };
    while i < chars.len() {
        let ch = chars[i];
        let is_loud = ch != ' ' && loud.get(*index).copied().unwrap_or(false);
        if ch != ' ' {
            *index += 1;
        }
        if ch != ' ' {
            let color = color_of(ch, is_loud);
            if run_color != Some(color) {
                if let Some(old) = run_color {
                    flush(g, start, i.min(take), old);
                }
                start = i;
                run_color = Some(color);
            }
        }
        i += 1;
    }
    if let Some(old) = run_color {
        flush(g, start, chars.len().min(take), old);
    }
}

fn draw_portrait(shared: &Shared, g: &mut Graphics, at: Vec2, mouth_open: bool, scale: f32) {
    // A dark frame with a brass rim, like the trump medallion's.
    let half = 19.0 * scale;
    let (x, y, w) = (at.x - half, at.y - half, half * 2.0);
    g.set_color(palette::INK.with_alpha(0.45));
    ui::chamfer(g, Fill, x + 2.0, y + 3.0, w, w, 5.0);
    g.set_color(palette::INK);
    ui::chamfer(g, Fill, x - 1.0, y - 1.0, w + 2.0, w + 2.0, 6.0);
    g.set_color(palette::BRASS_DARK);
    ui::chamfer(g, Fill, x, y, w, w, 5.0);
    g.set_color(palette::BRASS);
    ui::chamfer(g, Fill, x, y - 0.5, w, w - 0.5, 5.0);
    g.set_color(palette::FELT);
    ui::chamfer(g, Fill, x + 2.0, y + 2.0, w - 4.0, w - 4.0, 3.0);
    shared.cards.draw_portrait(g, vec2(at.x, at.y + 0.5 * scale), mouth_open, scale);
}

fn draw_chapter(ch: &Chapter, c: &Coach, shared: &Shared, g: &mut Graphics) {
    let font = &shared.font;
    let cx = layout::panel_x();
    let lines = font.wrap(&ch.line, layout::panel_max_width(320.0) - 28.0);
    let h = 92.0 + lines.len() as f32 * LINE;
    let w = layout::panel_max_width(348.0);
    let rect = Rect::centered(cx, layout::CENTER.y + 6.0, w, h);
    g.push();
    let center = rect.center();
    g.translate(center.x, center.y);
    g.scale(ch.pop.value, ch.pop.value);
    g.translate(-center.x, -center.y);
    ui::panel(g, rect, palette::PANEL);
    Text::new(&ch.tag, cx, rect.y + 18.0).color(palette::MUTED).center().draw(g, font);
    let title_scale = if font.width(&ch.title) * 2.5 > w - 24.0 { 2.0 } else { 2.5 };
    Text::new(&ch.title, cx, rect.y + 37.0).scale(title_scale).color(palette::GOLD).wave(shared.time, 1.0).center().draw(g, font);
    for (i, line) in lines.iter().enumerate() {
        Text::new(line, cx, rect.y + 60.0 + i as f32 * LINE).center().draw(g, font);
    }
    // Progress: one brass pip per lesson, lit up to this one.
    let y = rect.y + rect.h - 18.0;
    for i in 0..LESSONS {
        let x = cx + (i as f32 - (LESSONS as f32 - 1.0) / 2.0) * 16.0 - 18.0;
        let lit = i <= c.lesson;
        g.set_color(palette::INK);
        g.circle(Fill, x, y, 5.0);
        g.set_color(if lit { palette::BRASS_DARK } else { palette::PANEL_DARK });
        g.circle(Fill, x, y, 4.0);
        if lit {
            g.set_color(if i == c.lesson { palette::BRASS_LIGHT } else { palette::BRASS });
            g.circle(Fill, x, y - 0.5, 3.0);
        }
    }
    if ch.age > 0.25 {
        let label = if shared.input == InputMode::Touch { "TAP >" } else { "NEXT >" };
        let nudge = ((shared.time * 5.0).sin() * 1.5).round();
        Text::new(label, rect.x + rect.w - 16.0 + nudge, y).color(palette::BRASS_LIGHT).anchor(1.0, 0.5).draw(g, font);
    }
    g.pop();
}

/// The pointing glove, tapping away at whatever it's on.
fn draw_glove(c: &Coach, shared: &Shared, g: &mut Graphics) {
    let tap = (shared.time * 3.6).sin();
    let dip = tap.max(0.0) * 4.0;
    let pos = vec2(c.glove.0.value, c.glove.1.value + 4.0 - dip);
    shared.cards.draw_pointer(g, pos, -0.12, c.glove_alpha);
}

/// Gold corner brackets around a spotlight's hole.
fn bracket(g: &mut Graphics, r: Rect, color: Color) {
    g.set_color(color);
    let l = (r.w.min(r.h) * 0.3).clamp(3.0, 8.0);
    let t = 2.0;
    for (x, y, dx, dy) in [
        (r.x, r.y, 1.0, 1.0),
        (r.x + r.w, r.y, -1.0, 1.0),
        (r.x, r.y + r.h, 1.0, -1.0),
        (r.x + r.w, r.y + r.h, -1.0, -1.0),
    ] {
        let hx = if dx > 0.0 { x - t } else { x + t - (l + t) };
        let vy = if dy > 0.0 { y - t } else { y + t - (l + t) };
        g.rectangle(Fill, hx, if dy > 0.0 { y - t } else { y }, l + t, t);
        g.rectangle(Fill, if dx > 0.0 { x - t } else { x }, vy, t, l + t);
    }
}

/// A pencil check mark centered around `(x, y)`.
fn check(g: &mut Graphics, x: f32, y: f32, color: Color) {
    g.set_color(color);
    g.set_line_width(2.0);
    g.line(x - 4.0, y, x - 1.0, y + 3.0);
    g.line(x - 1.0, y + 3.0, x + 5.0, y - 4.0);
    g.set_line_width(1.0);
}
