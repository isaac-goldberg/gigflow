//! Real-time two-deck mix engine.
//!
//! Instead of rendering a fixed preview clip, playback is a live "program":
//! the current track plays on the main deck, and if a transition is armed
//! the incoming track is brought in on a second deck at exactly the planned
//! point — beatmatched, EQ'd and faded by `audio::mix::Plan` — after which
//! it becomes the main deck. Both decks report their own native playhead,
//! so the UI can show where each track is during the blend, and seeking
//! either one behaves the way you'd expect.

use crate::audio::dsp::{soft_limit, DjFilter, Echo, Isolator, Smooth};
use crate::audio::mix::{DeckParams, Kind, Plan};
use rodio::{OutputStream, Sink, Source};
use serde::Serialize;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct Deck {
    pub track_id: String,
    samples: Arc<Vec<f32>>,
    /// Beatmatched/gliding head (only on a deck that came in via a
    /// transition) and the output-sample cursor within it.
    plan: Option<Arc<Plan>>,
    prefix_cursor: usize,
    /// Raw read position (samples) once past any prefix.
    pos: f64,
    trim: f32,
    iso: Isolator,
    filter: DjFilter,
    gain: Smooth,
}

impl Deck {
    pub fn new(track_id: String, samples: Arc<Vec<f32>>, sr: u32, pos: f64) -> Self {
        let srf = sr as f32;
        Self {
            track_id,
            samples,
            plan: None,
            prefix_cursor: 0,
            pos,
            trim: 1.0,
            iso: Isolator::new(srf),
            filter: DjFilter::new(srf),
            gain: Smooth::new(srf, 0.005, 1.0),
        }
    }

    fn in_prefix(&self) -> bool {
        matches!(&self.plan, Some(p) if p.prefix.as_ref().is_some_and(|pre| self.prefix_cursor < pre.audio.out.len()))
    }

    pub fn native_pos(&self) -> f64 {
        if self.in_prefix() {
            let pre = self.plan.as_ref().unwrap().prefix.as_ref().unwrap();
            pre.audio.input_pos(self.prefix_cursor)
        } else {
            self.pos
        }
    }

    fn finished(&self) -> bool {
        !self.in_prefix() && self.pos >= self.samples.len() as f64
    }

    /// Positions an incoming deck `c` output samples after it entered.
    fn set_entry_cursor(&mut self, c: f64) {
        let plan = self.plan.clone().unwrap();
        match &plan.prefix {
            Some(pre) if (c as usize) < pre.audio.out.len() => {
                self.prefix_cursor = c.max(0.0) as usize;
            }
            Some(pre) => {
                self.prefix_cursor = pre.audio.out.len();
                self.pos = pre.resume as f64 + (c - pre.audio.out.len() as f64);
            }
            None => {
                self.prefix_cursor = 0;
                self.pos = plan.start_b + c.max(0.0);
            }
        }
    }

    /// Plain seek: drops any remaining beatmatched head (the track then plays
    /// at its own tempo from there).
    fn seek(&mut self, pos: f64) {
        if let Some(p) = &self.plan {
            if let Some(pre) = &p.prefix {
                self.prefix_cursor = pre.audio.out.len();
            }
        }
        self.pos = pos.clamp(0.0, self.samples.len() as f64);
    }

    #[inline]
    fn read_at(&self, pos: f64) -> f32 {
        if pos < 0.0 {
            return 0.0;
        }
        let i = pos.floor() as usize;
        let frac = (pos - i as f64) as f32;
        let s0 = self.samples.get(i).copied().unwrap_or(0.0);
        let s1 = self.samples.get(i + 1).copied().unwrap_or(0.0);
        s0 + (s1 - s0) * frac
    }

    /// Next sample in normal playback order.
    #[inline]
    fn next_sample(&mut self) -> f32 {
        if self.in_prefix() {
            let pre = self.plan.as_ref().unwrap().prefix.as_ref().unwrap();
            let s = pre.audio.out[self.prefix_cursor];
            self.prefix_cursor += 1;
            if self.prefix_cursor >= pre.audio.out.len() {
                self.pos = pre.resume as f64;
            }
            s
        } else {
            let i = self.pos as usize;
            self.pos += 1.0;
            self.samples.get(i).copied().unwrap_or(0.0)
        }
    }

    /// Channel strip: trim -> EQ -> filter -> fader. Returns (pre-fader,
    /// post-fader) so the echo can be fed pre-fader like a real send.
    #[inline]
    fn process(&mut self, x: f32, p: &DeckParams) -> (f32, f32) {
        let y = self.iso.process(x * self.trim, p.eq);
        let y = self.filter.process(y, p.filter);
        (y, y * self.gain.next(p.gain))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TransitionState {
    Pending,
    Mixing,
    Missed,
}

pub struct Transition {
    pub plan: Arc<Plan>,
    pub to_track_id: String,
    pub incoming: Deck,
    pub state: TransitionState,
    /// Samples since `plan.start_a` while mixing.
    t: f64,
    echo: Echo,
}

pub struct Engine {
    pub sr: u32,
    pub main: Option<Deck>,
    pub transition: Option<Transition>,
    pub finished: bool,
    /// Bumped on every arm/disarm so a slow, stale arm request can tell it
    /// has been superseded.
    pub arm_seq: u64,
    /// Master-bus loudness compensation during a blend.
    master: Smooth,
}

/// Rough energy of a channel at these settings (bass weighted most — that's
/// where the summed level piles up).
fn channel_energy(p: &DeckParams) -> f32 {
    p.gain * p.gain * (0.45 * p.eq[0] * p.eq[0] + 0.4 * p.eq[1] * p.eq[1] + 0.15 * p.eq[2] * p.eq[2])
}

#[derive(Serialize, Clone)]
pub struct TransitionZone {
    pub from_track_id: String,
    pub to_track_id: String,
    /// A's native timeline: where the transition starts / where A is gone.
    pub a_start_secs: f64,
    pub a_end_secs: f64,
    /// B's native timeline: where B enters / where it is playing solo.
    pub b_start_secs: f64,
    pub b_end_secs: f64,
    pub tempo_ratio: f64,
    pub beatmatched: bool,
    pub bars: f64,
}

#[derive(Serialize)]
pub struct DeckStatus {
    pub track_id: String,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub role: &'static str,
}

#[derive(Serialize)]
pub struct TransitionStatus {
    pub zone: TransitionZone,
    pub state: TransitionState,
}

#[derive(Serialize)]
pub struct EngineStatus {
    pub finished: bool,
    pub paused: bool,
    pub decks: Vec<DeckStatus>,
    pub transition: Option<TransitionStatus>,
}

pub fn zone_of(plan: &Plan, from: &str, to: &str, sr: u32) -> TransitionZone {
    let srf = sr as f64;
    let blend = (plan.a_exit - plan.b_enter).max(0.0);
    // Backspin/loop roll hold A's playhead in place (or run it backwards),
    // so on A's own timeline the zone is just the mix point.
    let a_end = match plan.kind {
        Kind::Backspin | Kind::LoopRoll => plan.start_a,
        _ => plan.start_a + plan.a_exit,
    };
    TransitionZone {
        from_track_id: from.to_string(),
        to_track_id: to.to_string(),
        a_start_secs: plan.start_a / srf,
        a_end_secs: a_end / srf,
        b_start_secs: plan.start_b / srf,
        b_end_secs: plan.b_native_at(blend) / srf,
        tempo_ratio: plan.speed,
        beatmatched: plan.beatmatched,
        bars: plan.a_exit / (4.0 * plan.beat),
    }
}

impl Engine {
    pub fn new(sr: u32, main: Deck) -> Self {
        Self { sr, main: Some(main), transition: None, finished: false, arm_seq: 0, master: Smooth::new(sr as f32, 0.05, 1.0) }
    }

    pub fn main_samples(&self) -> Option<(String, Arc<Vec<f32>>)> {
        self.main.as_ref().map(|d| (d.track_id.clone(), d.samples.clone()))
    }

    pub fn arm(&mut self, plan: Arc<Plan>, to_track_id: String, b_samples: Arc<Vec<f32>>) {
        let mut incoming = Deck::new(to_track_id.clone(), b_samples, self.sr, plan.start_b);
        incoming.trim = plan.trim_b;
        incoming.plan = Some(plan.clone());
        let echo = Echo::new((plan.beat * 2.0) as usize + 16);
        let mut tr = Transition { plan, to_track_id, incoming, state: TransitionState::Pending, t: 0.0, echo };
        tr.echo.set_delay(tr.plan.echo_delay());
        self.transition = Some(tr);
        self.arm_seq += 1;
        // Arming while already past the mix point (e.g. the user started
        // playback inside the zone) shouldn't jump-cut to B: mix from here
        // if inside the zone, otherwise wait until the user seeks back.
        self.sync(false);
    }

    pub fn disarm(&mut self) {
        self.transition = None;
        self.arm_seq += 1;
    }

    /// Reconciles transition state with the main deck's playhead.
    fn sync(&mut self, allow_promote: bool) {
        let Some(main) = &self.main else { return };
        let p = main.native_pos();
        let Some(tr) = &mut self.transition else { return };
        let t = p - tr.plan.start_a;
        tr.echo.clear();
        if t < 0.0 {
            tr.state = TransitionState::Pending;
            tr.t = 0.0;
            tr.incoming.set_entry_cursor(0.0);
        } else if t < tr.plan.a_exit {
            tr.state = TransitionState::Mixing;
            tr.t = t;
            tr.incoming.set_entry_cursor((t - tr.plan.b_enter).max(0.0));
        } else if allow_promote {
            tr.incoming.set_entry_cursor(t - tr.plan.b_enter);
            self.promote();
        } else {
            tr.state = TransitionState::Missed;
        }
    }

    fn promote(&mut self) {
        if let Some(tr) = self.transition.take() {
            self.main = Some(tr.incoming);
        }
    }

    pub fn seek(&mut self, track_id: Option<&str>, secs: f64) {
        let pos = secs * self.sr as f64;
        let on_incoming = matches!((&self.transition, track_id), (Some(tr), Some(id)) if tr.state == TransitionState::Mixing && tr.incoming.track_id == id && self.main.as_ref().is_some_and(|m| m.track_id != id));
        if on_incoming {
            // Scrubbing the incoming track mid-blend: finish the transition
            // right there and continue on B alone.
            let mut tr = self.transition.take().unwrap();
            tr.incoming.seek(pos);
            self.main = Some(tr.incoming);
        } else if let Some(main) = &mut self.main {
            main.seek(pos);
            self.sync(true);
        }
        self.finished = false;
    }

    pub fn status(&self, paused: bool) -> EngineStatus {
        let srf = self.sr as f64;
        let mut decks = Vec::new();
        if let Some(m) = &self.main {
            decks.push(DeckStatus {
                track_id: m.track_id.clone(),
                position_secs: m.native_pos() / srf,
                duration_secs: m.samples.len() as f64 / srf,
                role: "main",
            });
        }
        let transition = self.transition.as_ref().map(|tr| {
            if tr.state == TransitionState::Mixing && tr.t >= tr.plan.b_enter {
                decks.push(DeckStatus {
                    track_id: tr.incoming.track_id.clone(),
                    position_secs: tr.incoming.native_pos() / srf,
                    duration_secs: tr.incoming.samples.len() as f64 / srf,
                    role: "incoming",
                });
            }
            let from = self.main.as_ref().map(|m| m.track_id.as_str()).unwrap_or("");
            TransitionStatus { zone: zone_of(&tr.plan, from, &tr.to_track_id, self.sr), state: tr.state }
        });
        EngineStatus { finished: self.finished, paused, decks, transition }
    }

    #[cfg(test)]
    pub fn render_for_test(&mut self, out: &mut [f32]) {
        self.render(out);
    }

    fn render(&mut self, out: &mut [f32]) {
        for o in out.iter_mut() {
            *o = self.next_sample();
        }
    }

    #[inline]
    fn next_sample(&mut self) -> f32 {
        let Some(main) = &mut self.main else {
            self.finished = true;
            return 0.0;
        };

        // Trigger: main deck crossing the mix point.
        if let Some(tr) = &mut self.transition {
            match tr.state {
                TransitionState::Pending => {
                    let t = main.native_pos() - tr.plan.start_a;
                    if t >= 0.0 {
                        tr.state = TransitionState::Mixing;
                        tr.t = t;
                        tr.incoming.set_entry_cursor(0.0);
                    }
                }
                TransitionState::Missed => {
                    if main.native_pos() < tr.plan.start_a {
                        tr.state = TransitionState::Pending;
                    }
                }
                TransitionState::Mixing => {}
            }
        }

        let mixing = matches!(&self.transition, Some(tr) if tr.state == TransitionState::Mixing);
        if !mixing {
            if main.finished() {
                self.finished = true;
                return 0.0;
            }
            let x = main.next_sample();
            let g = self.master.next(1.0);
            return soft_limit(main.process(x, &DeckParams::UNITY).1 * g);
        }

        let tr = self.transition.as_mut().unwrap();
        let t = tr.t;
        let params = tr.plan.params(t);

        let a_raw = match tr.plan.a_read_override(t) {
            Some(pos) => {
                main.pos = pos;
                main.read_at(pos)
            }
            None => main.next_sample(),
        };
        let (a_pre, a_out) = main.process(a_raw, &params.a);
        let wet = tr.echo.process(a_pre * params.echo_send, 0.55);

        let b_out = if t >= tr.plan.b_enter {
            let x = tr.incoming.next_sample();
            tr.incoming.process(x, &params.b).1
        } else {
            0.0
        };

        // Two full-level masters summed are ~3 dB louder than either alone
        // and push the bus into the limiter (heard as a crunchy, "distorted"
        // middle of the blend). Normalise to roughly constant loudness, the
        // way auto-mix engines do.
        let e = channel_energy(&params.a) + channel_energy(&params.b) * tr.incoming.trim * tr.incoming.trim;
        let target = if e > 1.0 { (1.0 / e.sqrt()).max(0.6) } else { 1.0 };
        let g = self.master.next(target);

        tr.t += 1.0;
        let done = tr.t >= tr.plan.a_exit;
        if done {
            // Leave A's playhead wherever an effect left it: it's no longer
            // shown once B takes over.
            self.promote();
        }
        soft_limit((a_out + b_out + wet * params.echo_return) * g)
    }
}

const BLOCK: usize = 256;

/// The rodio-facing source: pulls blocks from the shared engine so the
/// lock is taken once per block, not once per sample.
struct EngineSource {
    engine: Arc<Mutex<Engine>>,
    sr: u32,
    buf: [f32; BLOCK],
    idx: usize,
    done: bool,
}

impl Iterator for EngineSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.idx >= BLOCK {
            if self.done {
                return None;
            }
            let mut e = self.engine.lock().unwrap();
            e.render(&mut self.buf);
            self.done = e.finished;
            self.idx = 0;
        }
        let s = self.buf[self.idx];
        self.idx += 1;
        Some(s)
    }
}

impl Source for EngineSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        1
    }
    fn sample_rate(&self) -> u32 {
        self.sr
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

enum PlayerCommand {
    Start(Arc<Mutex<Engine>>, u32),
    Stop,
}

/// Shared handle managed as Tauri state. Playback starts go through the
/// dedicated audio thread (which owns the output stream); everything else
/// — pause, seek, arming, status — talks to the sink/engine directly.
pub struct Player {
    tx: Sender<PlayerCommand>,
    sink: Arc<Mutex<Option<Sink>>>,
    engine: Mutex<Option<Arc<Mutex<Engine>>>>,
    /// Small decode cache so re-arming a transition (editing it while
    /// listening, toggling it on/off) doesn't re-decode the next track.
    pub decode_cache: Mutex<Vec<(String, u32, Arc<Vec<f32>>)>>,
}

impl Player {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<PlayerCommand>();
        let sink: Arc<Mutex<Option<Sink>>> = Arc::new(Mutex::new(None));
        let sink_thread = sink.clone();
        std::thread::spawn(move || {
            let (_stream, handle) = match OutputStream::try_default() {
                Ok(v) => v,
                Err(_) => return,
            };
            for cmd in rx {
                if let Some(s) = sink_thread.lock().unwrap().take() {
                    s.stop();
                }
                if let PlayerCommand::Start(engine, sr) = cmd {
                    if let Ok(new_sink) = Sink::try_new(&handle) {
                        new_sink.append(EngineSource { engine, sr, buf: [0.0; BLOCK], idx: BLOCK, done: false });
                        new_sink.play();
                        *sink_thread.lock().unwrap() = Some(new_sink);
                    }
                }
            }
        });
        Self { tx, sink, engine: Mutex::new(None), decode_cache: Mutex::new(Vec::new()) }
    }

    pub fn start(&self, engine: Engine) -> Result<(), String> {
        let sr = engine.sr;
        let engine = Arc::new(Mutex::new(engine));
        *self.engine.lock().unwrap() = Some(engine.clone());
        self.tx.send(PlayerCommand::Start(engine, sr)).map_err(|e| format!("could not start playback: {e}"))
    }

    pub fn stop(&self) -> Result<(), String> {
        *self.engine.lock().unwrap() = None;
        self.tx.send(PlayerCommand::Stop).map_err(|e| format!("could not stop playback: {e}"))
    }

    pub fn engine(&self) -> Option<Arc<Mutex<Engine>>> {
        self.engine.lock().unwrap().clone()
    }

    pub fn pause(&self) {
        if let Some(s) = self.sink.lock().unwrap().as_ref() {
            s.pause();
        }
    }

    pub fn resume(&self) {
        if let Some(s) = self.sink.lock().unwrap().as_ref() {
            s.play();
        }
    }

    pub fn is_paused(&self) -> bool {
        self.sink.lock().unwrap().as_ref().is_some_and(|s| s.is_paused())
    }

    pub fn cached_decode(&self, track_id: &str, sr: u32) -> Option<Arc<Vec<f32>>> {
        self.decode_cache.lock().unwrap().iter().find(|(id, r, _)| id == track_id && *r == sr).map(|(_, _, s)| s.clone())
    }

    pub fn cache_decode(&self, track_id: &str, sr: u32, samples: Arc<Vec<f32>>) {
        let mut c = self.decode_cache.lock().unwrap();
        c.retain(|(id, _, _)| id != track_id);
        c.insert(0, (track_id.to_string(), sr, samples));
        c.truncate(3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::mix::{self, Request, TrackInfo};

    fn tone(sr: u32, f: f32, secs: f64) -> Vec<f32> {
        (0..(secs * sr as f64) as usize).map(|i| (std::f32::consts::TAU * f * i as f32 / sr as f32).sin() * 0.3).collect()
    }

    #[test]
    fn plays_through_transition_and_promotes_incoming() {
        let sr = 22050;
        let a = Arc::new(tone(sr, 220.0, 40.0));
        let b = Arc::new(tone(sr, 330.0, 60.0));
        let req = Request { kind: Kind::Blend, crossfade_secs: Some(8.0), cue_out_secs: Some(20.0), cue_in_secs: Some(5.0) };
        let plan = mix::plan(TrackInfo { samples: &a, bpm: Some(120.0), key: None }, TrackInfo { samples: &b, bpm: Some(126.0), key: None }, sr, &req);
        let start_a = plan.start_a;
        let a_exit = plan.a_exit;
        let mut e = Engine::new(sr, Deck::new("a".into(), a.clone(), sr, 0.0));
        e.arm(Arc::new(plan), "b".into(), b.clone());
        e.seek(Some("a"), start_a / sr as f64 - 1.0);
        let mut buf = vec![0.0f32; sr as usize * 2];
        e.render(&mut buf);
        let st = e.status(false);
        assert_eq!(st.decks.len(), 2, "both decks visible mid-blend");
        // Run past the end of the blend.
        let mut rest = vec![0.0f32; (a_exit as usize) + sr as usize];
        e.render(&mut rest);
        let st = e.status(false);
        assert_eq!(st.decks.len(), 1);
        assert_eq!(st.decks[0].track_id, "b");
        assert!(st.transition.is_none());
        assert!(rest.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }

    #[test]
    fn seeking_back_before_zone_rearms() {
        let sr = 22050;
        let a = Arc::new(tone(sr, 220.0, 40.0));
        let b = Arc::new(tone(sr, 330.0, 60.0));
        let req = Request { kind: Kind::Cut, crossfade_secs: None, cue_out_secs: Some(20.0), cue_in_secs: None };
        let plan = mix::plan(TrackInfo { samples: &a, bpm: Some(120.0), key: None }, TrackInfo { samples: &b, bpm: Some(120.0), key: None }, sr, &req);
        let mut e = Engine::new(sr, Deck::new("a".into(), a, sr, 0.0));
        e.seek(Some("a"), 30.0);
        e.arm(Arc::new(plan), "b".into(), b);
        assert!(matches!(e.transition.as_ref().unwrap().state, TransitionState::Missed));
        e.seek(Some("a"), 10.0);
        assert!(matches!(e.transition.as_ref().unwrap().state, TransitionState::Pending));
    }
}
