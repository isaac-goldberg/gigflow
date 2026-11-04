import { create } from "zustand";
import { api } from "../lib/api";
import type { Track } from "../types";

interface LibraryState {
  tracks: Track[];
  loading: boolean;
  analyzingIds: Set<string>;
  refresh: () => Promise<void>;
  importFiles: (paths: string[]) => Promise<void>;
  analyze: (trackId: string) => Promise<void>;
  analyzeAll: () => Promise<void>;
  removeTrack: (trackId: string) => Promise<void>;
}

export const useLibrary = create<LibraryState>((set, get) => ({
  tracks: [],
  loading: false,
  analyzingIds: new Set(),

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

  analyzeAll: async () => {
    const unanalyzed = get().tracks.filter((t) => t.bpm == null);
    for (const t of unanalyzed) {
      await get().analyze(t.id);
    }
  },

  removeTrack: async (trackId) => {
    await api.deleteTrack(trackId);
    set({ tracks: get().tracks.filter((t) => t.id !== trackId) });
  },
}));
