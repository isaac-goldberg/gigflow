/// Automated hot cue detection: finds an intro point, an outro point, and the
/// biggest structural changes in between (drops / breakdowns / builds), then
/// snaps every cue to the track's actual beat/bar grid so it lands cleanly on
/// a downbeat — ideally a phrase boundary — when triggered during a mix.
///
/// This is a heuristic structural analysis (no true ML section-boundary
/// model), designed to seed hot cue pads a DJ can delete or re-trigger
/// analysis on. Three specific weaknesses in the original version motivated
/// a rewrite:
///
///  1. Phase-less grid snap. `t.round_to(beat_period)` from t=0 silently
///     assumes the first beat lands exactly at sample 0 — true only for a
///     click track, never for a real song (there's almost always a pickup/
///     lead-in offset). Every snapped cue could be off by up to half a beat
///     for no reason. Fixed by actually estimating the beat *phase* (see
///     `estimate_beat_phase`) the same way a beat tracker would: brute-force
///     search for the offset that best aligns a comb of predicted beat times
///     with the onset-novelty curve.
///  2. 1-second analysis resolution. A real drop is a one-bar (or faster)
///     event; finding its center within a whole second of RMS smoothing and
///     then "snapping" doesn't recover the lost precision — the snap just
///     picks whichever grid line happens to be nearest the already-blurry
///     estimate. Fixed by analyzing on ~11.6ms FFT-frame hops (the same
///     frame/hop as `bpm.rs`/`energy.rs`) and only aggregating into coarser
///     buckets *after* computing the fine-grained curves, for ~4x tighter
///     localization.
///  3. Broadband-only change detection. A brightness/riser swell (more energy
///     in the highs, no sub-bass) and an actual bass drop both show up as "a
///     rise in overall RMS" — the old code couldn't tell them apart, and had
///     no persistence check, so a single loud transient (a crash hit) could
///     register as a structural change just as easily as a real section
///     boundary. Fixed by tracking a separate sub/kick-band energy curve
///     specifically for drop confirmation, plus a short persistence window
///     so a one-off hit that decays within ~1s doesn't qualify.
///
/// Mid-track structural cues snap to the *bar* grid (4 beats, assuming 4/4 —
/// true for the overwhelming majority of EDM/dance material this is tuned
/// against) and prefer an 8-bar phrase boundary over a plain bar boundary
/// when one is close enough, since real drops and breakdowns in this genre
/// land on phrase boundaries far more often than on an arbitrary bar.
///
/// DROP has two independent, OR'd qualifying paths rather than one blended
/// score: the original bass-band confirmation (`qualifies_bass_drop`), plus
/// a filter-release path (`qualifies_filter_release_drop`) for
/// progressive-house/big-room drops that are a wall-of-synth filter release
/// rather than a sub-bass re-entry — the bass path alone never fires for
/// those. It requires both a wideband jump in a 2-10kHz band AND that the
/// jump be near-instant (`sharpness`, comparing a 1s-window before/after
/// diff to the 4s-window one) — either alone is too common in ordinary
/// multi-bar builds to be a reliable drop signature by itself. Keeping these
/// as two separately gated boolean paths (rather than blending a new signal
/// into one weighted score) matters: `energy.rs` tried exactly that shape of
/// fix — a "spectral fullness" signal blended into the whole-track energy
/// score for this same class of track — and had to revert it because the
/// weight it needed necessarily came out of signals doing more of the actual
/// work elsewhere, for a net regression. An OR'd independent path doesn't
/// have that failure mode structurally: it can only ever turn an
/// already-rising candidate into a DROP instead of a BUILD, never change an
/// existing correct answer elsewhere.
///
/// That said, the thresholds here are still real, calibrated boundaries, not
/// free parameters — this path catches a *strong, unambiguous* filter
/// release, not every progressive-house drop. Calvin Harris "Feel So
/// Close"/"Summer" are a known, deliberately *unresolved* gap: both have a
/// candidate drop point whose signal is real but just short of these
/// thresholds (e.g. Feel So Close: bass_diff 0.169 vs the 0.1 bar, but
/// after_level 0.552 vs the shared 0.6 floor; Summer: high_diff 0.389 vs the
/// 0.12 bar, but sharpness 0.426 vs the 0.55 bar). Loosening either shared
/// gate enough to cross those two specific margins — tried both as a flat
/// threshold drop and as a "strong signal on one axis compensates for a
/// weak one on another" exception — also flipped 10-14 *other*,
/// previously-correct tracks' BUILDs to DROP elsewhere in the same test
/// library (Beyonce/Jay-Z "Crazy In Love", Black Eyed Peas "Meet Me
/// Halfway", Ariana Grande "Break Free", more Calvin Harris tracks, etc.).
/// There is no way to tell, from aggregate energy curves alone and without
/// being able to listen to the audio, whether those newly-flipped labels are
/// correct catches or false positives — so this stays conservative rather
/// than shipping that trade blind.
pub struct HotCue {
    pub position_secs: f64,
    pub label: String,
}

const FRAME_SIZE: usize = 2048;
const HOP_SIZE: usize = 512;
const SUB_BAND_LO_HZ: f64 = 20.0;
const SUB_BAND_HI_HZ: f64 = 150.0;
// A filter-release band: where a riser/white-noise sweep "opens up" when a
// low-pass filter lifts on a progressive-house/big-room drop. These drops
// (e.g. Calvin Harris "Feel So Close"/"Summer") are wall-of-synth events, not
// sub-bass re-entries, so the bass-band confirmation below never fires for
// them on its own — see `qualifies_filter_release_drop`.
const HIGH_BAND_LO_HZ: f64 = 2000.0;
const HIGH_BAND_HI_HZ: f64 = 10000.0;

const BUCKET_SECS: f64 = 0.25;
const LOOK_SECS: f64 = 4.0;
const SHARPNESS_LOOK_SECS: f64 = 1.0;
const PERSIST_SECS: f64 = 2.0;
const MIN_SPACING_SECS: f64 = 12.0;
const MAX_CUES: usize = 8;

pub fn detect_hot_cues(samples: &[f32], sample_rate: u32, bpm: f64) -> Vec<HotCue> {
    let duration_secs = samples.len() as f64 / sample_rate as f64;
    if duration_secs < 20.0 || bpm <= 0.0 {
        return vec![HotCue { position_secs: 0.0, label: "INTRO".to_string() }];
    }

    let frames = analyze_frames(samples, sample_rate);
    if frames.broadband.len() < 8 {
        return vec![HotCue { position_secs: 0.0, label: "INTRO".to_string() }];
    }

    let beat_period = 60.0 / bpm;
    let beat_phase = estimate_beat_phase(&frames.novelty, &frames.bass_novelty, frames.hop_secs, beat_period);
    let bar_period = beat_period * 4.0;

    let bucket = to_buckets(&frames.broadband, frames.hop_secs, BUCKET_SECS);
    let bass_bucket = to_buckets(&frames.bass, frames.hop_secs, BUCKET_SECS);
    let high_bucket = to_buckets(&frames.high, frames.hop_secs, BUCKET_SECS);
    if bucket.len() < 8 {
        return vec![HotCue { position_secs: 0.0, label: "INTRO".to_string() }];
    }

    let norm = normalize(&bucket);
    let bass_norm = normalize(&bass_bucket);
    let high_norm = normalize(&high_bucket);

    // Intro: first bucket where energy crosses above 15% of peak (skip
    // leading silence/filter sweep), snapped to the phase-correct beat grid.
    let intro_idx = norm.iter().position(|v| *v > 0.15).unwrap_or(0);
    let intro_secs = snap_to_period(intro_idx as f64 * BUCKET_SECS, beat_period, beat_phase);

    // Outro: walk back from the end to find where sustained energy drops
    // below 30% of peak.
    let mut outro_idx = norm.len() - 1;
    for i in (0..norm.len()).rev() {
        if norm[i] > 0.30 {
            outro_idx = (i + 1).min(norm.len() - 1);
            break;
        }
    }
    let outro_secs = snap_to_period(outro_idx as f64 * BUCKET_SECS, beat_period, beat_phase);

    let look = (LOOK_SECS / BUCKET_SECS).round() as usize;
    let sharp_look = (SHARPNESS_LOOK_SECS / BUCKET_SECS).round() as usize;
    let persist = (PERSIST_SECS / BUCKET_SECS).round() as usize;

    let mut diffs = vec![0.0f64; norm.len()];
    for i in look..norm.len().saturating_sub(look) {
        let before: f64 = norm[i - look..i].iter().sum::<f64>() / look as f64;
        let after: f64 = norm[i..i + look].iter().sum::<f64>() / look as f64;
        diffs[i] = after - before;
    }

    // A short-window version of the same before/after comparison: how much
    // of the long-window rise already happened within ~1s of the candidate
    // point. A true filter-release drop is near-instant (short ≈ long); a
    // multi-bar riser/build spreads the same total rise out, so short << long.
    let mut diffs_short = vec![0.0f64; norm.len()];
    for i in sharp_look..norm.len().saturating_sub(sharp_look) {
        let before: f64 = norm[i - sharp_look..i].iter().sum::<f64>() / sharp_look as f64;
        let after: f64 = norm[i..i + sharp_look].iter().sum::<f64>() / sharp_look as f64;
        diffs_short[i] = after - before;
    }

    let mut candidates: Vec<(usize, f64)> = Vec::new();
    for i in 2..diffs.len().saturating_sub(2) {
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
    candidates.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap());

    let mut chosen: Vec<(f64, String)> = Vec::new();
    for (idx, diff) in candidates {
        let raw_secs = idx as f64 * BUCKET_SECS;
        let t = snap_to_phrase(raw_secs, beat_period, bar_period, beat_phase);
        if t <= intro_secs + MIN_SPACING_SECS || t >= outro_secs - MIN_SPACING_SECS {
            continue;
        }
        if chosen.iter().any(|(ct, _)| (ct - t).abs() < MIN_SPACING_SECS) {
            continue;
        }

        let after_level = norm[idx.min(norm.len() - 1)];
        let bass_before: f64 = bass_norm[idx.saturating_sub(look)..idx].iter().sum::<f64>().max(0.0)
            / look.min(idx).max(1) as f64;
        let bass_after_end = (idx + look).min(bass_norm.len());
        let bass_after: f64 = bass_norm[idx..bass_after_end].iter().sum::<f64>()
            / (bass_after_end - idx).max(1) as f64;
        let bass_diff = bass_after - bass_before;

        let high_before: f64 = high_norm[idx.saturating_sub(look)..idx].iter().sum::<f64>().max(0.0)
            / look.min(idx).max(1) as f64;
        let high_after_end = (idx + look).min(high_norm.len());
        let high_after: f64 = high_norm[idx..high_after_end].iter().sum::<f64>()
            / (high_after_end - idx).max(1) as f64;
        let high_diff = high_after - high_before;

        // How much of the long-window rise already happened within ~1s of
        // this point. 1.0 means the whole rise was essentially instant; a
        // slow multi-bar build spreads it out, so this stays well below 1.
        let sharpness = if diff.abs() > 1e-6 { (diffs_short[idx] / diff).clamp(0.0, 2.0) } else { 0.0 };

        // Persistence: a real structural change holds up for a couple of
        // seconds afterward. A single transient (crash hit, vocal stab)
        // decays back down almost immediately — don't let that register as
        // a drop/breakdown.
        let persist_end = (idx + persist).min(norm.len());
        let sustained: f64 = norm[idx..persist_end].iter().sum::<f64>() / (persist_end - idx).max(1) as f64;
        let holds = sustained >= after_level * 0.7;

        // Two independent, OR'd ways to qualify as a DROP rather than one
        // blended score — a progressive/big-room drop that fails the bass
        // path shouldn't have that path's weight diluted to accommodate it
        // (that tradeoff was tried and rejected for the whole-track energy
        // score in energy.rs; it doesn't apply here because these are
        // separately-gated boolean paths, not a weighted blend).
        let qualifies_bass_drop = diff > 0.0 && after_level > 0.6 && bass_diff > 0.1 && holds;
        // A riser/filter-sweep release: no sub-bass jump required, but the
        // rise has to be both near-instant (sharpness) and a genuine
        // wideband "opening up" (high_diff) — either alone is too common in
        // ordinary builds to be a reliable drop signature on its own.
        //
        // Thresholds below are the original, conservative calibration.
        // Loosening them (or the after_level floor shared with the bass
        // path above) is exactly what it would take to also flag Feel So
        // Close/Summer as DROP — tried both a flat threshold drop and a
        // "strong corroborating signal compensates for a weak one" version;
        // both fixed those two tracks but also flipped 10-14 *other*,
        // previously-correct tracks' BUILDs to DROP (Beyonce/Jay-Z "Crazy In
        // Love", Black Eyed Peas "Meet Me Halfway", Ariana Grande "Break
        // Free", several more Calvin Harris tracks, etc.) — see git history/
        // PR discussion for the exact numbers. There's no way to tell, from
        // aggregate energy curves alone and without being able to listen to
        // the audio, whether those are correct catches or false positives,
        // so shipping that trade blind is exactly the kind of unverified
        // regression this project avoids (same standard as the "spectral
        // fullness" signal rejected in energy.rs). Left conservative; Feel
        // So Close/Summer remain a known, documented gap rather than a
        // forced fix.
        let qualifies_filter_release_drop =
            diff > 0.0 && after_level > 0.6 && high_diff > 0.12 && sharpness > 0.55 && holds;

        let label = if qualifies_bass_drop || qualifies_filter_release_drop {
            "DROP"
        } else if diff > 0.0 {
            "BUILD"
        } else if after_level < 0.35 && holds {
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

struct FrameFeatures {
    broadband: Vec<f64>,
    bass: Vec<f64>,
    high: Vec<f64>,
    novelty: Vec<f64>,
    /// Spectral flux restricted to the sub/kick band only — see
    /// `estimate_beat_phase`'s doc comment for why this, not the full-band
    /// `novelty`, is what beat-phase locking actually searches against.
    bass_novelty: Vec<f64>,
    hop_secs: f64,
}

/// One windowed-FFT pass producing, per ~11.6ms hop: the broadband magnitude
/// sum (overall level), the sub/kick-band magnitude sum, the filter-release
/// band magnitude sum, and half-wave rectified spectral flux (onset novelty)
/// — the same primitives `energy.rs`/`bpm.rs` use, computed locally here to
/// keep this module self-contained.
fn analyze_frames(samples: &[f32], sample_rate: u32) -> FrameFeatures {
    use rustfft::num_complex::Complex;
    use rustfft::FftPlanner;

    if samples.len() < FRAME_SIZE * 2 {
        return FrameFeatures {
            broadband: vec![],
            bass: vec![],
            high: vec![],
            novelty: vec![],
            bass_novelty: vec![],
            hop_secs: HOP_SIZE as f64 / sample_rate as f64,
        };
    }

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(FRAME_SIZE);
    let window: Vec<f32> = (0..FRAME_SIZE)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (FRAME_SIZE as f32 - 1.0)).cos())
        .collect();

    let n_bins = FRAME_SIZE / 2;
    let n_frames = (samples.len() - FRAME_SIZE) / HOP_SIZE;
    let mut broadband = Vec::with_capacity(n_frames);
    let mut bass = Vec::with_capacity(n_frames);
    let mut high = Vec::with_capacity(n_frames);
    let mut novelty = Vec::with_capacity(n_frames);
    let mut bass_novelty = Vec::with_capacity(n_frames);
    let mut prev_mag = vec![0.0f64; n_bins];

    for i in 0..n_frames {
        let start = i * HOP_SIZE;
        let mut buf: Vec<Complex<f32>> = (0..FRAME_SIZE)
            .map(|j| Complex::new(samples[start + j] * window[j], 0.0))
            .collect();
        fft.process(&mut buf);

        let mut mag_sum = 0.0f64;
        let mut bass_sum = 0.0f64;
        let mut high_sum = 0.0f64;
        let mut flux = 0.0f64;
        let mut bass_flux = 0.0f64;
        for k in 1..n_bins {
            let freq = k as f64 * sample_rate as f64 / FRAME_SIZE as f64;
            let mag = buf[k].norm() as f64;
            mag_sum += mag;
            let in_bass_band = freq >= SUB_BAND_LO_HZ && freq <= SUB_BAND_HI_HZ;
            if in_bass_band {
                bass_sum += mag;
            }
            if freq >= HIGH_BAND_LO_HZ && freq <= HIGH_BAND_HI_HZ {
                high_sum += mag;
            }
            let diff = mag - prev_mag[k];
            if diff > 0.0 {
                flux += diff;
                if in_bass_band {
                    bass_flux += diff;
                }
            }
            prev_mag[k] = mag;
        }
        broadband.push(mag_sum);
        bass.push(bass_sum);
        high.push(high_sum);
        novelty.push(flux);
        bass_novelty.push(bass_flux);
    }

    FrameFeatures { broadband, bass, high, novelty, bass_novelty, hop_secs: HOP_SIZE as f64 / sample_rate as f64 }
}

/// Downsamples a fine per-frame curve into fixed-size time buckets by
/// averaging, trading resolution we no longer need for a smoother curve to
/// run the before/after structural-change comparison over.
fn to_buckets(frame_values: &[f64], hop_secs: f64, bucket_secs: f64) -> Vec<f64> {
    if frame_values.is_empty() {
        return vec![];
    }
    let frames_per_bucket = ((bucket_secs / hop_secs).round() as usize).max(1);
    frame_values
        .chunks(frames_per_bucket)
        .map(|chunk| chunk.iter().sum::<f64>() / chunk.len() as f64)
        .collect()
}

fn normalize(bucket: &[f64]) -> Vec<f64> {
    // Light smoothing to suppress single-bucket spikes, same intent as the
    // original's 3-tap moving average, just over buckets instead of 1s hops.
    let smoothed = moving_average(bucket, 1);
    let max_v = smoothed.iter().cloned().fold(0.0f64, f64::max).max(1e-9);
    smoothed.iter().map(|v| v / max_v).collect()
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

/// Finds the beat phase offset (in seconds, within `[0, beat_period)`) that
/// best aligns a comb of predicted beat times with a novelty curve — the
/// same idea a beat tracker uses to lock onto the grid, just applied here to
/// fix where "beat zero" actually is instead of assuming it's sample 0. Also
/// returns a 0..1 "sharpness": how far the winning phase's score stands
/// above the average candidate, relative to the full score spread — near 0
/// means the correlation landscape is flat (the "best" phase is close to
/// arbitrary noise); near 1 means one phase clearly dominates. See
/// `estimate_beat_phase`'s doc comment for why this matters.
fn phase_lock_with_sharpness(novelty: &[f64], hop_secs: f64, beat_period: f64) -> (f64, f64) {
    if novelty.is_empty() || beat_period <= 0.0 {
        return (0.0, 0.0);
    }
    let duration = novelty.len() as f64 * hop_secs;
    let n_beats = (duration / beat_period).floor() as usize;
    if n_beats < 4 {
        return (0.0, 0.0);
    }

    const TRIALS: usize = 60;
    let mut scores = [0.0f64; TRIALS];
    for (trial, score) in scores.iter_mut().enumerate() {
        let phase = beat_period * trial as f64 / TRIALS as f64;
        let mut t = phase;
        while t < duration {
            let idx = (t / hop_secs).round() as usize;
            if idx < novelty.len() {
                *score += novelty[idx];
            }
            t += beat_period;
        }
    }

    let (best_trial, &best_score) =
        scores.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).unwrap();
    let min = scores.iter().cloned().fold(f64::MAX, f64::min);
    let mean = scores.iter().sum::<f64>() / scores.len() as f64;
    let sharpness = if (best_score - min).abs() > 1e-12 { (best_score - mean) / (best_score - min) } else { 0.0 };
    let best_phase = beat_period * best_trial as f64 / TRIALS as f64;
    (best_phase, sharpness)
}

/// Picks beat phase from whichever of two novelty signals locks onto it more
/// confidently, rather than trusting one signal unconditionally.
///
/// The phase search originally ran on full-band spectral flux alone. For a
/// real reported case (two tracks at virtually identical tempo and the same
/// key — about as easy a beatmatch as exists — that still didn't "lock in"
/// when mixed), that signal turned out to have only a moderate, fairly
/// ambiguous correlation peak for one of the two tracks (measured sharpness
/// 0.42) — full-band flux reacts to hi-hats, claps and vocal transients just
/// as much as the kick, and when the kick isn't the single loudest element,
/// those compete with it for which periodic feature the search locks onto.
/// A sub/kick-band-only flux signal (`bass_novelty`) locked much more
/// confidently for that same track (0.57) — the kick is close to the only
/// thing left in that band, so it's a much more specific signal for "where
/// the actual downbeat is" in genres built around a prominent kick.
///
/// But the bass-only signal is *not* a universal upgrade: measured against
/// two other, already-reliable tracks, it was clearly *less* sharp than
/// full-band (0.51 vs 0.60, and 0.38 vs 0.65) — presumably because a track
/// with a thinner or more syncopated bassline gives the comb search less to
/// work with there than its fuller broadband mix does. Replacing full-band
/// with bass-band outright would have fixed the reported case at the cost of
/// making those other, already-correct tracks *less* reliable — the same
/// failure shape as the "spectral fullness" signal rejected in `energy.rs`
/// and the oversized semitone-shift cap rejected in `preview.rs`: a global
/// swap that trades one case for others. Picking whichever signal is
/// *actually* more confident for this specific track has no such downside by
/// construction — it can only ever choose the sharper of the two.
fn estimate_beat_phase(novelty: &[f64], bass_novelty: &[f64], hop_secs: f64, beat_period: f64) -> f64 {
    let (phase_broadband, sharp_broadband) = phase_lock_with_sharpness(novelty, hop_secs, beat_period);
    let (phase_bass, sharp_bass) = phase_lock_with_sharpness(bass_novelty, hop_secs, beat_period);
    if sharp_bass > sharp_broadband {
        phase_bass
    } else {
        phase_broadband
    }
}

/// Snaps to the nearest beat on the phase-corrected grid.
fn snap_to_period(t: f64, period: f64, phase: f64) -> f64 {
    if period <= 0.0 {
        return t.max(0.0);
    }
    let k = ((t - phase) / period).round();
    (phase + k * period).max(0.0)
}

/// Snaps a mid-track structural candidate to the bar grid, preferring an
/// 8-bar (or failing that, 4-bar) phrase boundary when one is close enough —
/// real drops/breakdowns in 4/4 dance music overwhelmingly land on a phrase
/// boundary, not an arbitrary bar.
fn snap_to_phrase(t: f64, beat_period: f64, bar_period: f64, phase: f64) -> f64 {
    if bar_period <= 0.0 {
        return t.max(0.0);
    }
    let bar_snap = snap_to_period(t, bar_period, phase);
    let bar_index = ((bar_snap - phase) / bar_period).round();

    let try_phrase = |bars_per_phrase: f64| -> Option<f64> {
        let phrase_index = (bar_index / bars_per_phrase).round() * bars_per_phrase;
        let phrase_secs = phase + phrase_index * bar_period;
        if (phrase_secs - bar_snap).abs() <= beat_period {
            Some(phrase_secs)
        } else {
            None
        }
    };

    try_phrase(8.0).or_else(|| try_phrase(4.0)).unwrap_or(bar_snap).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic track: silence, quiet intro, a loud "drop" section,
    /// a quiet breakdown, a second loud section, then a fade to silence. A
    /// `lead_in_secs` offset before any of that shifts where beat zero
    /// actually falls, so phase-correct snapping can be exercised for real.
    fn synth_track(sample_rate: u32, lead_in_secs: f64) -> Vec<f32> {
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
        v.extend(seg(lead_in_secs, 0.0, 0)); // true silence pickup
        v.extend(seg(15.0, 0.2, 2)); // intro
        v.extend(seg(20.0, 0.9, 3)); // drop
        v.extend(seg(15.0, 0.15, 4)); // breakdown
        v.extend(seg(20.0, 0.9, 5)); // second drop
        v.extend(seg(10.0, 0.03, 6)); // outro fade
        v
    }

    /// Builds a signal with two competing periodic events at the same
    /// period but opposite phase: a low-frequency "kick" exactly on the
    /// true beat, and a louder, higher-frequency "clap" exactly off-beat
    /// (half a beat later). Full-band flux reacts to both, so a phase
    /// search over broadband novelty alone can be pulled toward the louder
    /// off-beat clap instead of the true, quieter kick. A bass-band-only
    /// flux signal never sees the clap at all (it's outside that band), so
    /// it should lock onto the true phase cleanly.
    fn kick_vs_offbeat_clap(sample_rate: u32, beat_period: f64, true_phase: f64, n_beats: usize) -> Vec<f32> {
        let duration = true_phase + beat_period * n_beats as f64 + beat_period;
        let n = (sample_rate as f64 * duration) as usize;
        let mut v = vec![0.0f32; n];

        let burst = |v: &mut [f32], start_t: f64, freq: f32, amp: f32| {
            let start = (start_t * sample_rate as f64) as usize;
            let burst_len = (0.03 * sample_rate as f64) as usize; // 30ms
            for i in 0..burst_len {
                if start + i >= v.len() {
                    break;
                }
                let decay = 1.0 - (i as f32 / burst_len as f32);
                let t = i as f32 / sample_rate as f32;
                v[start + i] += amp * decay * (2.0 * std::f32::consts::PI * freq * t).sin();
            }
        };

        for beat in 0..n_beats {
            let kick_t = true_phase + beat_period * beat as f64;
            let clap_t = kick_t + beat_period * 0.5;
            burst(&mut v, kick_t, 80.0, 0.5); // low-frequency kick, on the true beat
            burst(&mut v, clap_t, 3000.0, 0.8); // louder high-frequency clap, exactly off-beat
        }
        v
    }

    #[test]
    fn phase_lock_prefers_the_bass_band_when_it_disambiguates_a_louder_offbeat_competitor() {
        let sample_rate = 44100;
        let beat_period = 0.5;
        let true_phase = 0.1;
        let samples = kick_vs_offbeat_clap(sample_rate, beat_period, true_phase, 20);

        let frames = analyze_frames(&samples, sample_rate);
        let (broadband_phase, broadband_sharp) = phase_lock_with_sharpness(&frames.novelty, frames.hop_secs, beat_period);
        let (bass_phase, bass_sharp) = phase_lock_with_sharpness(&frames.bass_novelty, frames.hop_secs, beat_period);

        // The setup should actually create the competing-signal scenario:
        // the bass-band lock should land on the true phase, within the
        // resolution of the 60-trial search grid (beat_period/60 ≈ 8.3ms
        // here) — not exactly 0, since the true phase won't always fall
        // precisely on a trial point.
        let bass_err = (bass_phase - true_phase).rem_euclid(beat_period);
        let bass_err = bass_err.min(beat_period - bass_err);
        assert!(bass_err < 0.035, "bass-band phase should land on the true kick phase, got {bass_phase:.4}s (err {bass_err:.4}s)");
        // ...while full-band, pulled by the louder off-beat clap, should be
        // measurably worse (closer to the clap's phase, half a beat away)
        // and/or less sharp than the bass-band lock — if it isn't, this
        // synthetic case isn't actually exercising the failure mode and the
        // test should be redesigned, not loosened.
        let broadband_err = (broadband_phase - true_phase).rem_euclid(beat_period);
        let broadband_err = broadband_err.min(beat_period - broadband_err);
        assert!(
            broadband_err > bass_err || bass_sharp > broadband_sharp,
            "expected full-band to be pulled off the true phase (or at least less sharp) by the louder off-beat clap: \
             broadband phase={broadband_phase:.4}s (err {broadband_err:.4}s, sharpness {broadband_sharp:.3}), \
             bass phase={bass_phase:.4}s (err {bass_err:.4}s, sharpness {bass_sharp:.3})"
        );

        // The actual function under test: does the adaptive picker land on
        // the true phase in this scenario, regardless of which raw signal
        // it had to pick to get there?
        let adaptive_phase = estimate_beat_phase(&frames.novelty, &frames.bass_novelty, frames.hop_secs, beat_period);
        let adaptive_err = (adaptive_phase - true_phase).rem_euclid(beat_period);
        let adaptive_err = adaptive_err.min(beat_period - adaptive_err);
        assert!(
            adaptive_err < 0.035,
            "expected the adaptive phase picker to land on the true kick phase ({true_phase}s), got {adaptive_phase:.4}s (err {adaptive_err:.4}s)"
        );
    }

    #[test]
    fn detects_intro_and_outro_and_structure() {
        let sample_rate = 44100;
        let samples = synth_track(sample_rate, 4.0);
        let cues = detect_hot_cues(&samples, sample_rate, 128.0);

        assert!(cues.len() >= 3, "expected at least intro/change/outro, got {}", cues.len());
        assert_eq!(cues.first().unwrap().label, "INTRO");
        assert_eq!(cues.last().unwrap().label, "OUTRO");
        assert!(cues.windows(2).all(|w| w[0].position_secs <= w[1].position_secs));

        let has_rise = cues.iter().any(|c| c.label == "DROP" || c.label == "BUILD");
        assert!(has_rise, "expected at least one rising structural cue, got {:?}", cues.iter().map(|c| &c.label).collect::<Vec<_>>());
    }

    #[test]
    fn works_regardless_of_lead_in_offset() {
        // The old implementation assumed beat zero was at sample zero. Here
        // the same musical content starts at two different lead-in offsets
        // that are NOT whole multiples of the beat period at 128 BPM
        // (0.46875s) — a phase-blind implementation would still "work" by
        // coincidence before, but shouldn't need the offset to be beat
        // aligned at all to find the same structural landmarks.
        let sample_rate = 44100;
        let a = detect_hot_cues(&synth_track(sample_rate, 1.3), sample_rate, 128.0);
        let b = detect_hot_cues(&synth_track(sample_rate, 7.9), sample_rate, 128.0);
        assert!(a.iter().any(|c| c.label == "DROP"), "expected a DROP with 1.3s lead-in");
        assert!(b.iter().any(|c| c.label == "DROP"), "expected a DROP with 7.9s lead-in");
    }

    #[test]
    fn midtrack_cues_land_on_the_beat_grid() {
        let sample_rate = 44100;
        let bpm = 128.0;
        let beat_period = 60.0 / bpm;
        let samples = synth_track(sample_rate, 2.7);
        let cues = detect_hot_cues(&samples, sample_rate, bpm);
        for c in &cues {
            if c.label == "INTRO" || c.label == "OUTRO" {
                continue;
            }
            let nearest_beat_multiple = (c.position_secs / beat_period).round() * beat_period;
            let err = (c.position_secs - nearest_beat_multiple).abs();
            // Allow for the chosen beat_period not perfectly dividing into
            // the bar grid's phase search resolution; should still be tight.
            assert!(
                err < beat_period * 0.26,
                "{} at {:.3}s is not close to a beat-grid line (nearest {:.3}s, err {:.3}s)",
                c.label,
                c.position_secs,
                nearest_beat_multiple,
                err
            );
        }
    }

    /// Builds synthetic content with energy ONLY in the filter-release band
    /// (no sub-bass at all), so `bass_diff` can never cross its threshold —
    /// any DROP produced on this signal must come from the filter-release
    /// path (`high_diff` + `sharpness`), not the bass-confirmation path.
    /// Returns the generated samples plus the running phase, so callers can
    /// chain segments without a phase discontinuity in the tone itself (only
    /// amplitude changes between segments, same as a real filter/volume
    /// move would produce).
    fn high_band_tone(sample_rate: u32, secs: f64, amp: f32, phase_start: f64) -> (Vec<f32>, f64) {
        let freqs = [3000.0_f64, 5000.0, 7000.0];
        let n = (sample_rate as f64 * secs) as usize;
        let mut v = Vec::with_capacity(n);
        let mut phase = phase_start;
        let dt = 1.0 / sample_rate as f64;
        for _ in 0..n {
            let s: f64 = freqs.iter().map(|f| (2.0 * std::f64::consts::PI * f * phase).sin()).sum();
            v.push((s / freqs.len() as f64 * amp as f64) as f32);
            phase += dt;
        }
        (v, phase)
    }

    #[test]
    fn filter_release_drop_is_detected_without_a_bass_jump() {
        // Progressive-house/big-room drops (e.g. Calvin Harris "Feel So
        // Close"/"Summer") are a wall-of-synth filter release, not a
        // sub-bass re-entry — the bass-confirmation path alone never fires
        // for them. This signal has zero energy below 2kHz by construction,
        // so only the filter-release path can possibly produce a DROP here.
        let sample_rate = 44100;
        let mut v = Vec::new();
        let mut phase = 0.0;
        let (seg, p) = high_band_tone(sample_rate, 4.0, 0.0, phase);
        v.extend(seg);
        phase = p;
        let (seg, p) = high_band_tone(sample_rate, 15.0, 0.15, phase); // quiet intro
        v.extend(seg);
        phase = p;
        let (seg, p) = high_band_tone(sample_rate, 20.0, 0.9, phase); // instant filter-release jump
        v.extend(seg);
        phase = p;
        let (seg, p) = high_band_tone(sample_rate, 15.0, 0.9, phase); // stays open well past the persistence window
        v.extend(seg);
        phase = p;
        let (seg, _) = high_band_tone(sample_rate, 10.0, 0.03, phase); // fade out
        v.extend(seg);

        let cues = detect_hot_cues(&v, sample_rate, 128.0);
        assert!(
            cues.iter().any(|c| c.label == "DROP"),
            "expected a DROP from the sudden wideband jump even with zero bass content, got {:?}",
            cues.iter().map(|c| (&c.label, c.position_secs)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_gradual_riser_is_build_not_drop() {
        // Same band, same total rise, same final level as the test above —
        // but spread gradually over 16s instead of landing instantly. The
        // sharpness gate should keep this a BUILD, not a DROP: without it,
        // `qualifies_filter_release_drop` would fire on every multi-bar
        // riser, not just real filter-release drops.
        let sample_rate = 44100;
        let mut v = Vec::new();
        let mut phase = 0.0;
        let (seg, p) = high_band_tone(sample_rate, 4.0, 0.0, phase);
        v.extend(seg);
        phase = p;
        let (seg, p) = high_band_tone(sample_rate, 15.0, 0.15, phase); // quiet intro
        v.extend(seg);
        phase = p;

        let steps = 16;
        let ramp_secs = 16.0;
        for i in 0..steps {
            let amp = 0.15 + (0.9 - 0.15) * (i as f32 + 1.0) / steps as f32;
            let (seg, p) = high_band_tone(sample_rate, ramp_secs / steps as f64, amp, phase);
            v.extend(seg);
            phase = p;
        }
        let (seg, p) = high_band_tone(sample_rate, 15.0, 0.9, phase); // sustained loud section
        v.extend(seg);
        phase = p;
        let (seg, _) = high_band_tone(sample_rate, 10.0, 0.03, phase); // fade out
        v.extend(seg);

        let cues = detect_hot_cues(&v, sample_rate, 128.0);
        assert!(
            !cues.iter().any(|c| c.label == "DROP"),
            "a gradual multi-bar riser should register as BUILD, not DROP, got {:?}",
            cues.iter().map(|c| (&c.label, c.position_secs)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_single_transient_hit_is_not_mistaken_for_a_structural_change() {
        // A ~0.3s crash/stab that decays right back down to the quiet
        // intro level should not register as a DROP or BREAKDOWN — it fails
        // the persistence check.
        let sample_rate = 44100;
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
        v.extend(seg(3.0, 0.0, 0));
        v.extend(seg(20.0, 0.2, 2)); // steady quiet section
        v.extend(seg(0.3, 0.9, 3)); // one loud transient hit
        v.extend(seg(20.0, 0.2, 4)); // back to the same quiet level
        v.extend(seg(10.0, 0.03, 6));

        let cues = detect_hot_cues(&v, sample_rate, 128.0);
        assert!(
            !cues.iter().any(|c| c.label == "DROP" || c.label == "BREAKDOWN"),
            "a single decaying transient should not register as a sustained structural change, got {:?}",
            cues.iter().map(|c| (&c.label, c.position_secs)).collect::<Vec<_>>()
        );
    }

    /// Diagnoses a reported real-world beatmatch failure: two tracks at
    /// nearly-identical BPM and the exact same key (so almost no tempo/pitch
    /// correction is applied) still reportedly don't "lock in" when mixed.
    /// `estimate_beat_phase` fits ONE phase/period model via a global sum
    /// over the *entire* track's novelty curve, then that single model is
    /// used to snap cues anywhere in the track — including an OUTRO cue
    /// hundreds of beats after the fit was made. If the true tempo isn't
    /// perfectly constant (it never is, exactly), any per-beat error
    /// compounds linearly with elapsed beat count. An INTRO cue near t=0 has
    /// almost no beats to accumulate error over; an OUTRO cue 200+ seconds in
    /// has hundreds. This test checks that directly: estimate phase from the
    /// WHOLE track vs. from ONLY a local window right around the cue, and
    /// compare how far apart they land (as a fraction of a beat).
    #[test]
    #[ignore]
    fn diagnose_global_vs_local_phase_drift() {
        let path_a = "/Users/isaac/Downloads/RAY VOLPE - SONG REQUEST (Official Visualizer + Lyric Video) [h5bSMPb7pVQ].mp3";
        let path_b = "/Users/isaac/Downloads/Dance/Calvin_Harris__Clementine_Douglas-Blessings-KETTAMA_Remix-81043040.mp3";

        for (label, path, bpm, cue_secs) in [
            ("A (Ray Volpe) at its OUTRO cue (203.28s)", path_a, 130.813953488372_f64, 203.281066666667_f64),
            ("A (Ray Volpe) at its INTRO cue (0.09s), control", path_a, 130.813953488372, 0.0917333333333333),
            ("B (Blessings KETTAMA) at its INTRO cue (0.33s), control", path_b, 129.19921875, 0.325079365079365),
        ] {
            let decoded = crate::audio::decode::decode_file(std::path::Path::new(path)).expect("decode failed");
            let frames = analyze_frames(&decoded.samples, decoded.sample_rate);
            let beat_period = 60.0 / bpm;

            // Global: fit over the entire track's novelty curve (what the
            // real pipeline actually does).
            let global_phase = phase_lock_with_sharpness(&frames.novelty, frames.hop_secs, beat_period).0;

            // Local: fit over only a +/-15s window centered on the cue, so
            // there's essentially no beat count to accumulate drift over.
            let window_secs = 15.0;
            let win_start_t = (cue_secs - window_secs).max(0.0);
            let win_end_t = cue_secs + window_secs;
            let start_idx = (win_start_t / frames.hop_secs) as usize;
            let end_idx = ((win_end_t / frames.hop_secs) as usize).min(frames.novelty.len());
            assert!(start_idx < end_idx, "window out of range for {label}");
            let local_novelty = &frames.novelty[start_idx..end_idx];
            let local_phase_in_window = phase_lock_with_sharpness(local_novelty, frames.hop_secs, beat_period).0;
            // estimate_beat_phase treats its input as starting at t=0; shift
            // back into absolute track time.
            let local_phase_absolute = win_start_t + local_phase_in_window;

            // Compare both phases modulo the beat period, as a signed offset
            // in [-0.5, 0.5] beats (0 = perfectly agree, 0.5 = maximally
            // disagree / exactly offbeat from each other).
            let diff_secs = local_phase_absolute - global_phase;
            let mut frac_beats = (diff_secs / beat_period).rem_euclid(1.0);
            if frac_beats > 0.5 {
                frac_beats -= 1.0;
            }
            println!(
                "{label}: global_phase={global_phase:.4}s  local_phase(abs)={local_phase_absolute:.4}s  drift={:.1}% of a beat",
                frac_beats.abs() * 100.0
            );
        }
    }

    /// Second data point: does the same global-vs-local phase comparison
    /// show drift for a different real pair, or was the first result
    /// pair-specific? Uses two tracks already in the main test library with
    /// a late OUTRO cue (same shape of case: far-from-start cue point).
    #[test]
    #[ignore]
    fn diagnose_phase_drift_second_pair() {
        let dir = std::env::var("GIGFLOW_TEST_LIBRARY").unwrap_or_else(|_| "/Users/isaac/Downloads/Dance".to_string());
        // Corona - Rhythm Of The Night: used elsewhere this session as a
        // real-library reference track; has a long runtime, giving a late
        // OUTRO cue plenty of distance to accumulate drift over.
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.file_name().unwrap().to_string_lossy().contains("Rhythm_Of_The_Night"))
            .collect();
        assert!(!files.is_empty(), "Rhythm Of The Night not found in {dir}");
        let path = files.remove(0);

        let decoded = crate::audio::decode::decode_file(&path).expect("decode failed");
        let bpm_result = crate::audio::bpm::detect_bpm(&decoded.samples, decoded.sample_rate);
        let cues = detect_hot_cues(&decoded.samples, decoded.sample_rate, bpm_result.bpm);
        let outro = cues.iter().find(|c| c.label == "OUTRO").expect("no OUTRO cue found");
        let intro = cues.iter().find(|c| c.label == "INTRO").expect("no INTRO cue found");
        println!("bpm={:.2}  intro={:.3}s  outro={:.3}s", bpm_result.bpm, intro.position_secs, outro.position_secs);

        let frames = analyze_frames(&decoded.samples, decoded.sample_rate);
        let beat_period = 60.0 / bpm_result.bpm;
        let global_phase = phase_lock_with_sharpness(&frames.novelty, frames.hop_secs, beat_period).0;

        for (label, cue_secs) in [("INTRO", intro.position_secs), ("OUTRO", outro.position_secs)] {
            let window_secs = 15.0;
            let win_start_t = (cue_secs - window_secs).max(0.0);
            let win_end_t = cue_secs + window_secs;
            let start_idx = (win_start_t / frames.hop_secs) as usize;
            let end_idx = ((win_end_t / frames.hop_secs) as usize).min(frames.novelty.len());
            assert!(start_idx < end_idx, "window out of range for {label}");
            let local_novelty = &frames.novelty[start_idx..end_idx];
            let local_phase_in_window = phase_lock_with_sharpness(local_novelty, frames.hop_secs, beat_period).0;
            let local_phase_absolute = win_start_t + local_phase_in_window;

            let diff_secs = local_phase_absolute - global_phase;
            let mut frac_beats = (diff_secs / beat_period).rem_euclid(1.0);
            if frac_beats > 0.5 {
                frac_beats -= 1.0;
            }
            println!(
                "{label} at {cue_secs:.3}s: drift={:.1}% of a beat",
                frac_beats.abs() * 100.0
            );
        }
    }

    /// `estimate_beat_phase` returns only its single best-scoring phase, with
    /// no confidence/sharpness signal — so a flat, ambiguous correlation
    /// landscape (many candidate phases scoring nearly the same) looks
    /// identical to a sharp, confident lock. The previous test found a
    /// 45%-of-a-beat disagreement between Ray Volpe's global phase estimate
    /// and a local one near its own intro — nearly exactly off-beat, worse
    /// than the disagreement near its OUTRO far later in the track. That's
    /// the opposite of what "drift accumulating with distance" would
    /// predict, which suggests the real issue isn't drift at all but that
    /// this track's phase estimate is simply poorly determined to begin
    /// with. Dump the full 60-candidate score landscape (same diagnostic
    /// pattern as `key.rs`'s `dump_full_key_scores_for_file`) to check.
    #[test]
    #[ignore]
    fn diagnose_phase_score_landscape_sharpness() {
        let path_a = "/Users/isaac/Downloads/RAY VOLPE - SONG REQUEST (Official Visualizer + Lyric Video) [h5bSMPb7pVQ].mp3";
        let path_b = "/Users/isaac/Downloads/Dance/Calvin_Harris__Clementine_Douglas-Blessings-KETTAMA_Remix-81043040.mp3";
        for (label, path, bpm) in [("Ray Volpe", path_a, 130.813953488372_f64), ("Blessings KETTAMA", path_b, 129.19921875)] {
            let decoded = crate::audio::decode::decode_file(std::path::Path::new(path)).expect("decode failed");
            let frames = analyze_frames(&decoded.samples, decoded.sample_rate);
            let beat_period = 60.0 / bpm;
            let duration = frames.novelty.len() as f64 * frames.hop_secs;

            const TRIALS: usize = 60;
            let mut scores = Vec::with_capacity(TRIALS);
            for trial in 0..TRIALS {
                let phase = beat_period * trial as f64 / TRIALS as f64;
                let mut score = 0.0f64;
                let mut t = phase;
                while t < duration {
                    let idx = (t / frames.hop_secs).round() as usize;
                    if idx < frames.novelty.len() {
                        score += frames.novelty[idx];
                    }
                    t += beat_period;
                }
                scores.push(score);
            }
            let max = scores.iter().cloned().fold(f64::MIN, f64::max);
            let min = scores.iter().cloned().fold(f64::MAX, f64::min);
            let mean = scores.iter().sum::<f64>() / scores.len() as f64;
            // "Sharpness": how far the best score stands above the average
            // candidate, relative to the full spread — near 0 means the
            // landscape is flat (the "best" phase is arbitrary noise); near
            // 1 means one phase clearly dominates.
            let sharpness = if (max - min).abs() > 1e-12 { (max - mean) / (max - min) } else { 0.0 };
            println!("{label}: score range=[{min:.1}, {max:.1}] mean={mean:.1} sharpness={sharpness:.3}");
        }
    }

    /// Sanity check on the mechanism itself: run the real pipeline
    /// end-to-end (decode -> detect_bpm -> detect_hot_cues, exactly as
    /// `analyze_track` does) and confirm the OUTRO/INTRO cues it actually
    /// emits land exactly on that same run's own (phase, period) grid — not
    /// hand-computed from DB values that might not match bit-for-bit.
    #[test]
    #[ignore]
    fn diagnose_cues_are_exactly_on_their_own_grid() {
        let path_a = "/Users/isaac/Downloads/RAY VOLPE - SONG REQUEST (Official Visualizer + Lyric Video) [h5bSMPb7pVQ].mp3";
        let decoded = crate::audio::decode::decode_file(std::path::Path::new(path_a)).expect("decode failed");
        let bpm_result = crate::audio::bpm::detect_bpm(&decoded.samples, decoded.sample_rate);
        let beat_period = 60.0 / bpm_result.bpm;

        let frames = analyze_frames(&decoded.samples, decoded.sample_rate);
        let phase = estimate_beat_phase(&frames.novelty, &frames.bass_novelty, frames.hop_secs, beat_period);

        let cues = detect_hot_cues(&decoded.samples, decoded.sample_rate, bpm_result.bpm);
        println!("bpm={:.6} beat_period={beat_period:.6} phase={phase:.6}", bpm_result.bpm);
        for c in &cues {
            let beats_from_phase = (c.position_secs - phase) / beat_period;
            let nearest_int = beats_from_phase.round();
            let residual_beats = (beats_from_phase - nearest_int).abs();
            println!(
                "{:10} {:>10.4}s   beats_from_phase={beats_from_phase:12.4}  residual={:.4} ({:.1}% of a beat)",
                c.label,
                c.position_secs,
                residual_beats,
                residual_beats * 100.0
            );
        }
    }

    /// Tests the fix hypothesis: restrict the phase-lock search to the
    /// sub/kick band's own flux instead of full-band flux. In EDM/house/
    /// techno the kick drum is the one element that reliably lands exactly
    /// on the beat grid; hi-hats, claps, and vocal transients that also
    /// drive full-band flux don't have to, and can out-compete a quieter
    /// kick in the correlation sum for a track where the kick isn't the
    /// loudest element. Compares sharpness (see
    /// `diagnose_phase_score_landscape_sharpness`) for both signals on both
    /// tracks from the reported bad transition, plus the already-fine
    /// Rhythm Of The Night as a regression check.
    #[test]
    #[ignore]
    fn diagnose_bass_band_phase_sharpness_comparison() {
        let dir = std::env::var("GIGFLOW_TEST_LIBRARY").unwrap_or_else(|_| "/Users/isaac/Downloads/Dance".to_string());
        let rhythm_path = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.file_name().unwrap().to_string_lossy().contains("Rhythm_Of_The_Night"))
            .expect("Rhythm Of The Night not found");

        let cases: Vec<(&str, std::path::PathBuf, f64)> = vec![
            (
                "Ray Volpe",
                std::path::PathBuf::from("/Users/isaac/Downloads/RAY VOLPE - SONG REQUEST (Official Visualizer + Lyric Video) [h5bSMPb7pVQ].mp3"),
                130.813953488372,
            ),
            (
                "Blessings KETTAMA",
                std::path::PathBuf::from("/Users/isaac/Downloads/Dance/Calvin_Harris__Clementine_Douglas-Blessings-KETTAMA_Remix-81043040.mp3"),
                129.19921875,
            ),
            ("Rhythm Of The Night", rhythm_path, 127.604166666667),
        ];

        fn sharpness(novelty: &[f64], hop_secs: f64, beat_period: f64) -> f64 {
            let duration = novelty.len() as f64 * hop_secs;
            const TRIALS: usize = 60;
            let mut scores = Vec::with_capacity(TRIALS);
            for trial in 0..TRIALS {
                let phase = beat_period * trial as f64 / TRIALS as f64;
                let mut score = 0.0f64;
                let mut t = phase;
                while t < duration {
                    let idx = (t / hop_secs).round() as usize;
                    if idx < novelty.len() {
                        score += novelty[idx];
                    }
                    t += beat_period;
                }
                scores.push(score);
            }
            let max = scores.iter().cloned().fold(f64::MIN, f64::max);
            let min = scores.iter().cloned().fold(f64::MAX, f64::min);
            let mean = scores.iter().sum::<f64>() / scores.len() as f64;
            if (max - min).abs() > 1e-12 { (max - mean) / (max - min) } else { 0.0 }
        }

        for (label, path, bpm) in cases {
            let decoded = crate::audio::decode::decode_file(&path).expect("decode failed");
            let frames = analyze_frames(&decoded.samples, decoded.sample_rate);
            let beat_period = 60.0 / bpm;
            let broadband_sharp = sharpness(&frames.novelty, frames.hop_secs, beat_period);
            let bass_sharp = sharpness(&frames.bass_novelty, frames.hop_secs, beat_period);

            let phase_broadband = phase_lock_with_sharpness(&frames.novelty, frames.hop_secs, beat_period).0;
            let phase_bass = phase_lock_with_sharpness(&frames.bass_novelty, frames.hop_secs, beat_period).0;
            let diff_secs = phase_bass - phase_broadband;
            let mut frac_beats = (diff_secs / beat_period).rem_euclid(1.0);
            if frac_beats > 0.5 {
                frac_beats -= 1.0;
            }
            println!(
                "{label}: broadband_sharpness={broadband_sharp:.3}  bass_sharpness={bass_sharp:.3}  phase_disagreement={:.1}% of a beat",
                frac_beats.abs() * 100.0
            );
        }
    }
}
