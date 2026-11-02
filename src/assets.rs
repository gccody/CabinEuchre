//! Asset lookup. Paths are resolved against the working directory first, then
//! the crate root (so `cargo run` from anywhere works during development), and
//! finally next to the executable (for shipped builds).
//!
//! Everything visual is generated in code (`text`, `cards`), so the only
//! optional asset today is `assets/music/theme.ogg`.

use std::path::{Path, PathBuf};

/// Finds `relative` (e.g. `"assets/music/theme.ogg"`), or `None` if it doesn't exist.
pub fn find(relative: &str) -> Option<PathBuf> {
    // Phones keep assets inside the app package, where `is_file` can't see
    // them. The loaders look there for the same relative path.
    if crate::device::NATIVE_MOBILE {
        return Some(PathBuf::from(relative));
    }
    let candidates = [
        Some(PathBuf::from(relative)),
        Some(Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)),
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join(relative))),
    ];
    candidates.into_iter().flatten().find(|p| p.is_file())
}

/// Minimal logging until the project needs a logger crate.
pub fn log_warn(msg: &str) {
    eprintln!("[euchre] warning: {msg}");
}
