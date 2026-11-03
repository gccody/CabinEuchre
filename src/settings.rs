//! Player settings, persisted as a tiny `key=value` file in the user's config
//! directory (`%APPDATA%\CabinEuchre` on Windows, `~/.config/cabin-euchre` elsewhere, the
//! app's own folder on phones).

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Master volume, 0..=1.
    pub volume: f32,
    /// Music volume, 0..=1 (relative to master).
    pub music: f32,
    /// Lamplight post-process strength (warm grade, vignette, grain), 0..=1.
    pub atmosphere: f32,
    /// 0 = relaxed, 1 = normal, 2 = fast.
    pub speed: usize,
    pub stick_the_dealer: bool,
    /// Points needed to win a game.
    pub target: u32,
    pub fullscreen: bool,
    /// Controller rumble: clicks on selections and focus moves, the shuffle and
    /// deal, and big moments (euchres, marches, game over).
    pub rumble: bool,
    /// Your name at online tables (empty: the Steam name, else "PLAYER").
    pub name: String,
    /// The tutorial has been finished once.
    pub tutorial_done: bool,
    /// Where the tutorial was left: `(lesson, scores)`, to offer picking it back up.
    pub lesson: Option<(usize, [u32; 2])>,
}

/// Where the game keeps its files: `%APPDATA%\CabinEuchre` on Windows,
/// `~/.config/cabin-euchre` elsewhere.
pub fn config_dir() -> Option<PathBuf> {
    // A phone's app only has its own private folder (and no APPDATA or HOME on Android).
    if crate::device::NATIVE_MOBILE {
        return Some(love2drs::filesystem::save_dir());
    }
    std::env::var_os("APPDATA")
        .map(|p| PathBuf::from(p).join("CabinEuchre"))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("cabin-euchre")))
}

pub const SPEED_NAMES: [&str; 3] = ["Relaxed", "Normal", "Fast"];
pub const TARGETS: [u32; 3] = [5, 7, 10];

impl Default for Settings {
    fn default() -> Self {
        Settings {
            volume: 0.8,
            music: 0.45,
            atmosphere: 1.0,
            speed: 1,
            stick_the_dealer: true,
            target: 10,
            fullscreen: false,
            rumble: true,
            name: String::new(),
            tutorial_done: false,
            lesson: None,
        }
    }
}

/// Reads the `lesson` line: `lesson,us,them`, anything else means none.
fn parse_lesson(v: &str) -> Option<(usize, [u32; 2])> {
    let mut parts = v.split(',').map(|p| p.trim().parse::<u32>().ok());
    let (lesson, us, them) = (parts.next()??, parts.next()??, parts.next()??);
    let target = crate::euchre::tutorial::TARGET;
    let fits = (lesson as usize) < crate::euchre::tutorial::LESSONS && us < target && them < target;
    fits.then_some((lesson as usize, [us, them]))
}

impl Settings {
    /// The name to sit down with online: yours, else the store's, else "PLAYER".
    pub fn player_name(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        crate::euchre::protocol::clean_name(&crate::platform::player_name().unwrap_or_default())
    }

    /// Multiplier for pauses and AI "thinking" time.
    pub fn delay_factor(&self) -> f32 {
        [1.4, 1.0, 0.55][self.speed.min(2)]
    }

    fn path() -> Option<PathBuf> {
        Some(config_dir()?.join("settings.txt"))
    }

    /// Debug: `EUCHRE_DEFAULT_SETTINGS=1` ignores (and never writes) the saved
    /// files (settings and the saved game), so screenshots don't depend on the
    /// player's options or a game they left.
    pub fn use_defaults() -> bool {
        crate::device::var("EUCHRE_DEFAULT_SETTINGS").is_some_and(|v| v == "1")
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
                // "crt" is the key older versions wrote for the same slider.
                "atmosphere" | "crt" => s.atmosphere = f.unwrap_or(s.atmosphere).clamp(0.0, 1.0),
                "speed" => s.speed = v.parse().unwrap_or(s.speed).min(2),
                "stick_the_dealer" => s.stick_the_dealer = b,
                "target" => s.target = v.parse().ok().filter(|t| TARGETS.contains(t)).unwrap_or(s.target),
                "fullscreen" => s.fullscreen = b,
                "rumble" => s.rumble = b,
                "name" if !v.is_empty() => s.name = crate::euchre::protocol::clean_name(v),
                "tutorial_done" => s.tutorial_done = b,
                "lesson" => s.lesson = parse_lesson(v),
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
            "volume={}\nmusic={}\natmosphere={}\nspeed={}\nstick_the_dealer={}\ntarget={}\nfullscreen={}\nrumble={}\nname={}\ntutorial_done={}\nlesson={}\n",
            self.volume,
            self.music,
            self.atmosphere,
            self.speed,
            self.stick_the_dealer,
            self.target,
            self.fullscreen,
            self.rumble,
            self.name,
            self.tutorial_done,
            self.lesson.map_or(String::new(), |(l, s)| format!("{l},{},{}", s[0], s[1]))
        );
        // Write beside the file and rename, so a crash mid-write can't lose the settings.
        let tmp = path.with_extension("tmp");
        if let Err(e) = std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, &path)) {
            crate::assets::log_warn(&format!("could not save settings: {e}"));
        }
    }
}
