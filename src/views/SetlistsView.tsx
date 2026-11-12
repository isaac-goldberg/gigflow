import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { Setlist } from "../types";

export function SetlistsView({ onOpen }: { onOpen: (id: string) => void }) {
  const [setlists, setSetlists] = useState<Setlist[]>([]);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [gigDate, setGigDate] = useState("");

  async function refresh() {
    setSetlists(await api.listSetlists());
  }

  useEffect(() => {
    refresh();
  }, []);

  async function createSetlist() {
    if (!name.trim()) return;
    const sl = await api.createSetlist(name.trim(), gigDate || null, null);
    setName("");
    setGigDate("");
    setCreating(false);
    await refresh();
    onOpen(sl.id);
  }

  async function removeSetlist(id: string) {
    await api.deleteSetlist(id);
    await refresh();
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between h-14 px-5 border-b border-gig-border bg-gig-panel">
        <div className="flex items-baseline gap-3">
          <h1 className="text-sm font-bold uppercase tracking-widest text-gig-text">Setlists</h1>
          <span className="label-xs">Build and export your sets</span>
        </div>
        <button
          data-tour="new-setlist-button"
          onClick={() => setCreating(true)}
          className="px-3 py-1.5 rounded-sm text-xs font-bold uppercase tracking-wide bg-gig-accent text-black hover:bg-gig-accent/80"
        >
          + New setlist
        </button>
      </div>

      {creating && (
        <div className="px-5 py-3 border-b border-gig-border bg-gig-panel2 flex gap-2 items-end">
          <div className="flex-1 max-w-sm">
            <label className="label-xs">Name</label>
            <input
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && createSetlist()}
              placeholder="Friday night warehouse set"
              className="w-full bg-black border border-gig-border rounded-sm px-2.5 py-1 text-sm mt-1 focus:outline-none"
            />
          </div>
          <div>
            <label className="label-xs">Gig date</label>
            <input
              type="date"
              value={gigDate}
              onChange={(e) => setGigDate(e.target.value)}
              className="w-full bg-black border border-gig-border rounded-sm px-2.5 py-1 text-sm mt-1 focus:outline-none"
            />
          </div>
          <button onClick={createSetlist} className="px-3 py-1.5 rounded-sm text-xs font-bold uppercase bg-gig-accent2 text-black">
            Create
          </button>
          <button onClick={() => setCreating(false)} className="px-3 py-1.5 rounded-sm text-xs font-bold uppercase text-gig-muted">
            Cancel
          </button>
        </div>
      )}

      <div className="flex-1 overflow-y-auto px-5 py-5">
        {setlists.length === 0 ? (
          <div className="text-gig-muted text-sm py-20 text-center border border-dashed border-gig-border uppercase tracking-wide">
            No setlists yet. Create one to start arranging tracks.
          </div>
        ) : (
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
            {setlists.map((s) => (
              <div
                key={s.id}
                onClick={() => onOpen(s.id)}
                className="cursor-pointer bg-gig-panel border border-gig-border rounded-sm p-3.5 hover:border-gig-accent group"
              >
                <div className="flex items-start justify-between">
                  <h3 className="font-bold text-base text-gig-text">{s.name}</h3>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      removeSetlist(s.id);
                    }}
                    className="text-[11px] font-bold uppercase text-gig-dim hover:text-gig-warn"
                  >
                    Delete
                  </button>
                </div>
                {s.gig_date && <div className="text-[11px] text-gig-muted mt-1 font-mono">{s.gig_date}</div>}
                <div className="text-[11px] text-gig-dim mt-3 uppercase tracking-wide">
                  Updated {new Date(s.updated_at).toLocaleDateString()}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
