use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

const FRAME_SIZE: usize = 2048;
const HOP_SIZE: usize = 1024;
const MAX_ANALYSIS_SECS: usize = 180;

// Loudness-war masters cluster tightly together in raw level, so RMS alone
// mostly tells you how hard a mastering engineer limited the track, not how
// musically energetic it is — that was the original bug (a brickwalled pop
// track could outscore an actual festival banger just by being mastered
// louder). So four of the five signals below are measured on a
// gain-normalized copy of the buffer and are ratios/rates, loudness-invariant
// by construction:
//
//   - tempo:      speed, from the already-detected BPM.
//   - onset rate: how rhythmically busy/dense the track is (kicks, hats,
//     claps, stabs per second) via spectral-flux peak-picking. This is the
//     main "is this actually driving or just chill" signal — a sparse
//     tropical-house track with one bass pluck and a vocal is NOT busy, even
//     if that pluck is loud and deep, while a four-on-the-floor banger with
//     hats on every 16th is. Without this, the formula had no way to tell
//     "chill, bass-forward, sparse arrangement" from "relentless, driving" —
//     which is exactly why a mellow track like Lonely Together, whose sparse
//     mix lets its bassline dominate the spectrum, was scoring as high as
//     actual festival bangers.
//   - bass share:  how much of the spectrum sits in the bass band. Kept, but
//     at a reduced weight since on its own it rewards any sparse,
//     bass-forward mix (chill or not) rather than specifically a driving
//     low end.
//   - brightness: a mild tie-breaker (more hi-hat/air reads as more
//     energetic than a dark mix), smallest weight of the loudness-invariant
//     signals since it's the weakest on its own.
//
// The fifth signal is deliberately NOT loudness-invariant: a small, bounded
// `loudness_score` measured from the track's actual (un-normalized) RMS, in
// dB rather than linear so a merely-normal master and a moderately loud one
// aren't miles apart. A track mastered louder really does hit harder in the
// booth — that's real signal, not just an artifact, and Spotify's own
// energy feature factors it in too — but it's exactly what caused the
// original bug when it was 55% of the score, so it gets a small slice
// borrowed proportionally from the other four, not reinstated at its old
// weight.
//
// Calibrated against measured spread across a real ~30-track commercial
// dance/pop/EDM library (see src/audio/diagnostic_test.rs), not guessed, and
// cross-checked against Spotify's own (now-deprecated but still
// third-party-cached) "Energy" audio feature for ~10 of those tracks.
//
// A "spectral fullness" signal (fraction of frequency bins carrying
// meaningful energy per frame, meant to catch big-room/progressive-house
// anthems that are wall-of-synths dense without being especially busy
// percussively) was tried and measured against that same Spotify reference
// set at two different weights. Both made the *average* match worse, not
// better — it meaningfully helped exactly one track (Feel So Close) while
// costing ground on several others, because the weight it needed had to come
// from the four signals above, which were doing more of the actual work.
// Removed rather than shipped as a net regression; see git history if
// revisiting this.
const TARGET_RMS: f64 = 0.2;
const CENTROID_LO_HZ: f64 = 2500.0;
const CENTROID_HI_HZ: f64 = 5200.0;
const BASS_BAND_LO_HZ: f64 = 20.0;
const BASS_BAND_HI_HZ: f64 = 250.0;
const BASS_RATIO_LO: f64 = 0.08;
const BASS_RATIO_HI: f64 = 0.28;
const ONSET_RATE_LO: f64 = 1.5;
const ONSET_RATE_HI: f64 = 5.0;
// Dance/pop tracks in practice run ~98-132 BPM; widened a little past that
// so a future techno/trance addition to the library doesn't immediately pin
// the ceiling, but tight enough that the realistic range actually spreads out.
const TEMPO_LO_BPM: f64 = 95.0;
const TEMPO_HI_BPM: f64 = 140.0;
// Observed raw RMS across the reference library ran ~0.15 (quietest) to
// ~0.57 (Pop Dat Thang, the loudest outlier) -> roughly -16.5dB to -4.9dB.
// Floor/ceiling are set a bit outside that so neither end pins at 0 or 1.
const LOUDNESS_DB_LO: f64 = -18.0;
const LOUDNESS_DB_HI: f64 = -6.0;

const TEMPO_WEIGHT: f64 = 0.22;
const ONSET_WEIGHT: f64 = 0.35;
const BASS_WEIGHT: f64 = 0.18;
const BRIGHTNESS_WEIGHT: f64 = 0.13;
const LOUDNESS_WEIGHT: f64 = 0.12;

// Individual components are only consumed by the (test-only) diagnostic
// dump, not by the production `compute_energy` path, which just reads
// `.energy` — kept here so that dump can show why a track scored what it did.
#[allow(dead_code)]
pub struct EnergyComponents {
    pub tempo_score: f64,
    pub onset_score: f64,
    pub bass_score: f64,
    pub brightness_score: f64,
    pub loudness_score: f64,
    pub onset_rate_per_sec: f64,
    pub bass_ratio: f64,
    pub centroid_hz: f64,
    pub loudness_db: f64,
    pub energy: f64,
}

pub fn compute_energy(samples: &[f32], sample_rate: u32, bpm: f64) -> f64 {
    compute_energy_components(samples, sample_rate, bpm).energy
}

pub fn compute_energy_components(samples: &[f32], sample_rate: u32, bpm: f64) -> EnergyComponents {
    if samples.is_empty() {
        return EnergyComponents {
            tempo_score: 0.0,
            onset_score: 0.0,
            bass_score: 0.0,
            brightness_score: 0.0,
            loudness_score: 0.0,
            onset_rate_per_sec: 0.0,
            bass_ratio: 0.0,
            centroid_hz: 0.0,
            loudness_db: LOUDNESS_DB_LO,
            energy: 0.0,
        };
    }

    let max_samples = (sample_rate as usize) * MAX_ANALYSIS_SECS;
    let raw = if samples.len() > max_samples { &samples[..max_samples] } else { samples };

    let raw_rms = rms_of(raw);
    let analysis = normalize_to_reference_level(raw, raw_rms);
    let features = spectral_features(&analysis, sample_rate);

    let brightness_score =
        ((features.centroid_hz - CENTROID_LO_HZ) / (CENTROID_HI_HZ - CENTROID_LO_HZ)).clamp(0.0, 1.0);
    let bass_score = ((features.bass_ratio - BASS_RATIO_LO) / (BASS_RATIO_HI - BASS_RATIO_LO)).clamp(0.0, 1.0);
    let onset_score =
        ((features.onset_rate_per_sec - ONSET_RATE_LO) / (ONSET_RATE_HI - ONSET_RATE_LO)).clamp(0.0, 1.0);

    let tempo_score = if bpm > 0.0 {
        ((bpm - TEMPO_LO_BPM) / (TEMPO_HI_BPM - TEMPO_LO_BPM)).clamp(0.0, 1.0)
    } else {
        0.5
    };

    // dB, not linear RMS: a merely-normal master and a moderately loud one
    // should be close together, with only genuinely loudness-war-brickwalled
    // masters pushing toward the top of this (small-weighted) component.
    let loudness_db = if raw_rms > 1e-9 { 20.0 * raw_rms.log10() } else { LOUDNESS_DB_LO };
    let loudness_score = ((loudness_db - LOUDNESS_DB_LO) / (LOUDNESS_DB_HI - LOUDNESS_DB_LO)).clamp(0.0, 1.0);

    let energy = (TEMPO_WEIGHT * tempo_score
        + ONSET_WEIGHT * onset_score
        + BASS_WEIGHT * bass_score
        + BRIGHTNESS_WEIGHT * brightness_score
        + LOUDNESS_WEIGHT * loudness_score)
        .clamp(0.0, 1.0);

    EnergyComponents {
        tempo_score,
        onset_score,
        bass_score,
        brightness_score,
        loudness_score,
        onset_rate_per_sec: features.onset_rate_per_sec,
        bass_ratio: features.bass_ratio,
        centroid_hz: features.centroid_hz,
        loudness_db,
        energy,
    }
}

fn rms_of(samples: &[f32]) -> f64 {
    let sum_sq: f64 = samples.iter().map(|s| (*s as f64) * (*s as f64)).sum();
    (sum_sq / samples.len() as f64).sqrt()
}

/// Rescales the whole buffer so its RMS sits at `TARGET_RMS`, removing
/// mastering-loudness differences before any of the loudness-invariant
/// signals are measured. `rms` is the buffer's own already-computed RMS
/// (also used separately, un-normalized, for the small loudness component).
fn normalize_to_reference_level(samples: &[f32], rms: f64) -> Vec<f32> {
    if rms <= 1e-9 {
        return samples.to_vec();
    }
    let gain = (TARGET_RMS / rms) as f32;
    samples.iter().map(|s| s * gain).collect()
}

struct SpectralFeatures {
    centroid_hz: f64,
    bass_ratio: f64,
    onset_rate_per_sec: f64,
}

/// Spectral centroid, bass-band share, and onset rate — all derived from one
/// pass of windowed FFT frames over the (already level-normalized) signal.
fn spectral_features(samples: &[f32], sample_rate: u32) -> SpectralFeatures {
    if samples.len() < FRAME_SIZE * 2 {
        return SpectralFeatures { centroid_hz: CENTROID_LO_HZ, bass_ratio: BASS_RATIO_LO, onset_rate_per_sec: ONSET_RATE_LO };
    }

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(FRAME_SIZE);
    let window: Vec<f32> = (0..FRAME_SIZE)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (FRAME_SIZE as f32 - 1.0)).cos())
        .collect();

    let n_bins = FRAME_SIZE / 2;
    let n_frames = (samples.len() - FRAME_SIZE) / HOP_SIZE;
    let mut weighted_sum = 0.0f64;
    let mut mag_sum = 0.0f64;
    let mut bass_sum = 0.0f64;
    let mut prev_mag: Vec<f64> = vec![0.0; n_bins];
    let mut novelty: Vec<f64> = Vec::with_capacity(n_frames);

    for i in 0..n_frames {
        let start = i * HOP_SIZE;
        let mut buf: Vec<Complex<f32>> = (0..FRAME_SIZE)
            .map(|j| Complex::new(samples[start + j] * window[j], 0.0))
            .collect();
        fft.process(&mut buf);

        let mut flux = 0.0f64;
        for k in 1..n_bins {
            let freq = k as f64 * sample_rate as f64 / FRAME_SIZE as f64;
            let mag = buf[k].norm() as f64;
            weighted_sum += freq * mag;
            mag_sum += mag;
            if freq >= BASS_BAND_LO_HZ && freq <= BASS_BAND_HI_HZ {
                bass_sum += mag;
            }
            // Half-wave rectified spectral flux: only count bins that got
            // louder than the previous frame — a standard onset-detection
            // novelty function (rising energy marks a new hit; decay doesn't).
            let diff = mag - prev_mag[k];
            if diff > 0.0 {
                flux += diff;
            }
            prev_mag[k] = mag;
        }
        novelty.push(flux);
    }

    let centroid_hz = if mag_sum <= 0.0 { CENTROID_LO_HZ } else { weighted_sum / mag_sum };
    let bass_ratio = if mag_sum <= 0.0 { BASS_RATIO_LO } else { bass_sum / mag_sum };

    let hop_secs = HOP_SIZE as f64 / sample_rate as f64;
    let analyzed_secs = n_frames as f64 * hop_secs;
    let onset_rate_per_sec = if analyzed_secs > 0.0 {
        count_onsets(&novelty) as f64 / analyzed_secs
    } else {
        ONSET_RATE_LO
    };

    SpectralFeatures { centroid_hz, bass_ratio, onset_rate_per_sec }
}

/// Peak-picks the novelty (spectral flux) curve: a frame is an onset if it's
/// a local max comfortably above the curve's own average activity, with a
/// short refractory gap afterwards so one hit's decay tail isn't counted
/// multiple times.
fn count_onsets(novelty: &[f64]) -> usize {
    if novelty.len() < 3 {
        return 0;
    }
    let mean: f64 = novelty.iter().sum::<f64>() / novelty.len() as f64;
    let variance: f64 = novelty.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / novelty.len() as f64;
    let std_dev = variance.sqrt();
    let threshold = mean + 0.8 * std_dev;

    const REFRACTORY_FRAMES: usize = 3;
    let mut count = 0usize;
    let mut last_onset: Option<usize> = None;
    for i in 1..novelty.len() - 1 {
        if novelty[i] <= threshold || novelty[i] < novelty[i - 1] || novelty[i] < novelty[i + 1] {
            continue;
        }
        if let Some(last) = last_onset {
            if i - last < REFRACTORY_FRAMES {
                continue;
            }
        }
        count += 1;
        last_onset = Some(i);
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, sample_rate: u32, seconds: f64, amp: f32) -> Vec<f32> {
        let n = (sample_rate as f64 * seconds) as usize;
        (0..n)
            .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate as f32).sin())
            .collect()
    }

    /// Pulses `hits_per_sec` short percussive-ish clicks per second into an
    /// otherwise silent buffer — a crude stand-in for "busy" vs "sparse"
    /// rhythmic content, independent of tone/pitch.
    fn pulse_train(sample_rate: u32, seconds: f64, hits_per_sec: f64, amp: f32) -> Vec<f32> {
        let n = (sample_rate as f64 * seconds) as usize;
        let period = (sample_rate as f64 / hits_per_sec) as usize;
        let click_len = 60usize;
        let mut buf = vec![0.0f32; n];
        let mut pos = 0usize;
        while pos < n {
            for j in 0..click_len.min(n - pos) {
                // A short decaying burst of a mid tone reads clearly as a
                // percussive hit to the FFT across several bins.
                let t = j as f32 / sample_rate as f32;
                buf[pos + j] += amp * (1.0 - j as f32 / click_len as f32) * (2.0 * std::f32::consts::PI * 900.0 * t).sin();
            }
            pos += period;
        }
        buf
    }

    #[test]
    fn mastering_loudness_has_a_small_bounded_effect_on_score() {
        // Same musical content (tone, tempo), mastered at very different
        // levels — a quiet, dynamic-feeling master and a loudness-war
        // brickwalled one. A louder master is allowed to score a bit higher
        // now (it really does hit harder), but only by up to LOUDNESS_WEIGHT
        // — it must never again be the dominant driver, which was the
        // original "Pop Dat Thang at 93 vs Wake Me Up at 62" bug.
        let sr = 44100;
        let quiet_master = sine(1200.0, sr, 10.0, 0.04);
        let loud_master = sine(1200.0, sr, 10.0, 0.35);
        let e_quiet = compute_energy(&quiet_master, sr, 126.0);
        let e_loud = compute_energy(&loud_master, sr, 126.0);
        assert!(e_loud > e_quiet, "expected the louder master to score at least a bit higher");
        assert!(
            e_loud - e_quiet <= LOUDNESS_WEIGHT + 0.01,
            "expected mastering level to have only a small, bounded effect on energy, got quiet={} loud={} (diff {})",
            e_quiet,
            e_loud,
            e_loud - e_quiet
        );
    }

    #[test]
    fn faster_tempo_scores_higher() {
        let sr = 44100;
        let samples = sine(1200.0, sr, 10.0, 0.2);
        let e_slow = compute_energy(&samples, sr, 90.0);
        let e_fast = compute_energy(&samples, sr, 170.0);
        assert!(e_fast > e_slow, "expected faster bpm to score higher energy");
    }

    #[test]
    fn busier_rhythm_scores_higher_than_sparse_bass_only_track() {
        // This is the "Lonely Together" bug: a sparse track whose spectrum
        // is dominated by one loud bass element must NOT outscore a busy,
        // rhythmically dense one just because its bass *ratio* is high.
        let sr = 44100;
        // Amplitudes chosen so total signal energy (and so raw RMS / the
        // loudness component) is comparable between the two — isolating the
        // comparison to rhythmic density, not mastering level.
        let sparse_bassy = pulse_train(sr, 20.0, 1.0, 0.49); // ~1 hit/sec
        let busy = pulse_train(sr, 20.0, 6.0, 0.2); // ~6 hits/sec, four-on-the-floor-plus-hats territory
        let e_sparse = compute_energy(&sparse_bassy, sr, 103.0);
        let e_busy = compute_energy(&busy, sr, 103.0);
        assert!(
            e_busy > e_sparse,
            "expected rhythmically busy track to score higher than a sparse one, got sparse={} busy={}",
            e_sparse,
            e_busy
        );
    }

    #[test]
    fn bassier_mix_scores_higher() {
        let sr = 44100;
        let bassy = sine(80.0, sr, 10.0, 0.2);
        let trebly = sine(4000.0, sr, 10.0, 0.2);
        let e_bassy = compute_energy(&bassy, sr, 126.0);
        let e_trebly = compute_energy(&trebly, sr, 126.0);
        assert!(
            e_bassy > e_trebly,
            "expected a bass-heavy mix to score higher on the bass component than a trebly one"
        );
    }
}
