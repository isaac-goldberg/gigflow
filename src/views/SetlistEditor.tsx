import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  DndContext,
  closestCenter,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  verticalListSortingStrategy,
  useSortable,
  arrayMove,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { api, formatDuration } from "../lib/api";
import { camelotRelation } from "../lib/camelot";
import { useEscapeKey } from "../lib/useEscapeKey";
import { usePlayback, useDeck, type MixContext } from "../store/playback";
import { estimateZone } from "../lib/transitionZone";
import { KeyBadge, BpmBadge } from "../components/KeyBadge";
import { EnergyGauge } from "../components/EnergyGauge";
import { HotCueStrip, type StripZone } from "../components/HotCueStrip";
import { TrackPicker } from "../components/TrackPicker";
import { TransitionPicker } from "../components/TransitionPicker";
import type { HotCue, Setlist, SetlistEntryWithTrack, TrackSuggestion, TransitionSpec, TransitionZone } from "../types";

export function SetlistEditor({ setlistId, onBack }: { setlistId: string; onBack: () => void }) {
  const [setlist, setSetlist] = useState<Setlist | null>(null);
  const [entries, setEntries] = useState<SetlistEntryWithTrack[]>([]);
  const [cuesByTrack, setCuesByTrack] = useState<Record<string, HotCue[]>>({});
  const [showPicker, setShowPicker] = useState(false);
  const [transitionTarget, setTransitionTarget] = useState<{
    entryId: string;
    trackAId: string;
    trackBId: string;
    trackATitle: string;
    trackBTitle: string;
  } | null>(null);
  const [suggestFor, setSuggestFor] = useState<string | null>(null);
  const [suggestions, setSuggestions] = useState<TrackSuggestion[]>([]);
  const [exportMsg, setExportMsg] = useState<string | null>(null);

  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 4 } }));

  async function load() {
    const lists = await api.listSetlists();
    setSetlist(lists.find((s) => s.id === setlistId) ?? null);
    const loadedEntries = await api.getSetlistEntries(setlistId);
    setEntries(loadedEntries);
    const cuePairs = await Promise.all(
      loadedEntries.map(async (e) => [e.track.id, await api.getHotCues(e.track.id)] as const)
    );
    setCuesByTrack(Object.fromEntries(cuePairs));
  }

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [setlistId]);

  async function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const oldIndex = entries.findIndex((e) => e.entry.id === active.id);
    const newIndex = entries.findIndex((e) => e.entry.id === over.id);
    const reordered = arrayMove(entries, oldIndex, newIndex);
    setEntries(reordered);
    await api.reorderSetlist(setlistId, reordered.map((e) => e.entry.id));
  }

  async function addTracks(trackIds: string[]) {
    // Sequential, not parallel — each insert reads "current max position + 1"
    // at call time, so adding out of order (or concurrently) could scramble
    // the order the user picked them in.
    for (const trackId of trackIds) {
      await api.addTrackToSetlist(setlistId, trackId);
    }
    setShowPicker(false);
    await load();
  }

  async function removeEntry(entryId: string) {
    const idx = entries.findIndex((e) => e.entry.id === entryId);
    if (idx === -1) return;
    const removedTrackId = entries[idx].track.id;

    // The entry *before* this one may have a transition blending into this
    // track — that transition's cue points were computed for this specific
    // track, so once it's gone the transition no longer means anything.
    const prev = idx > 0 ? entries[idx - 1] : null;
    if (prev && prev.entry.transition_type != null) {
      await api.setEntryTransition({
        entryId: prev.entry.id,
        transitionType: null,
        crossfadeSecs: null,
        cueOutSecs: null,
        cueInSecs: null,
        notes: null,
      });
    }

    // A track that's on a deck right now, or is about to be mixed in, can't
    // keep playing once it's removed.
    if (usePlayback.getState().involvesTrack(removedTrackId)) {
      await usePlayback.getState().stop();
    }

    await api.removeSetlistEntry(entryId);
    await load();
  }

  async function openSuggestions(entryId: string) {
    const entry = entries.find((e) => e.entry.id === entryId);
    if (!entry) return;
    setSuggestFor(entryId);
    const excludeIds = entries.map((e) => e.track.id);
    const results = await api.suggestNextTracks(entry.track.id, excludeIds);
    setSuggestions(results);
  }

  async function addSuggested(trackId: string) {
    await addTracks([trackId]);
    setSuggestFor(null);
    setSuggestions([]);
  }

  async function handleExport() {
    const dest = await open({ directory: true, multiple: false, title: "Choose USB drive or export folder" });
    if (!dest || Array.isArray(dest)) return;
    try {
      const folder = await api.exportSetlist(setlistId, dest);
      setExportMsg(
        `Exported to ${folder}. In Rekordbox, import "rekordbox.xml" (not playlist.m3u8) and use "Import To Collection" to bring in hot cues and transition markers.`
      );
    } catch (e) {
      setExportMsg(`Export failed: ${e}`);
    }
  }

  const trackIds = entries.map((e) => e.track.id);

  // Every enabled transition in this set, keyed by outgoing track: playing
  // any track here plays the set continuously through them.
  const mixContext: MixContext = useMemo(() => {
    const specs: Record<string, TransitionSpec> = {};
    entries.forEach((e, i) => {
      const next = entries[i + 1];
      if (!next || !e.entry.transition_type || !e.entry.transition_enabled) return;
      specs[e.track.id] = {
        fromTrackId: e.track.id,
        toTrackId: next.track.id,
        transitionType: e.entry.transition_type,
        crossfadeSecs: e.entry.crossfade_secs,
        cueOutSecs: e.entry.cue_out_secs,
        cueInSecs: e.entry.cue_in_secs,
      };
    });
    return { setlistId, specs };
  }, [entries, setlistId]);

  useEffect(() => {
    usePlayback.getState().updateMixContext(mixContext);
  }, [mixContext]);

  // Zones drawn on each row: the engine's exact zone for the transition
  // that's armed right now, otherwise an estimate from the stored settings.
  const liveTransition = usePlayback((s) => s.transition);
  const zonesFor = (i: number): StripZone[] => {
    const zones: StripZone[] = [];
    const pick = (from: SetlistEntryWithTrack, to: SetlistEntryWithTrack): { zone: TransitionZone; exact: boolean; live: boolean } | null => {
      if (!from.entry.transition_type || !from.entry.transition_enabled) return null;
      const lz = liveTransition?.zone;
      if (lz && lz.from_track_id === from.track.id && lz.to_track_id === to.track.id) {
        return { zone: lz, exact: true, live: liveTransition!.state === "mixing" };
      }
      const est = estimateZone(from.entry, from.track, to.track);
      return est ? { zone: est, exact: false, live: false } : null;
    };
    const next = entries[i + 1];
    if (next) {
      const z = pick(entries[i], next);
      if (z) zones.push({ kind: "out", start: z.zone.a_start_secs, end: z.zone.a_end_secs, label: "Mix out", detail: `Mixing out into ${next.track.title}`, exact: z.exact, live: z.live });
    }
    const prev = entries[i - 1];
    if (prev) {
      const z = pick(prev, entries[i]);
      if (z) zones.push({ kind: "in", start: z.zone.b_start_secs, end: z.zone.b_end_secs, label: "Mix in", detail: `Mixed in from ${prev.track.title}`, exact: z.exact, live: z.live });
    }
    return zones;
  };

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between h-14 px-5 border-b border-gig-border bg-gig-panel">
        <div>
          <button onClick={onBack} className="text-[11px] font-bold uppercase tracking-wide text-gig-muted hover:text-gig-accent">
            ← All setlists
          </button>
        </div>
        <div className="flex-1 px-4">
          <h1 className="text-sm font-bold uppercase tracking-widest text-gig-text">{setlist?.name ?? "Setlist"}</h1>
        </div>
        <span className="label-xs mr-4">
          {entries.length} track{entries.length === 1 ? "" : "s"}
          {" · "}
          {formatDuration(entries.reduce((sum, e) => sum + (e.track.duration_secs ?? 0), 0))} total
        </span>
        <div className="flex gap-2">
          <button
            data-tour="add-track-button"
            onClick={() => setShowPicker(true)}
            className="px-3 py-1.5 rounded-sm text-xs font-bold uppercase tracking-wide bg-gig-panel3 border border-gig-borderLight hover:border-gig-accent2"
          >
            + Add track
          </button>
          <button
            data-tour="export-button"
            onClick={handleExport}
            disabled={entries.length === 0}
            className="px-3 py-1.5 rounded-sm text-xs font-bold uppercase tracking-wide bg-gig-accent text-black hover:bg-gig-accent/80 disabled:opacity-30"
          >
            Export…
          </button>
        </div>
      </div>

      {exportMsg && (
        <div className="px-5 py-2 text-xs font-mono bg-gig-accent2/10 text-gig-accent2 border-b border-gig-border flex items-start justify-between gap-3">
          <span className="flex-1">{exportMsg}</span>
          <button onClick={() => setExportMsg(null)} className="shrink-0 text-gig-muted hover:text-gig-text">
            ✕
          </button>
        </div>
      )}

      <div className="flex-1 overflow-y-auto px-5 py-5">
        {entries.length === 0 ? (
          <div className="text-gig-muted text-sm py-20 text-center border border-dashed border-gig-border uppercase tracking-wide">
            Empty setlist. Add tracks from your library to start building the set.
          </div>
        ) : (
          <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
            <SortableContext items={entries.map((e) => e.entry.id)} strategy={verticalListSortingStrategy}>
              <div className="space-y-0.5">
                {entries.map((e, i) => (
                  <div key={e.entry.id}>
                    <SortableRow
                      entry={e}
                      index={i}
                      cues={cuesByTrack[e.track.id] ?? []}
                      zones={zonesFor(i)}
                      mixContext={mixContext}
                      onRemove={() => removeEntry(e.entry.id)}
                      onSuggest={() => openSuggestions(e.entry.id)}
                    />
                    {i < entries.length - 1 && (
                      <TransitionGap
                        fromKey={e.track.key_camelot}
                        toKey={entries[i + 1].track.key_camelot}
                        fromBpm={e.track.bpm}
                        toBpm={entries[i + 1].track.bpm}
                        entry={e.entry}
                        trackAId={e.track.id}
                        trackBId={entries[i + 1].track.id}
                        onToggled={load}
                        onClick={() =>
                          setTransitionTarget({
                            entryId: e.entry.id,
                            trackAId: e.track.id,
                            trackBId: entries[i + 1].track.id,
                            trackATitle: e.track.title,
                            trackBTitle: entries[i + 1].track.title,
                          })
                        }
                      />
                    )}
                  </div>
                ))}
              </div>
            </SortableContext>
          </DndContext>
        )}
      </div>

      {showPicker && (
        <TrackPicker excludeIds={trackIds} onPick={addTracks} onClose={() => setShowPicker(false)} />
      )}

      {transitionTarget &&
        (() => {
          const targetEntry = entries.find((e) => e.entry.id === transitionTarget.entryId)?.entry;
          if (!targetEntry) return null;
          return (
            <TransitionPicker
              entryId={transitionTarget.entryId}
              trackAId={transitionTarget.trackAId}
              trackBId={transitionTarget.trackBId}
              trackATitle={transitionTarget.trackATitle}
              trackBTitle={transitionTarget.trackBTitle}
              trackABpm={entries.find((e) => e.track.id === transitionTarget.trackAId)?.track.bpm ?? null}
              initial={{
                transitionType: targetEntry.transition_type,
                crossfadeSecs: targetEntry.crossfade_secs,
                cueOutSecs: targetEntry.cue_out_secs,
                cueInSecs: targetEntry.cue_in_secs,
                transitionNotes: targetEntry.transition_notes,
              }}
              onSaved={() => {
                setTransitionTarget(null);
                load();
              }}
              onClose={() => setTransitionTarget(null)}
            />
          );
        })()}

      {suggestFor && (
        <SuggestionsPanel
          suggestions={suggestions}
          onPick={addSuggested}
          onClose={() => {
            setSuggestFor(null);
            setSuggestions([]);
          }}
        />
      )}
    </div>
  );
}

function SortableRow({
  entry,
  index,
  cues,
  zones,
  mixContext,
  onRemove,
  onSuggest,
}: {
  entry: SetlistEntryWithTrack;
  index: number;
  cues: HotCue[];
  zones: StripZone[];
  mixContext: MixContext;
  onRemove: () => void;
  onSuggest: () => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: entry.entry.id,
  });
  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : 1,
  };

  const deck = useDeck(entry.track.id);
  const paused = usePlayback((s) => s.paused);
  const loading = usePlayback((s) => s.loadingTrackId === entry.track.id);
  const play = usePlayback((s) => s.play);
  const playAt = usePlayback((s) => s.playAt);
  const togglePause = usePlayback((s) => s.togglePause);
  const seek = usePlayback((s) => s.seek);
  const isPlayingThis = deck != null;

  return (
    <div
      ref={setNodeRef}
      style={style}
      className="bg-gig-panel border border-gig-border rounded-sm px-3 py-2"
    >
      <div className="flex items-center gap-3">
        <button {...attributes} {...listeners} className="text-gig-dim cursor-grab px-1" title="Drag to reorder">
          ⠿
        </button>
        <span className="text-[11px] font-mono text-gig-dim w-5 text-center">{index + 1}</span>
        <button
          onClick={() => (isPlayingThis ? togglePause() : play(entry.track.id, { mixContext }))}
          className="w-6 h-6 shrink-0 flex items-center justify-center rounded-full border border-gig-borderLight text-gig-text hover:border-gig-accent hover:text-gig-accent text-[11px]"
          title={isPlayingThis && !paused ? "Pause" : "Play from here — the set mixes into the next track through its transition"}
        >
          {loading ? (
            <span className="w-2.5 h-2.5 rounded-full border-[1.5px] border-gig-text/30 border-t-gig-text animate-spin" />
          ) : isPlayingThis && !paused ? (
            "❚❚"
          ) : (
            "▶"
          )}
        </button>
        <div className="flex-1 min-w-0">
          <div className="text-sm font-bold truncate">{entry.track.title}</div>
          <div className="text-[11px] text-gig-muted truncate">{entry.track.artist ?? "—"}</div>
        </div>
        <span className="text-[11px] font-mono text-gig-muted">{formatDuration(entry.track.duration_secs)}</span>
        <BpmBadge bpm={entry.track.bpm} />
        <KeyBadge camelot={entry.track.key_camelot} keyName={entry.track.key_name} />
        <EnergyGauge energy={entry.track.energy} size={30} />
        <button
          data-tour="suggest-next-button"
          onClick={onSuggest}
          className="text-[11px] font-bold uppercase px-2 py-1 rounded-sm bg-gig-accent2/15 text-gig-accent2 hover:bg-gig-accent2/25"
          title="Suggest a track to follow this one"
        >
          Suggest next
        </button>
        <button onClick={onRemove} className="text-[11px] font-bold px-2 py-1 rounded-sm text-gig-muted hover:text-gig-warn">
          ✕
        </button>
      </div>
      {(cues.length > 0 || isPlayingThis || entry.track.waveform_peaks || zones.length > 0) && (
        <div className="pl-9 pt-1.5" onClick={(e) => e.stopPropagation()}>
          <HotCueStrip
            duration={entry.track.duration_secs}
            cues={cues}
            waveform={entry.track.waveform_peaks}
            height={isPlayingThis ? 36 : 28}
            interactive={isPlayingThis}
            position={deck?.position_secs ?? 0}
            onSeek={isPlayingThis ? (secs) => seek(entry.track.id, secs) : undefined}
            onCueClick={(cue) => playAt(entry.track.id, cue.position_secs, { mixContext })}
            zones={zones}
          />
        </div>
      )}
    </div>
  );
}

function TransitionGap({
  fromKey,
  toKey,
  fromBpm,
  toBpm,
  entry,
  trackAId,
  trackBId,
  onToggled,
  onClick,
}: {
  fromKey: string | null;
  toKey: string | null;
  fromBpm: number | null;
  toBpm: number | null;
  entry: SetlistEntryWithTrack["entry"];
  trackAId: string;
  trackBId: string;
  onToggled: () => void;
  onClick: () => void;
}) {
  const relation = camelotRelation(fromKey, toKey);
  const bpmDelta = fromBpm != null && toBpm != null ? toBpm - fromBpm : null;
  const relationColor =
    relation === "same key" || relation === "relative major/minor"
      ? "text-gig-accent"
      : relation === "adjacent (perfect mix)"
      ? "text-gig-accent3"
      : relation === "clash"
      ? "text-gig-warn"
      : "text-gig-dim";

  // Live status of this gap while the set is playing.
  const status = usePlayback((s) => {
    const z = s.transition?.zone;
    if (z && z.from_track_id === trackAId && z.to_track_id === trackBId) return s.transition!.state;
    const main = s.decks.find((d) => d.role === "main");
    if (s.arming && main?.track_id === trackAId) return "preparing";
    return null;
  });
  const [toggling, setToggling] = useState(false);

  async function toggleEnabled(enabled: boolean) {
    setToggling(true);
    try {
      await api.setEntryTransitionEnabled(entry.id, enabled);
      onToggled();
    } finally {
      setToggling(false);
    }
  }

  return (
    <div className="w-full flex items-center gap-2 pl-9 py-1.5 text-[11px] text-gig-dim group">
      <span className="w-px h-4 bg-gig-border" />
      <button data-tour="assign-transition-button" onClick={onClick} className="hover:text-gig-text">
        {entry.transition_type ? (
          <span
            className={`px-2 py-0.5 rounded-sm uppercase tracking-wide ${
              entry.transition_enabled ? "bg-gig-accent2/15 text-gig-accent2" : "bg-gig-panel3 text-gig-dim line-through"
            }`}
          >
            {entry.transition_type}
          </span>
        ) : (
          <span className="px-2 py-0.5 rounded-sm border border-dashed border-gig-border uppercase tracking-wide group-hover:border-gig-accent2">
            + assign transition
          </span>
        )}
      </button>
      {entry.transition_type && (
        <label
          className="flex items-center gap-1 uppercase tracking-wide cursor-pointer select-none hover:text-gig-text"
          title="When off, the first track plays to its end without mixing — the transition's settings are kept"
        >
          <input
            type="checkbox"
            checked={entry.transition_enabled}
            disabled={toggling}
            onChange={(e) => toggleEnabled(e.target.checked)}
            className="accent-[#ff2ea6] w-3 h-3"
          />
          Enabled
        </label>
      )}
      {status === "preparing" && (
        <span className="flex items-center gap-1 text-gig-accent3 uppercase tracking-wide">
          <span className="w-2.5 h-2.5 rounded-full border-[1.5px] border-gig-accent3/30 border-t-gig-accent3 animate-spin" />
          Preparing mix…
        </span>
      )}
      {status === "pending" && <span className="text-gig-accent3 uppercase tracking-wide">● Ready</span>}
      {status === "mixing" && <span className="text-gig-accent2 uppercase tracking-wide animate-pulse">● Mixing</span>}
      <span className={`${relationColor} uppercase tracking-wide`}>{relation}</span>
      {bpmDelta != null && (
        <span className="text-gig-muted font-mono">
          {bpmDelta > 0 ? "+" : ""}
          {bpmDelta.toFixed(1)} BPM
        </span>
      )}
    </div>
  );
}

function SuggestionsPanel({
  suggestions,
  onPick,
  onClose,
}: {
  suggestions: TrackSuggestion[];
  onPick: (trackId: string) => void;
  onClose: () => void;
}) {
  useEscapeKey(onClose);
  return (
    <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50" onClick={onClose}>
      <div
        className="bg-gig-panel border border-gig-borderLight rounded-sm w-full max-w-lg max-h-[70vh] flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="p-3 border-b border-gig-border bg-gig-panel2">
          <h3 className="text-xs font-bold uppercase tracking-wider">Suggested next tracks</h3>
          <p className="text-[11px] text-gig-muted mt-0.5">Ranked by BPM, key (Camelot) and energy flow</p>
        </div>
        <div className="flex-1 overflow-y-auto p-1">
          {suggestions.length === 0 ? (
            <div className="text-gig-muted text-sm text-center py-10 uppercase tracking-wide">
              No candidates — analyze more tracks in your library.
            </div>
          ) : (
            suggestions.map((s) => (
              <button
                key={s.track.id}
                onClick={() => onPick(s.track.id)}
                className="w-full flex items-center gap-3 px-3 py-2 hover:bg-gig-panel2 text-left border-b border-gig-border/40"
              >
                <div className="flex-1 min-w-0">
                  <div className="text-sm font-bold truncate">{s.track.title}</div>
                  <div className="text-[11px] text-gig-muted truncate uppercase tracking-wide">
                    {s.track.artist ?? "—"} · {s.key_relation}
                    {s.bpm_delta != null && ` · ${s.bpm_delta > 0 ? "+" : ""}${s.bpm_delta.toFixed(1)} BPM`}
                    {s.energy_delta != null &&
                      ` · ENERGY ${s.energy_delta > 0 ? "+" : ""}${Math.round(s.energy_delta * 100)}%`}
                  </div>
                </div>
                <EnergyGauge energy={s.track.energy} size={28} />
                <BpmBadge bpm={s.track.bpm} />
                <KeyBadge camelot={s.track.key_camelot} keyName={s.track.key_name} />
                <span className="text-sm font-mono font-bold text-gig-accent w-10 text-right">
                  {Math.round(s.score * 100)}%
                </span>
              </button>
            ))
          )}
        </div>
        <div className="p-2.5 border-t border-gig-border text-right bg-gig-panel2">
          <button onClick={onClose} className="text-xs font-bold uppercase text-gig-muted hover:text-gig-text px-3 py-1.5">
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
