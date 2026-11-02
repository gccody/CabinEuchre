//! Euchre: entry point.
//!
//! Module map:
//! - `euchre`   the `euchre-core` crate: rules, scoring, AI, saves and the online protocol
//! - `app`      the love2drs `Game` impl: owns the active scene and the render pipeline
//! - `scene`    the `Scene` trait, shared resources and transitions between screens
//! - `scenes`   title, table and the options / how-to-play overlays
//! - `render`   virtual-resolution canvas, tabletop background and lamplight post-process
//! - `cards`    procedural pixel-art card faces and back
//! - `text`     the hand-made bitmap pixel font
//! - `ui`       buttons and panels
//! - `palette`  the color palette every screen draws from
//! - `sfx`      procedural sound effects with pitch jitter, plus music
//! - `juice`    springs and easing curves for animation
//! - `settings` persisted player options
//! - `assets`   asset path resolution
//! - `net`      where moves go: an in-process room (offline) or the online server
//! - `platform` Steam (optional feature): names, invites, rich presence, join requests

mod app;
mod assets;
mod cards;
use euchre_core as euchre;
mod juice;
mod net;
mod palette;
mod platform;
mod render;
mod savegame;
mod scene;
mod scenes;
mod settings;
mod sfx;
mod suits;
mod text;
mod ui;

use love2drs::Conf;

fn main() -> love2drs::Result {
    let conf = Conf::new("Euchre")
        .size(1280, 720)
        .resizable(true)
        .min_size(render::VIEW_W, render::VIEW_H);
    love2drs::run(conf, app::App::new)
}
