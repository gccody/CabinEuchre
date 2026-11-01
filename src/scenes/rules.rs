//! "How to play": a few short illustrated pages.

use love2drs::prelude::*;

use crate::cards::Pose;
use crate::euchre::{Card, Rank, Suit};
use crate::juice::{self, Spring};
use crate::palette;
use crate::scene::{Nav, Shared};
use crate::sfx::Sound;
use crate::text::{LINE_HEIGHT, Text};
use crate::ui::{self, Button, Rect};

const PANEL: Rect = Rect::new(120.0, 34.0, 400.0, 292.0);

struct Page {
    title: &'static str,
    body: &'static str,
    cards: &'static [(Card, &'static str)],
}

const PAGES: &[Page] = &[
    Page {
        title: "THE GAME",
        body: "Four players, two teams. You sit at the bottom and your partner sits across from you. \
               The opponents sit left and right.\n\n\
               Each hand, one team picks a trump suit and tries to win at least 3 of the 5 tricks. \
               The first team to 10 points wins the game (you can change this in Options).",
        cards: &[],
    },
    Page {
        title: "TRUMP & THE BOWERS",
        body: "Only 9, 10, J, Q, K and A are used. Any trump card beats every card of another suit.\n\n\
               The jack of trump is the RIGHT BOWER, the best card in the game. The other jack of the \
               same color is the LEFT BOWER, the second best. The left bower counts as a trump card, \
               not as its printed suit. Here, hearts are trump:",
        cards: &[
            (Card::new(Rank::Jack, Suit::Hearts), "Right"),
            (Card::new(Rank::Jack, Suit::Diamonds), "Left"),
            (Card::new(Rank::Ace, Suit::Hearts), "Ace"),
            (Card::new(Rank::King, Suit::Hearts), "King"),
        ],
    },
    Page {
        title: "PICKING TRUMP",
        body: "After the deal, one card is turned face up. Starting left of the dealer, each player may \
               ORDER IT UP to make its suit trump. The dealer then picks that card up and discards one.\n\n\
               If everyone passes, the card is turned down and each player may NAME any other suit as \
               trump, or pass. With \"stick the dealer\" on, the dealer must name a suit.",
        cards: &[(Card::new(Rank::Queen, Suit::Diamonds), "Up card")],
    },
    Page {
        title: "PLAYING TRICKS",
        body: "The player left of the dealer leads any card. Everyone must FOLLOW SUIT if they can \
               (remember the left bower is a trump). If you can't follow, play anything, including trump.\n\n\
               The highest trump wins the trick. With no trump played, the highest card of the suit \
               that was led wins. The winner leads the next trick. With spades as trump:",
        cards: &[
            (Card::new(Rank::Ace, Suit::Clubs), "Led"),
            (Card::new(Rank::King, Suit::Clubs), "Follows"),
            (Card::new(Rank::Nine, Suit::Spades), "Wins!"),
            (Card::new(Rank::Ace, Suit::Hearts), "Can't win"),
        ],
    },
    Page {
        title: "SCORING",
        body: "The team that chose trump are the MAKERS.\n\n\
               3 or 4 tricks: 1 point.\n\
               All 5 tricks (a MARCH): 2 points.\n\
               Fewer than 3: they are EUCHRED and the other team scores 2.\n\n\
               GOING ALONE: a maker with a great hand can play without their partner. \
               A lone march scores 4 points.",
        cards: &[],
    },
    Page {
        title: "CONTROLS",
        body: "Click or tap a card to play it, or press 1-6. On a touchscreen, slide your finger \
               along your hand to look the cards over, then lift it to play.\n\n\
               Bid with the buttons above your hand, or with keys:\n\
               O order up, P pass, A alone, C D S H name a suit.\n\
               Space or Enter continues after a hand. Esc pauses the game.\n\n\
               F2 toggles the lamplight effect. F11 toggles fullscreen.\n\
               Change speed, volume and house rules in Options.",
        cards: &[],
    },
    Page {
        title: "WITH A CONTROLLER",
        body: "D-pad or left stick: move the gold highlight.\n\
               A: play the highlighted card, or press the button.\n\
               B: back out of a menu.\n\
               Y: toggle going alone while bidding.\n\
               Start: pause the game.\n\
               LB / RB: turn these pages.\n\n\
               The arrow keys and Enter work the same way on a keyboard.\n\n\
               Big moments rumble the controller. Turn that off in Options.",
        cards: &[],
    },
];

pub struct RulesPanel {
    page: usize,
    prev: Button,
    next: Button,
    close: Button,
    pop: Spring,
    page_pop: Spring,
}

impl RulesPanel {
    pub fn new() -> Self {
        let y = PANEL.y + PANEL.h - 22.0;
        let mut pop = Spring::new(0.9);
        pop.target = 1.0;
        RulesPanel {
            page: 0,
            prev: Button::grey("<", Rect::centered(PANEL.x + 30.0, y, 30.0, 18.0)),
            next: Button::blue(">", Rect::centered(PANEL.x + PANEL.w - 30.0, y, 30.0, 18.0)),
            close: Button::brass("CLOSE", Rect::centered(PANEL.center().x, y, 76.0, 18.0)),
            pop,
            page_pop: Spring::new(1.0),
        }
    }

    pub fn update(&mut self, shared: &mut Shared, dt: f32) {
        self.pop.update(dt);
        self.page_pop.update(dt);
        self.prev.enabled = self.page > 0;
        self.next.enabled = self.page + 1 < PAGES.len();
        for b in [&mut self.prev, &mut self.next, &mut self.close] {
            b.update(shared.mouse, dt, &shared.sfx);
        }
    }

    fn turn(&mut self, shared: &mut Shared, dir: i32) {
        let p = (self.page as i32 + dir).clamp(0, PAGES.len() as i32 - 1) as usize;
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
            Nav::Confirm => {
                shared.tap();
                if self.page + 1 == PAGES.len() {
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
            Key::D | Key::Space | Key::Enter => {
                if self.page + 1 == PAGES.len() && matches!(key, Key::Space | Key::Enter) {
                    self.close.flash(&shared.sfx);
                    return true;
                }
                self.turn(shared, 1);
                false
            }
            _ => false,
        }
    }

    pub fn draw(&self, shared: &Shared, g: &mut Graphics) {
        let font = &shared.font;
        let page = &PAGES[self.page];
        ui::dim_screen(g, 0.55);
        g.push();
        let c = PANEL.center();
        g.translate(c.x, c.y);
        g.scale(self.pop.value, self.pop.value);
        g.translate(-c.x, -c.y);

        ui::panel(g, PANEL, palette::PANEL);
        Text::new("HOW TO PLAY", c.x, PANEL.y + 12.0)
            .color(palette::MUTED)
            .center()
            .draw(g, font);
        let s = self.page_pop.value;
        Text::new(page.title, c.x, PANEL.y + 30.0)
            .scale(2.0 * s)
            .color(palette::GOLD)
            .center()
            .draw(g, font);

        let body = Rect::new(PANEL.x + 12.0, PANEL.y + 46.0, PANEL.w - 24.0, PANEL.h - 80.0);
        ui::inset(g, body, palette::PANEL_DARK);
        let lines = font.wrap(page.body, body.w - 20.0);
        for (i, line) in lines.iter().enumerate() {
            Text::new(line, body.x + 10.0, body.y + 9.0 + i as f32 * LINE_HEIGHT).draw(g, font);
        }

        if self.page == 0 {
            draw_seating(shared, g, vec2(c.x, body.y + body.h - 58.0));
        }

        // Illustration cards along the bottom of the body box.
        let n = page.cards.len() as f32;
        for (i, &(card, caption)) in page.cards.iter().enumerate() {
            let x = c.x + (i as f32 - (n - 1.0) / 2.0) * 58.0;
            let y = body.y + body.h - 52.0 + juice::wobble(shared.time, i as f32, 2.0, 1.5);
            let rot = juice::wobble(shared.time, i as f32 + 3.0, 1.3, 0.03);
            let pose = Pose::new(x, y, rot, 1.0, 1.0);
            shared.cards.draw(g, None, pose.offset(2.0, 3.0), palette::INK.with_alpha(0.5));
            shared.cards.draw(g, Some(card), pose, Color::WHITE);
            Text::new(caption, x, y + 39.0).color(palette::BRASS_LIGHT).center().draw(g, font);
        }

        Text::new(&format!("{} / {}", self.page + 1, PAGES.len()), PANEL.x + 72.0, PANEL.y + PANEL.h - 22.0)
            .color(palette::MUTED)
            .anchor(0.0, 0.5)
            .draw(g, font);
        self.prev.draw(g, font);
        self.next.draw(g, font);
        self.close.draw(g, font);
        g.pop();
    }
}

/// A tiny diagram of who sits where: name plates in their team colors around
/// a little felt table with a brass rim.
fn draw_seating(shared: &Shared, g: &mut Graphics, c: Vec2) {
    let font = &shared.font;
    g.set_color(palette::INK);
    g.ellipse(Fill, c.x, c.y + 1.0, 73.0, 33.0);
    g.set_color(palette::BRASS_DARK);
    g.ellipse(Fill, c.x, c.y, 72.0, 32.0);
    g.set_color(palette::BRASS);
    g.ellipse(Fill, c.x, c.y - 1.0, 71.0, 31.0);
    g.set_color(palette::FELT_DEEP);
    g.ellipse(Fill, c.x, c.y, 68.0, 28.0);
    g.set_color(palette::FELT);
    g.ellipse(Fill, c.x, c.y + 1.0, 66.0, 26.0);
    let seats = [
        ("YOU", vec2(0.0, 38.0), palette::US),
        ("ROSA", vec2(-100.0, 0.0), palette::THEM),
        ("BENNY (partner)", vec2(0.0, -38.0), palette::US),
        ("JULES", vec2(100.0, 0.0), palette::THEM),
    ];
    for (name, off, color) in seats {
        let p = c + off;
        let w = font.width(name) + 18.0;
        let r = Rect::centered(p.x, p.y, w, 13.0);
        g.set_color(palette::INK);
        ui::chamfer(g, Fill, r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 3.0, 4.0);
        g.set_color(palette::PANEL_LIGHT);
        ui::chamfer(g, Fill, r.x, r.y, r.w, r.h, 3.0);
        let (l, t, b) = (r.x + 3.0, r.y + 3.0, r.y + r.h - 3.0);
        g.set_color(color);
        g.polygon(Fill, &[l, t, l + 4.0, t, l + 6.0, p.y, l + 4.0, b, l, b]);
        Text::new(name, r.x + 12.0, p.y).anchor(0.0, 0.5).draw(g, font);
    }
}
