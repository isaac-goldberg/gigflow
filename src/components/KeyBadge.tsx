import { camelotColor } from "../lib/camelot";

export function KeyBadge({ camelot, keyName }: { camelot: string | null; keyName?: string | null }) {
  return (
    <span
      className="inline-flex items-center justify-center min-w-[2.75rem] px-1.5 py-0.5 rounded-sm text-xs font-mono font-bold text-black border border-black/30"
      style={{ backgroundColor: camelotColor(camelot) }}
      title={keyName ?? undefined}
    >
      {camelot ?? "--"}
    </span>
  );
}

export function BpmBadge({ bpm }: { bpm: number | null }) {
  return (
    <span className="inline-flex items-center justify-center min-w-[3.75rem] px-1.5 py-0.5 rounded-sm text-xs font-mono font-bold bg-black border border-gig-border text-gig-accent">
      {bpm ? bpm.toFixed(1) : "--.--"}
    </span>
  );
}

const ENERGY_SEGMENTS = 5;

export function EnergyMeter({ energy }: { energy: number | null }) {
  const filled = energy == null ? 0 : Math.round(energy * ENERGY_SEGMENTS);
  const segColor = (i: number) => {
    if (i >= filled) return "#28282b";
    if (i < 2) return "#2e7bff";
    if (i < 4) return "#9bff1f";
    return "#ff3b3b";
  };
  return (
    <span className="inline-flex items-center gap-[2px]" title={energy != null ? `Energy ${Math.round(energy * 100)}%` : "Energy unknown"}>
      {Array.from({ length: ENERGY_SEGMENTS }).map((_, i) => (
        <span key={i} className="block w-[5px] h-3 rounded-[1px]" style={{ backgroundColor: segColor(i) }} />
      ))}
    </span>
  );
}
