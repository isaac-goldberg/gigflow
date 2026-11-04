import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { Transition } from "../types";
import { TRANSITION_TYPES } from "../types";

export function TransitionsView() {
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
    await api.saveTransition({
      name: name.trim(),
      transitionType: type,
      crossfadeSecs: crossfade,
      notes: notes || null,
    });
    setName("");
    setNotes("");
    setCreating(false);
    await refresh();
  }

  async function remove(id: string) {
    await api.deleteTransition(id);
    await refresh();
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between h-14 px-5 border-b border-gig-border bg-gig-panel">
        <div className="flex items-baseline gap-3">
          <h1 className="text-xs font-bold uppercase tracking-widest text-gig-text">Transition library</h1>
          <span className="label-xs">Save reusable blends and recall them from any setlist</span>
        </div>
        <button
          onClick={() => setCreating((c) => !c)}
          className="px-3 py-1.5 rounded-sm text-[11px] font-bold uppercase tracking-wide bg-gig-accent text-black hover:bg-gig-accent/80"
        >
          + New transition
        </button>
      </div>

      {creating && (
        <div className="px-5 py-3 border-b border-gig-border bg-gig-panel2 space-y-2">
          <input
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="TRANSITION NAME (E.G. 16-BAR HARMONIC BLEND)"
            className="w-full max-w-md bg-black border border-gig-border rounded-sm px-2.5 py-1 text-xs"
          />
          <div className="flex gap-2 items-center">
            <select
              value={type}
              onChange={(e) => setType(e.target.value)}
              className="bg-black border border-gig-border rounded-sm px-2.5 py-1 text-xs uppercase"
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
            />
            <span className="text-[10px] text-gig-muted uppercase">sec crossfade</span>
          </div>
          <textarea
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            placeholder="NOTES: EQ MOVES, EFFECTS, CUE POINTS…"
            className="w-full max-w-md bg-black border border-gig-border rounded-sm px-2.5 py-1 text-xs resize-none"
            rows={2}
          />
          <button onClick={create} className="bg-gig-accent2 text-black rounded-sm px-4 py-1.5 text-[11px] font-bold uppercase tracking-wide">
            Save to library
          </button>
        </div>
      )}

      <div className="flex-1 overflow-y-auto px-5 py-5">
        {transitions.length === 0 ? (
          <div className="text-gig-muted text-xs py-20 text-center border border-dashed border-gig-border uppercase tracking-wide">
            No transitions saved yet.
          </div>
        ) : (
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
            {transitions.map((t) => (
              <div key={t.id} className="bg-gig-panel border border-gig-border rounded-sm p-3.5">
                <div className="flex items-start justify-between">
                  <h3 className="font-bold text-xs">{t.name}</h3>
                  <button onClick={() => remove(t.id)} className="text-[10px] font-bold uppercase text-gig-dim hover:text-gig-warn">
                    Delete
                  </button>
                </div>
                <div className="text-[10px] text-gig-muted mt-1 uppercase tracking-wide">
                  {t.transition_type} · {t.crossfade_secs}s crossfade
                </div>
                {t.notes && <p className="text-xs text-gig-text/80 mt-2">{t.notes}</p>}
                <div className="mt-3 flex items-center justify-between">
                  <span className="label-xs">Private</span>
                  <button
                    disabled
                    title="Sharing transitions with other DJs needs Gigflow cloud accounts — coming in a future update"
                    className="text-[10px] font-bold uppercase px-2 py-1 rounded-sm border border-gig-border text-gig-dim"
                  >
                    Share… (soon)
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
