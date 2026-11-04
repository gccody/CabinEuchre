//! The trophy shelf: your lifetime numbers on the first page, then every
//! trophy with how to earn it and how far along you are. Also the little banner
//! that drops in when one is earned, wherever you are in the game.

use std::collections::VecDeque;

use love2drs::prelude::*;

use crate::device;
use crate::euchre::record::{Achievement, Record, Stats};
use crate::juice::Spring;
use crate::palette;
use crate::platform;
use crate::scene::{Nav, Shared};
use crate::sfx::Sound;
use crate::text::Text;
use crate::ui::{self, Button, Rect};

const PANEL: Rect = Rect::new(120.0, 34.0, 400.0, 292.0);
/// On a phone the panel fills the screen, so rows can be tall enough to read easily.
const MOBILE_PANEL: Rect = Rect::new(14.0, 8.0, 612.0, 344.0);
/// Trophies on a page, and the height of each row.
const ROWS: usize = 7;
const ROW_H: f32 = 30.0;
const MOBILE_ROWS: usize = 5;
const MOBILE_ROW_H: f32 = 46.0;
/// The numbers page on a phone: where the rows start and how far apart they are,
/// closer together when the sign-in button needs the space under them.
const MOBILE_NUMBERS_TOP: f32 = 34.0;
const MOBILE_NUMBERS_ROW_H: f32 = 23.0;
const MOBILE_NUMBERS_ROW_H_SIGN_IN: f32 = 19.5;
const MOBILE_SIGN_IN: Rect = Rect::new(170.0, 252.0, 300.0, 38.0);

fn panel() -> Rect {
    if device::mobile() { MOBILE_PANEL } else { PANEL }
}

fn rows_per_page() -> usize {
    if device::mobile() { MOBILE_ROWS } else { ROWS }
}

/// The numbers page, then enough pages for every trophy.
fn page_count() -> usize {
    1 + Achievement::ALL.len().div_ceil(rows_per_page())
}

fn percent(part: u32, whole: u32) -> String {
    if whole == 0 { "-".to_string() } else { format!("{}%", (part as f32 / whole as f32 * 100.0).round() as u32) }
}

/// The numbers page, in two columns.
fn stat_lines(s: &Stats) -> [(&'static str, String); 18] {
    [
        ("GAMES PLAYED", s.games.to_string()),
        ("GAMES WON", s.wins.to_string()),
        ("WIN RATE", percent(s.wins, s.games)),
        ("WIN STREAK", s.streak.to_string()),
        ("BEST STREAK", s.best_streak.to_string()),
        ("ONLINE GAMES", s.online_games.to_string()),
        ("ONLINE WINS", s.online_wins.to_string()),
        ("HANDS PLAYED", s.hands.to_string()),
        ("TRICKS TAKEN", s.tricks.to_string()),
        ("TRUMP CALLS", s.calls.to_string()),
        ("CALLS MADE", percent(s.calls_made, s.calls)),
        ("EUCHRES DEALT", s.euchres.to_string()),
        ("TIMES EUCHRED", s.euchred.to_string()),
        ("MARCHES", s.marches.to_string()),
        ("LONE HANDS", s.lones.to_string()),
        ("LONE MARCHES", s.lone_marches.to_string()),
        ("POINTS FOR", s.points_for.to_string()),
        ("POINTS AGAINST", s.points_against.to_string()),
    ]
}

/// A round medal: brass with a ribbon when earned, a dark empty socket when not.
fn draw_medal(g: &mut Graphics, shared: &Shared, c: Vec2, a: Achievement, earned: bool, scale: f32) {
    let symbol = ["♣", "♦", "♠", "♥"][Achievement::ALL.iter().position(|&x| x == a).unwrap_or(0) % 4];
    let r = 9.0 * scale;
    if earned {
        g.set_color(palette::THEM);
        g.polygon(Fill, &[c.x - r * 0.75, c.y, c.x - r * 0.15, c.y, c.x - r * 0.45, c.y + r * 1.35]);
        g.polygon(Fill, &[c.x + r * 0.15, c.y, c.x + r * 0.75, c.y, c.x + r * 0.45, c.y + r * 1.35]);
    }
    g.set_color(palette::INK);
    g.circle(Fill, c.x, c.y, r + 1.0);
    g.set_color(if earned { palette::BRASS_DARK } else { palette::PANEL_LIGHT });
    g.circle(Fill, c.x, c.y, r);
    g.set_color(if earned { palette::BRASS } else { palette::PANEL });
    g.circle(Fill, c.x, c.y - 0.5, r - 1.5);
    let color = if earned { palette::INK } else { palette::PANEL_LIGHT };
    Text::new(symbol, c.x, c.y).scale(scale).color(color).no_shadow().center().draw(g, &shared.font);
}

pub struct TrophyPanel {
    page: usize,
    prev: Button,
    next: Button,
    close: Button,
    /// Android: sign in to Play Games, on the numbers page.
    sign_in: Button,
    /// Gamepad focus is on the sign-in button.
    on_sign_in: bool,
    pop: Spring,
    page_pop: Spring,
}

impl TrophyPanel {
    pub fn new() -> Self {
        let mut pop = Spring::new(0.9);
        pop.target = 1.0;
        let (prev, next, close, sign_in) = if device::mobile() {
            let (p, y) = (MOBILE_PANEL, MOBILE_PANEL.y + MOBILE_PANEL.h - 30.0);
            (
                Button::grey("<", Rect::centered(p.x + 56.0, y, 76.0, 40.0)).scaled(1.5),
                Button::blue(">", Rect::centered(p.x + p.w - 56.0, y, 76.0, 40.0)).scaled(1.5),
                Button::brass("CLOSE", Rect::centered(p.center().x, y, 150.0, 40.0)).scaled(1.5),
                Button::green("SIGN IN TO PLAY GAMES", MOBILE_SIGN_IN).scaled(1.2),
            )
        } else {
            let y = PANEL.y + PANEL.h - 22.0;
            (
                Button::grey("<", Rect::centered(PANEL.x + 30.0, y, 30.0, 18.0)),
                Button::blue(">", Rect::centered(PANEL.x + PANEL.w - 30.0, y, 30.0, 18.0)),
                Button::brass("CLOSE", Rect::centered(PANEL.center().x, y, 76.0, 18.0)),
                Button::green("SIGN IN TO PLAY GAMES", Rect::centered(PANEL.center().x, PANEL.y + 244.0, 190.0, 18.0)),
            )
        };
        TrophyPanel { page: 0, prev, next, close, sign_in, on_sign_in: false, pop, page_pop: Spring::new(1.0) }
    }

    fn can_sign_in(&self) -> bool {
        self.page == 0 && platform::can_sign_in()
    }

    pub fn update(&mut self, shared: &mut Shared, dt: f32) {
        self.pop.update(dt);
        self.page_pop.update(dt);
        self.prev.enabled = self.page > 0;
        self.next.enabled = self.page + 1 < page_count();
        self.on_sign_in &= self.can_sign_in();
        self.sign_in.focused = self.on_sign_in && shared.focus_visible();
        for b in [&mut self.prev, &mut self.next, &mut self.close] {
            b.update(shared.mouse, dt, &shared.sfx);
        }
        if self.can_sign_in() {
            self.sign_in.update(shared.mouse, dt, &shared.sfx);
        }
    }

    fn turn(&mut self, shared: &mut Shared, dir: i32) {
        let p = (self.page as i32 + dir).clamp(0, page_count() as i32 - 1) as usize;
        if p != self.page {
            self.page = p;
            self.page_pop.value = 0.96;
            shared.sfx.play(Sound::Flip);
            shared.tick();
        }
    }

    /// Returns true when the panel should close.
    pub fn mouse_pressed(&mut self, shared: &mut Shared, pos: Vec2) -> bool {
        if self.close.click(pos, &shared.sfx) {
            return true;
        }
        if self.can_sign_in() && self.sign_in.click(pos, &shared.sfx) {
            shared.tap();
            platform::sign_in();
            return false;
        }
        if self.prev.click(pos, &shared.sfx) {
            self.turn(shared, -1);
        }
        if self.next.click(pos, &shared.sfx) {
            self.turn(shared, 1);
        }
        false
    }

    /// Returns true when the panel should close.
    pub fn nav(&mut self, shared: &mut Shared, nav: Nav) -> bool {
        match nav {
            Nav::Left | Nav::PrevPage => self.turn(shared, -1),
            Nav::Right | Nav::NextPage => self.turn(shared, 1),
            Nav::Up | Nav::Down if self.can_sign_in() => {
                self.on_sign_in = !self.on_sign_in;
                shared.tick();
            }
            Nav::Confirm if self.on_sign_in => {
                self.sign_in.flash(&shared.sfx);
                shared.tap();
                platform::sign_in();
            }
            Nav::Confirm => {
                shared.tap();
                if self.page + 1 == page_count() {
                    self.close.flash(&shared.sfx);
                    return true;
                }
                self.next.flash(&shared.sfx);
                self.turn(shared, 1);
            }
            Nav::Back | Nav::Menu => {
                self.close.flash(&shared.sfx);
                shared.tap();
                return true;
            }
            Nav::Up | Nav::Down | Nav::Alt => {}
        }
        false
    }

    pub fn key_pressed(&mut self, shared: &mut Shared, key: Key) -> bool {
        match key {
            Key::Escape => {
                self.close.flash(&shared.sfx);
                true
            }
            Key::A => {
                self.turn(shared, -1);
                false
            }
            Key::D => {
                self.turn(shared, 1);
                false
            }
            _ => false,
        }
    }

    pub fn draw(&self, shared: &Shared, g: &mut Graphics) {
        let font = &shared.font;
        let record = &shared.record.data;
        ui::dim_screen(g, 0.55);
        g.push();
        let panel = panel();
        let c = panel.center();
        g.translate(c.x, c.y);
        g.scale(self.pop.value, self.pop.value);
        g.translate(-c.x, -c.y);

        ui::panel(g, panel, palette::PANEL);
        Text::new("TROPHY SHELF", c.x, panel.y + 12.0).color(palette::MUTED).center().draw(g, font);
        let title = if self.page == 0 { "YOUR RECORD" } else { "TROPHIES" };
        Text::new(title, c.x, panel.y + 30.0).scale(2.0 * self.page_pop.value).color(palette::GOLD).center().draw(g, font);

        let footer = if device::mobile() { 104.0 } else { 80.0 };
        let body = Rect::new(panel.x + 12.0, panel.y + 46.0, panel.w - 24.0, panel.h - footer);
        if self.page == 0 {
            self.draw_numbers(shared, g, body);
        } else {
            self.draw_trophies(shared, g, body, record);
        }

        let (count_x, count_y) = if device::mobile() {
            (panel.x + 106.0, panel.y + panel.h - 30.0)
        } else {
            (panel.x + 72.0, panel.y + panel.h - 22.0)
        };
        Text::new(&format!("{} / {}", self.page + 1, page_count()), count_x, count_y)
            .color(palette::MUTED)
            .anchor(0.0, 0.5)
            .draw(g, font);
        self.prev.draw(g, font);
        self.next.draw(g, font);
        self.close.draw(g, font);
        g.pop();
    }

    fn draw_numbers(&self, shared: &Shared, g: &mut Graphics, body: Rect) {
        let font = &shared.font;
        let record = &shared.record.data;
        ui::inset(g, body, palette::PANEL_DARK);
        let row_h = match (device::mobile(), self.can_sign_in()) {
            (true, true) => MOBILE_NUMBERS_ROW_H_SIGN_IN,
            (true, false) => MOBILE_NUMBERS_ROW_H,
            (false, _) => 18.0,
        };
        let head_h = if device::mobile() { MOBILE_NUMBERS_TOP - 8.0 } else { 24.0 };
        let earned = format!("{} OF {} TROPHIES", record.unlocked.len(), Achievement::ALL.len());
        Text::new(&earned, body.center().x, body.y + head_h / 2.0 + 1.0).color(palette::BRASS_LIGHT).center().draw(g, font);
        let lines = stat_lines(&record.stats);
        let per_column = lines.len().div_ceil(2);
        let column_w = (body.w - 36.0) / 2.0;
        for (i, (label, value)) in lines.iter().enumerate() {
            let (column, row) = (i / per_column, i % per_column);
            let x = body.x + 12.0 + column as f32 * (column_w + 12.0);
            let y = body.y + head_h + 8.0 + row as f32 * row_h;
            Text::new(label, x, y).color(palette::MUTED).anchor(0.0, 0.5).draw(g, font);
            Text::new(value, x + column_w, y).anchor(1.0, 0.5).draw(g, font);
        }
        if self.can_sign_in() {
            self.sign_in.draw(g, font);
        } else if let Some(store) = platform::store_name() {
            let line = format!("TROPHIES SYNC WITH {store}");
            Text::new(&line, body.center().x, body.y + body.h - 9.0).color(palette::MUTED).center().draw(g, font);
        }
    }

    fn draw_trophies(&self, shared: &Shared, g: &mut Graphics, body: Rect, record: &Record) {
        let font = &shared.font;
        let mobile = device::mobile();
        let (per_page, row_h) = if mobile { (MOBILE_ROWS, MOBILE_ROW_H) } else { (ROWS, ROW_H) };
        let start = (self.page - 1) * per_page;
        for (i, &a) in Achievement::ALL.iter().skip(start).take(per_page).enumerate() {
            let earned = record.is_unlocked(a);
            let row = Rect::new(body.x, body.y + i as f32 * row_h, body.w, row_h - 2.0);
            ui::inset(g, row, if earned { palette::PANEL_LIGHT } else { palette::PANEL_DARK });
            let mid = row.center().y;
            draw_medal(g, shared, vec2(row.x + 18.0, mid - 1.0), a, earned, if mobile { 1.4 } else { 1.0 });
            let text_x = row.x + if mobile { 44.0 } else { 36.0 };
            let title = if earned { palette::GOLD } else { palette::CREAM };
            let how = if earned { palette::CREAM_SHADE } else { palette::MUTED };
            let (title_y, how_y) = if mobile { (mid - 9.0, mid + 9.0) } else { (mid - 6.0, mid + 6.0) };
            Text::new(a.title(), text_x, title_y).color(title).anchor(0.0, 0.5).draw(g, font);
            Text::new(a.how(), text_x, how_y).color(how).anchor(0.0, 0.5).draw(g, font);
            if !earned && a.goal() > 1 {
                self.draw_progress(shared, g, row, a, record);
            }
        }
    }

    /// `12/50` with a small bar under it, at the right of a counting trophy's row.
    fn draw_progress(&self, shared: &Shared, g: &mut Graphics, row: Rect, a: Achievement, record: &Record) {
        let done = a.progress(&record.stats).min(a.goal());
        let right = row.x + row.w - 8.0;
        let y = row.center().y - if device::mobile() { 9.0 } else { 6.0 };
        Text::new(&format!("{done}/{}", a.goal()), right, y).color(palette::BRASS_LIGHT).anchor(1.0, 0.5).draw(g, &shared.font);
        let (w, bar_y) = (44.0, y + 7.0);
        g.set_color(palette::INK);
        g.rectangle(Fill, right - w, bar_y, w, 3.0);
        g.set_color(palette::BRASS);
        g.rectangle(Fill, right - w, bar_y, w * done as f32 / a.goal() as f32, 3.0);
    }
}

/// How long each banner stays down.
const BANNER_SECONDS: f32 = 3.6;
/// How long it takes to slide back out at the end.
const BANNER_OUT: f32 = 0.3;

/// Banners for trophies just earned, one at a time. Drawn over everything, so it
/// shows wherever the player is, and never takes input.
pub struct TrophyBanners {
    waiting: VecDeque<Achievement>,
    showing: Option<(Achievement, f32)>,
    drop: Spring,
}

impl TrophyBanners {
    pub fn new() -> Self {
        TrophyBanners { waiting: VecDeque::new(), showing: None, drop: Spring::new(0.0) }
    }

    pub fn push(&mut self, list: impl IntoIterator<Item = Achievement>) {
        self.waiting.extend(list);
    }

    pub fn update(&mut self, shared: &mut Shared, dt: f32) {
        self.drop.update(dt);
        if let Some((_, age)) = &mut self.showing {
            *age += dt;
            if *age > BANNER_SECONDS - BANNER_OUT {
                self.drop.target = 0.0;
            }
            if *age > BANNER_SECONDS {
                self.showing = None;
            }
        }
        if self.showing.is_none()
            && let Some(next) = self.waiting.pop_front()
        {
            self.showing = Some((next, 0.0));
            self.drop.value = 0.0;
            self.drop.target = 1.0;
            shared.sfx.play(Sound::Trophy);
            shared.buzz(0.0, 0.25, 0.45, 0.12);
        }
    }

    pub fn draw(&self, shared: &Shared, g: &mut Graphics) {
        let Some((a, _)) = self.showing else { return };
        let font = &shared.font;
        let w = (font.width(a.title()) + 62.0).max(190.0);
        let h = 38.0;
        let top = crate::render::bounds().y + 6.0;
        let y = top - (h + 14.0) * (1.0 - self.drop.value);
        let rect = Rect::new((crate::render::VIEW_W as f32 - w) / 2.0, y, w, h);
        ui::panel(g, rect, palette::PANEL);
        draw_medal(g, shared, vec2(rect.x + 24.0, rect.center().y - 1.0), a, true, 1.2);
        let x = rect.x + 44.0;
        Text::new("TROPHY EARNED", x, rect.y + 12.0).color(palette::MUTED).anchor(0.0, 0.5).draw(g, font);
        Text::new(a.title(), x, rect.y + 26.0).color(palette::GOLD).anchor(0.0, 0.5).draw(g, font);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::measure;

    /// Room for a line of text in a row, on the smaller of the two panels.
    const TEXT_ROOM: f32 = PANEL.w - 24.0 - 36.0 - 8.0;

    #[test]
    fn every_trophy_fits_its_row_with_room_for_the_counter() {
        for a in Achievement::ALL {
            assert!(measure(a.how()) <= TEXT_ROOM, "{} is too wide", a.how());
            let counter = if a.goal() > 1 { 64.0 } else { 0.0 };
            assert!(measure(a.title()) + counter <= TEXT_ROOM, "{} is too wide", a.title());
        }
    }

    #[test]
    fn every_stat_fits_its_column() {
        let room = (PANEL.w - 24.0 - 36.0) / 2.0 - 12.0;
        let s = Stats { games: 99_999, points_for: 999_999, points_against: 999_999, ..Stats::default() };
        for (label, value) in stat_lines(&s) {
            assert!(measure(label) + measure(&value) <= room, "{label} doesn't fit");
        }
    }

    #[test]
    fn every_trophy_has_a_page() {
        assert!((page_count() - 1) * ROWS >= Achievement::ALL.len());
        assert!(MOBILE_ROW_H * MOBILE_ROWS as f32 <= MOBILE_PANEL.h - 104.0);
        assert!(ROW_H * ROWS as f32 <= PANEL.h - 80.0);
    }

    #[test]
    fn the_sign_in_button_leaves_the_numbers_readable() {
        let body_top = MOBILE_PANEL.y + 46.0;
        let body_bottom = body_top + MOBILE_PANEL.h - 104.0;
        let rows = stat_lines(&Stats::default()).len().div_ceil(2);
        let last_row = body_top + MOBILE_NUMBERS_TOP + (rows - 1) as f32 * MOBILE_NUMBERS_ROW_H_SIGN_IN;
        let half_a_line = 6.0;
        assert!(last_row + half_a_line <= MOBILE_SIGN_IN.y);
        assert!(MOBILE_SIGN_IN.y + MOBILE_SIGN_IN.h <= body_bottom);
        assert_eq!(MOBILE_SIGN_IN.center().x, MOBILE_PANEL.center().x);
    }
}
