import { camelotColor } from "../lib/camelot";

export function KeyBadge({ camelot, keyName }: { camelot: string | null; keyName?: string | null }) {
  return (
    <span
      className="inline-flex items-center justify-center min-w-[3.1rem] px-1.5 py-0.5 rounded-sm text-sm font-mono font-bold text-black border border-black/30"
      style={{ backgroundColor: camelotColor(camelot) }}
      title={keyName ?? undefined}
    >
      {camelot ?? "--"}
    </span>
  );
}

export function BpmBadge({ bpm }: { bpm: number | null }) {
  return (
    <span className="inline-flex items-center justify-center min-w-[4.1rem] px-1.5 py-0.5 rounded-sm text-sm font-mono font-bold bg-black border border-gig-border text-gig-accent">
      {bpm ? bpm.toFixed(1) : "--.--"}
    </span>
  );
}
