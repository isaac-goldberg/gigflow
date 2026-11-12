export interface Track {
  id: string;
  file_path: string;
  title: string;
  artist: string | null;
  album: string | null;
  genre: string | null;
  subgenre: string | null;
  duration_secs: number | null;
  bpm: number | null;
  bpm_confidence: number | null;
  key_camelot: string | null;
  key_name: string | null;
  key_confidence: number | null;
  energy: number | null;
  artwork_path: string | null;
  waveform_peaks: [number, number][] | null;
  date_added: string;
  date_analyzed: string | null;
}

export interface Setlist {
  id: string;
  name: string;
  gig_date: string | null;
  notes: string | null;
  created_at: string;
  updated_at: string;
}

// A transition's fields live inline on the entry, not as a separate saved/
// shared entity — its cue points only ever make sense for the specific pair
// of tracks on either side of this one gap.
export interface SetlistEntry {
  id: string;
  setlist_id: string;
  track_id: string;
  position: number;
  transition_type: string | null;
  crossfade_secs: number | null;
  cue_out_secs: number | null;
  cue_in_secs: number | null;
  transition_notes: string | null;
  /** Off = keep the transition's settings, but play the outgoing track to its end when listening. */
  transition_enabled: boolean;
}

export interface SetlistEntryWithTrack {
  entry: SetlistEntry;
  track: Track;
}

export interface TransitionSuggestion {
  transition_type: string;
  crossfade_secs: number;
  cue_out_secs: number | null;
  cue_in_secs: number | null;
  rationale: string;
}

export interface HotCue {
  id: string;
  track_id: string;
  pad: number;
  position_secs: number;
  label: string;
}

export interface TrackSuggestion {
  track: Track;
  score: number;
  bpm_delta: number | null;
  key_relation: string;
  energy_delta: number | null;
}

export interface SocialLink {
  platform: string;
  url: string;
}

export interface WorkLink {
  title: string;
  url: string;
}

export interface Profile {
  id: string;
  display_name: string;
  bio: string;
  socials: SocialLink[];
  work_links: WorkLink[];
  avatar_path: string | null;
}

export const TRANSITION_TYPES = ["crossfade", "cut", "echo out", "filter sweep", "backspin", "loop roll"] as const;

/** Where a transition happens on each track's own timeline (seconds). */
export interface TransitionZone {
  from_track_id: string;
  to_track_id: string;
  a_start_secs: number;
  a_end_secs: number;
  b_start_secs: number;
  b_end_secs: number;
  tempo_ratio: number;
  beatmatched: boolean;
  bars: number;
}

export interface DeckStatus {
  track_id: string;
  position_secs: number;
  duration_secs: number;
  role: "main" | "incoming";
}

export interface EngineStatus {
  finished: boolean;
  paused: boolean;
  decks: DeckStatus[];
  transition: { zone: TransitionZone; state: "pending" | "mixing" | "missed" } | null;
}

/** Everything the engine needs to arm one gap's transition. */
export interface TransitionSpec {
  fromTrackId: string;
  toTrackId: string;
  transitionType: string;
  crossfadeSecs: number | null;
  cueOutSecs: number | null;
  cueInSecs: number | null;
}
