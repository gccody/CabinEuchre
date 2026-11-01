//! Screens ("scenes") and the transitions between them.
//!
//! The app owns exactly one active scene. Scenes receive input already
//! converted to design-space coordinates (see `render::View`) and draw in
//! design space; they never deal with window size, the post-process pass or the
//! background.
//!
//! Input arrives three ways:
//! - pointer: `mouse_pressed` (mouse) or `touch_pressed` / `touch_released`
//!   (touchscreen; a tap acts when the finger lifts, so sliding across the
//!   hand previews cards before playing one),
//! - keys: `key_pressed` for shortcuts,
//! - [`Nav`]: gamepad buttons and the arrow keys, which move a focus highlight
//!   between controls and confirm or back out.

use love2drs::prelude::*;

use crate::cards::CardArt;
use crate::settings::Settings;
use crate::sfx::Sfx;
use crate::text::PixelFont;

/// `Shared::mouse` when there is no pointer over the screen (gamepad or
/// keyboard play, or no finger down), so nothing shows hover.
pub const NO_POINTER: Vec2 = Vec2::new(-99.0, -99.0);

/// The device the player used last. Hover effects follow the pointer, the
/// focus highlight only shows in `Buttons` mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Mouse,
    Touch,
    /// A gamepad or the arrow keys: a focus highlight moves between controls.
    Buttons,
}

/// A navigation command from a gamepad (or the arrow keys).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    Left,
    Right,
    /// A (bottom face button), or Enter / Space while the focus is showing.
    Confirm,
    /// B (right face button): close a panel, resume.
    Back,
    /// Start or Back/View: open or close the pause menu.
    Menu,
    /// Y: the "alone" toggle while bidding.
    Alt,
    /// Shoulder buttons: turn pages.
    PrevPage,
    NextPage,
}

impl Nav {
    /// -1 / +1 for left / right (and up / down), 0 otherwise.
    pub fn dx(self) -> i32 {
        match self {
            Nav::Left => -1,
            Nav::Right => 1,
            _ => 0,
        }
    }

    pub fn dy(self) -> i32 {
        match self {
            Nav::Up => -1,
            Nav::Down => 1,
            _ => 0,
        }
    }
}

/// Long-lived resources shared by every scene.
pub struct Shared {
    pub font: PixelFont,
    pub cards: CardArt,
    pub sfx: Sfx,
    pub settings: Settings,
    /// Seconds since the game started; drive idle animation from this.
    pub time: f32,
    /// Pointer position in design space, updated every frame: the mouse, the
    /// finger on a touchscreen, or [`NO_POINTER`].
    pub mouse: Vec2,
    /// Whether the left mouse button (or a finger) is held, for dragging sliders.
    pub mouse_down: bool,
    /// The device the player used last.
    pub input: InputMode,
    /// Whether a gamepad is connected (picks which control hints to show).
    pub gamepad: bool,
    /// Rumble pulses requested this frame, scheduled and played by the app.
    pub rumbles: Vec<Buzz>,
}

/// One controller rumble pulse. `low` is the heavy, low-frequency motor (a
/// thump), `high` the light, high-frequency one (a buzz or a click).
#[derive(Clone, Copy, Debug)]
pub struct Buzz {
    /// Seconds from now until it starts, so a scene can queue a pattern.
    pub delay: f32,
    pub low: f32,
    pub high: f32,
    pub seconds: f32,
}

impl Shared {
    /// Whether to draw the focus highlight.
    pub fn focus_visible(&self) -> bool {
        self.input == InputMode::Buttons
    }

    /// Asks for a controller rumble (0..1 strength) for the big moments. Only
    /// plays if the player is on a gamepad and has rumble enabled.
    pub fn rumble(&mut self, strength: f32, seconds: f32) {
        self.buzz(0.0, strength, strength * 0.6, seconds);
    }

    /// Queues one pulse on each motor, starting `delay` seconds from now.
    pub fn buzz(&mut self, delay: f32, low: f32, high: f32, seconds: f32) {
        self.rumbles.push(Buzz { delay, low, high, seconds });
    }

    /// The crisp click for a press that actually selected something (a menu
    /// item, an option value, a bid, a card). Not for moves or refused presses.
    pub fn tap(&mut self) {
        // Almost all high motor and very short, so it clicks rather than hums.
        self.buzz(0.0, 0.06, 0.65, 0.035);
    }

    /// A lighter tick for moving the focus (d-pad, stick, arrow keys) or
    /// stepping a slider.
    pub fn tick(&mut self) {
        self.buzz(0.0, 0.0, 0.32, 0.022);
    }

    /// A card flicked across the felt during the deal. Cards to your own seat
    /// land a little firmer.
    pub fn deal_flick(&mut self, to_you: bool) {
        if to_you {
            self.buzz(0.0, 0.05, 0.3, 0.03);
        } else {
            self.buzz(0.0, 0.0, 0.17, 0.022);
        }
    }

    /// A riffle shuffle: a fast flutter of clicks that speeds up and fades,
    /// then a soft thump as the deck is squared.
    pub fn riffle(&mut self) {
        let mut t = 0.0;
        for i in 0..9 {
            let k = i as f32 / 8.0;
            self.buzz(t, 0.0, 0.42 - 0.18 * k, 0.022);
            t += 0.05 - 0.018 * k;
        }
        self.buzz(t + 0.06, 0.3, 0.12, 0.07);
    }

    /// Cards sliding together into the pile.
    pub fn gather_slide(&mut self) {
        self.buzz(0.0, 0.12, 0.1, 0.14);
    }

    /// A control hint for the current device: `pad` with a gamepad, `keys` otherwise.
    pub fn hint<'a>(&self, pad: &'a str, keys: &'a str) -> &'a str {
        if self.gamepad && self.input == InputMode::Buttons { pad } else { keys }
    }
}

/// What the app should do after a scene callback.
pub enum Transition {
    None,
    Switch(Box<dyn Scene>),
    Quit,
}

#[allow(unused_variables)]
pub trait Scene {
    fn update(&mut self, shared: &mut Shared, ctx: &mut Context, dt: f32) -> Transition {
        Transition::None
    }

    /// Draw in design space (`render::VIEW_W x render::VIEW_H`), on top of the background.
    fn draw(&mut self, shared: &Shared, g: &mut Graphics);

    fn key_pressed(&mut self, shared: &mut Shared, key: Key) -> Transition {
        Transition::None
    }

    /// `pos` is in design space.
    fn mouse_pressed(&mut self, shared: &mut Shared, pos: Vec2, button: MouseButton) -> Transition {
        Transition::None
    }

    /// A finger touched down. Only things that need the press itself (sliders)
    /// react here; everything else acts on release.
    fn touch_pressed(&mut self, shared: &mut Shared, pos: Vec2) -> Transition {
        Transition::None
    }

    /// The finger lifted at `pos`: a tap on whatever is under it.
    fn touch_released(&mut self, shared: &mut Shared, pos: Vec2) -> Transition {
        self.mouse_pressed(shared, pos, MouseButton::Left)
    }

    /// A gamepad (or arrow key) command.
    fn nav(&mut self, shared: &mut Shared, nav: Nav) -> Transition {
        Transition::None
    }

    /// The app is about to close (the window was closed): keep anything worth
    /// keeping, like a game in progress.
    fn suspend(&mut self, shared: &mut Shared) {}
}
