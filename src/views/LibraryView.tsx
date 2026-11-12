import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useLibrary } from "../store/library";
import { usePlayback } from "../store/playback";
import { api, formatDuration } from "../lib/api";
import { KeyBadge, BpmBadge } from "../components/KeyBadge";
import { EnergyGauge } from "../components/EnergyGauge";
import { HotCueModal } from "../components/HotCueModal";
import { FilterMenu, EMPTY_FILTERS, applyFilters, countActiveFilters, type LibraryFilters } from "../components/FilterMenu";
import type { Track } from "../types";

const AUDIO_EXTENSIONS = ["mp3", "wav", "flac", "ogg", "m4a", "aac", "aif", "aiff", "alac"];

type SortKey = "title" | "genre" | "subgenre" | "time" | "bpm" | "key" | "energy";

function sortValue(t: Track, key: SortKey): string | number | null {
  switch (key) {
    case "title":
      return t.title.toLowerCase();
    case "genre":
      return t.genre?.toLowerCase() ?? null;
    case "subgenre":
      return t.subgenre?.toLowerCase() ?? null;
    case "time":
      return t.duration_secs;
    case "bpm":
      return t.bpm;
    case "key":
      return t.key_camelot ?? null;
    case "energy":
      return t.energy;
  }
}

/** Nulls always sort last, regardless of direction, so reversing never buries real values under empty ones. */
function sortTracks(tracks: Track[], key: SortKey | null, dir: "asc" | "desc"): Track[] {
  if (!key) return tracks;
  const mult = dir === "asc" ? 1 : -1;
  return [...tracks].sort((a, b) => {
    const va = sortValue(a, key);
    const vb = sortValue(b, key);
    if (va == null && vb == null) return 0;
    if (va == null) return 1;
    if (vb == null) return -1;
    if (va < vb) return -1 * mult;
    if (va > vb) return 1 * mult;
    return 0;
  });
}

function SortableHeader({
  label,
  sortKey: key,
  active,
  dir,
  onSort,
}: {
  label: string;
  sortKey: SortKey;
  active: SortKey | null;
  dir: "asc" | "desc";
  onSort: (key: SortKey) => void;
}) {
  const isActive = active === key;
  return (
    <th className="py-2 font-bold">
      <button
        onClick={() => onSort(key)}
        className={`flex items-center gap-1 hover:text-gig-text ${isActive ? "text-gig-accent2" : ""}`}
      >
        {label}
        <span className="text-[9px]">{isActive ? (dir === "asc" ? "▲" : "▼") : ""}</span>
      </button>
    </th>
  );
}

export function LibraryView() {
  const {
    tracks,
    loading,
    analyzingIds,
    batchProgress,
    refresh,
    importFiles,
    analyze,
    analyzeAll,
    removeTrack,
    removeAllTracks,
    resetAllAnalysis,
  } = useLibrary();
  const [showMaintenance, setShowMaintenance] = useState(false);
  const [normalizing, setNormalizing] = useState(false);
  const [resetting, setResetting] = useState(false);
  const [confirmingRemoveAll, setConfirmingRemoveAll] = useState(false);
  const [confirmingReanalyze, setConfirmingReanalyze] = useState(false);
  const [removingAll, setRemovingAll] = useState(false);
  const playback = usePlayback();
  const [filter, setFilter] = useState("");
  const [libFilters, setLibFilters] = useState<LibraryFilters>(EMPTY_FILTERS);
  const [cueTrack, setCueTrack] = useState<Track | null>(null);
  const [sortKey, setSortKey] = useState<SortKey | null>(null);
  const [sortDir, setSortDir] = useState<"asc" | "desc">("asc");
  const [genreModelDownloaded, setGenreModelDownloaded] = useState<boolean | null>(null);
  const [downloadingModel, setDownloadingModel] = useState(false);
  const [downloadError, setDownloadError] = useState<string | null>(null);

  useEffect(() => {
    api.genreModelDownloaded().then(setGenreModelDownloaded);
  }, []);

  async function downloadModel() {
    setDownloadingModel(true);
    try {
      await api.downloadGenreModel();
      setGenreModelDownloaded(true);
    } catch (e) {
      setDownloadError(`Couldn't download the AI genre model: ${e}`);
    } finally {
      setDownloadingModel(false);
      setShowMaintenance(false);
    }
  }

  function toggleSort(key: SortKey) {
    if (sortKey === key) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(key);
      setSortDir("asc");
    }
  }

  useEffect(() => {
    refresh();
  }, [refresh]);

  async function handleImport() {
    const selected = await open({
      multiple: true,
      directory: false,
      filters: [{ name: "Audio", extensions: AUDIO_EXTENSIONS }],
    });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    await importFiles(paths);
  }

  const unanalyzedCount = tracks.filter((t) => t.bpm == null).length;
  const searched = tracks.filter((t) => {
    const q = filter.toLowerCase();
    return (
      t.title.toLowerCase().includes(q) ||
      (t.artist ?? "").toLowerCase().includes(q) ||
      (t.album ?? "").toLowerCase().includes(q)
    );
  });
  const filtered = useMemo(() => applyFilters(searched, libFilters), [searched, libFilters]);
  const activeFilterCount = countActiveFilters(libFilters);
  const sorted = useMemo(() => sortTracks(filtered, sortKey, sortDir), [filtered, sortKey, sortDir]);

  async function saveField(trackId: string, field: "genre" | "subgenre", value: string) {
    await api.updateTrack({ trackId, [field]: value || "" });
    await refresh();
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between h-14 px-5 border-b border-gig-border bg-gig-panel">
        <div className="flex items-baseline gap-3">
          <h1 className="text-sm font-bold uppercase tracking-widest text-gig-text">Library</h1>
          <span className="label-xs">
            {tracks.length} track{tracks.length === 1 ? "" : "s"}
            {unanalyzedCount > 0 && ` · ${unanalyzedCount} need analysis`}
          </span>
        </div>
        <div className="flex gap-2">
          {unanalyzedCount > 0 && (
            <button
              data-tour="analyze-all-button"
              onClick={() => analyzeAll()}
              disabled={!!batchProgress && batchProgress.done < batchProgress.total}
              className="px-3 py-1.5 rounded-sm text-xs font-bold uppercase tracking-wide bg-gig-panel3 border border-gig-borderLight hover:border-gig-accent2 text-gig-text disabled:opacity-40"
            >
              Analyze all ({unanalyzedCount})
            </button>
          )}
          <div className="relative">
            <button
              onClick={() => {
                setShowMaintenance((v) => !v);
                setConfirmingRemoveAll(false);
                setConfirmingReanalyze(false);
              }}
              className="px-2.5 py-1.5 rounded-sm text-xs font-bold uppercase tracking-wide bg-gig-panel3 border border-gig-borderLight hover:border-gig-accent2 text-gig-text"
            >
              •••
            </button>
            {showMaintenance && (
              <div className="absolute right-0 mt-1 w-56 bg-gig-panel border border-gig-borderLight rounded-sm shadow-lg z-40 p-1">
                {confirmingReanalyze ? (
                  <div className="px-3 py-2">
                    <p className="text-[11px] text-gig-muted leading-snug mb-1.5">
                      Re-analyze all {tracks.length} track{tracks.length === 1 ? "" : "s"}? BPM, key, energy, waveform and
                      hot cues are recalculated — manual BPM/key edits are overwritten.
                    </p>
                    <div className="flex gap-1.5">
                      <button
                        onClick={() => {
                          setConfirmingReanalyze(false);
                          setShowMaintenance(false);
                          analyzeAll({ all: true });
                        }}
                        className="flex-1 text-[11px] font-bold uppercase px-2 py-1 rounded-sm bg-gig-accent/20 text-gig-accent hover:bg-gig-accent/30"
                      >
                        Re-analyze
                      </button>
                      <button
                        onClick={() => setConfirmingReanalyze(false)}
                        className="flex-1 text-[11px] font-bold uppercase px-2 py-1 rounded-sm text-gig-muted hover:bg-gig-panel2"
                      >
                        Cancel
                      </button>
                    </div>
                  </div>
                ) : (
                  <button
                    disabled={tracks.length === 0 || (!!batchProgress && batchProgress.done < batchProgress.total)}
                    onClick={() => setConfirmingReanalyze(true)}
                    className="w-full text-left px-3 py-2 text-xs font-bold uppercase text-gig-text hover:bg-gig-panel2 disabled:opacity-40"
                  >
                    Re-analyze all tracks
                  </button>
                )}
                <button
                  disabled={normalizing}
                  onClick={async () => {
                    setNormalizing(true);
                    try {
                      await api.normalizeAllGenres();
                      await refresh();
                    } finally {
                      setNormalizing(false);
                      setShowMaintenance(false);
                    }
                  }}
                  className="w-full text-left px-3 py-2 text-xs font-bold uppercase text-gig-text hover:bg-gig-panel2 disabled:opacity-50"
                >
                  {normalizing ? "Cleaning up…" : "Clean up genre tags"}
                </button>
                <button
                  disabled={resetting}
                  onClick={async () => {
                    setResetting(true);
                    try {
                      await resetAllAnalysis();
                    } finally {
                      setResetting(false);
                      setShowMaintenance(false);
                    }
                  }}
                  className="w-full text-left px-3 py-2 text-xs font-bold uppercase text-gig-warn hover:bg-gig-panel2 disabled:opacity-50"
                >
                  {resetting ? "Resetting…" : "Reset all analysis"}
                </button>
                {genreModelDownloaded === false && (
                  <button
                    disabled={downloadingModel}
                    onClick={downloadModel}
                    className="w-full text-left px-3 py-2 text-xs font-bold uppercase text-gig-accent2 hover:bg-gig-panel2 disabled:opacity-50"
                  >
                    {downloadingModel ? "Downloading… (a few minutes)" : "Download AI genre model (~2.5GB)"}
                  </button>
                )}
                <div className="my-1 border-t border-gig-border" />
                {confirmingRemoveAll ? (
                  <div className="px-3 py-2">
                    <p className="text-[11px] text-gig-warn leading-snug mb-1.5">
                      Delete all {tracks.length} track{tracks.length === 1 ? "" : "s"}? This can't be undone.
                    </p>
                    <div className="flex gap-1.5">
                      <button
                        disabled={removingAll}
                        onClick={async () => {
                          setRemovingAll(true);
                          try {
                            await removeAllTracks();
                          } finally {
                            setRemovingAll(false);
                            setConfirmingRemoveAll(false);
                            setShowMaintenance(false);
                          }
                        }}
                        className="flex-1 text-[11px] font-bold uppercase px-2 py-1 rounded-sm bg-gig-warn/20 text-gig-warn hover:bg-gig-warn/30 disabled:opacity-50"
                      >
                        {removingAll ? "Deleting…" : "Delete all"}
                      </button>
                      <button
                        disabled={removingAll}
                        onClick={() => setConfirmingRemoveAll(false)}
                        className="flex-1 text-[11px] font-bold uppercase px-2 py-1 rounded-sm text-gig-muted hover:bg-gig-panel2 disabled:opacity-50"
                      >
                        Cancel
                      </button>
                    </div>
                  </div>
                ) : (
                  <button
                    disabled={tracks.length === 0}
                    onClick={() => setConfirmingRemoveAll(true)}
                    className="w-full text-left px-3 py-2 text-xs font-bold uppercase text-gig-warn hover:bg-gig-panel2 disabled:opacity-40"
                  >
                    Remove all tracks
                  </button>
                )}
              </div>
            )}
          </div>
          <button
            data-tour="import-button"
            onClick={handleImport}
            className="px-3 py-1.5 rounded-sm text-xs font-bold uppercase tracking-wide bg-gig-accent text-black hover:bg-gig-accent/80"
          >
            + Import sound files
          </button>
        </div>
      </div>

      {downloadError && (
        <div className="px-5 py-2 text-xs font-mono bg-gig-warn/10 text-gig-warn border-b border-gig-border flex items-start justify-between gap-3">
          <span className="flex-1">{downloadError}</span>
          <button onClick={() => setDownloadError(null)} className="shrink-0 text-gig-muted hover:text-gig-text">
            ✕
          </button>
        </div>
      )}

      <div className="px-5 py-2.5 border-b border-gig-border bg-gig-panel2 flex items-center gap-2">
        <input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="SEARCH TITLE, ARTIST, ALBUM..."
          className="flex-1 max-w-sm bg-black border border-gig-border rounded-sm px-2.5 py-1 text-sm placeholder:text-gig-dim focus:outline-none"
        />
        <FilterMenu tracks={tracks} filters={libFilters} onChange={setLibFilters} />
      </div>

      <div className="flex-1 overflow-y-auto px-5 pb-6">
        {loading && tracks.length === 0 ? (
          <div className="text-gig-muted text-sm py-10 text-center uppercase tracking-wide">Loading library…</div>
        ) : filtered.length === 0 ? (
          <div className="text-gig-muted text-sm py-20 text-center border border-dashed border-gig-border mt-4">
            {tracks.length === 0 ? (
              <>
                No tracks yet. Import any sound files — MP3, WAV, FLAC, AAC/M4A, OGG, AIFF — to get
                started.
              </>
            ) : activeFilterCount > 0 ? (
              "No tracks match the current filters."
            ) : (
              "No tracks match your search."
            )}
          </div>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left text-gig-muted text-[11px] uppercase tracking-widest border-b border-gig-border sticky top-0 bg-gig-bg">
                <th className="py-2 font-bold w-8"></th>
                <SortableHeader label="Title" sortKey="title" active={sortKey} dir={sortDir} onSort={toggleSort} />
                <SortableHeader label="Genre" sortKey="genre" active={sortKey} dir={sortDir} onSort={toggleSort} />
                <SortableHeader label="Subgenre" sortKey="subgenre" active={sortKey} dir={sortDir} onSort={toggleSort} />
                <SortableHeader label="Time" sortKey="time" active={sortKey} dir={sortDir} onSort={toggleSort} />
                <SortableHeader label="BPM" sortKey="bpm" active={sortKey} dir={sortDir} onSort={toggleSort} />
                <SortableHeader label="Key" sortKey="key" active={sortKey} dir={sortDir} onSort={toggleSort} />
                <SortableHeader label="Energy" sortKey="energy" active={sortKey} dir={sortDir} onSort={toggleSort} />
                <th className="py-2 font-bold w-32"></th>
              </tr>
            </thead>
            <tbody>
              {sorted.map((t) => {
                const isAnalyzing = analyzingIds.has(t.id);
                const deck = playback.decks.find((d) => d.track_id === t.id);
                const isPlayingThis = deck != null;
                return (
                  <tr key={t.id} className="border-b border-gig-border/60 hover:bg-gig-panel2 align-top">
                    <td className="py-2 pr-1 pt-2.5">
                      <button
                        onClick={() => (isPlayingThis ? playback.togglePause() : playback.play(t.id))}
                        className="w-6 h-6 flex items-center justify-center rounded-full border border-gig-borderLight text-gig-text hover:border-gig-accent hover:text-gig-accent text-[11px]"
                        title={isPlayingThis && !playback.paused ? "Pause" : "Play"}
                      >
                        {isPlayingThis && !playback.paused ? "❚❚" : "▶"}
                      </button>
                    </td>
                    <td className="py-2 pr-3">
                      <div className="font-bold text-gig-text truncate max-w-xs">{t.title}</div>
                      <div className="text-[11px] text-gig-muted truncate max-w-xs">{t.artist ?? "—"}</div>
                      <div className="text-[11px] text-gig-muted truncate max-w-xs">{t.album}</div>
                      {isPlayingThis && (
                        <SeekBar
                          position={deck!.position_secs}
                          duration={deck!.duration_secs}
                          onSeek={(secs) => playback.seek(t.id, secs)}
                        />
                      )}
                    </td>
                    <td className="py-2 pr-3">
                      <GenreCell value={t.genre} onSave={(v) => saveField(t.id, "genre", v)} />
                    </td>
                    <td className="py-2 pr-3">
                      <GenreCell value={t.subgenre} onSave={(v) => saveField(t.id, "subgenre", v)} />
                    </td>
                    <td className="py-2 pr-3 text-gig-muted font-mono">{formatDuration(t.duration_secs)}</td>
                    <td className="py-2 pr-3">
                      <BpmBadge bpm={t.bpm} />
                    </td>
                    <td className="py-2 pr-3">
                      <KeyBadge camelot={t.key_camelot} keyName={t.key_name} />
                    </td>
                    <td className="py-2 pr-3">
                      <EnergyGauge energy={t.energy} size={30} />
                    </td>
                    <td className="py-2 text-right space-x-2">
                      {t.bpm == null ? (
                        <button
                          disabled={isAnalyzing}
                          onClick={() => analyze(t.id)}
                          className="text-[11px] font-bold uppercase px-2 py-1 rounded-sm bg-gig-accent2/20 text-gig-accent2 hover:bg-gig-accent2/30 disabled:opacity-50"
                        >
                          {isAnalyzing ? "Analyzing…" : "Analyze"}
                        </button>
                      ) : (
                        <button
                          disabled={isAnalyzing}
                          onClick={() => analyze(t.id)}
                          className="text-[11px] font-bold uppercase px-2 py-1 rounded-sm text-gig-muted hover:text-gig-text"
                        >
                          Re-analyze
                        </button>
                      )}
                      {t.bpm != null && (
                        <button
                          onClick={() => setCueTrack(t)}
                          className="text-[11px] font-bold uppercase px-2 py-1 rounded-sm text-gig-accent3 hover:bg-gig-accent3/15"
                        >
                          Cues
                        </button>
                      )}
                      <button
                        onClick={() => removeTrack(t.id)}
                        className="text-[11px] font-bold uppercase px-2 py-1 rounded-sm text-gig-muted hover:text-gig-warn"
                      >
                        Remove
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      {cueTrack && <HotCueModal track={cueTrack} onClose={() => setCueTrack(null)} />}
    </div>
  );
}

function GenreCell({ value, onSave }: { value: string | null; onSave: (v: string) => void }) {
  const [draft, setDraft] = useState(value ?? "");

  useEffect(() => {
    setDraft(value ?? "");
  }, [value]);

  return (
    <textarea
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => {
        if (draft !== (value ?? "")) onSave(draft);
      }}
      onKeyDown={(e) => {
        // Enter saves/blurs instead of inserting a newline — this is a
        // short label, not a place to actually write multiple lines.
        if (e.key === "Enter") {
          e.preventDefault();
          (e.target as HTMLTextAreaElement).blur();
        }
      }}
      placeholder="—"
      rows={2}
      className="w-24 resize-none bg-transparent border border-transparent hover:border-gig-border focus:border-gig-accent2 focus:bg-black rounded-sm px-1.5 py-0.5 text-sm leading-tight text-gig-muted focus:text-gig-text focus:outline-none"
    />
  );
}

function SeekBar({
  position,
  duration,
  onSeek,
}: {
  position: number;
  duration: number;
  onSeek: (secs: number) => void;
}) {
  const [dragPos, setDragPos] = useState<number | null>(null);
  const shown = dragPos ?? position;
  const pct = duration > 0 ? Math.min(100, Math.max(0, (shown / duration) * 100)) : 0;

  function commit(e: React.SyntheticEvent<HTMLInputElement>) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    setDragPos(null);
    onSeek(v);
  }

  return (
    <div className="mt-1.5 flex items-center gap-2 max-w-xs" onClick={(e) => e.stopPropagation()}>
      <span className="text-[10px] font-mono text-gig-dim w-8 shrink-0">{formatDuration(shown)}</span>
      <input
        type="range"
        min={0}
        max={duration || 0}
        step={0.1}
        value={shown}
        onChange={(e) => setDragPos(parseFloat(e.target.value))}
        onMouseUp={commit}
        onTouchEnd={commit}
        className="seek-range flex-1 cursor-pointer"
        style={{ background: `linear-gradient(to right, #ff2ea6 ${pct}%, #303033 ${pct}%)` }}
      />
      <span className="text-[10px] font-mono text-gig-dim w-8 shrink-0">{formatDuration(duration)}</span>
    </div>
  );
}

