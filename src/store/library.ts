import { create } from "zustand";
import { api } from "../lib/api";
import type { Track } from "../types";
import { usePlayback } from "./playback";

export interface BatchProgress {
  total: number;
  done: number;
  currentTitle: string | null;
  failed: { title: string; error: string }[];
  cancelRequested: boolean;
}

interface LibraryState {
  tracks: Track[];
  loading: boolean;
  analyzingIds: Set<string>;
  batchProgress: BatchProgress | null;
  refresh: () => Promise<void>;
  importFiles: (paths: string[]) => Promise<void>;
  analyze: (trackId: string) => Promise<void>;
  /** Analyzes tracks that haven't been analyzed yet, or with `all: true` every track (re-analysis). */
  analyzeAll: (opts?: { all?: boolean }) => Promise<void>;
  cancelAnalyzeAll: () => void;
  dismissBatchProgress: () => void;
  removeTrack: (trackId: string) => Promise<void>;
  removeAllTracks: () => Promise<void>;
  resetAllAnalysis: () => Promise<void>;
}

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export const useLibrary = create<LibraryState>((set, get) => ({
  tracks: [],
  loading: false,
  analyzingIds: new Set(),
  batchProgress: null,

  refresh: async () => {
    set({ loading: true });
    try {
      const tracks = await api.listTracks();
      set({ tracks });
    } finally {
      set({ loading: false });
    }
  },

  importFiles: async (paths) => {
    if (paths.length === 0) return;
    set({ loading: true });
    try {
      await api.importTracks(paths);
      await get().refresh();
    } finally {
      set({ loading: false });
    }
  },

  analyze: async (trackId) => {
    const analyzing = new Set(get().analyzingIds);
    analyzing.add(trackId);
    set({ analyzingIds: analyzing });
    try {
      const updated = await api.analyzeTrack(trackId);
      set({ tracks: get().tracks.map((t) => (t.id === trackId ? updated : t)) });
    } finally {
      const next = new Set(get().analyzingIds);
      next.delete(trackId);
      set({ analyzingIds: next });
    }
  },

  // Analyzes one track at a time (never in parallel — keeps memory/CPU use
  // bounded and tractable regardless of library size) and keeps live
  // progress in state so the UI can show it. A single track's failure is
  // caught and recorded rather than stopping the batch, so one bad file
  // never silently halts — or takes down — analysis of the rest. A short
  // pause between tracks deliberately caps how much of the CPU this can
  // hog in a row, so scrolling/clicking elsewhere stays responsive while a
  // big batch runs in the background.
  analyzeAll: async (opts) => {
    const unanalyzed = opts?.all ? [...get().tracks] : get().tracks.filter((t) => t.bpm == null);
    if (unanalyzed.length === 0) return;

    set({
      batchProgress: { total: unanalyzed.length, done: 0, currentTitle: null, failed: [], cancelRequested: false },
    });

    for (const t of unanalyzed) {
      if (get().batchProgress?.cancelRequested) break;

      set((s) => (s.batchProgress ? { batchProgress: { ...s.batchProgress, currentTitle: t.title } } : {}));

      try {
        await get().analyze(t.id);
      } catch (e) {
        set((s) =>
          s.batchProgress
            ? { batchProgress: { ...s.batchProgress, failed: [...s.batchProgress.failed, { title: t.title, error: String(e) }] } }
            : {}
        );
      }

      set((s) => (s.batchProgress ? { batchProgress: { ...s.batchProgress, done: s.batchProgress.done + 1 } } : {}));
      await sleep(300);
    }
  },

  cancelAnalyzeAll: () => {
    set((s) => (s.batchProgress ? { batchProgress: { ...s.batchProgress, cancelRequested: true } } : {}));
  },

  dismissBatchProgress: () => set({ batchProgress: null }),

  removeTrack: async (trackId) => {
    // A deleted track can't keep playing, whether it's on a deck or queued
    // to be mixed in next — stop before it's gone instead of leaving a
    // dangling player.
    if (usePlayback.getState().involvesTrack(trackId)) {
      await usePlayback.getState().stop();
    }
    await api.deleteTrack(trackId);
    set({ tracks: get().tracks.filter((t) => t.id !== trackId) });
  },

  removeAllTracks: async () => {
    // Every track is about to be gone, so whatever's on a deck (or queued to
    // mix in next) can't keep playing, no matter which track it is.
    if (usePlayback.getState().decks.length > 0 || usePlayback.getState().transition) {
      await usePlayback.getState().stop();
    }
    await api.deleteAllTracks();
    set({ tracks: [] });
  },

  resetAllAnalysis: async () => {
    await api.resetAllAnalysis();
    await get().refresh();
  },
}));
