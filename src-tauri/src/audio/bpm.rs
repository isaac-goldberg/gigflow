use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

const FRAME_SIZE: usize = 2048;
const HOP_SIZE: usize = 512;

pub struct BpmResult {
    pub bpm: f64,
    pub confidence: f64,
}

/// Estimate BPM from mono samples using spectral-flux onset detection +
/// autocorrelation of the onset envelope. Standard approach used by most
/// beat trackers (aubio, essentia) in simplified form.
pub fn detect_bpm(samples: &[f32], sample_rate: u32) -> BpmResult {
    // Cap analysis to the first ~3 minutes for speed; representative enough for a steady track.
    let max_samples = (sample_rate as usize) * 180;
    let samples = if samples.len() > max_samples {
        &samples[..max_samples]
    } else {
        samples
    };

    if samples.len() < FRAME_SIZE * 4 {
        return BpmResult { bpm: 0.0, confidence: 0.0 };
    }

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(FRAME_SIZE);

    let window: Vec<f32> = (0..FRAME_SIZE)
        .map(|i| {
            0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (FRAME_SIZE as f32 - 1.0)).cos()
        })
        .collect();

    let n_frames = (samples.len() - FRAME_SIZE) / HOP_SIZE;
    let mut onset_env: Vec<f32> = Vec::with_capacity(n_frames);
    let mut prev_mag = vec![0.0f32; FRAME_SIZE / 2];

    for i in 0..n_frames {
        let start = i * HOP_SIZE;
        let mut buf: Vec<Complex<f32>> = (0..FRAME_SIZE)
            .map(|j| Complex::new(samples[start + j] * window[j], 0.0))
            .collect();
        fft.process(&mut buf);

        let mut flux = 0.0f32;
        for k in 0..FRAME_SIZE / 2 {
            let mag = buf[k].norm();
            let diff = mag - prev_mag[k];
            if diff > 0.0 {
                flux += diff;
            }
            prev_mag[k] = mag;
        }
        onset_env.push(flux);
    }

    // Remove DC / slow-moving bias via local mean subtraction.
    let smooth_window = 43; // ~0.5s at this hop/sample-rate for 44.1kHz
    let mut detrended = vec![0.0f32; onset_env.len()];
    for i in 0..onset_env.len() {
        let lo = i.saturating_sub(smooth_window / 2);
        let hi = (i + smooth_window / 2).min(onset_env.len());
        let local_mean: f32 = onset_env[lo..hi].iter().sum::<f32>() / (hi - lo) as f32;
        detrended[i] = (onset_env[i] - local_mean).max(0.0);
    }

    let frame_duration_secs = HOP_SIZE as f64 / sample_rate as f64;

    // Search lag range corresponding to 60-200 BPM.
    let min_bpm = 60.0;
    let max_bpm = 200.0;
    let min_lag = (60.0 / max_bpm / frame_duration_secs).round() as usize;
    let max_lag = (60.0 / min_bpm / frame_duration_secs).round() as usize;
    let max_lag = max_lag.min(detrended.len().saturating_sub(1));

    if max_lag <= min_lag {
        return BpmResult { bpm: 0.0, confidence: 0.0 };
    }

    let mut best_lag = min_lag;
    let mut best_score = f64::MIN;
    let mut scores = vec![0.0f64; max_lag + 1];

    for lag in min_lag..=max_lag {
        let mut acc = 0.0f64;
        let n = detrended.len() - lag;
        for i in 0..n {
            acc += (detrended[i] as f64) * (detrended[i + lag] as f64);
        }
        let score = acc / n as f64;
        scores[lag] = score;
        if score > best_score {
            best_score = score;
            best_lag = lag;
        }
    }

    let mut bpm = 60.0 / (best_lag as f64 * frame_duration_secs);

    // Snap into the "DJ-friendly" 80-180 range via octave correction, since
    // autocorrelation often locks onto a half/double-tempo harmonic.
    while bpm < 80.0 {
        bpm *= 2.0;
    }
    while bpm > 180.0 {
        bpm /= 2.0;
    }

    // Confidence: how peaked the best lag is relative to the mean score.
    let mean_score: f64 = scores[min_lag..=max_lag].iter().sum::<f64>() / (max_lag - min_lag + 1) as f64;
    let confidence = if mean_score > 0.0 {
        ((best_score / mean_score) - 1.0).clamp(0.0, 1.0)
    } else {
        0.0
    };

    BpmResult { bpm, confidence }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic click track: short noise bursts spaced at `bpm`,
    /// so the correct answer is known exactly.
    fn click_track(bpm: f64, sample_rate: u32, seconds: f64) -> Vec<f32> {
        let n = (sample_rate as f64 * seconds) as usize;
        let mut samples = vec![0.0f32; n];
        let period_samples = (60.0 / bpm * sample_rate as f64) as usize;
        let mut seed: u32 = 12345;
        let mut i = 0;
        while i < n {
            for j in 0..200.min(n - i) {
                // cheap deterministic PRNG noise burst (has broadband spectral content)
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let v = ((seed >> 16) & 0x7fff) as f32 / 32768.0 - 0.5;
                samples[i + j] = v * (1.0 - j as f32 / 200.0);
            }
            i += period_samples;
        }
        samples
    }

    #[test]
    fn detects_approximate_bpm() {
        let sample_rate = 44100;
        let samples = click_track(128.0, sample_rate, 20.0);
        let result = detect_bpm(&samples, sample_rate);
        assert!(
            (result.bpm - 128.0).abs() < 3.0,
            "expected ~128 BPM, got {}",
            result.bpm
        );
    }
}
