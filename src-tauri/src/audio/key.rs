use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

const FRAME_SIZE: usize = 4096;
const HOP_SIZE: usize = 2048;
const MIN_FREQ: f32 = 55.0; // ~A1
const MAX_FREQ: f32 = 5000.0;

// Krumhansl-Kessler key profiles, tonic-relative (index 0 = tonic).
const MAJOR_PROFILE: [f64; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const MINOR_PROFILE: [f64; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];

const PITCH_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

// Camelot number for each major-key tonic (pitch class 0=C .. 11=B).
const MAJOR_CAMELOT: [u8; 12] = [8, 3, 10, 5, 12, 7, 2, 9, 4, 11, 6, 1];
// Camelot number for each minor-key tonic (pitch class 0=C .. 11=B).
const MINOR_CAMELOT: [u8; 12] = [5, 12, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10];

pub struct KeyResult {
    pub camelot: String,
    pub name: String,
    pub confidence: f64,
}

pub fn detect_key(samples: &[f32], sample_rate: u32) -> KeyResult {
    // Analyze a representative ~90s window from the middle of the track (skip intros/outros).
    let target_len = (sample_rate as usize) * 90;
    let samples = if samples.len() > target_len {
        let start = (samples.len() - target_len) / 2;
        &samples[start..start + target_len]
    } else {
        samples
    };

    if samples.len() < FRAME_SIZE * 2 {
        return KeyResult { camelot: "?".into(), name: "unknown".into(), confidence: 0.0 };
    }

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(FRAME_SIZE);
    let window: Vec<f32> = (0..FRAME_SIZE)
        .map(|i| {
            0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (FRAME_SIZE as f32 - 1.0)).cos()
        })
        .collect();

    let mut chroma = [0.0f64; 12];
    let n_frames = (samples.len() - FRAME_SIZE) / HOP_SIZE;

    for i in 0..n_frames {
        let start = i * HOP_SIZE;
        let mut buf: Vec<Complex<f32>> = (0..FRAME_SIZE)
            .map(|j| Complex::new(samples[start + j] * window[j], 0.0))
            .collect();
        fft.process(&mut buf);

        for k in 1..FRAME_SIZE / 2 {
            let freq = k as f32 * sample_rate as f32 / FRAME_SIZE as f32;
            if freq < MIN_FREQ || freq > MAX_FREQ {
                continue;
            }
            let mag = buf[k].norm() as f64;
            // pitch class relative to A4=440Hz, pc 9 = A
            let midi_pc = (69.0 + 12.0 * (freq as f64 / 440.0).log2()).round();
            let pc = ((midi_pc as i64 % 12) + 12) % 12;
            chroma[pc as usize] += mag;
        }
    }

    let total: f64 = chroma.iter().sum();
    if total <= 0.0 {
        return KeyResult { camelot: "?".into(), name: "unknown".into(), confidence: 0.0 };
    }
    for v in chroma.iter_mut() {
        *v /= total;
    }

    let mut best_score = f64::MIN;
    let mut second_best = f64::MIN;
    let mut best_tonic = 0usize;
    let mut best_is_major = true;

    for tonic in 0..12 {
        let major_score = correlate(&chroma, &MAJOR_PROFILE, tonic);
        let minor_score = correlate(&chroma, &MINOR_PROFILE, tonic);

        if major_score > best_score {
            second_best = best_score;
            best_score = major_score;
            best_tonic = tonic;
            best_is_major = true;
        } else if major_score > second_best {
            second_best = major_score;
        }

        if minor_score > best_score {
            second_best = best_score;
            best_score = minor_score;
            best_tonic = tonic;
            best_is_major = false;
        } else if minor_score > second_best {
            second_best = minor_score;
        }
    }

    let camelot_num = if best_is_major {
        MAJOR_CAMELOT[best_tonic]
    } else {
        MINOR_CAMELOT[best_tonic]
    };
    let camelot = format!("{}{}", camelot_num, if best_is_major { "B" } else { "A" });
    let name = format!(
        "{} {}",
        PITCH_NAMES[best_tonic],
        if best_is_major { "major" } else { "minor" }
    );

    // Confidence: separation between best and runner-up correlation, normalized.
    let confidence = ((best_score - second_best) / best_score.abs().max(0.001)).clamp(0.0, 1.0);

    KeyResult { camelot, name, confidence }
}

/// Pearson correlation between the chroma vector and a key profile rotated so
/// its tonic sits at `tonic`.
fn correlate(chroma: &[f64; 12], profile: &[f64; 12], tonic: usize) -> f64 {
    let rotated: Vec<f64> = (0..12).map(|i| profile[(i + 12 - tonic) % 12]).collect();

    let mean_c: f64 = chroma.iter().sum::<f64>() / 12.0;
    let mean_p: f64 = rotated.iter().sum::<f64>() / 12.0;

    let mut num = 0.0;
    let mut den_c = 0.0;
    let mut den_p = 0.0;
    for i in 0..12 {
        let dc = chroma[i] - mean_c;
        let dp = rotated[i] - mean_p;
        num += dc * dp;
        den_c += dc * dc;
        den_p += dp * dp;
    }
    if den_c <= 0.0 || den_p <= 0.0 {
        return 0.0;
    }
    num / (den_c.sqrt() * den_p.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthesize a sustained C major chord (C4+E4+G4, plus octave doubling)
    /// so the correct tonal center is known.
    fn c_major_chord(sample_rate: u32, seconds: f64) -> Vec<f32> {
        let n = (sample_rate as f64 * seconds) as usize;
        let freqs = [261.63, 329.63, 392.0, 523.25]; // C4 E4 G4 C5
        (0..n)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                freqs.iter().map(|f| (2.0 * std::f32::consts::PI * f * t).sin()).sum::<f32>() / freqs.len() as f32
            })
            .collect()
    }

    #[test]
    fn detects_c_major() {
        let sample_rate = 44100;
        let samples = c_major_chord(sample_rate, 10.0);
        let result = detect_key(&samples, sample_rate);
        assert_eq!(result.camelot, "8B", "expected C major (8B), got {} ({})", result.camelot, result.name);
    }

    #[test]
    fn camelot_relation_same_key() {
        assert_eq!(camelot_relation("8B", "8B"), "same key");
        assert_eq!(camelot_relation("8B", "8A"), "relative major/minor");
        assert_eq!(camelot_relation("8B", "9B"), "adjacent (perfect mix)");
        assert_eq!(camelot_relation("8B", "2B"), "clash");
    }
}

/// Camelot adjacency relationship between two keys, used by the suggestion engine.
pub fn camelot_relation(a: &str, b: &str) -> &'static str {
    let parse = |s: &str| -> Option<(u8, char)> {
        let letter = s.chars().last()?;
        let num_str = &s[..s.len() - 1];
        let num: u8 = num_str.parse().ok()?;
        Some((num, letter))
    };
    let (Some((na, la)), Some((nb, lb))) = (parse(a), parse(b)) else {
        return "unknown";
    };
    if na == nb && la == lb {
        "same key"
    } else if na == nb {
        "relative major/minor"
    } else if la == lb && (nb == (na % 12) + 1 || na == (nb % 12) + 1) {
        "adjacent (perfect mix)"
    } else if la != lb && (nb == (na % 12) + 1 || na == (nb % 12) + 1) {
        "energy shift"
    } else {
        "clash"
    }
}
