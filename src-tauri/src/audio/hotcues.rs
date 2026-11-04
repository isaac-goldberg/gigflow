/// Automated hot cue detection: finds an intro point, an outro point, and the
/// biggest energy-structure changes in between (drops / breakdowns / builds),
/// then snaps every cue to the track's beat grid so it lands on a downbeat
/// when triggered during a mix. This is a heuristic energy-contour analysis
/// (no true ML structure/section detection), good enough to seed hot cue pads
/// that a DJ can delete or re-trigger analysis on — not a claim of perfect
/// phrase detection.
pub struct HotCue {
    pub position_secs: f64,
    pub label: String,
}

const WINDOW_SECS: f64 = 2.0;
const HOP_SECS: f64 = 1.0;
const MIN_SPACING_SECS: f64 = 12.0;
const MAX_CUES: usize = 8;

pub fn detect_hot_cues(samples: &[f32], sample_rate: u32, bpm: f64) -> Vec<HotCue> {
    let duration_secs = samples.len() as f64 / sample_rate as f64;
    if duration_secs < 20.0 || bpm <= 0.0 {
        return vec![HotCue { position_secs: 0.0, label: "INTRO".to_string() }];
    }

    let window = (WINDOW_SECS * sample_rate as f64) as usize;
    let hop = (HOP_SECS * sample_rate as f64) as usize;
    let mut energy: Vec<f64> = Vec::new();
    let mut pos = 0usize;
    while pos + window <= samples.len() {
        let slice = &samples[pos..pos + window];
        let rms = (slice.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>() / slice.len() as f64).sqrt();
        energy.push(rms);
        pos += hop;
    }
    if energy.len() < 8 {
        return vec![HotCue { position_secs: 0.0, label: "INTRO".to_string() }];
    }

    // Smooth with a small moving average to suppress single-beat spikes.
    let smoothed = moving_average(&energy, 3);
    let max_energy = smoothed.iter().cloned().fold(0.0f64, f64::max).max(1e-6);
    let norm: Vec<f64> = smoothed.iter().map(|v| v / max_energy).collect();

    let beat_period = 60.0 / bpm;

    // Intro: first hop where energy crosses above 15% of peak (skip leading silence/filter sweep).
    let intro_idx = norm.iter().position(|v| *v > 0.15).unwrap_or(0);
    let intro_secs = snap_to_grid(intro_idx as f64 * HOP_SECS, beat_period);

    // Outro: walk back from the end to find where sustained energy drops below 30% of peak.
    let mut outro_idx = norm.len() - 1;
    for i in (0..norm.len()).rev() {
        if norm[i] > 0.30 {
            outro_idx = (i + 1).min(norm.len() - 1);
            break;
        }
    }
    let outro_secs = snap_to_grid(outro_idx as f64 * HOP_SECS, beat_period);

    // Structural change points: compare average energy in a look-back window
    // vs a look-ahead window at each point; large jumps mark drops/breakdowns.
    let look = 4; // ~4s either side
    let mut diffs = vec![0.0f64; norm.len()];
    for i in look..norm.len() - look.min(norm.len()) {
        if i + look >= norm.len() {
            break;
        }
        let before: f64 = norm[i - look..i].iter().sum::<f64>() / look as f64;
        let after: f64 = norm[i..i + look].iter().sum::<f64>() / look as f64;
        diffs[i] = after - before;
    }

    let mut candidates: Vec<(usize, f64)> = Vec::new();
    for i in 2..diffs.len() - 2 {
        let v = diffs[i];
        if v.abs() < 0.14 {
            continue;
        }
        let is_local_extreme = (v > 0.0 && v >= diffs[i - 1] && v >= diffs[i + 1])
            || (v < 0.0 && v <= diffs[i - 1] && v <= diffs[i + 1]);
        if is_local_extreme {
            candidates.push((i, v));
        }
    }
    // Strongest changes first.
    candidates.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap());

    let mut chosen: Vec<(f64, String)> = Vec::new();
    for (idx, diff) in candidates {
        let t = snap_to_grid(idx as f64 * HOP_SECS, beat_period);
        if t <= intro_secs + MIN_SPACING_SECS || t >= outro_secs - MIN_SPACING_SECS {
            continue;
        }
        if chosen.iter().any(|(ct, _)| (ct - t).abs() < MIN_SPACING_SECS) {
            continue;
        }
        let after_level = norm[idx.min(norm.len() - 1)];
        let label = if diff > 0.0 && after_level > 0.6 {
            "DROP"
        } else if diff > 0.0 {
            "BUILD"
        } else if after_level < 0.35 {
            "BREAKDOWN"
        } else {
            "TRANSITION"
        };
        chosen.push((t, label.to_string()));
        if chosen.len() >= MAX_CUES - 2 {
            break;
        }
    }

    let mut cues = vec![(intro_secs, "INTRO".to_string())];
    cues.extend(chosen);
    cues.push((outro_secs, "OUTRO".to_string()));
    cues.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    cues.dedup_by(|a, b| (a.0 - b.0).abs() < MIN_SPACING_SECS);
    cues.truncate(MAX_CUES);

    cues.into_iter()
        .map(|(position_secs, label)| HotCue { position_secs, label })
        .collect()
}

fn snap_to_grid(t: f64, beat_period: f64) -> f64 {
    if beat_period <= 0.0 {
        return t.max(0.0);
    }
    (t / beat_period).round() * beat_period
}

fn moving_average(data: &[f64], radius: usize) -> Vec<f64> {
    let n = data.len();
    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(radius);
            let hi = (i + radius + 1).min(n);
            data[lo..hi].iter().sum::<f64>() / (hi - lo) as f64
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic track: silence, quiet intro, a loud "drop" section,
    /// a quiet breakdown, a second loud section, then a fade to silence.
    fn synth_track(sample_rate: u32) -> Vec<f32> {
        let seg = |secs: f64, amp: f32, seed_start: u32| -> Vec<f32> {
            let n = (sample_rate as f64 * secs) as usize;
            let mut seed = seed_start;
            (0..n)
                .map(|_| {
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    let v = ((seed >> 16) & 0x7fff) as f32 / 32768.0 - 0.5;
                    v * 2.0 * amp
                })
                .collect()
        };
        let mut v = Vec::new();
        v.extend(seg(4.0, 0.05, 1)); // lead-in silence-ish
        v.extend(seg(15.0, 0.2, 2)); // intro
        v.extend(seg(20.0, 0.9, 3)); // drop
        v.extend(seg(15.0, 0.15, 4)); // breakdown
        v.extend(seg(20.0, 0.9, 5)); // second drop
        v.extend(seg(10.0, 0.03, 6)); // outro fade
        v
    }

    #[test]
    fn detects_intro_and_outro_and_structure() {
        let sample_rate = 44100;
        let samples = synth_track(sample_rate);
        let cues = detect_hot_cues(&samples, sample_rate, 128.0);

        assert!(cues.len() >= 3, "expected at least intro/change/outro, got {}", cues.len());
        assert_eq!(cues.first().unwrap().label, "INTRO");
        assert_eq!(cues.last().unwrap().label, "OUTRO");
        assert!(cues.windows(2).all(|w| w[0].position_secs <= w[1].position_secs));

        let has_rise = cues.iter().any(|c| c.label == "DROP" || c.label == "BUILD");
        assert!(has_rise, "expected at least one rising structural cue, got {:?}", cues.iter().map(|c| &c.label).collect::<Vec<_>>());
    }
}
