import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { Transition } from "../types";
import { TRANSITION_TYPES } from "../types";

export function TransitionPicker({
  onPick,
  onClose,
}: {
  onPick: (transitionId: string) => void;
  onClose: () => void;
}) {
  const [transitions, setTransitions] = useState<Transition[]>([]);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [type, setType] = useState<string>(TRANSITION_TYPES[0]);
  const [crossfade, setCrossfade] = useState(8);
  const [notes, setNotes] = useState("");

  async function refresh() {
    setTransitions(await api.listTransitions());
  }

  useEffect(() => {
    refresh();
  }, []);

  async function create() {
    if (!name.trim()) return;
    const t = await api.saveTransition({
      name: name.trim(),
      transitionType: type,
      crossfadeSecs: crossfade,
      notes: notes || null,
    });
    setName("");
    setNotes("");
    setCreating(false);
    await refresh();
    onPick(t.id);
  }

  return (
    <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50" onClick={onClose}>
      <div
        className="bg-gig-panel border border-gig-borderLight rounded-sm w-full max-w-md max-h-[75vh] flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="p-3 border-b border-gig-border bg-gig-panel2 flex items-center justify-between">
          <h3 className="text-[11px] font-bold uppercase tracking-wider">Choose a transition</h3>
          <button
            onClick={() => setCreating((c) => !c)}
            className="text-[10px] font-bold uppercase px-2 py-1 rounded-sm bg-gig-accent2/20 text-gig-accent2 hover:bg-gig-accent2/30"
          >
            {creating ? "Cancel new" : "+ New"}
          </button>
        </div>

        {creating && (
          <div className="p-3 border-b border-gig-border space-y-2 bg-black/30">
            <input
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="TRANSITION NAME (E.G. 16-BAR BLEND)"
              className="w-full bg-black border border-gig-border rounded-sm px-2.5 py-1 text-xs"
            />
            <div className="flex gap-2">
              <select
                value={type}
                onChange={(e) => setType(e.target.value)}
                className="flex-1 bg-black border border-gig-border rounded-sm px-2.5 py-1 text-xs uppercase"
              >
                {TRANSITION_TYPES.map((t) => (
                  <option key={t} value={t}>
                    {t}
                  </option>
                ))}
              </select>
              <input
                type="number"
                min={1}
                value={crossfade}
                onChange={(e) => setCrossfade(parseFloat(e.target.value) || 0)}
                className="w-20 bg-black border border-gig-border rounded-sm px-2.5 py-1 text-xs font-mono"
                title="Crossfade length (seconds)"
              />
              <span className="text-[10px] text-gig-muted self-center uppercase">sec</span>
            </div>
            <textarea
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              placeholder="NOTES: EQ MOVES, EFFECTS, CUE POINTS…"
              className="w-full bg-black border border-gig-border rounded-sm px-2.5 py-1 text-xs resize-none"
              rows={2}
            />
            <button onClick={create} className="w-full bg-gig-accent2 text-black rounded-sm py-1.5 text-[11px] font-bold uppercase tracking-wide">
              Save to library & assign
            </button>
          </div>
        )}

        <div className="flex-1 overflow-y-auto p-1">
          {transitions.length === 0 && !creating ? (
            <div className="text-gig-muted text-xs text-center py-10 uppercase tracking-wide">
              No saved transitions yet. Create one above.
            </div>
          ) : (
            transitions.map((t) => (
              <button
                key={t.id}
                onClick={() => onPick(t.id)}
                className="w-full flex items-center justify-between px-3 py-2 hover:bg-gig-panel2 text-left border-b border-gig-border/40"
              >
                <div>
                  <div className="text-xs font-bold">{t.name}</div>
                  <div className="text-[10px] text-gig-muted uppercase">
                    {t.transition_type} · {t.crossfade_secs}s
                  </div>
                </div>
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
