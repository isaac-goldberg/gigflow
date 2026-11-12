import { useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
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

/**
 * A transition window drawn on one track's timeline. "out": the outgoing
 * track blends away from `start` and is gone by `end` (everything after is
 * never heard). "in": the incoming track enters at `start` and is fully in
 * by `end` (everything before is skipped).
 */
export interface StripZone {
  kind: "out" | "in";
  start: number;
  end: number;
  /** Short text drawn on the strip, e.g. "MIX OUT". */
  label: string;
  /** Longer hover text (e.g. which track it blends with). */
  detail?: string;
  /** Exact (engine-fitted) rather than estimated — drawn solid. */
  exact?: boolean;
  /** The blend is happening right now. */
  live?: boolean;
}

const MARKER_WIDTH = 5;
const SNAP_PX = 10;

/** Builds a mirrored top/bottom polygon outline from (min, max) peak pairs — one SVG shape regardless of point count, so this stays cheap even with hundreds of buckets. */
function waveformPolygonPoints(waveform: [number, number][]): string {
  const n = waveform.length;
  if (n === 0) return "";
  const x = (i: number) => (n === 1 ? 50 : (i / (n - 1)) * 100);
  const clamp = (v: number) => Math.max(-1, Math.min(1, v));
  const top = waveform.map(([, hi], i) => `${x(i)},${50 - clamp(hi) * 48}`);
  const bottom = waveform.map(([lo], i) => `${x(i)},${50 - clamp(lo) * 48}`).reverse();
  return [...top, ...bottom].join(" ");
}

export function HotCueStrip({
  duration,
  cues,
  height = 16,
  onCueClick,
  interactive = false,
  position = 0,
  onSeek,
  waveform = null,
  zones = [],
}: {
  duration: number | null;
  cues: HotCue[];
  height?: number;
  onCueClick?: (cue: HotCue) => void;
  /** When true, renders a draggable position thumb that snaps to nearby cues. */
  interactive?: boolean;
  position?: number;
  onSeek?: (secs: number) => void;
  /** (min, max) amplitude per time bucket — draws a real waveform behind the cue markers, Rekordbox/DAW-style. */
  waveform?: [number, number][] | null;
  zones?: StripZone[];
}) {
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [dragPos, setDragPos] = useState<number | null>(null);
  const barRef = useRef<HTMLDivElement>(null);

  if (!duration || duration <= 0) {
    return <div className="w-full bg-black border border-gig-border" style={{ height }} />;
  }

  const shown = dragPos ?? position;
  const pct = Math.min(100, Math.max(0, (shown / duration) * 100));

  function secsFromClientX(clientX: number): number {
    const rect = barRef.current!.getBoundingClientRect();
    const frac = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
    let secs = frac * duration!;
    const snapSecs = (SNAP_PX / rect.width) * duration!;
    let nearest: HotCue | null = null;
    let nearestDist = Infinity;
    for (const c of cues) {
      const d = Math.abs(c.position_secs - secs);
      if (d < nearestDist) {
        nearestDist = d;
        nearest = c;
      }
    }
    if (nearest && nearestDist <= snapSecs) secs = nearest.position_secs;
    return secs;
  }

  function handlePointerDown(e: ReactPointerEvent<HTMLDivElement>) {
    if (!interactive || !onSeek) return;
    e.stopPropagation();
    setDragPos(secsFromClientX(e.clientX));
    function onMove(ev: PointerEvent) {
      setDragPos(secsFromClientX(ev.clientX));
    }
    function onUp(ev: PointerEvent) {
      const finalSecs = secsFromClientX(ev.clientX);
      setDragPos(null);
      onSeek!(finalSecs);
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    }
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }

  return (
    <div
      ref={barRef}
      onPointerDown={handlePointerDown}
      className={`relative w-full bg-black border border-gig-border ${interactive ? "cursor-pointer" : ""}`}
      style={{ height }}
    >
      {waveform && waveform.length > 0 && (
        <svg className="absolute inset-0 w-full h-full pointer-events-none" viewBox="0 0 100 100" preserveAspectRatio="none">
          <polygon points={waveformPolygonPoints(waveform)} className="fill-gig-dim/60" />
        </svg>
      )}
      {zones.map((z, i) => {
        const pctOf = (t: number) => Math.min(100, Math.max(0, (t / duration) * 100));
        const left = pctOf(Math.min(z.start, z.end));
        const right = pctOf(Math.max(z.start, z.end));
        const width = Math.max(right - left, 0);
        // Cyan hatched band with horizontal brackets: reads as a *range*,
        // unlike the solid, multi-colour vertical bars of hot cues.
        const rgb = "34, 211, 238";
        const alpha = z.live ? 0.5 : 0.32;
        // Keep the label inside the strip: anchor it to whichever side of
        // the zone has room.
        const anchorLeft = z.kind === "out" ? left <= 78 : right < 22;
        const labelStyle = anchorLeft
          ? { left: `${z.kind === "out" ? left : right}%` }
          : { right: `${100 - (z.kind === "out" ? left : right)}%` };
        return (
          <div key={`zone-${i}`} className="pointer-events-none">
            {/* The part of the track that's never heard in the mix. */}
            {z.kind === "out" ? (
              <div className="absolute top-0 bottom-0 bg-black/60" style={{ left: `${right}%`, right: 0 }} />
            ) : (
              <div className="absolute top-0 bottom-0 bg-black/60" style={{ left: 0, width: `${left}%` }} />
            )}
            <div
              className="absolute top-0 bottom-0"
              style={{
                left: `${left}%`,
                width: width > 0 ? `${width}%` : 3,
                minWidth: 3,
                backgroundImage: `repeating-linear-gradient(135deg, rgba(${rgb}, ${alpha}) 0 3px, rgba(${rgb}, ${alpha * 0.25}) 3px 6px)`,
                borderTop: `2px ${z.exact ? "solid" : "dashed"} rgb(${rgb})`,
                borderBottom: `2px ${z.exact ? "solid" : "dashed"} rgb(${rgb})`,
                borderLeft: z.kind === "out" ? `2px solid rgb(${rgb})` : undefined,
                borderRight: z.kind === "in" ? `2px solid rgb(${rgb})` : undefined,
              }}
              title={z.detail ?? z.label}
            />
            <div
              className="absolute top-[3px] text-[9px] leading-none font-extrabold uppercase tracking-wider text-black px-1 py-[2px] whitespace-nowrap rounded-sm"
              style={{ ...labelStyle, backgroundColor: `rgb(${rgb})` }}
            >
              {z.label}
            </div>
          </div>
        );
      })}
      {interactive && (
        <div className="absolute top-0 bottom-0 left-0 bg-gig-accent3/25 pointer-events-none" style={{ width: `${pct}%` }} />
      )}
      {cues.map((c) => {
        const left = Math.min(99, Math.max(0, (c.position_secs / duration) * 100));
        const color = cueColor(c.pad);
        return (
          <div key={c.id} className="absolute top-0 bottom-0" style={{ left: `${left}%` }}>
            <button
              onClick={(e) => {
                e.stopPropagation();
                onCueClick?.(c);
              }}
              onPointerDown={(e) => e.stopPropagation()}
              onMouseEnter={() => setHoveredId(c.id)}
              onMouseLeave={() => setHoveredId(null)}
              className="absolute top-0 bottom-0"
              style={{ width: MARKER_WIDTH, marginLeft: -MARKER_WIDTH / 2, backgroundColor: color }}
            />
            {hoveredId === c.id && (
              <div
                className="absolute bottom-full mb-1.5 -translate-x-1/2 whitespace-nowrap bg-black border border-gig-borderLight rounded-sm px-2 py-1 text-[10px] font-bold text-gig-text z-50 pointer-events-none shadow-lg"
                style={{ left: 0 }}
              >
                {padLetter(c.pad)} · {c.label} · {formatClock(c.position_secs)}
              </div>
            )}
          </div>
        );
      })}
      {interactive && (
        <div
          className="absolute rounded-full bg-gig-accent3 border-2 border-black pointer-events-none"
          style={{ left: `${pct}%`, top: "50%", width: 12, height: 12, transform: "translate(-50%, -50%)" }}
        />
      )}
    </div>
  );
}

export function formatClock(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  const ms = Math.floor((secs % 1) * 10);
  return `${m}:${s.toString().padStart(2, "0")}.${ms}`;
}
