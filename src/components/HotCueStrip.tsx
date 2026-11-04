import type { HotCue } from "../types";

export const CUE_COLORS = [
  "#ff3b3b",
  "#ff8a1f",
  "#ffd93b",
  "#9bff1f",
  "#1fffa3",
  "#2e7bff",
  "#9b5cff",
  "#ff2ea6",
];

export function cueColor(pad: number): string {
  return CUE_COLORS[(pad - 1) % CUE_COLORS.length];
}

export function padLetter(pad: number): string {
  return String.fromCharCode(64 + pad);
}

export function HotCueStrip({
  duration,
  cues,
  height = 16,
  onCueClick,
}: {
  duration: number | null;
  cues: HotCue[];
  height?: number;
  onCueClick?: (cue: HotCue) => void;
}) {
  if (!duration || duration <= 0) {
    return <div className="w-full bg-black border border-gig-border" style={{ height }} />;
  }
  return (
    <div className="relative w-full bg-black border border-gig-border" style={{ height }}>
      {cues.map((c) => {
        const left = Math.min(99, Math.max(0, (c.position_secs / duration) * 100));
        const color = cueColor(c.pad);
        return (
          <button
            key={c.id}
            onClick={(e) => {
              e.stopPropagation();
              onCueClick?.(c);
            }}
            title={`${padLetter(c.pad)} · ${c.label} · ${formatClock(c.position_secs)}`}
            className="absolute top-0 bottom-0"
            style={{ left: `${left}%`, width: 3, backgroundColor: color }}
          />
        );
      })}
    </div>
  );
}

export function formatClock(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  const ms = Math.floor((secs % 1) * 10);
  return `${m}:${s.toString().padStart(2, "0")}.${ms}`;
}
