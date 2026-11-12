//! Small real-time DSP building blocks for the mix engine: the same pieces a
//! DJ mixer channel is made of — a 3-band kill EQ (isolator), a bipolar
//! low-pass/high-pass "filter" knob, and a tempo-synced echo.
//!
//! Everything here is per-sample, allocation-free after construction, and
//! keeps running even at unity settings so that turning a knob never
//! switches a processing path in or out mid-stream (which would click).

use std::f32::consts::PI;

/// RBJ-cookbook biquad, transposed direct form II.
#[derive(Clone, Copy, Default)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    pub fn lowpass(sr: f32, fc: f32, q: f32) -> Self {
        let w0 = 2.0 * PI * fc / sr;
        let (s, c) = w0.sin_cos();
        let alpha = s / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - c) / 2.0 / a0,
            b1: (1.0 - c) / a0,
            b2: (1.0 - c) / 2.0 / a0,
            a1: -2.0 * c / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    pub fn highpass(sr: f32, fc: f32, q: f32) -> Self {
        let w0 = 2.0 * PI * fc / sr;
        let (s, c) = w0.sin_cos();
        let alpha = s / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 + c) / 2.0 / a0,
            b1: -(1.0 + c) / a0,
            b2: (1.0 + c) / 2.0 / a0,
            a1: -2.0 * c / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

const BUTTERWORTH_Q: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// 4th-order Linkwitz-Riley crossover: LP + HP sum to an allpass (flat
/// magnitude), which is what lets the isolator sit at unity without
/// colouring the sound.
#[derive(Clone, Copy)]
struct Lr4 {
    lp: [Biquad; 2],
    hp: [Biquad; 2],
}

impl Lr4 {
    fn new(sr: f32, fc: f32) -> Self {
        Self {
            lp: [Biquad::lowpass(sr, fc, BUTTERWORTH_Q); 2],
            hp: [Biquad::highpass(sr, fc, BUTTERWORTH_Q); 2],
        }
    }

    #[inline]
    fn split(&mut self, x: f32) -> (f32, f32) {
        let lo = self.lp[0].process(x);
        let lo = self.lp[1].process(lo);
        let hi = self.hp[0].process(x);
        let hi = self.hp[1].process(hi);
        (lo, hi)
    }
}

/// One-pole parameter smoother so automation steps never zipper.
#[derive(Clone, Copy)]
pub struct Smooth {
    pub value: f32,
    coeff: f32,
}

impl Smooth {
    pub fn new(sr: f32, time_secs: f32, initial: f32) -> Self {
        Self { value: initial, coeff: (-1.0 / (time_secs * sr)).exp() }
    }

    #[inline]
    pub fn next(&mut self, target: f32) -> f32 {
        self.value = target + (self.value - target) * self.coeff;
        self.value
    }
}

/// DJ-mixer style 3-band isolator (low < 200 Hz < mid < 2.5 kHz < high).
/// A gain of 0 is a full kill of that band — that's what makes a clean bass
/// swap possible.
pub struct Isolator {
    x_low: Lr4,
    x_high: Lr4,
    // The low band never goes through the high crossover, so it gets the
    // matching allpass (LP+HP at the same frequency) to stay phase-aligned
    // with mid+high — otherwise the bands wouldn't sum flat at unity.
    low_ap: Lr4,
    g: [Smooth; 3],
}

impl Isolator {
    pub fn new(sr: f32) -> Self {
        Self {
            x_low: Lr4::new(sr, 200.0),
            x_high: Lr4::new(sr, 2500.0),
            low_ap: Lr4::new(sr, 2500.0),
            g: [Smooth::new(sr, 0.008, 1.0); 3],
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32, gains: [f32; 3]) -> f32 {
        let (low, rest) = self.x_low.split(x);
        let (la, lb) = self.low_ap.split(low);
        let low = la + lb;
        let (mid, high) = self.x_high.split(rest);
        let gl = self.g[0].next(gains[0]);
        let gm = self.g[1].next(gains[1]);
        let gh = self.g[2].next(gains[2]);
        low * gl + mid * gm + high * gh
    }
}

/// Bipolar DJ filter knob: -1..0 sweeps a low-pass down from 20 kHz to
/// 20 Hz, 0..1 sweeps a high-pass up from 20 Hz to 20 kHz, 0 is bypass.
/// Topology-preserving-transform state-variable filter (Simper), which stays
/// stable and click-free under fast cutoff modulation.
pub struct DjFilter {
    sr: f32,
    ic1: f32,
    ic2: f32,
    knob: Smooth,
    counter: u32,
    a1: f32,
    a2: f32,
    a3: f32,
    k: f32,
}

const FILTER_RESONANCE_Q: f32 = 0.9;

impl DjFilter {
    pub fn new(sr: f32) -> Self {
        let mut f = Self { sr, ic1: 0.0, ic2: 0.0, knob: Smooth::new(sr, 0.02, 0.0), counter: 0, a1: 0.0, a2: 0.0, a3: 0.0, k: 0.0 };
        f.update_coeffs(20.0);
        f
    }

    fn update_coeffs(&mut self, fc: f32) {
        let fc = fc.clamp(10.0, self.sr * 0.45);
        let g = (PI * fc / self.sr).tan();
        self.k = 1.0 / FILTER_RESONANCE_Q;
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    #[inline]
    pub fn process(&mut self, x: f32, knob_target: f32) -> f32 {
        let knob = self.knob.next(knob_target.clamp(-1.0, 1.0));
        if self.counter == 0 {
            let fc = if knob >= 0.0 { 20.0 * 1000f32.powf(knob) } else { 20000.0 * 0.001f32.powf(-knob) };
            self.update_coeffs(fc);
        }
        self.counter = (self.counter + 1) % 16;

        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        let low = v2;
        let high = x - self.k * v1 - v2;

        // Near the centre detent the filtered signal is already essentially
        // transparent (cutoff at the edge of hearing), so fading between dry
        // and filtered over that small range has nothing to click on.
        let amt = (knob.abs() / 0.03).min(1.0);
        let wet = if knob >= 0.0 { high } else { low };
        x + (wet - x) * amt
    }
}

/// Mono feedback delay with a darkening one-pole in the loop — the classic
/// beat-synced "echo out" effect.
pub struct Echo {
    buf: Vec<f32>,
    pos: usize,
    delay: usize,
    lp_state: f32,
}

impl Echo {
    pub fn new(max_samples: usize) -> Self {
        Self { buf: vec![0.0; max_samples.max(1)], pos: 0, delay: 1, lp_state: 0.0 }
    }

    pub fn set_delay(&mut self, samples: usize) {
        self.delay = samples.clamp(1, self.buf.len() - 1);
    }

    pub fn clear(&mut self) {
        self.buf.iter_mut().for_each(|s| *s = 0.0);
        self.lp_state = 0.0;
    }

    /// Feeds `input` in and returns the wet echo signal.
    #[inline]
    pub fn process(&mut self, input: f32, feedback: f32) -> f32 {
        let len = self.buf.len();
        let read = (self.pos + len - self.delay) % len;
        let delayed = self.buf[read];
        self.lp_state += (delayed - self.lp_state) * 0.35;
        self.buf[self.pos] = input + self.lp_state * feedback;
        self.pos = (self.pos + 1) % len;
        delayed
    }
}

/// Gentle soft-knee limiter for the master bus — two full-scale tracks can
/// sum past 0 dBFS for a moment mid-blend.
#[inline]
pub fn soft_limit(x: f32) -> f32 {
    const KNEE: f32 = 0.9;
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        let over = (a - KNEE) / (1.0 - KNEE);
        x.signum() * (KNEE + (1.0 - KNEE) * over.tanh())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(sr: f32, f: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| (2.0 * PI * f * i as f32 / sr).sin() * 0.5).collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn isolator_is_flat_at_unity() {
        let sr = 44100.0;
        for f in [60.0, 200.0, 1000.0, 2500.0, 8000.0] {
            let x = sine(sr, f, 44100);
            let mut iso = Isolator::new(sr);
            let y: Vec<f32> = x.iter().map(|&s| iso.process(s, [1.0, 1.0, 1.0])).collect();
            let ratio = rms(&y[22050..]) / rms(&x[22050..]);
            assert!((ratio - 1.0).abs() < 0.05, "{f} Hz: unity isolator gain {ratio}");
        }
    }

    #[test]
    fn isolator_kills_bass() {
        let sr = 44100.0;
        let x = sine(sr, 50.0, 44100);
        let mut iso = Isolator::new(sr);
        let y: Vec<f32> = x.iter().map(|&s| iso.process(s, [0.0, 1.0, 1.0])).collect();
        assert!(rms(&y[22050..]) < rms(&x[22050..]) * 0.1);
    }

    #[test]
    fn filter_bypass_is_transparent_and_hp_cuts_lows() {
        let sr = 44100.0;
        let x = sine(sr, 80.0, 44100);
        let mut f = DjFilter::new(sr);
        let y: Vec<f32> = x.iter().map(|&s| f.process(s, 0.0)).collect();
        assert!((rms(&y[22050..]) / rms(&x[22050..]) - 1.0).abs() < 0.01);
        let mut f = DjFilter::new(sr);
        let y: Vec<f32> = x.iter().map(|&s| f.process(s, 0.6)).collect();
        assert!(rms(&y[22050..]) < rms(&x[22050..]) * 0.2);
    }
}
