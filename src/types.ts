export interface Track {
  id: string;
  file_path: string;
  title: string;
  artist: string | null;
  album: string | null;
  duration_secs: number | null;
  bpm: number | null;
  bpm_confidence: number | null;
  key_camelot: string | null;
  key_name: string | null;
  key_confidence: number | null;
  energy: number | null;
  artwork_path: string | null;
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

export interface SetlistEntry {
  id: string;
  setlist_id: string;
  track_id: string;
  position: number;
  transition_id: string | null;
}

export interface Transition {
  id: string;
  name: string;
  transition_type: string;
  crossfade_secs: number;
  cue_out_secs: number | null;
  cue_in_secs: number | null;
  notes: string | null;
  from_key: string | null;
  to_key: string | null;
  visibility: string;
  author: string | null;
  created_at: string;
}

export interface SetlistEntryWithTrack {
  entry: SetlistEntry;
  track: Track;
  transition: Transition | null;
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
