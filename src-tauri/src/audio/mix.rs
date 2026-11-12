//! Transition planning and automation — what a DJ actually does with their
//! hands during a mix, expressed as functions of time.
//!
//! The model follows how real DJ blends (and Spotify's Mix feature) work:
//!
//! 1. **Beatmatch, don't key-shift.** The incoming track (B) is time
//!    stretched with key lock to the outgoing track's (A) exact tempo,
//!    measured from the audio at the mix point (see `beatgrid`), and its
//!    entry point is placed on one of A's beats. B is never pitch-shifted:
//!    harmonic compatibility is handled the way DJs handle it — by choosing
//!    compatible keys, and by keeping clashing mids apart with the EQ —
//!    rather than by audibly transposing a song and then snapping it back.
//! 2. **Bass swap on the phrase.** The two basslines never play at full
//!    together. B comes in with its lows killed, A's and B's lows trade
//!    places on a bar line in the middle of the blend, and A's remaining
//!    mids/highs then fade out. This is the single biggest difference
//!    between "two songs playing at once" and a mix.
//! 3. **Short, bar-quantised blends.** Lengths are whole bars at A's tempo.
//! 4. **Gain match.** B's level is trimmed to A's like a channel gain knob.
//! 5. **Glide home.** After A is gone, B's tempo glides back to its native
//!    tempo over several bars (inaudible), so the rest of B plays at its
//!    true speed and the playhead tracks its real timeline.

use super::beatgrid::{self, Grid};
use super::stretch::{self, Stretched};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Blend,
    FilterSweep,
    Cut,
    EchoOut,
    Backspin,
    LoopRoll,
}

impl Kind {
    pub fn parse(s: &str) -> Kind {
        match s.to_ascii_lowercase().as_str() {
            "filter sweep" => Kind::FilterSweep,
            "cut" => Kind::Cut,
            "echo out" => Kind::EchoOut,
            "backspin" => Kind::Backspin,
            "loop roll" => Kind::LoopRoll,
            _ => Kind::Blend,
        }
    }
}

pub struct Request {
    pub kind: Kind,
    pub crossfade_secs: Option<f64>,
    pub cue_out_secs: Option<f64>,
    pub cue_in_secs: Option<f64>,
}

pub struct TrackInfo<'a> {
    pub samples: &'a [f32],
    pub bpm: Option<f64>,
    pub key: Option<&'a str>,
}

/// B's processed head: the beatmatched blend plus the tempo glide, ending in
/// a short splice into B's untouched audio at `resume`.
pub struct Prefix {
    pub audio: Stretched,
    pub resume: usize,
}

pub struct Plan {
    pub kind: Kind,
    /// A sample where the transition begins.
    pub start_a: f64,
    /// Offset (samples, A's timeline) from `start_a` at which B starts.
    pub b_enter: f64,
    /// Offset from `start_a` at which A is fully gone and B takes over.
    pub a_exit: f64,
    /// B sample (native timeline) that plays at `start_a + b_enter`.
    pub start_b: f64,
    /// B samples consumed per output sample during the blend.
    pub speed: f64,
    pub beatmatched: bool,
    /// A's beat period in samples.
    pub beat: f64,
    pub clash: bool,
    pub trim_b: f32,
    pub prefix: Option<Prefix>,
    pub sr: f64,
}

#[derive(Clone, Copy, Default)]
pub struct DeckParams {
    pub gain: f32,
    pub eq: [f32; 3],
    pub filter: f32,
}

impl DeckParams {
    pub const UNITY: DeckParams = DeckParams { gain: 1.0, eq: [1.0, 1.0, 1.0], filter: 0.0 };
}

pub struct MixParams {
    pub a: DeckParams,
    pub b: DeckParams,
    pub echo_send: f32,
    pub echo_return: f32,
}

const MAX_STRETCH: f64 = 1.16;
const SPLICE_SECS: f64 = 0.04;

#[inline]
fn clamp01(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

#[inline]
fn ease_in(x: f64) -> f32 {
    (clamp01(x) * std::f64::consts::FRAC_PI_2).sin() as f32
}

#[inline]
fn ease_out(x: f64) -> f32 {
    (clamp01(x) * std::f64::consts::FRAC_PI_2).cos() as f32
}

/// Gated loudness estimate: 75th percentile of 400 ms block RMS — roughly
/// "how loud is the body of the track", ignoring quiet intros/outros.
fn loudness(x: &[f32], sr: u32) -> f32 {
    let block = (sr as f64 * 0.4) as usize;
    let mut v: Vec<f32> = x
        .chunks(block)
        .filter(|c| c.len() == block)
        .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / block as f32).sqrt())
        .filter(|r| *r > 1e-4)
        .collect();
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[(v.len() * 3 / 4).min(v.len() - 1)]
}

fn first_sound(x: &[f32]) -> usize {
    x.iter().position(|s| s.abs() > 0.02).unwrap_or(0)
}

pub fn plan(a: TrackInfo, b: TrackInfo, sr: u32, req: &Request) -> Plan {
    let srf = sr as f64;
    let a_len = a.samples.len() as f64;
    let b_len = b.samples.len() as f64;
    let kind = req.kind;

    // --- A's grid at the mix point -------------------------------------
    let rough_beat_a = a.bpm.filter(|b| *b > 0.0).map(|bpm| 60.0 * srf / bpm).unwrap_or(0.5 * srf);
    let length_beats = |beat: f64| -> (f64, f64) {
        // (b_enter, a_exit) in samples for this transition type.
        match kind {
            Kind::Blend | Kind::FilterSweep => {
                let secs = req.crossfade_secs.unwrap_or(8.0 * 4.0 * beat / srf).max(0.5);
                let bars = (secs * srf / (4.0 * beat)).round().clamp(1.0, 32.0);
                (0.0, bars * 4.0 * beat)
            }
            Kind::Cut => (0.0, 0.012 * srf),
            Kind::EchoOut => (4.0 * beat, 12.0 * beat),
            Kind::Backspin => (2.0 * beat, 2.0 * beat),
            Kind::LoopRoll => (4.0 * beat, 4.0 * beat),
        }
    };
    let (_, rough_exit) = length_beats(rough_beat_a);
    let guess_a = req
        .cue_out_secs
        .map(|s| s * srf)
        .unwrap_or(a_len - rough_exit)
        .clamp(0.0, (a_len - rough_exit).max(0.0));
    let grid_a = a.bpm.and_then(|bpm| {
        let lo = (guess_a - 32.0 * rough_beat_a).max(0.0) as usize;
        let hi = (guess_a + rough_exit + 8.0 * rough_beat_a).min(a_len) as usize;
        let hi = hi.max((lo as f64 + 20.0 * srf).min(a_len) as usize);
        beatgrid::fit_grid(a.samples, sr, bpm, lo, hi)
    });
    let beat_a = grid_a.map(|g| g.period).unwrap_or(rough_beat_a);
    let (b_enter, a_exit) = length_beats(beat_a);

    let mut start_a = grid_a.map(|g| g.snap(guess_a)).unwrap_or(guess_a);
    // Must fit before A ends: pull back by whole bars so the phrase
    // alignment of the cue is kept.
    let room = if kind == Kind::Backspin { 0.0 } else { a_exit };
    while start_a + room > a_len - 1.0 && start_a > 4.0 * beat_a {
        start_a -= 4.0 * beat_a;
    }
    let start_a = start_a.clamp(0.0, a_len);

    // --- B's grid at its entry point -----------------------------------
    let rough_beat_b = b.bpm.filter(|v| *v > 0.0).map(|bpm| 60.0 * srf / bpm).unwrap_or(0.5 * srf);
    let guess_b = req.cue_in_secs.map(|s| s * srf).unwrap_or(first_sound(b.samples) as f64).clamp(0.0, b_len);
    let grid_b: Option<Grid> = b.bpm.and_then(|bpm| {
        let lo = guess_b as usize;
        let span = ((a_exit - b_enter) * 1.2 + 16.0 * rough_beat_b).max(20.0 * srf);
        let hi = (guess_b + span).min(b_len) as usize;
        beatgrid::fit_grid(b.samples, sr, bpm, lo, hi)
    });
    let start_b = match (grid_b, req.cue_in_secs) {
        (Some(g), Some(_)) => g.snap(guess_b).max(0.0),
        (Some(g), None) => g.next_beat(guess_b - 0.25 * g.period).max(0.0),
        (None, _) => guess_b,
    };

    // --- Tempo ---------------------------------------------------------
    let (speed, beatmatched) = match (a.bpm, b.bpm) {
        (Some(_), Some(_)) => {
            let pb = grid_b.map(|g| g.period).unwrap_or(rough_beat_b);
            let (s, _) = beatgrid::tempo_ratio(beat_a, pb);
            if s.ln().abs() > MAX_STRETCH.ln() { (1.0, false) } else { (s, true) }
        }
        _ => (1.0, false),
    };

    let clash = matches!((a.key, b.key), (Some(ka), Some(kb)) if super::key::camelot_relation(ka, kb) == "clash");

    // --- Gain match ----------------------------------------------------
    let la = loudness(a.samples, sr);
    let lb = loudness(b.samples, sr);
    let trim_b = if la > 0.0 && lb > 0.0 { (la / lb).clamp(0.5, 1.25) } else { 1.0 };

    // --- B's processed head --------------------------------------------
    let prefix = if (speed - 1.0).abs() > 0.0005 {
        let blend = (a_exit - b_enter).max(0.0);
        let glide_bars = ((speed - 1.0).abs() * 400.0).ceil().clamp(8.0, 32.0);
        let glide = glide_bars * 4.0 * beat_a;
        let tail = 0.25 * srf;
        let out_len = (blend + glide + tail) as usize;
        let speed_at = move |o: usize| -> f64 {
            let o = o as f64;
            if o < blend {
                speed
            } else {
                let u = clamp01((o - blend) / glide);
                // Smoothstep: no sudden change in the *rate* of the glide either.
                let s = u * u * (3.0 - 2.0 * u);
                speed + (1.0 - speed) * s
            }
        };
        Some(render_prefix(b.samples, sr, start_b, out_len, speed, speed_at))
    } else {
        None
    };

    Plan { kind, start_a, b_enter, a_exit, start_b, speed, beatmatched, beat: beat_a, clash, trim_b, prefix, sr: srf }
}

fn render_prefix(x: &[f32], sr: u32, start_b: f64, out_len: usize, speed0: f64, speed_at: impl Fn(usize) -> f64) -> Prefix {
    // Start half a WSOLA frame early and drop it, so the first real output
    // sample isn't inside the stretcher's fade-in.
    let lead = (sr as f64 * 0.023) as usize;
    let pre_start = (start_b - lead as f64 * speed0).max(0.0);
    let actual_lead = ((start_b - pre_start) / speed0).round() as usize;
    let mut s = stretch::stretch_variable(x, sr, pre_start as usize, out_len + actual_lead, |o| {
        if o < actual_lead { speed0 } else { speed_at(o - actual_lead) }
    });
    s.out.drain(..actual_lead);
    // Re-base the map so output index 0 is the real entry point.
    let hop = s.map_hop;
    let new_map: Vec<f64> = (0..=(out_len / hop + 1)).map(|i| s.input_pos(i * hop + actual_lead)).collect();
    s.map = new_map;
    s.out.truncate(out_len);

    // Splice into B's raw audio at the exact source position we've reached.
    let resume = (s.input_pos(out_len).round().max(0.0) as usize).min(x.len());
    let splice = ((SPLICE_SECS * sr as f64) as usize).min(out_len).min(resume);
    for i in 0..splice {
        let o = out_len - splice + i;
        let raw = x[resume - splice + i];
        let t = (i as f64 + 0.5) / splice as f64;
        let g_from = (t * std::f64::consts::FRAC_PI_2).cos() as f32;
        let g_to = (t * std::f64::consts::FRAC_PI_2).sin() as f32;
        s.out[o] = s.out[o] * g_from + raw * g_to;
    }
    Prefix { audio: s, resume }
}

impl Plan {
    /// B's native sample position after `c` output samples since it entered.
    pub fn b_native_at(&self, c: f64) -> f64 {
        match &self.prefix {
            Some(p) => {
                let n = p.audio.out.len() as f64;
                if c < n { p.audio.input_pos(c.max(0.0) as usize) } else { p.resume as f64 + (c - n) }
            }
            None => self.start_b + c,
        }
    }

    /// When A's playhead is taken over by an effect (backspin, loop roll),
    /// the A sample to read at transition time `t`.
    pub fn a_read_override(&self, t: f64) -> Option<f64> {
        match self.kind {
            Kind::Backspin => {
                let dur = self.a_exit.max(1.0);
                let u = clamp01(t / dur);
                // Velocity -V0*(1-u)^2 (a hand flick decelerating to a stop),
                // integrated.
                const V0: f64 = 3.0;
                Some(self.start_a - V0 * dur / 3.0 * (1.0 - (1.0 - u).powi(3)))
            }
            Kind::LoopRoll => {
                let b = self.beat;
                let len = if t < 2.0 * b { 0.5 * b } else if t < 3.0 * b { 0.25 * b } else { 0.125 * b };
                let seg_start = if t < 2.0 * b { 0.0 } else if t < 3.0 * b { 2.0 * b } else { 3.0 * b };
                Some(self.start_a + ((t - seg_start) % len))
            }
            _ => None,
        }
    }

    /// Loop length active at `t`, for click-free loop wraps.
    pub fn loop_len(&self, t: f64) -> Option<(f64, f64)> {
        if self.kind != Kind::LoopRoll {
            return None;
        }
        let b = self.beat;
        let (len, seg) = if t < 2.0 * b { (0.5 * b, 0.0) } else if t < 3.0 * b { (0.25 * b, 2.0 * b) } else { (0.125 * b, 3.0 * b) };
        Some((len, (t - seg) % len))
    }

    pub fn echo_delay(&self) -> usize {
        (self.beat * 0.75) as usize
    }

    /// The mixer moves at transition time `t` (samples since `start_a`).
    pub fn params(&self, t: f64) -> MixParams {
        let beat = self.beat;
        let mut a = DeckParams::UNITY;
        let mut b = DeckParams::UNITY;
        let mut echo_send = 0.0;
        let mut echo_return = 0.0;
        let len = self.a_exit.max(1.0);

        match self.kind {
            Kind::Blend => {
                let bars = (len / (4.0 * beat)).round().max(1.0);
                let swap = if bars >= 2.0 { (bars / 2.0).round() * 4.0 * beat } else { 2.0 * beat };
                let after = (len - swap).max(1.0);
                // Incoming: fader up through the first half with the bass
                // killed; bass returns on the swap bar.
                b.gain = ease_in(t / swap);
                b.eq[0] = clamp01((t - swap) / beat) as f32;
                if self.clash {
                    // Keys clash: keep B's melodic band mostly out until A's
                    // has gone, so the two never sit on top of each other.
                    b.eq[1] = if t < swap { 0.3 } else { 0.3 + 0.7 * clamp01((t - swap) / (2.0 * beat)) as f32 };
                    b.eq[2] = if t < swap { 0.8 } else { 1.0 };
                } else {
                    b.eq[1] = 0.6 + 0.4 * clamp01(t / swap) as f32;
                }
                // Outgoing: full until the swap, bass out on the swap, then
                // the rest fades away.
                a.eq[0] = 1.0 - clamp01((t - swap) / beat) as f32;
                a.gain = if t < swap { 1.0 } else { ease_out((t - swap) / after) };
                a.eq[2] = 1.0 - 0.4 * clamp01((t - swap) / after) as f32;
                if self.clash {
                    a.eq[1] = 1.0 - 0.7 * clamp01((t - swap) / (2.0 * beat)) as f32;
                }
            }
            Kind::FilterSweep => {
                let swap = (len / (4.0 * beat) / 2.0).round().max(0.5) * 4.0 * beat;
                b.gain = ease_in(t / (0.25 * len));
                b.filter = -0.75 * (1.0 - clamp01(t / (0.75 * len))) as f32;
                b.eq[0] = clamp01((t - swap) / beat) as f32;
                a.filter = 0.55 * clamp01((t - 0.25 * len) / (0.75 * len)) as f32;
                a.eq[0] = 1.0 - clamp01((t - swap) / beat) as f32;
                a.gain = if t < 0.75 * len { 1.0 } else { ease_out((t - 0.75 * len) / (0.25 * len)) };
            }
            Kind::Cut => {
                a.gain = 1.0 - clamp01(t / len) as f32;
                b.gain = clamp01(t / (0.003 * self.sr)) as f32;
            }
            Kind::EchoOut => {
                let drop = self.b_enter;
                if t < drop {
                    echo_send = if t >= drop - beat { 1.0 } else { 0.0 };
                    a.filter = 0.25 * clamp01((t - (drop - 2.0 * beat)) / beat) as f32;
                    b.gain = 0.0;
                } else {
                    a.gain = 0.0;
                    a.filter = 0.25;
                }
                echo_return = if t < drop + 4.0 * beat { 1.0 } else { ease_out((t - drop - 4.0 * beat) / (len - drop - 4.0 * beat).max(1.0)) };
            }
            Kind::Backspin => {
                let u = t / len;
                a.gain = if u < 0.6 { 1.0 } else { ease_out((u - 0.6) / 0.4) };
                a.filter = 0.2 * clamp01(u) as f32;
                b.gain = if t >= self.b_enter { 1.0 } else { 0.0 };
            }
            Kind::LoopRoll => {
                a.filter = 0.4 * clamp01(t / len) as f32;
                if let Some((l, phase)) = self.loop_len(t) {
                    // ~1.5 ms micro-fades at each loop wrap.
                    let edge = 64.0f64.min(l / 4.0);
                    a.gain = (clamp01(phase / edge) * clamp01((l - phase) / edge)) as f32;
                }
                b.gain = if t >= self.b_enter { 1.0 } else { 0.0 };
            }
        }
        MixParams { a, b, echo_send, echo_return }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic "track": kick drum at `bpm` plus a sustained tone.
    fn track(sr: u32, bpm: f64, secs: f64, tone: f32) -> Vec<f32> {
        let n = (secs * sr as f64) as usize;
        let period = 60.0 * sr as f64 / bpm;
        let mut x: Vec<f32> = (0..n).map(|i| (std::f32::consts::TAU * tone * i as f32 / sr as f32).sin() * 0.1).collect();
        let mut t = 0.0;
        while (t as usize) < n {
            let s = t as usize;
            for j in 0..3000.min(n - s) {
                let env = (-(j as f32) / 400.0).exp();
                x[s + j] += (std::f32::consts::TAU * 55.0 * j as f32 / sr as f32).sin() * env * 0.6;
            }
            t += period;
        }
        x
    }

    #[test]
    fn blend_beatmatches_and_glides_home() {
        let sr = 44100;
        let a = track(sr, 128.0, 90.0, 220.0);
        let b = track(sr, 124.0, 120.0, 330.0);
        let req = Request { kind: Kind::Blend, crossfade_secs: Some(15.0), cue_out_secs: Some(60.0), cue_in_secs: Some(0.0) };
        let p = plan(
            TrackInfo { samples: &a, bpm: Some(128.0), key: Some("8A") },
            TrackInfo { samples: &b, bpm: Some(124.0), key: Some("8A") },
            sr,
            &req,
        );
        assert!(p.beatmatched);
        assert!((p.speed - 128.0 / 124.0).abs() < 0.003, "speed {}", p.speed);
        // 15 s at 128 BPM ≈ 8 bars.
        let bars = p.a_exit / (4.0 * p.beat);
        assert!((bars - 8.0).abs() < 0.01, "bars {bars}");
        // Start lands on one of A's beats.
        let beat_phase = (p.start_a / p.beat).fract();
        assert!(beat_phase < 0.02 || beat_phase > 0.98, "phase {beat_phase}");
        // Glides back to native speed — prefix ends and raw takes over.
        let pre = p.prefix.as_ref().expect("prefix");
        assert!(pre.resume > p.start_b as usize);
        // At the end of the blend, B has advanced by exactly speed * blend.
        let b_at_exit = p.b_native_at(p.a_exit);
        let expected = p.start_b + p.a_exit * p.speed;
        assert!((b_at_exit - expected).abs() < 0.008 * sr as f64, "{b_at_exit} vs {expected}");
    }

    #[test]
    fn transition_fits_before_end_of_track() {
        let sr = 44100;
        let a = track(sr, 128.0, 60.0, 220.0);
        let b = track(sr, 128.0, 60.0, 330.0);
        let req = Request { kind: Kind::Blend, crossfade_secs: Some(15.0), cue_out_secs: Some(55.0), cue_in_secs: None };
        let p = plan(TrackInfo { samples: &a, bpm: Some(128.0), key: None }, TrackInfo { samples: &b, bpm: Some(128.0), key: None }, sr, &req);
        assert!(p.start_a + p.a_exit <= a.len() as f64);
    }

    #[test]
    fn automation_ends_at_unity_for_incoming() {
        let sr = 44100;
        let a = track(sr, 128.0, 60.0, 220.0);
        let b = track(sr, 128.0, 60.0, 330.0);
        for kind in [Kind::Blend, Kind::FilterSweep, Kind::Cut, Kind::EchoOut, Kind::Backspin, Kind::LoopRoll] {
            let req = Request { kind, crossfade_secs: Some(8.0), cue_out_secs: Some(30.0), cue_in_secs: Some(0.0) };
            let p = plan(TrackInfo { samples: &a, bpm: Some(128.0), key: Some("8A") }, TrackInfo { samples: &b, bpm: Some(128.0), key: Some("2B") }, sr, &req);
            let end = p.params(p.a_exit);
            assert!((end.b.gain - 1.0).abs() < 1e-3, "{kind:?} b gain {}", end.b.gain);
            assert!(end.b.eq.iter().all(|g| (g - 1.0).abs() < 1e-3), "{kind:?} b eq {:?}", end.b.eq);
            assert!(end.b.filter.abs() < 1e-3, "{kind:?} b filter");
            // Bass of both tracks is never full at the same time.
            let mut t = 0.0;
            while t < p.a_exit {
                let m = p.params(t);
                let a_low = m.a.gain * m.a.eq[0];
                let b_low = m.b.gain * m.b.eq[0];
                assert!(a_low.min(b_low) < 0.75 || kind == Kind::Cut, "{kind:?} bass clash at {t}");
                t += p.beat / 8.0;
            }
        }
    }
}

/// Real-audio check, run manually:
///   GIGFLOW_MIX_A=/path/a.mp3 GIGFLOW_MIX_B=/path/b.mp3 GIGFLOW_MIX_OUT=/tmp/out.wav \
///   cargo test --lib real_mix -- --ignored --nocapture
/// Renders the transition through the live engine to a WAV and measures how
/// tightly B's beats land on A's during the blend.
#[cfg(test)]
mod real_audio {
    use super::*;
    use crate::audio::{bpm, decode, hotcues, key};
    use crate::audio_player::{Deck, Engine};
    use std::sync::Arc;

    fn write_wav(path: &str, x: &[f32], sr: u32) {
        let mut v = Vec::with_capacity(44 + x.len() * 2);
        let data_len = (x.len() * 2) as u32;
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data_len).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&sr.to_le_bytes());
        v.extend_from_slice(&(sr * 2).to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data_len.to_le_bytes());
        for s in x {
            v.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
        std::fs::write(path, v).unwrap();
    }

    fn cues(samples: &[f32], sr: u32, bpm: f64) -> Vec<crate::models::HotCue> {
        hotcues::detect_hot_cues(samples, sr, bpm)
            .into_iter()
            .enumerate()
            .map(|(i, c)| crate::models::HotCue { id: i.to_string(), track_id: "t".into(), pad: i as i64 + 1, position_secs: c.position_secs, label: c.label })
            .collect()
    }

    #[test]
    #[ignore]
    fn real_mix() {
        let pa = std::env::var("GIGFLOW_MIX_A").expect("GIGFLOW_MIX_A");
        let pb = std::env::var("GIGFLOW_MIX_B").expect("GIGFLOW_MIX_B");
        let kind = Kind::parse(&std::env::var("GIGFLOW_MIX_KIND").unwrap_or_else(|_| "crossfade".into()));
        let a = decode::decode_file(std::path::Path::new(&pa)).unwrap();
        let b = decode::decode_file(std::path::Path::new(&pb)).unwrap();
        let sr = a.sample_rate;
        let bs = decode::resample_linear(&b.samples, b.sample_rate, sr);
        let bpm_a = bpm::detect_bpm(&a.samples, sr).bpm;
        let bpm_b = bpm::detect_bpm(&bs, sr).bpm;
        let key_a = key::detect_key(&a.samples, sr).camelot;
        let key_b = key::detect_key(&bs, sr).camelot;
        let ca = cues(&a.samples, sr, bpm_a);
        let cb = cues(&bs, sr, bpm_b);
        let cue_out = crate::transition_suggest::pick_cue_out(&ca).map(|c| c.position_secs);
        let cue_in = crate::transition_suggest::pick_cue_in(&cb).map(|c| c.position_secs);
        let secs = crate::transition_suggest::recommended_crossfade_secs(Some(bpm_a), false, None, None);
        println!("A bpm={bpm_a:.2} key={key_a} cue_out={cue_out:?}  B bpm={bpm_b:.2} key={key_b} cue_in={cue_in:?}  len={secs:.1}s");

        let t0 = std::time::Instant::now();
        let req = Request { kind, crossfade_secs: Some(secs), cue_out_secs: cue_out, cue_in_secs: cue_in };
        let p = plan(
            TrackInfo { samples: &a.samples, bpm: Some(bpm_a), key: Some(&key_a) },
            TrackInfo { samples: &bs, bpm: Some(bpm_b), key: Some(&key_b) },
            sr,
            &req,
        );
        println!(
            "plan in {:?}: start_a={:.2}s start_b={:.2}s speed={:.4} beatA={:.3}ms bars={:.1} trim={:.2} clash={}",
            t0.elapsed(),
            p.start_a / sr as f64,
            p.start_b / sr as f64,
            p.speed,
            p.beat / sr as f64 * 1000.0,
            p.a_exit / (4.0 * p.beat),
            p.trim_b,
            p.clash
        );

        // Beat alignment: B's processed audio vs A, over the blend.
        if let Some(pre) = &p.prefix {
            let blend = p.a_exit as usize;
            let a_seg = &a.samples[p.start_a as usize..(p.start_a as usize + blend).min(a.samples.len())];
            let b_seg = &pre.audio.out[..blend.min(pre.audio.out.len())];
            let bpm_target = 60.0 * sr as f64 / p.beat;
            for (name, lo, hi) in [("first half", 0, blend / 2), ("second half", blend / 2, blend)] {
                let ga = beatgrid::fit_grid(a_seg, sr, bpm_target, lo, hi.min(a_seg.len()));
                let gb = beatgrid::fit_grid(b_seg, sr, bpm_target, lo, hi.min(b_seg.len()));
                if let (Some(ga), Some(gb)) = (ga, gb) {
                    let mut d = (gb.anchor - ga.anchor) % ga.period;
                    if d > ga.period / 2.0 { d -= ga.period; }
                    if d < -ga.period / 2.0 { d += ga.period; }
                    println!("{name}: B beat offset vs A = {:+.1} ms (A {:.2} bpm, B {:.2} bpm)", d / sr as f64 * 1000.0, 60.0 * sr as f64 / ga.period, 60.0 * sr as f64 / gb.period);
                } else {
                    println!("{name}: no grid (ga={} gb={})", ga.is_some(), gb.is_some());
                }
            }
        }

        // Direct drift check: cross-correlate onset envelopes of A and B
        // (as heard) per 2-bar block; the lag is how far B's hits are from A's.
        if let Some(pre) = &p.prefix {
            let blend = p.a_exit as usize;
            let sa = p.start_a as usize;
            let a_seg: Vec<f32> = a.samples[sa..(sa + blend).min(a.samples.len())].to_vec();
            let b_seg: Vec<f32> = pre.audio.out[..blend.min(pre.audio.out.len())].to_vec();
            let ea = beatgrid::onset_envelope(&a_seg, sr, 0, a_seg.len());
            let eb = beatgrid::onset_envelope(&b_seg, sr, 0, b_seg.len());
            let block = (8.0 * p.beat / beatgrid::HOP as f64) as usize;
            let max_lag = (0.06 * sr as f64 / beatgrid::HOP as f64) as isize;
            let mut lags = Vec::new();
            let mut i = 0;
            while i + block <= ea.len().min(eb.len()) {
                let mut best = (f32::MIN, 0isize);
                for lag in -max_lag..=max_lag {
                    let mut c = 0.0f32;
                    for j in i..i + block {
                        let k = j as isize + lag;
                        if k >= 0 && (k as usize) < eb.len() { c += ea[j] * eb[k as usize]; }
                    }
                    if c > best.0 { best = (c, lag); }
                }
                lags.push(best.1 as f64 * beatgrid::HOP as f64 / sr as f64 * 1000.0);
                i += block;
            }
            println!("B-vs-A lag per 2 bars (ms): {:?}", lags.iter().map(|l| l.round() as i64).collect::<Vec<_>>());
        }

        let start_a = p.start_a;
        let a_exit = p.a_exit;
        let mut e = Engine::new(sr, Deck::new("a".into(), Arc::new(a.samples.clone()), sr, 0.0));
        e.arm(Arc::new(p), "b".into(), Arc::new(bs));
        e.seek(Some("a"), (start_a / sr as f64 - 10.0).max(0.0));
        let total = (a_exit + 30.0 * sr as f64) as usize;
        let mut out = vec![0.0f32; total];
        // Render in engine-sized blocks like the audio thread would.
        for chunk in out.chunks_mut(256) {
            for s in chunk.iter_mut() {
                *s = 0.0;
            }
            let mut tmp = vec![0.0f32; chunk.len()];
            for t in tmp.iter_mut() {
                *t = 0.0;
            }
            e_render(&mut e, &mut tmp);
            chunk.copy_from_slice(&tmp);
        }
        let st = e.status(false);
        println!("after render: decks={:?}", st.decks.iter().map(|d| (d.track_id.clone(), d.position_secs)).collect::<Vec<_>>());
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let lead = (start_a - (start_a / sr as f64 - 10.0).max(0.0) * sr as f64) as usize;
        let pct = |r: &[f32]| 100.0 * r.iter().filter(|s| s.abs() > 0.85).count() as f64 / r.len().max(1) as f64;
        let rms = |r: &[f32]| (r.iter().map(|s| s * s).sum::<f32>() / r.len().max(1) as f32).sqrt();
        let (pre, rest) = out.split_at(lead.min(out.len()));
        let (blend, post) = rest.split_at((a_exit as usize).min(rest.len()));
        println!(
            "peak={peak:.3}  >0.85: solo-A {:.2}% blend {:.2}% solo-B {:.2}%   rms: A {:.3} blend {:.3} B {:.3}",
            pct(pre), pct(blend), pct(post), rms(pre), rms(blend), rms(post)
        );
        if let Ok(path) = std::env::var("GIGFLOW_MIX_OUT") {
            write_wav(&path, &out, sr);
            println!("wrote {path}");
        }
    }

    fn e_render(e: &mut Engine, buf: &mut [f32]) {
        e.render_for_test(buf);
    }
}
