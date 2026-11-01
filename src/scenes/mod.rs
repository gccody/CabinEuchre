//! Individual screens, plus the modal overlays they share.

pub mod options;
pub mod rules;
pub mod table;
pub mod title;

use love2drs::prelude::*;

use crate::scene::Shared;
use options::OptionsPanel;
use rules::RulesPanel;

/// A modal panel drawn over a scene. While one is open the scene below is paused.
pub enum Overlay {
    Options(Box<OptionsPanel>),
    Rules(Box<RulesPanel>),
}

impl Overlay {
    pub fn update(&mut self, shared: &mut Shared, dt: f32) {
        match self {
            Overlay::Options(p) => p.update(shared, dt),
            Overlay::Rules(p) => p.update(shared, dt),
        }
    }

    pub fn draw(&self, shared: &Shared, g: &mut Graphics) {
        match self {
            Overlay::Options(p) => p.draw(shared, g),
            Overlay::Rules(p) => p.draw(shared, g),
        }
    }

    /// Returns true when the overlay should close.
    pub fn mouse_pressed(&mut self, shared: &mut Shared, pos: Vec2) -> bool {
        match self {
            Overlay::Options(p) => p.mouse_pressed(shared, pos),
            Overlay::Rules(p) => p.mouse_pressed(shared, pos),
        }
    }

    /// Returns true when the overlay should close.
    pub fn key_pressed(&mut self, shared: &mut Shared, key: Key) -> bool {
        match self {
            Overlay::Options(p) => p.key_pressed(shared, key),
            Overlay::Rules(p) => p.key_pressed(shared, key),
        }
    }
}
