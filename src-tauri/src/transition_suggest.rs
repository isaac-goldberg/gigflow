/// Picks a structure-aware transition between two tracks: mix out of the
/// outgoing track's lower-energy section, into the incoming track's intro,
/// over roughly a phrase length — never a DROP on either side, so the two
/// tracks' drops never collide. See research notes in `suggest_transition`
/// (commands.rs) for why these specific labels, in this specific order.
use crate::models::HotCue;

pub const DEFAULT_CROSSFADE_SECS: f64 = 8.0;
/// 8 bars: long enough for a proper bass-swap blend, short enough that the
/// two-songs-at-once section is over quickly and the new track lands.
pub const RECOMMENDED_PHRASE_BARS: f64 = 8.0;
/// Clashing keys: keep the overlap short so the harmonies barely meet.
pub const CLASH_PHRASE_BARS: f64 = 4.0;

/// Where to mix OUT of the outgoing track: its OUTRO if it has one (the
/// cleanest option — producers build these specifically as a thinned-back
/// mix-out point), else its last BREAKDOWN, else its last TRANSITION point.
/// Never a DROP — fading out through a track's own peak kills its impact
/// and risks colliding with the incoming track's drop.
pub fn pick_cue_out(cues: &[HotCue]) -> Option<&HotCue> {
    cues.iter()
        .rev()
        .find(|c| c.label == "OUTRO")
        .or_else(|| cues.iter().rev().find(|c| c.label == "BREAKDOWN"))
        .or_else(|| cues.iter().rev().find(|c| c.label == "TRANSITION"))
}

/// Where to mix IN to the incoming track: its INTRO if it has one (the
/// simplified, kick-and-hats section producers build for exactly this),
/// else its earliest BUILD. Never a DROP — that should land solo, after
/// the outgoing track has already faded out.
pub fn pick_cue_in(cues: &[HotCue]) -> Option<&HotCue> {
    cues.iter().find(|c| c.label == "INTRO").or_else(|| cues.iter().find(|c| c.label == "BUILD"))
}

/// An 8-bar blend at `bpm_a` (4 bars when the keys clash), no longer than
/// the incoming track has left after its cue-in point (if known). The mix
/// point on the outgoing track is where the blend *starts*; if there isn't
/// room before the end of the track, playback pulls it back by whole bars
/// rather than shortening the blend (see `audio::mix::plan`).
pub fn recommended_crossfade_secs(
    bpm_a: Option<f64>,
    clash: bool,
    cue_in_secs: Option<f64>,
    track_b_duration_secs: Option<f64>,
) -> f64 {
    let bars = if clash { CLASH_PHRASE_BARS } else { RECOMMENDED_PHRASE_BARS };
    let mut secs = match bpm_a {
        Some(bpm) if bpm > 0.0 => bars * 4.0 * (60.0 / bpm),
        _ => DEFAULT_CROSSFADE_SECS,
    };
    if let (Some(in_secs), Some(dur)) = (cue_in_secs, track_b_duration_secs) {
        secs = secs.min((dur - in_secs).max(1.0));
    }
    secs.max(2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(label: &str, secs: f64) -> HotCue {
        HotCue { id: "x".into(), track_id: "t".into(), pad: 1, position_secs: secs, label: label.into() }
    }

    #[test]
    fn prefers_outro_over_breakdown_and_transition() {
        let cues = vec![cue("INTRO", 0.0), cue("BREAKDOWN", 60.0), cue("TRANSITION", 90.0), cue("OUTRO", 180.0)];
        assert_eq!(pick_cue_out(&cues).unwrap().label, "OUTRO");
    }

    #[test]
    fn falls_back_to_breakdown_when_no_outro() {
        let cues = vec![cue("INTRO", 0.0), cue("BREAKDOWN", 60.0), cue("TRANSITION", 90.0)];
        assert_eq!(pick_cue_out(&cues).unwrap().label, "BREAKDOWN");
    }

    #[test]
    fn falls_back_to_transition_when_no_outro_or_breakdown() {
        let cues = vec![cue("INTRO", 0.0), cue("TRANSITION", 90.0), cue("DROP", 100.0)];
        assert_eq!(pick_cue_out(&cues).unwrap().label, "TRANSITION");
    }

    #[test]
    fn never_picks_a_drop_for_mix_out() {
        let cues = vec![cue("INTRO", 0.0), cue("DROP", 60.0), cue("DROP", 180.0)];
        assert!(pick_cue_out(&cues).is_none(), "no safe mix-out point should mean no suggestion, not a drop");
    }

    #[test]
    fn prefers_intro_over_build_for_mix_in() {
        let cues = vec![cue("INTRO", 0.0), cue("BUILD", 20.0), cue("DROP", 40.0)];
        assert_eq!(pick_cue_in(&cues).unwrap().label, "INTRO");
    }

    #[test]
    fn falls_back_to_build_when_no_intro() {
        let cues = vec![cue("BUILD", 20.0), cue("DROP", 40.0)];
        assert_eq!(pick_cue_in(&cues).unwrap().label, "BUILD");
    }

    #[test]
    fn never_picks_a_drop_for_mix_in() {
        let cues = vec![cue("DROP", 20.0), cue("DROP", 40.0)];
        assert!(pick_cue_in(&cues).is_none());
    }

    #[test]
    fn crossfade_targets_an_8_bar_phrase_at_128bpm() {
        // 8 bars * 4 beats * (60/128) = 15s exactly.
        let secs = recommended_crossfade_secs(Some(128.0), false, None, None);
        assert!((secs - 15.0).abs() < 1e-6, "expected 15s, got {secs}");
    }

    #[test]
    fn clashing_keys_get_a_shorter_blend() {
        let secs = recommended_crossfade_secs(Some(128.0), true, None, None);
        assert!((secs - 7.5).abs() < 1e-6, "expected 4 bars (7.5s), got {secs}");
    }

    #[test]
    fn crossfade_is_clamped_to_available_runway_after_cue_in() {
        // Track B is 15s long and cue-in is at 10s — only 5s left to play with.
        let secs = recommended_crossfade_secs(Some(128.0), false, Some(10.0), Some(15.0));
        assert!(secs <= 5.0 + 1e-6, "expected clamped to ~5s, got {secs}");
    }

    #[test]
    fn falls_back_to_a_flat_default_with_no_bpm() {
        let secs = recommended_crossfade_secs(None, false, None, None);
        assert_eq!(secs, DEFAULT_CROSSFADE_SECS);
    }
}
