use std::sync::atomic::{AtomicU32, Ordering};

const NUM_BANDS: usize = 3;
const FREQS: [f32; NUM_BANDS] = [80.0, 1000.0, 12000.0];
const Q: f32 = 0.707;

#[derive(Clone, Copy)]
struct Coeffs { b0: f32, b1: f32, b2: f32, a1: f32, a2: f32 }

#[derive(Clone, Copy, Default)]
struct State { x1: f32, x2: f32, y1: f32, y2: f32 }

impl State {
    #[inline(always)]
    fn process(&mut self, input: f32, c: &Coeffs) -> f32 {
        let out = c.b0 * input + c.b1 * self.x1 + c.b2 * self.x2 - c.a1 * self.y1 - c.a2 * self.y2;
        self.x2 = self.x1; self.x1 = input;
        self.y2 = self.y1; self.y1 = out;
        out
    }
}

fn low_shelf(sr: f32, freq: f32, gain_db: f32) -> Coeffs {
    let a = 10.0_f32.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f32::consts::PI * freq / sr;
    let (sin_w, cos_w) = (w0.sin(), w0.cos());
    let alpha = sin_w / (2.0 * Q);
    let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
    let a0 = (a + 1.0) + (a - 1.0) * cos_w + two_sqrt_a_alpha;
    Coeffs {
        b0: (a * ((a + 1.0) - (a - 1.0) * cos_w + two_sqrt_a_alpha)) / a0,
        b1: (2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w)) / a0,
        b2: (a * ((a + 1.0) - (a - 1.0) * cos_w - two_sqrt_a_alpha)) / a0,
        a1: (-2.0 * ((a - 1.0) + (a + 1.0) * cos_w)) / a0,
        a2: ((a + 1.0) + (a - 1.0) * cos_w - two_sqrt_a_alpha) / a0,
    }
}

fn high_shelf(sr: f32, freq: f32, gain_db: f32) -> Coeffs {
    let a = 10.0_f32.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f32::consts::PI * freq / sr;
    let (sin_w, cos_w) = (w0.sin(), w0.cos());
    let alpha = sin_w / (2.0 * Q);
    let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
    let a0 = (a + 1.0) - (a - 1.0) * cos_w + two_sqrt_a_alpha;
    Coeffs {
        b0: (a * ((a + 1.0) + (a - 1.0) * cos_w + two_sqrt_a_alpha)) / a0,
        b1: (-2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w)) / a0,
        b2: (a * ((a + 1.0) + (a - 1.0) * cos_w - two_sqrt_a_alpha)) / a0,
        a1: (2.0 * ((a - 1.0) - (a + 1.0) * cos_w)) / a0,
        a2: ((a + 1.0) - (a - 1.0) * cos_w - two_sqrt_a_alpha) / a0,
    }
}

fn peaking(sr: f32, freq: f32, gain_db: f32) -> Coeffs {
    let a = 10.0_f32.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f32::consts::PI * freq / sr;
    let (sin_w, cos_w) = (w0.sin(), w0.cos());
    let alpha = sin_w / (2.0 * Q);
    let a0 = 1.0 + alpha / a;
    Coeffs {
        b0: (1.0 + alpha * a) / a0,
        b1: (-2.0 * cos_w) / a0,
        b2: (1.0 - alpha * a) / a0,
        a1: (-2.0 * cos_w) / a0,
        a2: (1.0 - alpha / a) / a0,
    }
}

fn flat() -> Coeffs {
    Coeffs { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 }
}

fn compute_band(sr: f32, band: usize, gain_db: f32) -> Coeffs {
    if gain_db.abs() < 0.01 { return flat(); }
    match band {
        0 => low_shelf(sr, FREQS[0], gain_db),
        1 => peaking(sr, FREQS[1], gain_db),
        2 => high_shelf(sr, FREQS[2], gain_db),
        _ => flat(),
    }
}

/// Lock-free atomic f32 wrapper using AtomicU32 bit reinterpretation.
struct AtomicF32(AtomicU32);

impl AtomicF32 {
    fn new(v: f32) -> Self { Self(AtomicU32::new(v.to_bits())) }
    fn store(&self, v: f32) { self.0.store(v.to_bits(), Ordering::Relaxed); }
}

pub struct ParametricEQ {
    sample_rate: f32,
    gains: [AtomicF32; NUM_BANDS],
    coeffs: [Coeffs; NUM_BANDS],
    state_l: [State; NUM_BANDS],
    state_r: [State; NUM_BANDS],
}

impl ParametricEQ {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            gains: [AtomicF32::new(0.0), AtomicF32::new(0.0), AtomicF32::new(0.0)],
            coeffs: [flat(); NUM_BANDS],
            state_l: [State::default(); NUM_BANDS],
            state_r: [State::default(); NUM_BANDS],
        }
    }

    pub fn set_band(&mut self, band: usize, gain_db: f32) {
        if band >= NUM_BANDS { return; }
        let gain_db = gain_db.clamp(-12.0, 12.0);
        self.gains[band].store(gain_db);
        self.coeffs[band] = compute_band(self.sample_rate, band, gain_db);
    }

    pub fn reset_gains(&mut self) {
        for i in 0..NUM_BANDS {
            self.gains[i].store(0.0);
            self.coeffs[i] = flat();
        }
    }

    #[inline(always)]
    pub fn process(&mut self, mut left: f32, mut right: f32) -> (f32, f32) {
        for i in 0..NUM_BANDS {
            left = self.state_l[i].process(left, &self.coeffs[i]);
            right = self.state_r[i].process(right, &self.coeffs[i]);
        }
        (left, right)
    }
}
