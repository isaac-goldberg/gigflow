import { useState } from "react";
import { useLibrary } from "../store/library";
import { KeyBadge, BpmBadge } from "./KeyBadge";
import { formatDuration } from "../lib/api";

export function TrackPicker({
  excludeIds,
  onPick,
  onClose,
}: {
  excludeIds: string[];
  onPick: (trackId: string) => void;
  onClose: () => void;
}) {
  const { tracks } = useLibrary();
  const [q, setQ] = useState("");

  const available = tracks.filter(
    (t) =>
      !excludeIds.includes(t.id) &&
      (t.title.toLowerCase().includes(q.toLowerCase()) ||
        (t.artist ?? "").toLowerCase().includes(q.toLowerCase()))
  );

  return (
    <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50" onClick={onClose}>
      <div
        className="bg-gig-panel border border-gig-borderLight rounded-sm w-full max-w-lg max-h-[70vh] flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="p-3 border-b border-gig-border bg-gig-panel2">
          <h3 className="text-[11px] font-bold uppercase tracking-wider mb-2">Add track to setlist</h3>
          <input
            autoFocus
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="SEARCH YOUR LIBRARY..."
            className="w-full bg-black border border-gig-border rounded-sm px-2.5 py-1 text-xs focus:outline-none"
          />
        </div>
        <div className="flex-1 overflow-y-auto p-1">
          {available.length === 0 ? (
            <div className="text-gig-muted text-xs text-center py-10 uppercase tracking-wide">No matching tracks.</div>
          ) : (
            available.map((t) => (
              <button
                key={t.id}
                onClick={() => onPick(t.id)}
                className="w-full flex items-center gap-3 px-3 py-2 hover:bg-gig-panel2 text-left border-b border-gig-border/40"
              >
                <div className="flex-1 min-w-0">
                  <div className="text-xs font-bold truncate">{t.title}</div>
                  <div className="text-[10px] text-gig-muted truncate">{t.artist ?? "—"}</div>
                </div>
                <span className="text-[10px] text-gig-muted font-mono">{formatDuration(t.duration_secs)}</span>
                <BpmBadge bpm={t.bpm} />
                <KeyBadge camelot={t.key_camelot} keyName={t.key_name} />
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
