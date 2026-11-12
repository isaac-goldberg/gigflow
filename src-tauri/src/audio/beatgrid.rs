//! Local beat-grid fitting around a mix point.
//!
//! The track-level BPM stored in the library is a whole-track estimate,
//! typically only good to ±0.5 BPM. That is fine for sorting and
//! suggestions, but not for beatmatching: over a 16-beat overlap a 0.5 BPM
//! error drifts by ~60 ms, which is an obvious "galloping" flam. Real DJs
//! (and Spotify's mixer) beatmatch against an actual beat grid, so here we
//! refine both the exact beat period and the beat *phase* from the audio
//! right where the two tracks will overlap.

use super::dsp::Biquad;

pub(crate) const HOP: usize = 128;
const WIN: usize = 512;

/// A beat grid in sample units: beats fall at `anchor + k * period`.
#[derive(Clone, Copy, Debug)]
pub struct Grid {
    pub period: f64,
    pub anchor: f64,
}

impl Grid {
    /// Nearest beat to sample position `t`.
    pub fn snap(&self, t: f64) -> f64 {
        let k = ((t - self.anchor) / self.period).round();
        self.anchor + k * self.period
    }

    /// First beat at or after `t`.
    pub fn next_beat(&self, t: f64) -> f64 {
        let k = ((t - self.anchor) / self.period).ceil();
        self.anchor + k * self.period
    }
}

/// Kick-weighted onset strength, sampled every `HOP` samples over
/// `samples[start..end]`. Frame `i` describes the window centred at
/// `start + i*HOP + WIN/2`.
pub(crate) fn onset_envelope(samples: &[f32], sr: u32, start: usize, end: usize) -> Vec<f32> {
    let end = end.min(samples.len());
    if end <= start + WIN * 4 {
        return Vec::new();
    }
    let sr_f = sr as f32;
    let mut lp1 = Biquad::lowpass(sr_f, 150.0, 0.707);
    let mut lp2 = Biquad::lowpass(sr_f, 150.0, 0.707);
    let mut hp = Biquad::highpass(sr_f, 3000.0, 0.707);
    // Warm the filters up a little before the region so the first frames
    // don't contain a startup transient.
    let warm = start.saturating_sub(2048);
    for &s in &samples[warm..start] {
        lp2.process(lp1.process(s));
        hp.process(s);
    }
    let region = &samples[start..end];
    let low: Vec<f32> = region.iter().map(|&s| lp2.process(lp1.process(s))).collect();
    let high: Vec<f32> = region.iter().map(|&s| hp.process(s)).collect();

    let n = (region.len() - WIN) / HOP;
    let mut env = Vec::with_capacity(n);
    let mut prev_l = 0.0f32;
    let mut prev_h = 0.0f32;
    for i in 0..n {
        let o = i * HOP;
        let el = (low[o..o + WIN].iter().map(|v| v * v).sum::<f32>() / WIN as f32).sqrt();
        let eh = (high[o..o + WIN].iter().map(|v| v * v).sum::<f32>() / WIN as f32).sqrt();
        let fl = (el - prev_l).max(0.0);
        let fh = (eh - prev_h).max(0.0);
        prev_l = el;
        prev_h = eh;
        env.push(fl * 2.0 + fh);
    }
    // Subtract a local mean so sustained loudness doesn't bias the fit.
    let half = 16usize;
    let mut out = vec![0.0f32; env.len()];
    for i in 0..env.len() {
        let a = i.saturating_sub(half);
        let b = (i + half + 1).min(env.len());
        let mean = env[a..b].iter().sum::<f32>() / (b - a) as f32;
        out[i] = (env[i] - mean).max(0.0);
    }
    out
}

/// Fits a beat grid to `samples[start..end]`, searching periods within ±3%
/// of `approx_bpm`. Returns `None` when the region has no usable rhythm
/// (silence, beatless ambient intro) — callers fall back to the stored BPM.
pub fn fit_grid(samples: &[f32], sr: u32, approx_bpm: f64, start: usize, end: usize) -> Option<Grid> {
    if approx_bpm <= 0.0 {
        return None;
    }
    let env = onset_envelope(samples, sr, start, end);
    if env.len() < 64 {
        return None;
    }
    let total: f32 = env.iter().sum();
    if total <= 1e-6 {
        return None;
    }

    let base_period = 60.0 * sr as f64 / approx_bpm;
    const BINS: usize = 96;
    let mut best = (f64::MIN, base_period, 0usize);
    let mut scores = Vec::new();
    let steps = 240i32;
    for step in -steps..=steps {
        let period = base_period * (1.0 + step as f64 * 0.000125);
        let mut hist = [0.0f64; BINS];
        for (i, &v) in env.iter().enumerate() {
            if v <= 0.0 {
                continue;
            }
            let t = (i * HOP + WIN / 2) as f64;
            let ph = (t / period).fract();
            hist[((ph * BINS as f64) as usize).min(BINS - 1)] += v as f64;
        }
        // Score the peak bin with its neighbours (a slightly fuzzy comb).
        let mut local = (f64::MIN, 0usize);
        for b in 0..BINS {
            let s = hist[b] + 0.5 * (hist[(b + 1) % BINS] + hist[(b + BINS - 1) % BINS]);
            if s > local.0 {
                local = (s, b);
            }
        }
        scores.push(local.0);
        if local.0 > best.0 {
            best = (local.0, period, local.1);
        }
    }

    // Sharpness check: a real pulse makes the best period stand well above
    // the average; a beatless region gives a flat landscape.
    let mean_score = scores.iter().sum::<f64>() / scores.len() as f64;
    if best.0 < mean_score * 1.15 {
        return None;
    }

    // Refine phase: circular mean of onset mass near the winning bin.
    let period = best.1;
    let centre = (best.2 as f64 + 0.5) / BINS as f64;
    let (mut sx, mut sy) = (0.0f64, 0.0f64);
    for (i, &v) in env.iter().enumerate() {
        if v <= 0.0 {
            continue;
        }
        let t = (i * HOP + WIN / 2) as f64;
        let ph = (t / period).fract();
        let mut d = ph - centre;
        if d > 0.5 {
            d -= 1.0;
        }
        if d < -0.5 {
            d += 1.0;
        }
        if d.abs() < 0.08 {
            let a = ph * std::f64::consts::TAU;
            sx += v as f64 * a.cos();
            sy += v as f64 * a.sin();
        }
    }
    let phase = if sx == 0.0 && sy == 0.0 { centre } else { (sy.atan2(sx) / std::f64::consts::TAU).rem_euclid(1.0) };
    Some(Grid { period, anchor: start as f64 + phase * period })
}

/// Picks the beat period for B that is closest to A's, allowing for the
/// stored BPMs disagreeing by a factor of two (half/double-time detection),
/// and returns B's consumption speed relative to A (B samples per output
/// sample) along with B's effective period.
pub fn tempo_ratio(period_a: f64, period_b: f64) -> (f64, f64) {
    let mut best = (f64::MAX, period_b);
    for k in [0.5, 1.0, 2.0] {
        let pb = period_b * k;
        let d = (pb / period_a).ln().abs();
        if d < best.0 {
            best = (d, pb);
        }
    }
    (best.1 / period_a, best.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click_track(sr: u32, bpm: f64, offset_secs: f64, secs: f64) -> Vec<f32> {
        let n = (secs * sr as f64) as usize;
        let period = 60.0 * sr as f64 / bpm;
        let mut x = vec![0.0f32; n];
        let mut t = offset_secs * sr as f64;
        while (t as usize) < n {
            let s = t as usize;
            for j in 0..2000.min(n - s) {
                let env = (-(j as f32) / 300.0).exp();
                x[s + j] += (2.0 * std::f32::consts::PI * 55.0 * j as f32 / sr as f32).sin() * env;
            }
            t += period;
        }
        x
    }

    #[test]
    fn recovers_exact_period_and_phase() {
        let sr = 44100;
        let true_bpm = 127.6;
        let x = click_track(sr, true_bpm, 0.137, 40.0);
        // Stored BPM is off by half a BPM, as the library estimate often is.
        let g = fit_grid(&x, sr, 128.1, 0, x.len()).expect("grid");
        let true_period = 60.0 * sr as f64 / true_bpm;
        assert!((g.period - true_period).abs() / true_period < 0.0006, "period {} vs {}", g.period, true_period);
        // Beat positions should land within ~6 ms of the true onsets across the region.
        let true_anchor = 0.137 * sr as f64;
        for k in [0.0, 40.0, 80.0] {
            let truth = true_anchor + k * true_period;
            let got = g.snap(truth);
            assert!((got - truth).abs() < 0.006 * sr as f64, "beat {k}: {got} vs {truth}");
        }
    }

    #[test]
    fn silence_has_no_grid() {
        let x = vec![0.0f32; 44100 * 20];
        assert!(fit_grid(&x, 44100, 128.0, 0, x.len()).is_none());
    }

    #[test]
    fn half_time_bpm_is_folded() {
        let (speed, _) = tempo_ratio(100.0, 205.0);
        assert!((speed - 1.025).abs() < 1e-9);
    }
}
