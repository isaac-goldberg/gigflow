import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useLibrary } from "../store/library";
import { formatDuration } from "../lib/api";
import { KeyBadge, BpmBadge, EnergyMeter } from "../components/KeyBadge";
import { HotCueModal } from "../components/HotCueModal";
import type { Track } from "../types";

const AUDIO_EXTENSIONS = ["mp3", "wav", "flac", "ogg", "m4a", "aac", "aif", "aiff", "alac"];

export function LibraryView() {
  const { tracks, loading, analyzingIds, refresh, importFiles, analyze, analyzeAll, removeTrack } =
    useLibrary();
  const [filter, setFilter] = useState("");
  const [cueTrack, setCueTrack] = useState<Track | null>(null);

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
  const filtered = tracks.filter((t) => {
    const q = filter.toLowerCase();
    return (
      t.title.toLowerCase().includes(q) ||
      (t.artist ?? "").toLowerCase().includes(q) ||
      (t.album ?? "").toLowerCase().includes(q)
    );
  });

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between h-14 px-5 border-b border-gig-border bg-gig-panel">
        <div className="flex items-baseline gap-3">
          <h1 className="text-xs font-bold uppercase tracking-widest text-gig-text">Library</h1>
          <span className="label-xs">
            {tracks.length} track{tracks.length === 1 ? "" : "s"}
            {unanalyzedCount > 0 && ` · ${unanalyzedCount} need analysis`}
          </span>
        </div>
        <div className="flex gap-2">
          {unanalyzedCount > 0 && (
            <button
              onClick={() => analyzeAll()}
              className="px-3 py-1.5 rounded-sm text-[11px] font-bold uppercase tracking-wide bg-gig-panel3 border border-gig-borderLight hover:border-gig-accent2 text-gig-text"
            >
              Analyze all ({unanalyzedCount})
            </button>
          )}
          <button
            onClick={handleImport}
            className="px-3 py-1.5 rounded-sm text-[11px] font-bold uppercase tracking-wide bg-gig-accent text-black hover:bg-gig-accent/80"
          >
            + Import sound files
          </button>
        </div>
      </div>

      <div className="px-5 py-2.5 border-b border-gig-border bg-gig-panel2">
        <input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="SEARCH TITLE, ARTIST, ALBUM..."
          className="w-full max-w-sm bg-black border border-gig-border rounded-sm px-2.5 py-1 text-xs placeholder:text-gig-dim focus:outline-none"
        />
      </div>

      <div className="flex-1 overflow-y-auto px-5 pb-6">
        {loading && tracks.length === 0 ? (
          <div className="text-gig-muted text-xs py-10 text-center uppercase tracking-wide">Loading library…</div>
        ) : filtered.length === 0 ? (
          <div className="text-gig-muted text-xs py-20 text-center border border-dashed border-gig-border mt-4">
            {tracks.length === 0 ? (
              <>
                No tracks yet. Import any sound files — MP3, WAV, FLAC, AAC/M4A, OGG, AIFF — to get
                started.
              </>
            ) : (
              "No tracks match your search."
            )}
          </div>
        ) : (
          <table className="w-full text-xs">
            <thead>
              <tr className="text-left text-gig-dim text-[10px] uppercase tracking-widest border-b border-gig-border sticky top-0 bg-gig-bg">
                <th className="py-2 font-bold">Title</th>
                <th className="py-2 font-bold">Artist</th>
                <th className="py-2 font-bold">Time</th>
                <th className="py-2 font-bold">BPM</th>
                <th className="py-2 font-bold">Key</th>
                <th className="py-2 font-bold">Energy</th>
                <th className="py-2 font-bold w-32"></th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((t) => {
                const isAnalyzing = analyzingIds.has(t.id);
                return (
                  <tr key={t.id} className="border-b border-gig-border/60 hover:bg-gig-panel2">
                    <td className="py-2 pr-3">
                      <div className="font-bold text-gig-text truncate max-w-xs">{t.title}</div>
                      <div className="text-[10px] text-gig-dim truncate max-w-xs">{t.album}</div>
                    </td>
                    <td className="py-2 pr-3 text-gig-muted">{t.artist ?? "—"}</td>
                    <td className="py-2 pr-3 text-gig-muted font-mono">{formatDuration(t.duration_secs)}</td>
                    <td className="py-2 pr-3">
                      <BpmBadge bpm={t.bpm} />
                    </td>
                    <td className="py-2 pr-3">
                      <KeyBadge camelot={t.key_camelot} keyName={t.key_name} />
                    </td>
                    <td className="py-2 pr-3">
                      <EnergyMeter energy={t.energy} />
                    </td>
                    <td className="py-2 text-right space-x-2">
                      {t.bpm == null ? (
                        <button
                          disabled={isAnalyzing}
                          onClick={() => analyze(t.id)}
                          className="text-[10px] font-bold uppercase px-2 py-1 rounded-sm bg-gig-accent2/20 text-gig-accent2 hover:bg-gig-accent2/30 disabled:opacity-50"
                        >
                          {isAnalyzing ? "Analyzing…" : "Analyze"}
                        </button>
                      ) : (
                        <button
                          disabled={isAnalyzing}
                          onClick={() => analyze(t.id)}
                          className="text-[10px] font-bold uppercase px-2 py-1 rounded-sm text-gig-muted hover:text-gig-text"
                        >
                          Re-analyze
                        </button>
                      )}
                      {t.bpm != null && (
                        <button
                          onClick={() => setCueTrack(t)}
                          className="text-[10px] font-bold uppercase px-2 py-1 rounded-sm text-gig-accent3 hover:bg-gig-accent3/15"
                        >
                          Cues
                        </button>
                      )}
                      <button
                        onClick={() => removeTrack(t.id)}
                        className="text-[10px] font-bold uppercase px-2 py-1 rounded-sm text-gig-muted hover:text-gig-warn"
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
