// Map each of the 12 Camelot wheel positions to a hue, so compatible keys
// (adjacent numbers) read as visually adjacent colors.
export function camelotColor(camelot: string | null): string {
  if (!camelot) return "#3a3a4a";
  const match = camelot.match(/^(\d+)([AB])$/);
  if (!match) return "#3a3a4a";
  const num = parseInt(match[1], 10);
  const isMinor = match[2] === "A";
  const hue = ((num - 1) / 12) * 360;
  const sat = isMinor ? 70 : 85;
  const light = isMinor ? 48 : 58;
  return `hsl(${hue}, ${sat}%, ${light}%)`;
}

export function camelotRelation(a: string | null, b: string | null): string {
  if (!a || !b) return "unknown";
  const pa = a.match(/^(\d+)([AB])$/);
  const pb = b.match(/^(\d+)([AB])$/);
  if (!pa || !pb) return "unknown";
  const [na, la] = [parseInt(pa[1], 10), pa[2]];
  const [nb, lb] = [parseInt(pb[1], 10), pb[2]];
  if (na === nb && la === lb) return "same key";
  if (na === nb) return "relative major/minor";
  const adjacent = nb === (na % 12) + 1 || na === (nb % 12) + 1;
  if (la === lb && adjacent) return "adjacent (perfect mix)";
  if (adjacent) return "energy shift";
  return "clash";
}

