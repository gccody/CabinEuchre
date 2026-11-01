//! The color palette. Draw from these constants rather than ad-hoc hex values
//! so the whole game stays cohesive.
//!
//! The look is a Great Lakes cabin card night: walnut planks under a
//! lake-teal felt mat, warm lamplight, brass trim, cream cards and a paper
//! score pad, with lodge accents (brass, pine, lake blue, cranberry).

use love2drs::prelude::Color;

// Table (the background shader paints these).
pub const WOOD_DARK: Color = Color::hex(0x24140d);
pub const WOOD: Color = Color::hex(0x55321f);
pub const WOOD_LIGHT: Color = Color::hex(0x7a4b2e);
pub const FELT_DEEP: Color = Color::hex(0x0b2830);
pub const FELT: Color = Color::hex(0x175260);
pub const FELT_GLOW: Color = Color::hex(0x2b7d84);
/// Tint of the pendant lamp's light.
pub const LAMP: Color = Color::hex(0xffd49a);

// UI
pub const INK: Color = Color::hex(0x1b110c); // outlines and drop shadows (warm near-black)
pub const PANEL_DARK: Color = Color::hex(0x22150f);
pub const PANEL: Color = Color::hex(0x352219);
pub const PANEL_LIGHT: Color = Color::hex(0x523627);
/// Brass trim on panels, buttons and the trump medallion.
pub const BRASS: Color = Color::hex(0xd9a441);
pub const BRASS_DARK: Color = Color::hex(0x7f5520);
pub const BRASS_LIGHT: Color = Color::hex(0xf7d887);
pub const CREAM: Color = Color::hex(0xfbefd8); // card faces, primary text
pub const CREAM_SHADE: Color = Color::hex(0xe4cfab);
pub const MUTED: Color = Color::hex(0xb9a68d);
pub const DIM: Color = Color::hex(0x6e5a4a);

// The paper score pad.
pub const PAPER: Color = Color::hex(0xf4e8c8);
pub const PAPER_SHADE: Color = Color::hex(0xdccb9f);
pub const PAPER_RULE: Color = Color::hex(0xa9bfcc);
pub const PENCIL: Color = Color::hex(0x3a332f);
pub const PENCIL_LIGHT: Color = Color::hex(0x8c8173);
/// Team colors written on paper (darker than the on-felt team colors).
pub const US_INK: Color = Color::hex(0x235e8f);
pub const THEM_INK: Color = Color::hex(0xa62a35);

// Accents (button colors): cranberry, lake blue, brass, pine and walnut.
pub const RED: Color = Color::hex(0xd0464f);
pub const RED_DARK: Color = Color::hex(0x7c2129);
pub const BLUE: Color = Color::hex(0x3a8bc2);
pub const BLUE_DARK: Color = Color::hex(0x1f4f72);
pub const GOLD: Color = Color::hex(0xf2bf4f);
pub const ORANGE: Color = Color::hex(0xe0902f); // amber
pub const ORANGE_DARK: Color = Color::hex(0x854d14);
pub const GREEN: Color = Color::hex(0x4c9a5c);
pub const GREEN_DARK: Color = Color::hex(0x255433);
pub const GREY_BUTTON: Color = Color::hex(0x6d4b37); // walnut
pub const GREY_BUTTON_DARK: Color = Color::hex(0x3a261b);

// Teams
pub const US: Color = Color::hex(0x5eaee3);
pub const THEM: Color = Color::hex(0xe8636b);

// Suits: hearts/diamonds share red, clubs/spades share near-black, as on real cards.
pub const SUIT_RED: Color = Color::hex(0xd33a3c);
pub const SUIT_BLACK: Color = INK;
/// Red suit symbols drawn on dark panels.
pub const SUIT_RED_LIGHT: Color = Color::hex(0xff6f62);

// Card art
pub const SKIN: Color = Color::hex(0xf2c49b);
pub const HAIR: Color = Color::hex(0x7a4a2a);
pub const WHITE: Color = Color::hex(0xffffff);
/// Card backs: a buffalo-check plaid.
pub const BACK: Color = Color::hex(0xb9343c);
pub const BACK_DARK: Color = Color::hex(0x7a1d26);
pub const BACK_BLACK: Color = Color::hex(0x2a1215);
/// Court card clothes, per suit (clubs, diamonds, spades, hearts).
pub const COURT: [Color; 4] = [
    Color::hex(0x3c7a4a),
    Color::hex(0xd9772f),
    Color::hex(0x3a5a9e),
    Color::hex(0xc9414a),
];

/// Drop shadows are `INK` at this alpha.
pub const SHADOW_ALPHA: f32 = 0.55;
