// How numbers and lists read on a wall label. Each returns null or "" rather than a broken string.

const finite = (n: number | null | undefined): n is number =>
  typeof n === "number" && Number.isFinite(n);

/** "2 h 42 min", "58 min", "2 h"; null for unknown or zero. */
export function runtime(min: number | null): string | null {
  if (!finite(min) || min <= 0) return null;
  const total = Math.round(min);
  const h = Math.floor(total / 60);
  const m = total % 60;
  if (h === 0) return `${m} min`;
  return m === 0 ? `${h} h` : `${h} h ${m} min`;
}

/** "1:04:12", "4:05". */
export function clock(seconds: number): string {
  const total = finite(seconds) && seconds > 0 ? Math.floor(seconds) : 0;
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

/**
 * "4K", "1080p", "720p", "SD", with HDR after it ("4K Dolby Vision"). The width, when given, names
 * a frame cropped to a wide ratio (3840x1608 is 4K, not 1080p).
 */
export function resolution(
  height: number | null,
  hdr: string | null,
  width?: number | null,
): string | null {
  if (!finite(height) || height <= 0) return null;
  const w = finite(width) ? width : 0;
  const name =
    height >= 2000 || w >= 3200
      ? "4K"
      : height >= 1000 || w >= 1800
        ? "1080p"
        : height >= 700 || w >= 1200
          ? "720p"
          : "SD";
  const extra = hdr?.trim();
  return extra ? `${name} ${extra}` : name;
}

const EMPTY = new Set(["", "null", "undefined", "nan"]);

/** The present parts joined with ", ". */
export function facts(parts: Array<string | null | undefined>): string {
  return parts
    .map((p) => (typeof p === "string" ? p.trim().replace(/^,+|,+$/g, "").trim() : ""))
    .filter((p) => !EMPTY.has(p.toLowerCase()))
    .join(", ");
}

/** "S2 E4", "E4", "S2", or "". */
export function episodeLabel(s: number | null, e: number | null): string {
  const season = finite(s) ? `S${s}` : null;
  const episode = finite(e) ? `E${e}` : null;
  return [season, episode].filter((p) => p !== null).join(" ");
}

/** "1 file", "214 files". */
export function count(n: number, one: string, many: string): string {
  return `${n.toLocaleString("en-GB")} ${n === 1 ? one : many}`;
}

/** How a placard and a work name the watched state and a missing drive. */
export const WATCHED = "Watched";
export const OFFLINE = "Offline";

/** How far through a started, unwatched work is (0..1), or null: nothing to show as progress. */
export function started(watched: boolean, progress: number | null): number | null {
  return !watched && progress !== null && Number.isFinite(progress) && progress > 0 ? Math.min(1, progress) : null;
}
