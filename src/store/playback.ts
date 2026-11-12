import { create } from "zustand";
import { api } from "../lib/api";
import type { DeckStatus, EngineStatus, TransitionSpec } from "../types";

/**
 * Playback is a live two-deck mix (see `audio_player.rs`): one main deck,
 * plus an incoming deck while a transition is blending. Every UI spot reads
 * "is my track playing, and where" from `decks`, so there's one source of
 * truth for both tracks' playheads during a transition.
 *
 * When playback was started from a setlist, `mixContext` holds that
 * setlist's enabled transitions keyed by outgoing track. After every status
 * poll the store makes sure the transition out of whatever is on the main
 * deck is armed (or disarmed) — so a set plays continuously, track into
 * track, and edits/toggles take effect while you listen.
 */
export interface MixContext {
  setlistId: string;
  specs: Record<string, TransitionSpec>;
}

interface PlaybackState {
  decks: DeckStatus[];
  transition: EngineStatus["transition"];
  paused: boolean;
  /** Track being decoded to start playback. */
  loadingTrackId: string | null;
  mixContext: MixContext | null;
  /** True while the next track is being prepared (decode + beat grid + stretch). */
  arming: boolean;

  play: (trackId: string, opts?: { mixContext?: MixContext | null }) => Promise<void>;
  /** Plays trackId (if not already on a deck) and seeks to secs — for clicking a hot cue. */
  playAt: (trackId: string, secs: number, opts?: { mixContext?: MixContext | null }) => Promise<void>;
  togglePause: () => Promise<void>;
  seek: (trackId: string, secs: number) => Promise<void>;
  stop: () => Promise<void>;
  /** Refreshes the transitions of the setlist currently being listened to (no-op for any other setlist). */
  updateMixContext: (ctx: MixContext) => void;
  deckFor: (trackId: string) => DeckStatus | undefined;
  /** Is this track on a deck or about to be mixed in — for stopping playback when it's removed/deleted. */
  involvesTrack: (trackId: string) => boolean;
}

let pollHandle: ReturnType<typeof setInterval> | null = null;
let armedKey: string | null = null;
let pollInFlight = false;

function stopPolling() {
  if (pollHandle) {
    clearInterval(pollHandle);
    pollHandle = null;
  }
}

const IDLE = { decks: [] as DeckStatus[], transition: null, paused: false, loadingTrackId: null, arming: false };

export const usePlayback = create<PlaybackState>((set, get) => {
  async function reconcileTransition(status: EngineStatus) {
    const main = status.decks.find((d) => d.role === "main");
    const ctx = get().mixContext;
    const desired = main && ctx ? ctx.specs[main.track_id] ?? null : null;
    const desiredKey = desired ? JSON.stringify(desired) : null;
    if (desiredKey === armedKey || get().arming) return;
    armedKey = desiredKey;
    if (!desired) {
      if (status.transition) await api.disarmTransition();
      return;
    }
    set({ arming: true });
    try {
      await api.armTransition(desired);
    } catch (e) {
      console.error("could not prepare transition", e);
    } finally {
      set({ arming: false });
    }
  }

  async function poll() {
    if (pollInFlight) return;
    pollInFlight = true;
    try {
      const status = await api.getPlaybackStatus();
      if (!status || status.finished || status.decks.length === 0) {
        stopPolling();
        armedKey = null;
        set({ ...IDLE });
        return;
      }
      set({ decks: status.decks, transition: status.transition, paused: status.paused });
      reconcileTransition(status);
    } finally {
      pollInFlight = false;
    }
  }

  function startPolling() {
    stopPolling();
    pollHandle = setInterval(poll, 100);
    poll();
  }

  return {
    ...IDLE,
    mixContext: null,

    play: async (trackId, opts) => {
      stopPolling();
      armedKey = null;
      set({ ...IDLE, loadingTrackId: trackId, mixContext: opts?.mixContext ?? null });
      try {
        const duration = await api.playTrack(trackId);
        set({
          loadingTrackId: null,
          decks: [{ track_id: trackId, position_secs: 0, duration_secs: duration, role: "main" }],
        });
        startPolling();
      } catch (e) {
        set({ loadingTrackId: null });
        throw e;
      }
    },

    playAt: async (trackId, secs, opts) => {
      if (!get().deckFor(trackId)) {
        await get().play(trackId, opts);
      }
      await get().seek(trackId, secs);
    },

    togglePause: async () => {
      const { paused } = get();
      if (paused) {
        await api.resumeTrack();
      } else {
        await api.pauseTrack();
      }
      set({ paused: !paused });
    },

    seek: async (trackId, secs) => {
      await api.seekTrack(trackId, secs);
      set({ decks: get().decks.map((d) => (d.track_id === trackId ? { ...d, position_secs: secs } : d)) });
      poll();
    },

    stop: async () => {
      stopPolling();
      armedKey = null;
      await api.stopTrack();
      set({ ...IDLE, mixContext: null });
    },

    updateMixContext: (ctx) => {
      if (get().mixContext?.setlistId === ctx.setlistId) set({ mixContext: ctx });
    },

    deckFor: (trackId) => get().decks.find((d) => d.track_id === trackId),

    involvesTrack: (trackId) => {
      const { decks, transition, loadingTrackId } = get();
      return (
        loadingTrackId === trackId ||
        decks.some((d) => d.track_id === trackId) ||
        transition?.zone.to_track_id === trackId
      );
    },
  };
});

/** The deck for `trackId`, as a hook (re-renders only that row on poll). */
export function useDeck(trackId: string): DeckStatus | undefined {
  return usePlayback((s) => s.decks.find((d) => d.track_id === trackId));
}
