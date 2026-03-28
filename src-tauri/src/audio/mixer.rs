use std::sync::atomic::{AtomicU32, Ordering};

/// Atomic f32 using bit-level transmute via AtomicU32.
pub struct AtomicF32 {
    bits: AtomicU32,
}

impl AtomicF32 {
    pub fn new(val: f32) -> Self {
        Self {
            bits: AtomicU32::new(val.to_bits()),
        }
    }

    pub fn load(&self, order: Ordering) -> f32 {
        f32::from_bits(self.bits.load(order))
    }

    pub fn store(&self, val: f32, order: Ordering) {
        self.bits.store(val.to_bits(), order);
    }
}

/// Shared mixer state, readable from audio thread via atomics.
pub struct MixerState {
    pub master_volume: AtomicF32,
    pub deck_a_gain: AtomicF32,
    pub deck_b_gain: AtomicF32,
}

impl MixerState {
    pub fn new() -> Self {
        Self {
            master_volume: AtomicF32::new(1.0),
            deck_a_gain: AtomicF32::new(1.0),
            deck_b_gain: AtomicF32::new(1.0),
        }
    }
}

impl Default for MixerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Fast sin approximation for [0, π/2] using Bhaskara I formula.
/// Max error <0.2%.
#[inline(always)]
fn fast_sin_half_pi(t: f32) -> f32 {
    if t <= 0.0 { return 0.0; }
    if t >= 1.0 { return 1.0; }
    let x = t * std::f32::consts::FRAC_PI_2;
    let pi = std::f32::consts::PI;
    let xpi = pi - x;
    (16.0 * x * xpi) / (5.0 * pi * pi - 4.0 * x * xpi)
}

/// Mix two stereo frames (left, right) from deck A and deck B given crossfade position and gains.
/// Returns (left, right) of the mixed output.
#[inline(always)]
pub fn mix_stereo(
    a_left: f32,
    a_right: f32,
    b_left: f32,
    b_right: f32,
    crossfade: f32,
    a_gain: f32,
    b_gain: f32,
    master: f32,
) -> (f32, f32) {
    // Equal-power crossfade using fast sin/cos approximation
    let cf = crossfade.clamp(0.0, 1.0);
    let sin_val = fast_sin_half_pi(cf);
    let cos_val = (1.0 - sin_val * sin_val).sqrt();
    let gain_a = cos_val * a_gain;
    let gain_b = sin_val * b_gain;

    let left = (a_left * gain_a + b_left * gain_b) * master;
    let right = (a_right * gain_a + b_right * gain_b) * master;

    (left, right)
}
