/// Downsamples a track into a fixed number of (min, max) amplitude pairs —
/// the same min/max-per-column representation Rekordbox's track overview or
/// a video editor's audio waveform uses, so a wide strip can show real
/// detail without holding onto (or re-decoding) the full sample buffer.
const NUM_BUCKETS: usize = 600;

pub fn compute_peaks(samples: &[f32]) -> Vec<(f32, f32)> {
    if samples.is_empty() {
        return Vec::new();
    }
    let bucket_size = (samples.len() as f64 / NUM_BUCKETS as f64).ceil() as usize;
    let bucket_size = bucket_size.max(1);

    samples
        .chunks(bucket_size)
        .map(|chunk| {
            let mut lo = 0.0f32;
            let mut hi = 0.0f32;
            for &s in chunk {
                if s < lo {
                    lo = s;
                }
                if s > hi {
                    hi = s;
                }
            }
            (lo, hi)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_gives_empty_output() {
        assert!(compute_peaks(&[]).is_empty());
    }

    #[test]
    fn captures_a_known_peak() {
        let mut samples = vec![0.0f32; 44100];
        samples[22050] = 0.8;
        samples[22051] = -0.6;
        let peaks = compute_peaks(&samples);
        let overall_max = peaks.iter().fold(0.0f32, |m, (_, hi)| m.max(*hi));
        let overall_min = peaks.iter().fold(0.0f32, |m, (lo, _)| m.min(*lo));
        assert!((overall_max - 0.8).abs() < 1e-6, "expected max ~0.8, got {overall_max}");
        assert!((overall_min - (-0.6)).abs() < 1e-6, "expected min ~-0.6, got {overall_min}");
    }

    #[test]
    fn short_input_does_not_panic_and_stays_bounded() {
        let samples = vec![0.5f32, -0.3, 0.1];
        let peaks = compute_peaks(&samples);
        assert!(!peaks.is_empty());
        assert!(peaks.len() <= NUM_BUCKETS);
    }

    #[test]
    fn long_input_is_downsampled_to_roughly_num_buckets() {
        let samples = vec![0.0f32; 44100 * 180]; // a 3-minute track
        let peaks = compute_peaks(&samples);
        // chunks() rounds up, so the count can land one bucket short of the
        // target depending on remainder — not an off-by-one bug to "fix".
        assert!(
            peaks.len() >= NUM_BUCKETS - 1 && peaks.len() <= NUM_BUCKETS,
            "expected ~{NUM_BUCKETS} buckets, got {}",
            peaks.len()
        );
    }
}
