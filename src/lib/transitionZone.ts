import type { SetlistEntry, Track, TransitionZone } from "../types";

/**
 * Estimates where a transition sits on each track's timeline, for drawing
 * the zone markers before anything is playing. Mirrors the engine's
 * planning (`audio/mix.rs`) using stored BPMs; once the transition is armed
 * for playback the engine's exact zone (fitted to the real beat grid)
 * replaces this.
 */
export function estimateZone(entry: SetlistEntry, a: Track, b: Track): TransitionZone | null {
  if (!entry.transition_type) return null;
  const type = entry.transition_type.toLowerCase();
  const beat = a.bpm && a.bpm > 0 ? 60 / a.bpm : 0.5;
  const durA = a.duration_secs ?? 0;

  let bEnter = 0;
  let aExit: number;
  switch (type) {
    case "cut":
      aExit = 0.012;
      break;
    case "echo out":
      bEnter = 4 * beat;
      aExit = 12 * beat;
      break;
    case "backspin":
      bEnter = aExit = 2 * beat;
      break;
    case "loop roll":
      bEnter = aExit = 4 * beat;
      break;
    default: {
      const bars = Math.min(32, Math.max(1, Math.round((entry.crossfade_secs ?? 8 * 4 * beat) / (4 * beat))));
      aExit = bars * 4 * beat;
    }
  }

  const holdsA = type === "backspin" || type === "loop roll";
  let start = entry.cue_out_secs ?? durA - aExit;
  const room = type === "backspin" ? 0 : aExit;
  while (start + room > durA && start > 4 * beat) start -= 4 * beat;
  start = Math.max(0, start);

  let ratio = 1;
  if (a.bpm && b.bpm && a.bpm > 0 && b.bpm > 0) {
    // B samples consumed per output sample, folding half/double time.
    const candidates = [0.5, 1, 2].map((k) => a.bpm! / (b.bpm! / k));
    ratio = candidates.reduce((best, r) => (Math.abs(Math.log(r)) < Math.abs(Math.log(best)) ? r : best));
    if (Math.abs(Math.log(ratio)) > Math.log(1.16)) ratio = 1;
  }
  const bStart = entry.cue_in_secs ?? 0;
  return {
    from_track_id: a.id,
    to_track_id: b.id,
    a_start_secs: start,
    a_end_secs: holdsA ? start : start + aExit,
    b_start_secs: bStart,
    b_end_secs: bStart + Math.max(0, aExit - bEnter) * ratio,
    tempo_ratio: ratio,
    beatmatched: ratio !== 1,
    bars: aExit / (4 * beat),
  };
}

/** Bars <-> seconds at a given BPM, for the transition length picker. */
export function barsToSecs(bars: number, bpm: number | null): number {
  return bpm && bpm > 0 ? bars * 4 * (60 / bpm) : bars * 2;
}

export function secsToBars(secs: number, bpm: number | null): number {
  return bpm && bpm > 0 ? secs / (4 * (60 / bpm)) : secs / 2;
}
