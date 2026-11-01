//! The color palette. Draw from these constants rather than ad-hoc hex values
//! so the whole game stays cohesive: saturated suit colors, deep felt
//! backgrounds, warm cream card faces and near-black outlines/shadows.

use love2drs::prelude::Color;

// Backgrounds (the swirl shader blends these three).
pub const FELT_DEEP: Color = Color::hex(0x0a3326);
pub const FELT: Color = Color::hex(0x17704f);
pub const FELT_GLOW: Color = Color::hex(0x4cc27a);

// UI
pub const INK: Color = Color::hex(0x1a1423); // outlines and drop shadows
pub const PANEL_DARK: Color = Color::hex(0x1f2133);
pub const PANEL: Color = Color::hex(0x2b2d42);
pub const PANEL_LIGHT: Color = Color::hex(0x3d405b);
pub const PANEL_EDGE: Color = Color::hex(0x575b80);
pub const CREAM: Color = Color::hex(0xfdf0d5); // card faces, primary text
pub const CREAM_SHADE: Color = Color::hex(0xe6d3b0);
pub const MUTED: Color = Color::hex(0x9aa5b1);
pub const DIM: Color = Color::hex(0x5c6370);

// Accents (button colors)
pub const RED: Color = Color::hex(0xfe5f55);
pub const RED_DARK: Color = Color::hex(0xb83a3a);
pub const BLUE: Color = Color::hex(0x009dff);
pub const BLUE_DARK: Color = Color::hex(0x0067b3);
pub const GOLD: Color = Color::hex(0xfdb833);
pub const ORANGE: Color = Color::hex(0xff9f1c);
pub const ORANGE_DARK: Color = Color::hex(0xbf6a00);
pub const GREEN: Color = Color::hex(0x3fbf6f);
pub const GREEN_DARK: Color = Color::hex(0x237a45);
pub const GREY_BUTTON: Color = Color::hex(0x4f5470);
pub const GREY_BUTTON_DARK: Color = Color::hex(0x33364d);

// Teams
pub const US: Color = BLUE;
pub const THEM: Color = RED;

// Suits: hearts/diamonds share red, clubs/spades share near-black, as on real cards.
pub const SUIT_RED: Color = Color::hex(0xe03c3c);
pub const SUIT_BLACK: Color = INK;
/// Red suit symbols drawn on dark panels.
pub const SUIT_RED_LIGHT: Color = Color::hex(0xff6b5e);

// Card art
pub const SKIN: Color = Color::hex(0xf2c49b);
pub const HAIR: Color = Color::hex(0x7a4a2a);
pub const WHITE: Color = Color::hex(0xffffff);
pub const BACK: Color = Color::hex(0xc8403a);
pub const BACK_DARK: Color = Color::hex(0x8f2b2b);
/// Court card clothes, per suit (clubs, diamonds, spades, hearts).
pub const COURT: [Color; 4] = [
    Color::hex(0x3c7a4a),
    Color::hex(0xe8703a),
    Color::hex(0x3a5a9e),
    Color::hex(0xd8434a),
];

/// Drop shadows are `INK` at this alpha.
pub const SHADOW_ALPHA: f32 = 0.55;
