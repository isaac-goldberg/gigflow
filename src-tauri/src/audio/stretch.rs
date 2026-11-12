//! Key-locked, variable-rate time stretching (WSOLA).
//!
//! This is the "master tempo" / key-lock that DJ software uses when you move
//! the pitch fader: tempo changes, musical key doesn't. Two properties
//! matter for mixing and are why this replaces the old fixed-ratio stretch:
//!
//!  * **Variable rate.** The incoming track is held at the outgoing track's
//!    tempo for the blend, then *glides* back to its own tempo over several
//!    bars once it's playing alone. A glide of a few percent over 8+ bars is
//!    inaudible; the old approach snapped straight back, which is exactly
//!    the jarring jump users heard right after the blend.
//!  * **No drift.** The nominal input position is integrated independently
//!    of WSOLA's per-frame similarity search, so the search jitter (a few
//!    ms) never accumulates — after N output beats you are exactly N input
//!    beats in, which is what keeps the two tracks' kicks locked together.

/// Output of a stretch: the audio plus, every `map_hop` output samples, the
/// input position that output sample came from.
pub struct Stretched {
    pub out: Vec<f32>,
    pub map: Vec<f64>,
    pub map_hop: usize,
}

impl Stretched {
    /// Input (source) position corresponding to output sample `o`.
    pub fn input_pos(&self, o: usize) -> f64 {
        if self.map.is_empty() {
            return o as f64;
        }
        let f = o as f64 / self.map_hop as f64;
        let i = f.floor() as usize;
        if i + 1 >= self.map.len() {
            let last = *self.map.last().unwrap();
            return last + (o as f64 - ((self.map.len() - 1) * self.map_hop) as f64);
        }
        let frac = f - i as f64;
        self.map[i] + (self.map[i + 1] - self.map[i]) * frac
    }
}

/// Stretches `x` starting at input position `start`, producing `out_len`
/// samples. `speed(o)` is the input-samples-per-output-sample rate at output
/// sample `o` (1.0 = unchanged, >1 = faster).
pub fn stretch_variable(x: &[f32], sr: u32, start: usize, out_len: usize, speed: impl Fn(usize) -> f64) -> Stretched {
    // ~46 ms frames at 44.1 kHz: long enough for good tonal quality, short
    // enough that kick transients don't smear noticeably.
    let n = ((sr as f64 * 0.046) as usize).max(256) & !1;
    let hs = n / 2;
    let tol = (sr as f64 * 0.006) as usize;
    let window: Vec<f32> = (0..n).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos()).collect();

    let mut out = vec![0.0f32; out_len + n];
    let mut map = Vec::with_capacity(out_len / hs + 2);

    let at = |i: isize| -> f32 { if i < 0 { 0.0 } else { x.get(i as usize).copied().unwrap_or(0.0) } };

    let mut nominal = start as f64;
    let mut prev_actual: Option<isize> = None;
    let mut o = 0usize;
    while o < out_len {
        let target = nominal.round() as isize;
        let actual = match prev_actual {
            None => target,
            Some(prev) => {
                // Natural continuation of the previous frame — the segment
                // that would have followed it with no stretching at all.
                let nat = prev + hs as isize;
                best_offset(&at, nat, target, tol as isize, hs)
            }
        };
        for j in 0..n {
            out[o + j] += window[j] * at(actual + j as isize);
        }
        map.push(actual as f64);
        prev_actual = Some(actual);
        nominal += hs as f64 * speed(o);
        o += hs;
    }
    out.truncate(out_len);
    Stretched { out, map, map_hop: hs }
}

/// Finds the position within `target ± tol` whose next `len` samples best
/// match the natural continuation at `nat`. Coarse search on every 4th
/// sample/offset, then a fine search around the winner.
fn best_offset(at: &impl Fn(isize) -> f32, nat: isize, target: isize, tol: isize, len: usize) -> isize {
    let corr = |cand: isize, stride: usize| -> f32 {
        let mut s = 0.0f32;
        let mut j = 0usize;
        while j < len {
            s += at(nat + j as isize) * at(cand + j as isize);
            j += stride;
        }
        s
    };
    let mut best = (f32::MIN, target);
    let mut d = -tol;
    while d <= tol {
        let c = corr(target + d, 4);
        if c > best.0 {
            best = (c, target + d);
        }
        d += 4;
    }
    let coarse = best.1;
    let mut best = (f32::MIN, coarse);
    for d in -4..=4 {
        let cand = coarse + d;
        if (cand - target).abs() > tol {
            continue;
        }
        let c = corr(cand, 1);
        if c > best.0 {
            best = (c, cand);
        }
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_speed_consumes_exact_input() {
        let sr = 44100;
        let x: Vec<f32> = (0..sr * 20).map(|i| ((i as f32) * 0.05).sin() * 0.3).collect();
        let out_len = sr as usize * 10;
        let s = stretch_variable(&x, sr, 1000, out_len, |_| 1.03);
        assert_eq!(s.out.len(), out_len);
        let end = s.input_pos(out_len - 1);
        let expected = 1000.0 + out_len as f64 * 1.03;
        // Within the search tolerance (6 ms) — no accumulated drift.
        assert!((end - expected).abs() < 0.008 * sr as f64, "end {end} expected {expected}");
    }

    #[test]
    fn preserves_pitch() {
        // A 440 Hz tone stretched 5% faster should still be ~440 Hz.
        let sr = 44100;
        let x: Vec<f32> = (0..sr * 6).map(|i| (std::f32::consts::TAU * 440.0 * i as f32 / sr as f32).sin()).collect();
        let s = stretch_variable(&x, sr, 0, sr as usize * 4, |_| 1.05);
        let seg = &s.out[sr as usize..sr as usize * 3];
        let zc = seg.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        let freq = zc as f64 / 2.0;
        assert!((freq - 440.0).abs() < 6.0, "freq {freq}");
    }
}
