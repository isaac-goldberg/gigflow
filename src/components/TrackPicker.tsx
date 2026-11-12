import { useState, type MouseEvent } from "react";
import { useLibrary } from "../store/library";
import { KeyBadge, BpmBadge } from "./KeyBadge";
import { formatDuration } from "../lib/api";
import { useEscapeKey } from "../lib/useEscapeKey";

export function TrackPicker({
  excludeIds,
  onPick,
  onClose,
}: {
  excludeIds: string[];
  /** Called with one or more track ids, in the order they should be added. */
  onPick: (trackIds: string[]) => void;
  onClose: () => void;
}) {
  const { tracks } = useLibrary();
  const [q, setQ] = useState("");
  const [selected, setSelected] = useState<string[]>([]);
  const [anchorIndex, setAnchorIndex] = useState<number | null>(null);
  useEscapeKey(onClose);

  const available = tracks.filter(
    (t) =>
      !excludeIds.includes(t.id) &&
      (t.title.toLowerCase().includes(q.toLowerCase()) ||
        (t.artist ?? "").toLowerCase().includes(q.toLowerCase()))
  );

  function handleRowClick(e: MouseEvent, trackId: string, index: number) {
    const multiKey = e.metaKey || e.ctrlKey;
    if (e.shiftKey) {
      const start = anchorIndex ?? index;
      const [lo, hi] = start <= index ? [start, index] : [index, start];
      setSelected(available.slice(lo, hi + 1).map((t) => t.id));
      return;
    }
    if (multiKey) {
      setSelected((prev) => (prev.includes(trackId) ? prev.filter((id) => id !== trackId) : [...prev, trackId]));
      setAnchorIndex(index);
      return;
    }
    // A plain click with nothing else selected adds this one track right
    // away, same as before multi-select existed. Once a selection is
    // building, a plain click instead restarts it at just this row, so an
    // accidental unmodified click can't silently add a partial selection.
    if (selected.length === 0) {
      onPick([trackId]);
      return;
    }
    setSelected([trackId]);
    setAnchorIndex(index);
  }

  function addSelected() {
    if (selected.length === 0) return;
    // "In order" means list order, not click order — matches how Finder
    // keeps a multi-selection's original order when you act on it.
    const ordered = available.filter((t) => selected.includes(t.id)).map((t) => t.id);
    onPick(ordered);
  }

  return (
    <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50" onClick={onClose}>
      <div
        className="bg-gig-panel border border-gig-borderLight rounded-sm w-full max-w-lg max-h-[70vh] flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="p-3 border-b border-gig-border bg-gig-panel2">
          <h3 className="text-xs font-bold uppercase tracking-wider mb-2">Add track to setlist</h3>
          <input
            autoFocus
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="SEARCH YOUR LIBRARY..."
            className="w-full bg-black border border-gig-border rounded-sm px-2.5 py-1 text-sm focus:outline-none"
          />
          <p className="text-[10px] text-gig-dim mt-1.5">
            Hold ⌘/Ctrl or Shift to select multiple tracks — they're added in the order shown here.
          </p>
        </div>
        <div className="flex-1 overflow-y-auto p-1">
          {available.length === 0 ? (
            <div className="text-gig-muted text-sm text-center py-10 uppercase tracking-wide">No matching tracks.</div>
          ) : (
            available.map((t, i) => {
              const isSelected = selected.includes(t.id);
              return (
                <button
                  key={t.id}
                  onClick={(e) => handleRowClick(e, t.id, i)}
                  className={`w-full flex items-center gap-3 px-3 py-2 text-left border-b border-gig-border/40 ${
                    isSelected ? "bg-gig-accent2/15" : "hover:bg-gig-panel2"
                  }`}
                >
                  <span
                    className={`w-3.5 h-3.5 shrink-0 rounded-sm border flex items-center justify-center text-[9px] ${
                      isSelected ? "bg-gig-accent2 border-gig-accent2 text-black" : "border-gig-borderLight"
                    }`}
                  >
                    {isSelected ? "✓" : ""}
                  </span>
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-bold truncate">{t.title}</div>
                    <div className="text-[11px] text-gig-muted truncate">{t.artist ?? "—"}</div>
                  </div>
                  <span className="text-[11px] text-gig-muted font-mono">{formatDuration(t.duration_secs)}</span>
                  <BpmBadge bpm={t.bpm} />
                  <KeyBadge camelot={t.key_camelot} keyName={t.key_name} />
                </button>
              );
            })
          )}
        </div>
        <div className="p-2.5 border-t border-gig-border flex items-center justify-between gap-2 bg-gig-panel2">
          <span className="text-[11px] text-gig-muted uppercase tracking-wide">
            {selected.length > 0 ? `${selected.length} selected` : ""}
          </span>
          <div className="flex items-center gap-2">
            <button onClick={onClose} className="text-xs font-bold uppercase text-gig-muted hover:text-gig-text px-3 py-1.5">
              Close
            </button>
            {selected.length > 0 && (
              <button
                onClick={addSelected}
                className="bg-gig-accent text-black rounded-sm px-4 py-1.5 text-xs font-bold uppercase tracking-wide hover:bg-gig-accent/80"
              >
                Add {selected.length} track{selected.length === 1 ? "" : "s"}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
