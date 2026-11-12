import { usePlayback } from "../store/playback";
import { useLibrary } from "../store/library";
import { formatDuration } from "../lib/api";

/// Small persistent now-playing widget, pinned to the bottom-left corner of
/// the window so playback stays controllable from any view. During a
/// transition it shows both tracks; the seek bar always drives the main
/// (outgoing) deck.
export function MiniPlayer() {
  const { decks, transition, paused, loadingTrackId, arming, togglePause, stop, seek } = usePlayback();
  const tracks = useLibrary((s) => s.tracks);
  const titleOf = (id: string) => tracks.find((t) => t.id === id)?.title ?? "—";

  if (loadingTrackId && decks.length === 0) {
    return (
      <div className="fixed bottom-3 left-3 z-40 w-72 bg-gig-panel border border-gig-borderLight rounded-sm shadow-lg px-3 py-2.5 flex items-center gap-2.5">
        <span className="w-3 h-3 rounded-full border-2 border-gig-accent3/30 border-t-gig-accent3 animate-spin shrink-0" />
        <span className="text-xs font-bold text-gig-muted truncate">Loading {titleOf(loadingTrackId)}…</span>
      </div>
    );
  }

  const main = decks.find((d) => d.role === "main");
  if (!main) return null;
  const incoming = decks.find((d) => d.role === "incoming");
  const mainTrack = tracks.find((t) => t.id === main.track_id);

  const title = incoming ? `${titleOf(main.track_id)} → ${titleOf(incoming.track_id)}` : mainTrack?.title ?? "—";
  let subtitle = mainTrack?.artist ?? "—";
  if (incoming) subtitle = "Mixing";
  else if (arming) subtitle = "Preparing next transition…";
  else if (transition?.state === "pending") subtitle = `Next: ${titleOf(transition.zone.to_track_id)} at ${formatDuration(transition.zone.a_start_secs)}`;

  const position = main.position_secs;
  const duration = main.duration_secs;
  const pct = duration > 0 ? Math.min(100, Math.max(0, (position / duration) * 100)) : 0;

  return (
    <div className="fixed bottom-3 left-3 z-40 w-72 bg-gig-panel border border-gig-borderLight rounded-sm shadow-lg">
      <div className="flex items-center gap-2 px-3 py-2">
        <button
          onClick={() => togglePause()}
          className="w-7 h-7 shrink-0 flex items-center justify-center rounded-full border border-gig-borderLight text-gig-text hover:border-gig-accent hover:text-gig-accent text-xs"
          title={paused ? "Play (space)" : "Pause (space)"}
        >
          {paused ? "▶" : "❚❚"}
        </button>
        <div className="flex-1 min-w-0">
          <div className="text-xs font-bold truncate">{title}</div>
          <div className={`text-[10px] truncate ${incoming ? "text-gig-accent2" : "text-gig-muted"}`}>{subtitle}</div>
        </div>
        <button
          onClick={() => stop()}
          className="shrink-0 text-xs font-bold text-gig-muted hover:text-gig-warn px-1"
          title="Stop"
        >
          ■
        </button>
      </div>
      <div className="px-2.5 pb-2 flex items-center gap-2">
        <span className="text-[10px] font-mono text-gig-dim w-8 shrink-0">{formatDuration(position)}</span>
        <input
          type="range"
          min={0}
          max={duration || 0}
          step={0.1}
          value={position}
          onChange={(e) => seek(main.track_id, parseFloat(e.target.value))}
          className="seek-range flex-1 cursor-pointer"
          style={{ background: `linear-gradient(to right, #ff2ea6 ${pct}%, #303033 ${pct}%)` }}
        />
        <span className="text-[10px] font-mono text-gig-dim w-8 shrink-0">{formatDuration(duration)}</span>
      </div>
    </div>
  );
}
