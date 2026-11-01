//! Player settings, persisted as a tiny `key=value` file in the user's config
//! directory (`%APPDATA%\Euchre` on Windows, `~/.config/euchre` elsewhere).

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Master volume, 0..=1.
    pub volume: f32,
    /// Music volume, 0..=1 (relative to master).
    pub music: f32,
    /// CRT effect strength, 0..=1.
    pub crt: f32,
    /// 0 = relaxed, 1 = normal, 2 = fast.
    pub speed: usize,
    pub stick_the_dealer: bool,
    /// Points needed to win a game.
    pub target: u32,
    pub fullscreen: bool,
}

pub const SPEED_NAMES: [&str; 3] = ["Relaxed", "Normal", "Fast"];
pub const TARGETS: [u32; 3] = [5, 7, 10];

impl Default for Settings {
    fn default() -> Self {
        Settings {
            volume: 0.8,
            music: 0.45,
            crt: 1.0,
            speed: 1,
            stick_the_dealer: true,
            target: 10,
            fullscreen: false,
        }
    }
}

impl Settings {
    /// Multiplier for pauses and AI "thinking" time.
    pub fn delay_factor(&self) -> f32 {
        [1.4, 1.0, 0.55][self.speed.min(2)]
    }

    fn path() -> Option<PathBuf> {
        let base = std::env::var_os("APPDATA")
            .map(|p| PathBuf::from(p).join("Euchre"))
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("euchre")))?;
        Some(base.join("settings.txt"))
    }

    /// Debug: `EUCHRE_DEFAULT_SETTINGS=1` ignores (and never writes) the saved
    /// file, so screenshots don't depend on the player's options.
    fn use_defaults() -> bool {
        std::env::var("EUCHRE_DEFAULT_SETTINGS").is_ok_and(|v| v == "1")
    }

    pub fn load() -> Settings {
        let mut s = Settings::default();
        if Settings::use_defaults() {
            return s;
        }
        let Some(text) = Settings::path().and_then(|p| std::fs::read_to_string(p).ok()) else {
            return s;
        };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let f = v.parse::<f32>().ok();
            let b = v == "true";
            match k {
                "volume" => s.volume = f.unwrap_or(s.volume).clamp(0.0, 1.0),
                "music" => s.music = f.unwrap_or(s.music).clamp(0.0, 1.0),
                "crt" => s.crt = f.unwrap_or(s.crt).clamp(0.0, 1.0),
                "speed" => s.speed = v.parse().unwrap_or(s.speed).min(2),
                "stick_the_dealer" => s.stick_the_dealer = b,
                "target" => s.target = v.parse().ok().filter(|t| TARGETS.contains(t)).unwrap_or(s.target),
                "fullscreen" => s.fullscreen = b,
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) {
        if Settings::use_defaults() {
            return;
        }
        let Some(path) = Settings::path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text = format!(
            "volume={}\nmusic={}\ncrt={}\nspeed={}\nstick_the_dealer={}\ntarget={}\nfullscreen={}\n",
            self.volume, self.music, self.crt, self.speed, self.stick_the_dealer, self.target, self.fullscreen
        );
        if let Err(e) = std::fs::write(&path, text) {
            crate::assets::log_warn(&format!("could not save settings: {e}"));
        }
    }
}
