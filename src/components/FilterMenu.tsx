import { useEffect, useMemo, useRef, useState } from "react";
import type { Track } from "../types";

export interface LibraryFilters {
  genre: string | null;
  subgenre: string | null;
  key: string | null;
  bpmMin: number | null;
  bpmMax: number | null;
  energyMin: number | null; // 0-100
  energyMax: number | null; // 0-100
}

export const EMPTY_FILTERS: LibraryFilters = {
  genre: null,
  subgenre: null,
  key: null,
  bpmMin: null,
  bpmMax: null,
  energyMin: null,
  energyMax: null,
};

export function countActiveFilters(f: LibraryFilters): number {
  return Object.values(f).filter((v) => v != null && v !== "").length;
}

export function applyFilters(tracks: Track[], f: LibraryFilters): Track[] {
  return tracks.filter((t) => {
    if (f.genre && t.genre !== f.genre) return false;
    if (f.subgenre && t.subgenre !== f.subgenre) return false;
    if (f.key && t.key_camelot !== f.key) return false;
    if (f.bpmMin != null && (t.bpm == null || t.bpm < f.bpmMin)) return false;
    if (f.bpmMax != null && (t.bpm == null || t.bpm > f.bpmMax)) return false;
    if (f.energyMin != null && (t.energy == null || t.energy * 100 < f.energyMin)) return false;
    if (f.energyMax != null && (t.energy == null || t.energy * 100 > f.energyMax)) return false;
    return true;
  });
}

export function FilterMenu({
  tracks,
  filters,
  onChange,
}: {
  tracks: Track[];
  filters: LibraryFilters;
  onChange: (f: LibraryFilters) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function onDocClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", onDocClick);
    return () => document.removeEventListener("mousedown", onDocClick);
  }, []);

  const genres = useMemo(
    () => Array.from(new Set(tracks.map((t) => t.genre).filter((g): g is string => !!g))).sort(),
    [tracks]
  );
  const subgenres = useMemo(
    () =>
      Array.from(
        new Set(
          tracks
            .filter((t) => !filters.genre || t.genre === filters.genre)
            .map((t) => t.subgenre)
            .filter((g): g is string => !!g)
        )
      ).sort(),
    [tracks, filters.genre]
  );
  const keys = useMemo(
    () => Array.from(new Set(tracks.map((t) => t.key_camelot).filter((k): k is string => !!k))).sort(),
    [tracks]
  );

  const activeCount = countActiveFilters(filters);

  function set<K extends keyof LibraryFilters>(key: K, value: LibraryFilters[K]) {
    onChange({ ...filters, [key]: value });
  }

  return (
    <div className="relative" ref={ref}>
      <button
        onClick={() => setOpen((o) => !o)}
        className={`px-3 py-1.5 rounded-sm text-xs font-bold uppercase tracking-wide border flex items-center gap-1.5 ${
          activeCount > 0
            ? "bg-gig-accent3/20 border-gig-accent3 text-gig-accent3"
            : "bg-gig-panel3 border-gig-borderLight text-gig-text hover:border-gig-accent2"
        }`}
      >
        Filter
        {activeCount > 0 && (
          <span className="inline-flex items-center justify-center w-4 h-4 rounded-full bg-gig-accent3 text-black text-[10px] font-mono">
            {activeCount}
          </span>
        )}
      </button>

      {open && (
        <div className="absolute right-0 mt-1 w-72 bg-gig-panel border border-gig-borderLight rounded-sm shadow-lg z-40 p-3 space-y-3">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold uppercase tracking-wider">Filter library</span>
            {activeCount > 0 && (
              <button
                onClick={() => onChange(EMPTY_FILTERS)}
                className="text-[11px] font-bold uppercase text-gig-muted hover:text-gig-warn"
              >
                Clear all
              </button>
            )}
          </div>

          <div>
            <label className="label-xs">Genre</label>
            <select
              value={filters.genre ?? ""}
              onChange={(e) => set("genre", e.target.value || null)}
              className="w-full mt-1 bg-black border border-gig-border rounded-sm px-2 py-1 text-sm"
            >
              <option value="">All genres</option>
              {genres.map((g) => (
                <option key={g} value={g}>
                  {g}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className="label-xs">Subgenre</label>
            <select
              value={filters.subgenre ?? ""}
              onChange={(e) => set("subgenre", e.target.value || null)}
              className="w-full mt-1 bg-black border border-gig-border rounded-sm px-2 py-1 text-sm"
            >
              <option value="">All subgenres</option>
              {subgenres.map((g) => (
                <option key={g} value={g}>
                  {g}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className="label-xs">Key (Camelot)</label>
            <select
              value={filters.key ?? ""}
              onChange={(e) => set("key", e.target.value || null)}
              className="w-full mt-1 bg-black border border-gig-border rounded-sm px-2 py-1 text-sm font-mono"
            >
              <option value="">All keys</option>
              {keys.map((k) => (
                <option key={k} value={k}>
                  {k}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className="label-xs">BPM range</label>
            <div className="flex items-center gap-2 mt-1">
              <input
                type="number"
                placeholder="Min"
                value={filters.bpmMin ?? ""}
                onChange={(e) => set("bpmMin", e.target.value ? parseFloat(e.target.value) : null)}
                className="w-full bg-black border border-gig-border rounded-sm px-2 py-1 text-sm font-mono"
              />
              <span className="text-gig-dim text-sm">–</span>
              <input
                type="number"
                placeholder="Max"
                value={filters.bpmMax ?? ""}
                onChange={(e) => set("bpmMax", e.target.value ? parseFloat(e.target.value) : null)}
                className="w-full bg-black border border-gig-border rounded-sm px-2 py-1 text-sm font-mono"
              />
            </div>
          </div>

          <div>
            <label className="label-xs">Energy % range</label>
            <div className="flex items-center gap-2 mt-1">
              <input
                type="number"
                placeholder="Min"
                min={0}
                max={100}
                value={filters.energyMin ?? ""}
                onChange={(e) => set("energyMin", e.target.value ? parseFloat(e.target.value) : null)}
                className="w-full bg-black border border-gig-border rounded-sm px-2 py-1 text-sm font-mono"
              />
              <span className="text-gig-dim text-sm">–</span>
              <input
                type="number"
                placeholder="Max"
                min={0}
                max={100}
                value={filters.energyMax ?? ""}
                onChange={(e) => set("energyMax", e.target.value ? parseFloat(e.target.value) : null)}
                className="w-full bg-black border border-gig-border rounded-sm px-2 py-1 text-sm font-mono"
              />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
