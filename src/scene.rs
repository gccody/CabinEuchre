//! Screens ("scenes") and the transitions between them.
//!
//! The app owns exactly one active scene. Scenes receive input already
//! converted to design-space coordinates (see `render::View`) and draw in
//! design space; they never deal with window size, the CRT pass or the
//! background.

use love2drs::prelude::*;

use crate::cards::CardArt;
use crate::settings::Settings;
use crate::sfx::Sfx;
use crate::text::PixelFont;

/// Long-lived resources shared by every scene.
pub struct Shared {
    pub font: PixelFont,
    pub cards: CardArt,
    pub sfx: Sfx,
    pub settings: Settings,
    /// Seconds since the game started; drive idle animation from this.
    pub time: f32,
    /// Mouse position in design space, updated every frame.
    pub mouse: Vec2,
    /// Whether the left mouse button is held (for dragging sliders).
    pub mouse_down: bool,
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
}
