import { useEffect, useState } from "react";
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
import { KeyBadge, BpmBadge, EnergyMeter } from "../components/KeyBadge";
import { HotCueStrip } from "../components/HotCueStrip";
import { TrackPicker } from "../components/TrackPicker";
import { TransitionPicker } from "../components/TransitionPicker";
import type { HotCue, Setlist, SetlistEntryWithTrack, TrackSuggestion } from "../types";

export function SetlistEditor({ setlistId, onBack }: { setlistId: string; onBack: () => void }) {
  const [setlist, setSetlist] = useState<Setlist | null>(null);
  const [entries, setEntries] = useState<SetlistEntryWithTrack[]>([]);
  const [cuesByTrack, setCuesByTrack] = useState<Record<string, HotCue[]>>({});
  const [showPicker, setShowPicker] = useState(false);
  const [transitionTargetEntry, setTransitionTargetEntry] = useState<string | null>(null);
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

  async function addTrack(trackId: string) {
    await api.addTrackToSetlist(setlistId, trackId);
    setShowPicker(false);
    await load();
  }

  async function removeEntry(entryId: string) {
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
    await addTrack(trackId);
    setSuggestFor(null);
    setSuggestions([]);
  }

  async function assignTransition(entryId: string, transitionId: string) {
    await api.assignTransition(entryId, transitionId);
    setTransitionTargetEntry(null);
    await load();
  }

  async function handleExport() {
    const dest = await open({ directory: true, multiple: false, title: "Choose USB drive or export folder" });
    if (!dest || Array.isArray(dest)) return;
    try {
      const folder = await api.exportSetlist(setlistId, dest);
      setExportMsg(`Exported to ${folder}`);
    } catch (e) {
      setExportMsg(`Export failed: ${e}`);
    }
  }

  const trackIds = entries.map((e) => e.track.id);

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between h-14 px-5 border-b border-gig-border bg-gig-panel">
        <div>
          <button onClick={onBack} className="text-[10px] font-bold uppercase tracking-wide text-gig-muted hover:text-gig-accent">
            ← All setlists
          </button>
        </div>
        <div className="flex-1 px-4">
          <h1 className="text-xs font-bold uppercase tracking-widest text-gig-text">{setlist?.name ?? "Setlist"}</h1>
        </div>
        <span className="label-xs mr-4">
          {entries.length} track{entries.length === 1 ? "" : "s"}
          {" · "}
          {formatDuration(entries.reduce((sum, e) => sum + (e.track.duration_secs ?? 0), 0))} total
        </span>
        <div className="flex gap-2">
          <button
            onClick={() => setShowPicker(true)}
            className="px-3 py-1.5 rounded-sm text-[11px] font-bold uppercase tracking-wide bg-gig-panel3 border border-gig-borderLight hover:border-gig-accent2"
          >
            + Add track
          </button>
          <button
            onClick={handleExport}
            disabled={entries.length === 0}
            className="px-3 py-1.5 rounded-sm text-[11px] font-bold uppercase tracking-wide bg-gig-accent text-black hover:bg-gig-accent/80 disabled:opacity-30"
          >
            Export…
          </button>
        </div>
      </div>

      {exportMsg && (
        <div className="px-5 py-2 text-[11px] font-mono bg-gig-accent2/10 text-gig-accent2 border-b border-gig-border flex justify-between">
          <span>{exportMsg}</span>
          <button onClick={() => setExportMsg(null)} className="text-gig-muted hover:text-gig-text">
            ✕
          </button>
        </div>
      )}

      <div className="flex-1 overflow-y-auto px-5 py-5">
        {entries.length === 0 ? (
          <div className="text-gig-muted text-xs py-20 text-center border border-dashed border-gig-border uppercase tracking-wide">
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
                      onRemove={() => removeEntry(e.entry.id)}
                      onSuggest={() => openSuggestions(e.entry.id)}
                    />
                    {i < entries.length - 1 && (
                      <TransitionGap
                        fromKey={e.track.key_camelot}
                        toKey={entries[i + 1].track.key_camelot}
                        fromBpm={e.track.bpm}
                        toBpm={entries[i + 1].track.bpm}
                        transitionName={e.transition?.name ?? null}
                        onClick={() => setTransitionTargetEntry(e.entry.id)}
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
        <TrackPicker excludeIds={trackIds} onPick={addTrack} onClose={() => setShowPicker(false)} />
      )}

      {transitionTargetEntry && (
        <TransitionPicker
          onPick={(tid) => assignTransition(transitionTargetEntry, tid)}
          onClose={() => setTransitionTargetEntry(null)}
        />
      )}

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
  onRemove,
  onSuggest,
}: {
  entry: SetlistEntryWithTrack;
  index: number;
  cues: HotCue[];
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
        <span className="text-[10px] font-mono text-gig-dim w-5 text-center">{index + 1}</span>
        <div className="flex-1 min-w-0">
          <div className="text-xs font-bold truncate">{entry.track.title}</div>
          <div className="text-[10px] text-gig-muted truncate">{entry.track.artist ?? "—"}</div>
        </div>
        <span className="text-[10px] font-mono text-gig-muted">{formatDuration(entry.track.duration_secs)}</span>
        <BpmBadge bpm={entry.track.bpm} />
        <KeyBadge camelot={entry.track.key_camelot} keyName={entry.track.key_name} />
        <EnergyMeter energy={entry.track.energy} />
        <button
          onClick={onSuggest}
          className="text-[10px] font-bold uppercase px-2 py-1 rounded-sm bg-gig-accent2/15 text-gig-accent2 hover:bg-gig-accent2/25"
          title="Suggest a track to follow this one"
        >
          Suggest next
        </button>
        <button onClick={onRemove} className="text-[10px] font-bold px-2 py-1 rounded-sm text-gig-muted hover:text-gig-warn">
          ✕
        </button>
      </div>
      {cues.length > 0 && (
        <div className="pl-9 pt-1.5">
          <HotCueStrip duration={entry.track.duration_secs} cues={cues} height={6} />
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
  transitionName,
  onClick,
}: {
  fromKey: string | null;
  toKey: string | null;
  fromBpm: number | null;
  toBpm: number | null;
  transitionName: string | null;
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

  return (
    <button
      onClick={onClick}
      className="w-full flex items-center gap-2 pl-9 py-1.5 text-[10px] text-gig-dim hover:text-gig-text group"
    >
      <span className="w-px h-4 bg-gig-border" />
      {transitionName ? (
        <span className="px-2 py-0.5 rounded-sm bg-gig-accent2/15 text-gig-accent2">{transitionName}</span>
      ) : (
        <span className="px-2 py-0.5 rounded-sm border border-dashed border-gig-border uppercase tracking-wide group-hover:border-gig-accent2">
          + assign transition
        </span>
      )}
      <span className={`${relationColor} uppercase tracking-wide`}>{relation}</span>
      {bpmDelta != null && (
        <span className="text-gig-muted font-mono">
          {bpmDelta > 0 ? "+" : ""}
          {bpmDelta.toFixed(1)} BPM
        </span>
      )}
    </button>
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
  return (
    <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50" onClick={onClose}>
      <div
        className="bg-gig-panel border border-gig-borderLight rounded-sm w-full max-w-lg max-h-[70vh] flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="p-3 border-b border-gig-border bg-gig-panel2">
          <h3 className="text-[11px] font-bold uppercase tracking-wider">Suggested next tracks</h3>
          <p className="text-[10px] text-gig-muted mt-0.5">Ranked by BPM, key (Camelot) and energy flow</p>
        </div>
        <div className="flex-1 overflow-y-auto p-1">
          {suggestions.length === 0 ? (
            <div className="text-gig-muted text-xs text-center py-10 uppercase tracking-wide">
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
                  <div className="text-xs font-bold truncate">{s.track.title}</div>
                  <div className="text-[10px] text-gig-muted truncate uppercase tracking-wide">
                    {s.track.artist ?? "—"} · {s.key_relation}
                    {s.bpm_delta != null && ` · ${s.bpm_delta > 0 ? "+" : ""}${s.bpm_delta.toFixed(1)} BPM`}
                    {s.energy_delta != null &&
                      ` · ENERGY ${s.energy_delta > 0 ? "+" : ""}${Math.round(s.energy_delta * 100)}%`}
                  </div>
                </div>
                <EnergyMeter energy={s.track.energy} />
                <BpmBadge bpm={s.track.bpm} />
                <KeyBadge camelot={s.track.key_camelot} keyName={s.track.key_name} />
                <span className="text-xs font-mono font-bold text-gig-accent w-10 text-right">
                  {Math.round(s.score * 100)}%
                </span>
              </button>
            ))
          )}
        </div>
        <div className="p-2.5 border-t border-gig-border text-right bg-gig-panel2">
          <button onClick={onClose} className="text-[11px] font-bold uppercase text-gig-muted hover:text-gig-text px-3 py-1.5">
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
