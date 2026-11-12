import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { HotCue, Track } from "../types";
import { HotCueStrip, cueColor, padLetter, formatClock } from "./HotCueStrip";
import { useEscapeKey } from "../lib/useEscapeKey";

export function HotCueModal({ track, onClose }: { track: Track; onClose: () => void }) {
  useEscapeKey(onClose);
  const [cues, setCues] = useState<HotCue[]>([]);
  const [busy, setBusy] = useState(false);

  async function refresh() {
    setCues(await api.getHotCues(track.id));
  }

  useEffect(() => {
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [track.id]);

  async function reDetect() {
    setBusy(true);
    try {
      await api.analyzeTrack(track.id);
      await refresh();
    } finally {
      setBusy(false);
    }
  }

  async function remove(id: string) {
    await api.deleteHotCue(id);
    await refresh();
  }

  return (
    <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50" onClick={onClose}>
      <div
        className="bg-gig-panel border border-gig-borderLight rounded-sm w-full max-w-lg max-h-[75vh] flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="p-3 border-b border-gig-border bg-gig-panel2 flex items-center justify-between">
          <div>
            <h3 className="text-xs font-bold uppercase tracking-wider">{track.title}</h3>
            <p className="label-xs mt-0.5">Automated hot cues</p>
          </div>
          <button
            onClick={reDetect}
            disabled={busy}
            className="text-[11px] font-bold uppercase px-2 py-1 rounded-sm bg-gig-accent2/20 text-gig-accent2 hover:bg-gig-accent2/30 disabled:opacity-50"
          >
            {busy ? "Detecting…" : "Re-detect"}
          </button>
        </div>

        <div className="p-3 border-b border-gig-border">
          <HotCueStrip duration={track.duration_secs} cues={cues} height={28} />
        </div>

        <div className="flex-1 overflow-y-auto p-1">
          {cues.length === 0 ? (
            <div className="text-gig-muted text-sm text-center py-10 uppercase tracking-wide">
              No hot cues yet — analyze this track first.
            </div>
          ) : (
            cues.map((c) => (
              <div key={c.id} className="w-full flex items-center gap-3 px-3 py-2 border-b border-gig-border/40">
                <span
                  className="w-5 h-5 flex items-center justify-center rounded-sm text-[11px] font-mono font-bold text-black shrink-0"
                  style={{ backgroundColor: cueColor(c.pad) }}
                >
                  {padLetter(c.pad)}
                </span>
                <span className="text-sm font-mono text-gig-text w-16">{formatClock(c.position_secs)}</span>
                <span className="text-sm font-bold uppercase tracking-wide flex-1">{c.label}</span>
                <button
                  onClick={() => remove(c.id)}
                  className="text-[11px] font-bold uppercase px-2 py-1 rounded-sm text-gig-muted hover:text-gig-warn"
                >
                  Remove
                </button>
              </div>
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
