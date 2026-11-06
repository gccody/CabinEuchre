//! Animation helpers. The presentation leans heavily on overshooting springs
//! and wobble, so everything that moves (cards, text, buttons) should go
//! through these rather than snapping.
//!
//! They also notice motion: a spring or [`approach`] that's still on its way
//! calls [`keep_awake`], and the app draws at the full frame rate until things
//! settle (see `app::frame_cap`). Other quick motion (a shake, flying particles)
//! calls it too. Slow ambient motion (wobble, pulses, the lamp) doesn't, and
//! plays fine at the idle rate.

use std::sync::atomic::{AtomicBool, Ordering};

static AWAKE: AtomicBool = AtomicBool::new(false);

/// Something on screen is moving quickly this frame, so the next few frames
/// should be drawn at the full rate.
pub fn keep_awake() {
    AWAKE.store(true, Ordering::Relaxed);
}

/// Whether anything called [`keep_awake`] since the last call, and starts over.
pub fn take_awake() -> bool {
    AWAKE.swap(false, Ordering::Relaxed)
}

/// Closer than this to the target, at slower than [`REST_SPEED`], a spring is
/// at rest. Small enough that what's left can't be seen in pixels, scale or radians.
const REST_DISTANCE: f32 = 0.002;
const REST_SPEED: f32 = 0.02;

/// [`love2drs::math::approach`], noticing motion: eases `current` toward `target`.
pub fn approach(current: f32, target: f32, rate: f32, dt: f32) -> f32 {
    let next = love2drs::math::approach(current, target, rate, dt);
    if (next - target).abs() > REST_DISTANCE {
        keep_awake();
    }
    next
}

/// A damped spring that chases `target`. Kick it with [`Spring::impulse`] for
/// "pop" feedback.
#[derive(Debug, Clone, Copy)]
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
    pub target: f32,
    pub stiffness: f32,
    pub damping: f32,
}

impl Spring {
    /// A spring at rest at `value`, with a snappy, slightly bouncy feel.
    pub fn new(value: f32) -> Self {
        Spring {
            value,
            velocity: 0.0,
            target: value,
            stiffness: 260.0,
            damping: 14.0,
        }
    }

    /// Adds instantaneous velocity (e.g. `impulse(4.0)` on a scale spring for a pop).
    pub fn impulse(&mut self, velocity: f32) {
        self.velocity += velocity;
    }

    /// Advances the simulation. Sub-steps keep it stable at low frame rates.
    pub fn update(&mut self, dt: f32) {
        let steps = (dt / (1.0 / 240.0)).ceil().max(1.0) as u32;
        let h = dt / steps as f32;
        for _ in 0..steps {
            let accel = (self.target - self.value) * self.stiffness - self.velocity * self.damping;
            self.velocity += accel * h;
            self.value += self.velocity * h;
        }
        if (self.target - self.value).abs() > REST_DISTANCE || self.velocity.abs() > REST_SPEED {
            keep_awake();
        }
    }
}

#[allow(dead_code)] // scaffolding for card motion
/// Ease-out with overshoot, `t` in `0..=1`.
pub fn ease_out_back(t: f32) -> f32 {
    const C1: f32 = 1.701_58;
    const C3: f32 = C1 + 1.0;
    let t = t - 1.0;
    1.0 + C3 * t * t * t + C1 * t * t
}

#[allow(dead_code)] // scaffolding for card motion
/// Smooth ease-out, `t` in `0..=1`.
pub fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// Gentle idle wobble: a sine in `-amplitude..=amplitude`, phase-shifted by `seed`.
pub fn wobble(time: f32, seed: f32, speed: f32, amplitude: f32) -> f32 {
    (time * speed + seed * 1.618).sin() * amplitude
}
