// Interpolated color: blue at low energy, green in the middle, red at high
// energy — smoothly blended, not hard bands, with green centered around the
// 33-66% zone as the reference point.
const BLUE: [number, number, number] = [46, 123, 255]; // gig-accent3
const GREEN: [number, number, number] = [92, 230, 110];
const RED: [number, number, number] = [255, 59, 59]; // gig-warn

function lerp(a: number, b: number, t: number) {
  return a + (b - a) * t;
}

function energyColor(pct: number): string {
  const clamped = Math.max(0, Math.min(1, pct));
  const [from, to, t] =
    clamped <= 0.5 ? [BLUE, GREEN, clamped / 0.5] : [GREEN, RED, (clamped - 0.5) / 0.5];
  const r = Math.round(lerp(from[0], to[0], t));
  const g = Math.round(lerp(from[1], to[1], t));
  const b = Math.round(lerp(from[2], to[2], t));
  return `rgb(${r}, ${g}, ${b})`;
}

/// Arc path for the filled portion of the ring: starts at the top (12
/// o'clock) and sweeps counter-clockwise (top -> left -> bottom -> right),
/// so 25% ends at the left, 50% is the left half-circle, 75% ends at the
/// right/horizontal, 100% is a full circle.
function describeArc(cx: number, cy: number, r: number, pct: number): string {
  if (pct <= 0 || pct >= 1) return "";
  const toRad = (d: number) => (d * Math.PI) / 180;
  const startAngle = -90;
  const endAngle = -90 - pct * 360;
  const sx = cx + r * Math.cos(toRad(startAngle));
  const sy = cy + r * Math.sin(toRad(startAngle));
  const ex = cx + r * Math.cos(toRad(endAngle));
  const ey = cy + r * Math.sin(toRad(endAngle));
  const largeArc = pct > 0.5 ? 1 : 0;
  return `M ${sx} ${sy} A ${r} ${r} 0 ${largeArc} 0 ${ex} ${ey}`;
}

export function EnergyGauge({ energy, size = 34 }: { energy: number | null; size?: number }) {
  if (energy == null) {
    return (
      <span
        className="inline-flex items-center justify-center rounded-full border border-gig-border text-gig-dim font-mono"
        style={{ width: size, height: size, fontSize: size * 0.28 }}
        title="Energy unknown"
      >
        --
      </span>
    );
  }

  const pct = Math.max(0, Math.min(1, energy));
  const color = energyColor(pct);
  const r = size / 2 - 2.5;
  const cx = size / 2;
  const cy = size / 2;

  return (
    <span
      className="inline-flex items-center justify-center relative shrink-0"
      style={{ width: size, height: size }}
      title={`Energy ${Math.round(pct * 100)}%`}
    >
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
        <circle cx={cx} cy={cy} r={r} fill="none" stroke="#242427" strokeWidth={2.5} />
        {pct >= 1 ? (
          <circle cx={cx} cy={cy} r={r} fill="none" stroke={color} strokeWidth={2.5} />
        ) : pct > 0 ? (
          <path d={describeArc(cx, cy, r, pct)} fill="none" stroke={color} strokeWidth={2.5} strokeLinecap="round" />
        ) : null}
      </svg>
      <span
        className="absolute inset-0 flex items-center justify-center font-mono font-bold"
        style={{ color, fontSize: size * 0.3 }}
      >
        {Math.round(pct * 100)}
      </span>
    </span>
  );
}
