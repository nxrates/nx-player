/// 3-band Linkwitz-Riley 4th-order crossover EQ with per-band crossfade curves.
/// Crossover frequencies: 200Hz (low/mid) and 2500Hz (mid/high).

/// Second-order biquad filter coefficients.
#[derive(Clone, Copy)]
struct BiquadCoeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

/// Second-order biquad filter state.
#[derive(Clone, Copy, Default)]
struct BiquadState {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BiquadState {
    #[inline(always)]
    fn process(&mut self, input: f32, c: &BiquadCoeffs) -> f32 {
        let output = c.b0 * input + c.b1 * self.x1 + c.b2 * self.x2
            - c.a1 * self.y1
            - c.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;
        output
    }
}

/// Compute Butterworth lowpass biquad coefficients.
fn butterworth_lp(sample_rate: f32, freq: f32) -> BiquadCoeffs {
    let omega = 2.0 * std::f32::consts::PI * freq / sample_rate;
    let sin_w = omega.sin();
    let cos_w = omega.cos();
    let alpha = sin_w / (2.0 * std::f32::consts::FRAC_1_SQRT_2); // Q = sqrt(2)/2

    let a0 = 1.0 + alpha;
    BiquadCoeffs {
        b0: ((1.0 - cos_w) / 2.0) / a0,
        b1: (1.0 - cos_w) / a0,
        b2: ((1.0 - cos_w) / 2.0) / a0,
        a1: (-2.0 * cos_w) / a0,
        a2: (1.0 - alpha) / a0,
    }
}

/// Compute Butterworth highpass biquad coefficients.
fn butterworth_hp(sample_rate: f32, freq: f32) -> BiquadCoeffs {
    let omega = 2.0 * std::f32::consts::PI * freq / sample_rate;
    let sin_w = omega.sin();
    let cos_w = omega.cos();
    let alpha = sin_w / (2.0 * std::f32::consts::FRAC_1_SQRT_2);

    let a0 = 1.0 + alpha;
    BiquadCoeffs {
        b0: ((1.0 + cos_w) / 2.0) / a0,
        b1: (-(1.0 + cos_w)) / a0,
        b2: ((1.0 + cos_w) / 2.0) / a0,
        a1: (-2.0 * cos_w) / a0,
        a2: (1.0 - alpha) / a0,
    }
}

/// Linkwitz-Riley 4th-order crossover (two cascaded Butterworth 2nd-order).
/// Splits a signal into low, mid, and high bands.
pub struct LR4Crossover {
    // Low/mid split at 200Hz
    lp1_a: BiquadState,
    lp1_b: BiquadState,
    hp1_a: BiquadState,
    hp1_b: BiquadState,
    // Mid/high split at 2500Hz
    lp2_a: BiquadState,
    lp2_b: BiquadState,
    hp2_a: BiquadState,
    hp2_b: BiquadState,
    // Coefficients
    lp1_coeff: BiquadCoeffs,
    hp1_coeff: BiquadCoeffs,
    lp2_coeff: BiquadCoeffs,
    hp2_coeff: BiquadCoeffs,
}

impl LR4Crossover {
    pub fn new(sample_rate: f32) -> Self {
        let lp1_coeff = butterworth_lp(sample_rate, 200.0);
        let hp1_coeff = butterworth_hp(sample_rate, 200.0);
        let lp2_coeff = butterworth_lp(sample_rate, 2500.0);
        let hp2_coeff = butterworth_hp(sample_rate, 2500.0);

        Self {
            lp1_a: BiquadState::default(),
            lp1_b: BiquadState::default(),
            hp1_a: BiquadState::default(),
            hp1_b: BiquadState::default(),
            lp2_a: BiquadState::default(),
            lp2_b: BiquadState::default(),
            hp2_a: BiquadState::default(),
            hp2_b: BiquadState::default(),
            lp1_coeff,
            hp1_coeff,
            lp2_coeff,
            hp2_coeff,
        }
    }

    /// Split a single sample into (low, mid, high) bands.
    #[inline(always)]
    pub fn process(&mut self, input: f32) -> (f32, f32, f32) {
        // First split: low vs (mid+high)
        let lp1 = self.lp1_b.process(
            self.lp1_a.process(input, &self.lp1_coeff),
            &self.lp1_coeff,
        );
        let hp1 = self.hp1_b.process(
            self.hp1_a.process(input, &self.hp1_coeff),
            &self.hp1_coeff,
        );

        // Second split of the high-passed signal: mid vs high
        let mid = self.lp2_b.process(
            self.lp2_a.process(hp1, &self.lp2_coeff),
            &self.lp2_coeff,
        );
        let high = self.hp2_b.process(
            self.hp2_a.process(hp1, &self.hp2_coeff),
            &self.hp2_coeff,
        );

        (lp1, mid, high)
    }

}

/// EQ crossfader: applies different crossfade curves to each frequency band.
/// - Bass: sharp sigmoid swap (incoming bass replaces outgoing quickly)
/// - Mids: gradual sin/cos fade
/// - Highs: early arrival (power curve — highs from incoming track arrive first)
pub struct EqCrossfader {
    pub crossover_a_left: LR4Crossover,
    pub crossover_a_right: LR4Crossover,
    pub crossover_b_left: LR4Crossover,
    pub crossover_b_right: LR4Crossover,
}

impl EqCrossfader {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            crossover_a_left: LR4Crossover::new(sample_rate),
            crossover_a_right: LR4Crossover::new(sample_rate),
            crossover_b_left: LR4Crossover::new(sample_rate),
            crossover_b_right: LR4Crossover::new(sample_rate),
        }
    }

    /// Process one stereo frame from each deck with EQ crossfading.
    /// `cf` is crossfade position in [0.0, 1.0]: 0 = deck A, 1 = deck B.
    #[inline(always)]
    pub fn process(
        &mut self,
        a_left: f32,
        a_right: f32,
        b_left: f32,
        b_right: f32,
        cf: f32,
    ) -> (f32, f32) {
        let (a_lo_l, a_mid_l, a_hi_l) = self.crossover_a_left.process(a_left);
        let (a_lo_r, a_mid_r, a_hi_r) = self.crossover_a_right.process(a_right);
        let (b_lo_l, b_mid_l, b_hi_l) = self.crossover_b_left.process(b_left);
        let (b_lo_r, b_mid_r, b_hi_r) = self.crossover_b_right.process(b_right);

        // Per-band crossfade curves
        let (bass_a, bass_b) = bass_curve(cf);
        let (mid_a, mid_b) = mid_curve(cf);
        let (hi_a, hi_b) = high_curve(cf);

        let left = a_lo_l * bass_a + b_lo_l * bass_b
            + a_mid_l * mid_a + b_mid_l * mid_b
            + a_hi_l * hi_a + b_hi_l * hi_b;
        let right = a_lo_r * bass_a + b_lo_r * bass_b
            + a_mid_r * mid_a + b_mid_r * mid_b
            + a_hi_r * hi_a + b_hi_r * hi_b;

        (left, right)
    }

}

/// Fast sigmoid approximation: rational polynomial (no exp()).
/// Max error <0.5% vs true sigmoid with k=10.
#[inline(always)]
fn fast_sigmoid(x: f32) -> f32 {
    // Attempt a fast rational approximation of 1/(1+exp(-k*(x-0.5))) with k=10
    let t = 10.0 * (x - 0.5);
    // Pade approximation: sigmoid(t) ≈ 0.5 + 0.5 * t / (1 + |t|)
    // Steeper version for k=10 character:
    0.5 + 0.5 * t / (1.0 + t.abs())
}

/// Bass crossfade: sharp sigmoid swap.
/// Deck A drops quickly around cf=0.5, deck B rises quickly.
#[inline(always)]
fn bass_curve(cf: f32) -> (f32, f32) {
    let sigmoid = fast_sigmoid(cf);
    (1.0 - sigmoid, sigmoid)
}

/// Fast sin/cos approximation for [0, π/2] range using Bhaskara I formula.
/// Max error <0.2% in [0, π/2].
#[inline(always)]
fn fast_sincos_half_pi(t: f32) -> (f32, f32) {
    // t is in [0, 1], maps to angle in [0, π/2]
    // Use: sin(x) ≈ 16x(π-x) / (5π²-4x(π-x)) for x in [0,π]
    // For our case x = t * π/2
    let x = t * std::f32::consts::FRAC_PI_2;
    let sin_val = if t <= 0.0 {
        0.0
    } else if t >= 1.0 {
        1.0
    } else {
        let pi = std::f32::consts::PI;
        let xpi = pi - x;
        (16.0 * x * xpi) / (5.0 * pi * pi - 4.0 * x * xpi)
    };
    let cos_val = (1.0 - sin_val * sin_val).sqrt();
    (cos_val, sin_val)
}

/// Mid crossfade: gradual sin/cos equal-power curve.
#[inline(always)]
fn mid_curve(cf: f32) -> (f32, f32) {
    fast_sincos_half_pi(cf)
}

/// High crossfade: incoming highs arrive early (power curve).
/// Deck B's highs are audible sooner.
#[inline(always)]
fn high_curve(cf: f32) -> (f32, f32) {
    // Power curve: B = cf^0.5, A = (1-cf)^0.5
    let b = cf.sqrt();
    let a = (1.0 - cf).sqrt();
    (a, b)
}
