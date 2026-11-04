import { invoke } from "@tauri-apps/api/core";
import type {
  HotCue,
  Profile,
  SetlistEntry,
  SetlistEntryWithTrack,
  Setlist,
  Track,
  TrackSuggestion,
  Transition,
} from "../types";

export const api = {
  importTracks: (paths: string[]) => invoke<Track[]>("import_tracks", { paths }),
  analyzeTrack: (trackId: string) => invoke<Track>("analyze_track", { trackId }),
  listTracks: () => invoke<Track[]>("list_tracks"),
  updateTrack: (args: {
    trackId: string;
    bpm?: number | null;
    keyCamelot?: string | null;
    keyName?: string | null;
    title?: string | null;
    artist?: string | null;
  }) => invoke<Track>("update_track", args),
  deleteTrack: (trackId: string) => invoke<void>("delete_track", { trackId }),

  getHotCues: (trackId: string) => invoke<HotCue[]>("get_hot_cues", { trackId }),
  deleteHotCue: (cueId: string) => invoke<void>("delete_hot_cue", { cueId }),

  suggestNextTracks: (currentTrackId: string, excludeIds: string[]) =>
    invoke<TrackSuggestion[]>("suggest_next_tracks", { currentTrackId, excludeIds }),

  createSetlist: (name: string, gigDate?: string | null, notes?: string | null) =>
    invoke<Setlist>("create_setlist", { name, gigDate: gigDate ?? null, notes: notes ?? null }),
  listSetlists: () => invoke<Setlist[]>("list_setlists"),
  deleteSetlist: (setlistId: string) => invoke<void>("delete_setlist", { setlistId }),
  getSetlistEntries: (setlistId: string) =>
    invoke<SetlistEntryWithTrack[]>("get_setlist_entries", { setlistId }),
  addTrackToSetlist: (setlistId: string, trackId: string) =>
    invoke<SetlistEntry>("add_track_to_setlist", { setlistId, trackId }),
  removeSetlistEntry: (entryId: string) => invoke<void>("remove_setlist_entry", { entryId }),
  reorderSetlist: (setlistId: string, orderedEntryIds: string[]) =>
    invoke<void>("reorder_setlist", { setlistId, orderedEntryIds }),
  assignTransition: (entryId: string, transitionId: string | null) =>
    invoke<void>("assign_transition", { entryId, transitionId }),

  saveTransition: (args: {
    name: string;
    transitionType: string;
    crossfadeSecs: number;
    cueOutSecs?: number | null;
    cueInSecs?: number | null;
    notes?: string | null;
    fromKey?: string | null;
    toKey?: string | null;
  }) => invoke<Transition>("save_transition", args),
  listTransitions: () => invoke<Transition[]>("list_transitions"),
  deleteTransition: (transitionId: string) => invoke<void>("delete_transition", { transitionId }),

  getProfile: () => invoke<Profile>("get_profile"),
  updateProfile: (profile: Profile) => invoke<Profile>("update_profile", { profile }),

  exportSetlist: (setlistId: string, destFolder: string) =>
    invoke<string>("export_setlist", { setlistId, destFolder }),
};

export function formatDuration(secs: number | null): string {
  if (secs == null || Number.isNaN(secs)) return "--:--";
  const m = Math.floor(secs / 60);
  const s = Math.round(secs % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}
