//! Animation helpers. The presentation leans heavily on overshooting springs
//! and wobble, so everything that moves (cards, text, buttons) should go
//! through these rather than snapping.

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
