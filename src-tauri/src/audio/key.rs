use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

// Raised from 4096 (bin width ~10.8Hz at 44.1kHz/48kHz, the two rates in
// this library). A semitone at MIN_FREQ=110Hz spans only ~6.5Hz — narrower
// than that bin — so bass-register pitch content, exactly the register that
// matters most for telling a true tonic from its dominant, was being
// smeared into the wrong pitch-class bin before any key-matching math ran
// (confirmed directly: at FRAME_SIZE=4096, a pure 110Hz/A2 tone put 90% of
// its energy in pitch class A#/Bb instead of A — see
// `bass_register_resolves_adjacent_semitones` below). This is a basic
// time/frequency-resolution tradeoff key detection can afford to make:
// losing time resolution costs nothing here (key doesn't change
// sub-second), while gaining frequency resolution fixes real errors.
// Validated empirically against 28 tracks in the real test library with
// independently-researched official keys (not just synthetic signals):
// exact-match accuracy went from 39.3% (11/28) at the old size to 53.6%
// (15/28) at this one, including fixing the specific tonic/dominant
// ("fifth") confusion case that triggered this investigation
// (DaBaby - Pop Dat Thang (David Guetta Mix), 3A -> 2A, correct). Tested
// 8192/16384/32768/65536: accuracy climbed monotonically through 32768 then
// was exactly flat at 65536 (identical per-track results, double the FFT
// cost) — 32768 is the point of diminishing returns, not an arbitrary
// choice.
const FRAME_SIZE: usize = 32768;
const HOP_SIZE: usize = 16384;
// Raised from the ~A1 (55Hz) used previously: that range overlaps kick-drum
// and sub-bass fundamentals in dance music, which have no tonal meaning and
// were dominating the chroma vector on bass-heavy tracks. 110Hz (~A2) keeps
// bassline harmonics (a fundamental's 2nd+ partials) while dropping most kick
// energy.
const MIN_FREQ: f32 = 110.0;
const MAX_FREQ: f32 = 5000.0;

const ENERGY_WINDOW_SECS: f64 = 2.0;
const ANALYSIS_WINDOW_SECS: f64 = 75.0;

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

/// Reverse lookup of `MAJOR_CAMELOT`/`MINOR_CAMELOT`: given a Camelot code
/// like "8B" or "10A", returns the tonic's pitch class (0=C..11=B). The mix
/// engine deliberately never pitch-shifts (see `mix.rs`), so this is
/// currently only exercised by tests.
#[allow(dead_code)]
pub fn tonic_pitch_class(camelot: &str) -> Option<u8> {
    let letter = camelot.chars().last()?;
    let num: u8 = camelot[..camelot.len() - 1].parse().ok()?;
    let table = match letter {
        'B' => &MAJOR_CAMELOT,
        'A' => &MINOR_CAMELOT,
        _ => return None,
    };
    table.iter().position(|&n| n == num).map(|pc| pc as u8)
}

pub struct KeyResult {
    pub camelot: String,
    pub name: String,
    pub confidence: f64,
}

pub fn detect_key(samples: &[f32], sample_rate: u32) -> KeyResult {
    let samples = select_analysis_window(samples, sample_rate);
    let Some(chroma) = compute_chroma(samples, sample_rate) else {
        return KeyResult { camelot: "?".into(), name: "unknown".into(), confidence: 0.0 };
    };

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

/// Build a normalized 12-bin chroma vector from a window of samples, with
/// percussive-transient down-weighting (see inline comment) and the bass
/// cutoff applied. Returns None if the window is too short to analyze.
pub(crate) fn compute_chroma(samples: &[f32], sample_rate: u32) -> Option<[f64; 12]> {
    if samples.len() < FRAME_SIZE * 2 {
        return None;
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
    let n_bins = FRAME_SIZE / 2;
    let mut prev_mag = vec![0.0f64; n_bins];

    for i in 0..n_frames {
        let start = i * HOP_SIZE;
        let mut buf: Vec<Complex<f32>> = (0..FRAME_SIZE)
            .map(|j| Complex::new(samples[start + j] * window[j], 0.0))
            .collect();
        fft.process(&mut buf);

        // Percussive-transient down-weighting: a kick/snare hit produces a
        // broadband magnitude spike (high spectral flux) that is not tonal
        // information. Frames dominated by a transient get down-weighted so
        // they contribute less to the chroma vector than sustained,
        // harmonic-rich frames (held bass notes, leads, chords) — a cheap
        // approximation of full harmonic/percussive source separation.
        let mut frame_total = 0.0f64;
        let mut flux = 0.0f64;
        let mags: Vec<f64> = (0..n_bins).map(|k| buf[k].norm() as f64).collect();
        for k in 0..n_bins {
            frame_total += mags[k];
            let d = mags[k] - prev_mag[k];
            if d > 0.0 {
                flux += d;
            }
        }
        prev_mag.copy_from_slice(&mags);

        let transient_ratio = if frame_total > 0.0 { flux / frame_total } else { 0.0 };
        let weight = 1.0 / (1.0 + 3.0 * transient_ratio);

        for k in 1..n_bins {
            let freq = k as f32 * sample_rate as f32 / FRAME_SIZE as f32;
            if freq < MIN_FREQ || freq > MAX_FREQ {
                continue;
            }
            // pitch class relative to A4=440Hz, pc 9 = A
            let midi_pc = (69.0 + 12.0 * (freq as f64 / 440.0).log2()).round();
            let pc = ((midi_pc as i64 % 12) + 12) % 12;
            chroma[pc as usize] += mags[k] * weight;
        }
    }

    let total: f64 = chroma.iter().sum();
    if total <= 0.0 {
        return None;
    }
    for v in chroma.iter_mut() {
        *v /= total;
    }
    Some(chroma)
}

/// Pick the loudest ~75s region of the track to analyze, rather than blindly
/// taking the middle. The loudest sustained section is almost always the
/// drop/chorus, where the bassline and lead are most present and the tonal
/// center is clearest — quiet intros, breakdowns and outros are mostly
/// non-tonal (risers, white noise, reverb tails) and were diluting the chroma
/// vector when a track's quiet section happened to land in the middle.
fn select_analysis_window(samples: &[f32], sample_rate: u32) -> &[f32] {
    let target_len = ((ANALYSIS_WINDOW_SECS * sample_rate as f64) as usize).min(samples.len());
    if samples.len() <= target_len {
        return samples;
    }

    let hop = (ENERGY_WINDOW_SECS * sample_rate as f64) as usize;
    if hop == 0 {
        return samples;
    }
    let mut energy: Vec<f64> = Vec::new();
    let mut pos = 0usize;
    while pos + hop <= samples.len() {
        let slice = &samples[pos..pos + hop];
        let rms = (slice.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>() / slice.len() as f64).sqrt();
        energy.push(rms);
        pos += hop;
    }
    if energy.is_empty() {
        return samples;
    }

    // Skip the first/last 8% of the track (intro/outro) when searching.
    let skip = (energy.len() as f64 * 0.08) as usize;
    let lo = skip.min(energy.len().saturating_sub(1));
    let hi = (energy.len() - skip).max(lo + 1);

    let window_hops = (target_len / hop).max(1).min(hi - lo);
    let mut best_start_hop = lo;
    let mut running_sum: f64 = energy[lo..(lo + window_hops).min(hi)].iter().sum();
    let mut best_sum = running_sum;
    for start in (lo + 1)..=(hi.saturating_sub(window_hops)) {
        running_sum += energy[start + window_hops - 1] - energy[start - 1];
        if running_sum > best_sum {
            best_sum = running_sum;
            best_start_hop = start;
        }
    }

    let start_sample = best_start_hop * hop;
    let end_sample = (start_sample + target_len).min(samples.len());
    &samples[start_sample..end_sample]
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

    /// At the bottom of the analysis range (`MIN_FREQ`), a semitone spans
    /// only ~6.5Hz (110Hz * (2^(1/12)-1)). The original `FRAME_SIZE=4096`
    /// gave a bin width of ~10.8Hz at 44.1kHz — physically wider than that
    /// gap, so two single notes a semitone apart in the bass register could
    /// land in the same or overlapping FFT bins regardless of which was
    /// actually played, corrupting chroma input before any key-matching math
    /// ran. This proves the current `FRAME_SIZE` actually resolves them.
    #[test]
    fn bass_register_resolves_adjacent_semitones() {
        let sample_rate = 44100;
        let seconds = 3.0;
        let tone = |freq: f32| -> Vec<f32> {
            let n = (sample_rate as f64 * seconds) as usize;
            (0..n)
                .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate as f32).sin())
                .collect()
        };

        let a2 = tone(110.0); // A2
        let bb2 = tone(116.54); // A#2/Bb2, one semitone up

        let chroma_a = compute_chroma(&a2, sample_rate).expect("chroma for A2");
        let chroma_bb = compute_chroma(&bb2, sample_rate).expect("chroma for Bb2");

        let argmax = |c: &[f64; 12]| {
            c.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).unwrap().0
        };
        assert_eq!(argmax(&chroma_a), 9, "A2 (110Hz) should peak at pitch class A (9), got chroma {chroma_a:?}");
        assert_eq!(
            argmax(&chroma_bb),
            10,
            "Bb2 (116.54Hz) should peak at pitch class A#/Bb (10), not smeared into A's bin, got chroma {chroma_bb:?}"
        );
    }

    #[test]
    fn tonic_pitch_class_round_trips_through_camelot() {
        // C major is 8B; A minor (its relative minor, same pitch class) is 8A.
        assert_eq!(tonic_pitch_class("8B"), Some(0));
        assert_eq!(tonic_pitch_class("8A"), Some(9));
        // DaBaby's confirmed-correct key: 2A = D#/Eb minor = pitch class 3.
        assert_eq!(tonic_pitch_class("2A"), Some(3));
        assert_eq!(tonic_pitch_class("not-a-key"), None);
    }

    #[test]
    fn camelot_relation_same_key() {
        assert_eq!(camelot_relation("8B", "8B"), "same key");
        assert_eq!(camelot_relation("8B", "8A"), "relative major/minor");
        assert_eq!(camelot_relation("8B", "9B"), "adjacent (perfect mix)");
        assert_eq!(camelot_relation("8B", "2B"), "clash");
    }

    /// Dumps every one of the 24 major/minor correlation scores for one real
    /// file, to see how contested the top result actually is (a close race
    /// among several adjacent keys means genuine ambiguity, not a confident
    /// wrong answer).
    #[test]
    #[ignore]
    fn dump_full_key_scores_for_file() {
        let path = std::env::var("GIGFLOW_TEST_FILE").expect("set GIGFLOW_TEST_FILE");
        let decoded = crate::audio::decode::decode_file(std::path::Path::new(&path)).expect("decode failed");
        let analysis_window = select_analysis_window(&decoded.samples, decoded.sample_rate);
        let chroma = compute_chroma(analysis_window, decoded.sample_rate).expect("chroma failed");

        println!("chroma: {:?}", chroma.iter().map(|v| format!("{:.3}", v)).collect::<Vec<_>>());

        let mut scores: Vec<(String, f64)> = Vec::new();
        for tonic in 0..12 {
            let major_score = correlate(&chroma, &MAJOR_PROFILE, tonic);
            let minor_score = correlate(&chroma, &MINOR_PROFILE, tonic);
            scores.push((format!("{}{}", MAJOR_CAMELOT[tonic], "B"), major_score));
            scores.push((format!("{}{}", MINOR_CAMELOT[tonic], "A"), minor_score));
        }
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        println!("ranked scores for {}:", path);
        for (camelot, score) in &scores {
            println!("  {camelot:4}  {score:.4}");
        }
    }

    fn parse_camelot(s: &str) -> Option<(u8, char)> {
        let letter = s.chars().last()?;
        let num: u8 = s[..s.len() - 1].parse().ok()?;
        Some((num, letter))
    }

    /// Researched official key per track in the user's real test library
    /// (`/Users/isaac/Downloads/Dance`, plus two YouTube-sourced tracks
    /// directly under `/Users/isaac/Downloads`), cross-referenced against
    /// multiple sources, matched to the *exact* edit/remix named in each
    /// filename. The DaBaby entry is user-confirmed directly (this is the
    /// correction that triggered this investigation); `None` entries could
    /// not be verified against a reliable source and are excluded from the
    /// accuracy count below rather than guessed.
    #[test]
    #[ignore]
    fn key_accuracy_against_researched_ground_truth() {
        let dance = "/Users/isaac/Downloads/Dance";
        let dl = "/Users/isaac/Downloads";
        let ground_truth: Vec<(String, Option<&str>)> = vec![
            (format!("{dance}/All Night Longer.mp3"), Some("3B")),
            (format!("{dance}/Calvin_Harris__Clementine_Douglas-Blessings-Extended_Mix-81043037.mp3"), Some("10A")),
            (format!("{dance}/Calvin_Harris__Clementine_Douglas-Blessings-KETTAMA_Remix-81043040.mp3"), Some("4A")),
            (format!("{dance}/Ariana Grande ft Zedd - Break Free (Clean Ex.mp3"), Some("6A")),
            (format!("{dance}/15726860_Clarity_(Extended Mix).mp3"), Some("4B")),
            (format!("{dance}/Beyonce__Jay_Z-Crazy_In_Love-Album_Version-85283622.mp3"), Some("7A")),
            (format!("{dance}/DaBaby - POP DAT THANG (David Guetta Mix) [kTpZlpyiRKU].mp3"), Some("2A")),
            (format!("{dance}/Dancing Queen (CFLO Intro).mp3"), Some("11B")),
            (format!("{dance}/Abba-Dancing_Queen-Single-42187476.mp3"), Some("11B")),
            (format!("{dance}/15717527_Don_t You Worry Child_(Original Mix).mp3"), Some("10B")),
            (format!("{dance}/Calvin Harris - Feel So Close (Extended Mix).mp3"), Some("8B")),
            (format!("{dance}/Alesso-Heroes-Radio_Edit-80977913.mp3"), Some("7B")),
            (format!("{dance}/Calvin_Harris__Disciples-How_Deep_Is_Your_Lo.mp3"), Some("10A")),
            (format!("{dance}/Black_Eyed_Peas-I_Gotta_Feeling-Radio-60492024.mp3"), Some("8B")),
            (format!("{dance}/Calvin Harris - I_m Not Alone (Original Mix).mp3"), None),
            (format!("{dl}/Knock2 & NGHTMRE - One Chance feat. Marlhy  (Official Audio) (Visualizer) [SpwppDwCyjM].mp3"), Some("10A")),
            (format!("{dance}/Black_Eyed_Peas-Let_s_Get_It_Started-Radio_Edit-85364757.mp3"), Some("10A")),
            (format!("{dance}/Avicii-Levels-Original_Mix-126.mp3"), Some("12A")),
            (format!("{dance}/Avicii_ft._Rita_Ora-Lonely_Together-Extended_Mix-81230168.mp3"), Some("7A")),
            (format!("{dance}/Black_Eyed_Peas-Meet_Me_Halfway-DJ_Intro_Clean_Mix-85533053.mp3"), Some("10A")),
            (format!("{dance}/01 My Love (feat. T.I.) (Single Version).mp3"), None),
            (format!("{dance}/Calvin Harris, Ellie Goulding - Outside (Original Mix).mp3"), Some("7A")),
            (format!("{dance}/David_Guetta_ft._Ne_Yo___Akon-Play_Hard-Extended_Mix-81678039.mp3"), None),
            (format!("{dance}/Black_Eyed_Peas-Pump_It-Original_Mix-85532800.mp3"), Some("12A")),
            (format!("{dl}/RAY VOLPE - SONG REQUEST (Official Visualizer + Lyric Video) [h5bSMPb7pVQ].mp3"), None),
            (format!("{dance}/Clean_Bandit-Rather_Be-Radio_Edit-42156407.mp3"), Some("1B")),
            (format!("{dance}/6728169_Runaway (U & I)_(Original Mix).mp3"), Some("3B")),
            (format!("{dance}/Calvin Harris - Summer (Extended Mix).mp3"), Some("9B")),
            (format!("{dance}/Corona-The_Rhythm_Of_The_Night-Single_Version-84322101.mp3"), Some("4A")),
            (format!("{dance}/David_Guetta_ft._Nicki_Minaj-Turn_Me_On-Explicit-81678067.mp3"), Some("4B")),
            (format!("{dance}/Avicii-Waiting_For_Love-Extended_Mix-81230103.mp3"), Some("11A")),
            (format!("{dance}/Avicii-Wake_Me_Up-Extended_Mix-81230121.mp3"), Some("10B")),
        ];

        let mut exact = 0;
        let mut relative = 0;
        let mut fifth = 0;
        let mut other_miss = 0;
        let mut unverifiable = 0;
        let mut total = 0;

        for (path, official) in &ground_truth {
            let decoded = match crate::audio::decode::decode_file(std::path::Path::new(path)) {
                Ok(d) => d,
                Err(e) => {
                    println!("ERROR decoding {path}: {e}");
                    continue;
                }
            };
            let result = detect_key(&decoded.samples, decoded.sample_rate);
            let name = std::path::Path::new(path).file_name().unwrap().to_string_lossy();

            let Some(official) = official else {
                unverifiable += 1;
                println!("{:70} detected={:4}  official=UNVERIFIABLE", name, result.camelot);
                continue;
            };
            total += 1;

            let verdict = if result.camelot == *official {
                exact += 1;
                "MATCH"
            } else if let (Some((dn, dl)), Some((on, ol))) =
                (parse_camelot(&result.camelot), parse_camelot(official))
            {
                if dn == on && dl != ol {
                    relative += 1;
                    "relative maj/min"
                } else if dl == ol && (dn % 12 + 1 == on || on % 12 + 1 == dn) {
                    fifth += 1;
                    "FIFTH CONFUSION"
                } else {
                    other_miss += 1;
                    "other miss"
                }
            } else {
                other_miss += 1;
                "other miss"
            };
            println!(
                "{:70} detected={:4}  official={:4}  [{}]  conf={:.2}",
                name, result.camelot, official, verdict, result.confidence
            );
        }

        println!(
            "\nSUMMARY: exact={exact} relative_maj_min={relative} fifth_confusion={fifth} other_miss={other_miss} unverifiable={unverifiable} total_scored={total}"
        );
        println!(
            "accuracy (exact / total_scored) = {:.1}%",
            100.0 * exact as f64 / total.max(1) as f64
        );
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
