//! Sound. Every effect is synthesized in code (soft blips, card swishes,
//! chip clinks) and played with a little random pitch jitter so repeated
//! sounds never feel mechanical. Replace any of them later by dropping a file
//! in `assets/sounds/` and loading it in [`Sfx::new`].
//!
//! Music: `assets/music/theme.ogg` plays if present; otherwise a mellow
//! synthesized lounge loop is generated at startup.

use std::f32::consts::TAU;

use love2drs::prelude::*;

use crate::assets;

/// Every sound effect in the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    /// Confirming a menu choice.
    Select,
    /// Hovering over something interactive.
    Hover,
    /// A card sliding across the felt (gathering, collecting a trick).
    CardSlide,
    /// A card landing on the table.
    CardPlace,
    /// A trick won (play pitched up for streaks).
    Chips,
    /// One card dealt.
    Deal,
    /// A card turning over.
    Flip,
    /// A player passes.
    Pass,
    /// Trump has been made.
    Trump,
    /// Clicking something you can't do.
    Deny,
    /// Riffle shuffle.
    Shuffle,
    /// Score ticking up.
    Tick,
    /// Panels popping open.
    Pop,
    /// Good news: euchred them, march, victory.
    Fanfare,
    /// Bad news: we got euchred, defeat.
    Womp,
}

impl Sound {
    const ALL: [Sound; 15] = [
        Sound::Select,
        Sound::Hover,
        Sound::CardSlide,
        Sound::CardPlace,
        Sound::Chips,
        Sound::Deal,
        Sound::Flip,
        Sound::Pass,
        Sound::Trump,
        Sound::Deny,
        Sound::Shuffle,
        Sound::Tick,
        Sound::Pop,
        Sound::Fanfare,
        Sound::Womp,
    ];

    /// Base volume and how much the pitch may randomly vary (fraction).
    fn mix(self) -> (f32, f32) {
        match self {
            Sound::Select => (0.7, 0.04),
            Sound::Hover => (0.3, 0.10),
            Sound::CardSlide => (0.6, 0.12),
            Sound::CardPlace => (0.85, 0.08),
            Sound::Chips => (0.6, 0.03),
            Sound::Deal => (0.5, 0.15),
            Sound::Flip => (0.55, 0.12),
            Sound::Pass => (0.5, 0.05),
            Sound::Trump => (0.6, 0.02),
            Sound::Deny => (0.5, 0.05),
            Sound::Shuffle => (0.7, 0.05),
            Sound::Tick => (0.45, 0.02),
            Sound::Pop => (0.5, 0.06),
            Sound::Fanfare => (0.6, 0.0),
            Sound::Womp => (0.6, 0.0),
        }
    }

    fn synthesize(self) -> SoundData {
        match self {
            Sound::Select => tone(&[660.0, 990.0], 0.11, Wave::Square, 0.16),
            Sound::Hover => tone(&[1320.0], 0.035, Wave::Triangle, 0.12),
            Sound::CardSlide => swish(0.16, 0.25),
            Sound::CardPlace => thump(0.09),
            Sound::Chips => tone(&[1568.0, 2093.0, 2637.0], 0.22, Wave::Triangle, 0.15),
            Sound::Deal => swish(0.075, 0.2),
            Sound::Flip => flip(0.05),
            Sound::Pass => sweep(420.0, 260.0, 0.12, Wave::Triangle, 0.16),
            Sound::Trump => tone(&[523.3, 659.3, 784.0, 1046.5], 0.36, Wave::Square, 0.11),
            Sound::Deny => sweep(150.0, 110.0, 0.12, Wave::Square, 0.12),
            Sound::Shuffle => riffle(0.4),
            Sound::Tick => tone(&[1760.0], 0.04, Wave::Square, 0.07),
            Sound::Pop => tone(&[880.0, 1318.5], 0.07, Wave::Triangle, 0.16),
            Sound::Fanfare => tone(&[523.3, 659.3, 784.0, 1046.5, 1318.5, 1568.0], 0.66, Wave::Square, 0.11),
            Sound::Womp => sweep(330.0, 150.0, 0.6, Wave::Triangle, 0.22),
        }
    }
}

pub struct Sfx {
    sources: Vec<(Sound, Source)>,
    music: Option<Source>,
}

impl Sfx {
    pub fn new(audio: &Audio) -> Self {
        let sources = Sound::ALL
            .iter()
            .map(|&s| {
                let src = audio.new_source_from_data(s.synthesize());
                src.set_volume(s.mix().0);
                (s, src)
            })
            .collect();

        let file_music = assets::find("assets/music/theme.ogg").and_then(|path| {
            audio
                .new_source(&path)
                .inspect_err(|e| assets::log_warn(&format!("music: {e}")))
                .ok()
        });
        let music = file_music.unwrap_or_else(|| audio.new_source_from_data(lounge_loop()));
        music.set_looping(true);
        Sfx { sources, music: Some(music) }
    }

    /// Plays `sound` as an independent voice with random pitch jitter.
    pub fn play(&self, sound: Sound) {
        self.play_pitched(sound, 1.0);
    }

    /// Like [`Sfx::play`] but shifted by `pitch` (e.g. rising pitch for combos).
    pub fn play_pitched(&self, sound: Sound, pitch: f32) {
        let Some((_, src)) = self.sources.iter().find(|(s, _)| *s == sound) else {
            return;
        };
        let jitter = sound.mix().1;
        let voice = src.duplicate();
        voice.set_pitch(pitch * random_range(1.0 - jitter, 1.0 + jitter));
        voice.play();
    }

    pub fn start_music(&self) {
        if let Some(m) = &self.music {
            m.play();
        }
    }

    /// `volume` is 0..=1 (the master volume is applied on top by the audio device).
    pub fn set_music_volume(&self, volume: f32) {
        if let Some(m) = &self.music {
            m.set_volume(volume * 0.55);
        }
    }
}

// ---------------------------------------------------------------------------
// Tiny synthesizer
// ---------------------------------------------------------------------------

const RATE: u32 = 44_100;

#[derive(Clone, Copy)]
enum Wave {
    Square,
    Triangle,
}

fn osc(wave: Wave, phase: f32) -> f32 {
    let p = phase.fract();
    match wave {
        Wave::Square => {
            if p < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        Wave::Triangle => 1.0 - 4.0 * (p - 0.5).abs(),
    }
}

/// A short arpeggio through `notes` (Hz) with a plucky envelope.
fn tone(notes: &[f32], seconds: f32, wave: Wave, gain: f32) -> SoundData {
    let notes = notes.to_vec();
    let step = seconds / notes.len() as f32;
    SoundData::generate(RATE, seconds, move |t| {
        let i = ((t / step) as usize).min(notes.len() - 1);
        let local = t - i as f32 * step;
        let env = (-local * 18.0).exp() * (1.0 - t / seconds).max(0.0).sqrt();
        osc(wave, notes[i] * t) * env * gain
    })
}

/// A pitch glide from `f0` to `f1` Hz.
fn sweep(f0: f32, f1: f32, seconds: f32, wave: Wave, gain: f32) -> SoundData {
    let mut phase = 0.0f32;
    SoundData::generate(RATE, seconds, move |t| {
        let k = t / seconds;
        phase += (f0 + (f1 - f0) * k) / RATE as f32;
        let env = (k * 40.0).min(1.0) * (1.0 - k).powf(1.5);
        osc(wave, phase) * env * gain
    })
}

/// Band-limited noise burst whose brightness sweeps down: a card swish.
fn swish(seconds: f32, gain: f32) -> SoundData {
    let mut rng = Rng::new(0x5eed);
    let mut low = 0.0f32;
    SoundData::generate(RATE, seconds, move |t| {
        let k = t / seconds;
        let white = rng.random() * 2.0 - 1.0;
        let cutoff = 0.35 * (1.0 - k) + 0.03;
        low += (white - low) * cutoff;
        let env = (k * 12.0).min(1.0) * (1.0 - k).powi(2);
        (white - low) * env * gain
    })
}

/// A soft low thump with a click on top: a card hitting the table.
fn thump(seconds: f32) -> SoundData {
    let mut rng = Rng::new(0xca7d);
    SoundData::generate(RATE, seconds, move |t| {
        let body = (TAU * (140.0 - t * 600.0).max(60.0) * t).sin() * (-t * 45.0).exp() * 0.5;
        let click = (rng.random() * 2.0 - 1.0) * (-t * 400.0).exp() * 0.25;
        body + click
    })
}

/// A papery snap: bright noise plus a tiny high ping.
fn flip(seconds: f32) -> SoundData {
    let mut rng = Rng::new(0xf11b);
    let mut low = 0.0f32;
    SoundData::generate(RATE, seconds, move |t| {
        let white = rng.random() * 2.0 - 1.0;
        low += (white - low) * 0.2;
        let snap = (white - low) * (-t * 120.0).exp() * 0.3;
        let ping = (TAU * 2200.0 * t).sin() * (-t * 90.0).exp() * 0.06;
        snap + ping
    })
}

/// A burst of tiny clicks that speeds up and slows down: cards riffling.
fn riffle(seconds: f32) -> SoundData {
    let mut rng = Rng::new(0x51ff);
    let mut low = 0.0f32;
    let clicks: Vec<f32> = (0..22)
        .map(|i| {
            let k = i as f32 / 22.0;
            (k + 0.08 * (k * TAU).sin()) * seconds * 0.9
        })
        .collect();
    SoundData::generate(RATE, seconds, move |t| {
        let white = rng.random() * 2.0 - 1.0;
        low += (white - low) * 0.25;
        let hp = white - low;
        let env: f32 = clicks
            .iter()
            .map(|&c| if t >= c { (-(t - c) * 260.0).exp() } else { 0.0 })
            .sum();
        hp * env.min(1.0) * 0.28
    })
}

fn midi(note: i32) -> f32 {
    440.0 * 2f32.powf((note - 69) as f32 / 12.0)
}

/// A 16-bar jazzy lounge loop: soft detuned pad, walking bass, a plucked
/// arpeggio and a quiet shaker. Mellow on purpose so it sits under the effects.
fn lounge_loop() -> SoundData {
    let bpm = 92.0;
    let beat = 60.0 / bpm;
    let bar = beat * 4.0;
    // Am7, Dm7, G7, Cmaj7, Fmaj7, Bm7b5, E7, Am7 (MIDI notes).
    let chords: [[i32; 4]; 8] = [
        [57, 60, 64, 67],
        [50, 57, 60, 65],
        [55, 59, 62, 65],
        [48, 55, 59, 64],
        [53, 57, 60, 64],
        [47, 57, 62, 65],
        [52, 56, 62, 64],
        [57, 60, 64, 67],
    ];
    let bars = 16;
    let n = (bar * bars as f32 * RATE as f32) as usize;
    let dt = 1.0 / RATE as f32;
    let mut out = vec![0.0f32; n];

    let mut pad_phase = [0.0f32; 8];
    let mut pad_lp = 0.0f32;
    let mut bass_phase = 0.0f32;
    let mut bass_lp = 0.0f32;
    let mut arp_phase = 0.0f32;
    let mut arp_lp = 0.0f32;
    let mut noise = Rng::new(0x10b);
    let mut hat_lp = 0.0f32;
    let walk = [0, 7, 12, 7];
    let arp_a = [0, 1, 2, 3, 2, 1, 2, 3];
    let arp_b = [3, 2, 1, 2, 0, 2, 3, 1];

    for (i, sample) in out.iter_mut().enumerate() {
        let t = i as f32 * dt;
        let bar_i = (t / bar) as usize;
        let chord = chords[bar_i % 8];
        let tb = t - bar_i as f32 * bar;

        // Pad: two detuned triangles per chord tone, slow swell each bar.
        let swell = (tb / 0.35).min(1.0) * ((bar - tb) / 0.35).min(1.0);
        let vib = 1.0 + 0.0025 * (TAU * 0.3 * t).sin();
        let mut pad = 0.0;
        for (k, &note) in chord.iter().enumerate() {
            let f = midi(note) * vib;
            pad_phase[k * 2] += f * dt;
            pad_phase[k * 2 + 1] += f * 1.0045 * dt;
            pad += osc(Wave::Triangle, pad_phase[k * 2]) + osc(Wave::Triangle, pad_phase[k * 2 + 1]);
        }
        pad_lp += (pad - pad_lp) * 0.06;
        let pad_out = pad_lp * swell * 0.05;

        // Walking bass on each beat.
        let beat_i = (tb / beat) as usize;
        let tbeat = tb - beat_i as f32 * beat;
        let root = chord[0] - 24 + if chord[0] < 52 { 12 } else { 0 };
        bass_phase += midi(root + walk[beat_i % 4]) * dt;
        let bass = osc(Wave::Triangle, bass_phase) * (-tbeat * 3.5).exp() * (tbeat / 0.01).min(1.0);
        bass_lp += (bass - bass_lp) * 0.15;
        let bass_out = bass_lp * 0.32;

        // Plucked arpeggio in eighths, a second pattern on the B section.
        let eighth = beat / 2.0;
        let e_i = (tb / eighth) as usize;
        let te = tb - e_i as f32 * eighth;
        let pattern = if bar_i >= 8 { arp_b } else { arp_a };
        let note = chord[pattern[e_i % 8]] + 12;
        arp_phase += midi(note) * dt;
        let arp = osc(Wave::Square, arp_phase) * (-te * 9.0).exp() * (te / 0.004).min(1.0);
        arp_lp += (arp - arp_lp) * 0.09;
        let arp_level = if bar_i >= 8 { 0.07 } else { 0.05 };
        let arp_out = arp_lp * arp_level;

        // Shaker on the off-beats.
        let white = noise.random() * 2.0 - 1.0;
        hat_lp += (white - hat_lp) * 0.5;
        let off = if e_i % 2 == 1 { 1.0 } else { 0.45 };
        let hat = (white - hat_lp) * (-te * 55.0).exp() * 0.035 * off;

        *sample = pad_out + bass_out + arp_out + hat;
    }

    // Normalize, then fade the very ends a hair so the loop point never clicks.
    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-6);
    let fade = (RATE as f32 * 0.01) as usize;
    let len = out.len();
    for (i, s) in out.iter_mut().enumerate() {
        let edge = (i.min(len - 1 - i) as f32 / fade as f32).min(1.0);
        *s = *s / peak * 0.6 * edge;
    }
    SoundData::from_samples(RATE, 1, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn music_loop_is_well_formed() {
        let d = lounge_loop();
        assert!(d.duration() > 30.0);
    }
}
