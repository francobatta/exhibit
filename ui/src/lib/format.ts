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

/** A frame this wide (or tall) is 4K, whatever it is cropped to. */
export const UHD = { width: 3200, height: 2000 };

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
    height >= UHD.height || w >= UHD.width
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

const languages = new Intl.DisplayNames(["en"], { type: "language" });

/** "English" for "eng" or "en"; null for no code or one that names no language ("und"). */
export function language(code: string | null): string | null {
  const c = code?.trim();
  if (!c) return null;
  try {
    const name = languages.of(c);
    return name === undefined || name.toLowerCase() === c.toLowerCase() ? null : name;
  } catch {
    return null;
  }
}

const CODECS: Record<string, string> = {
  hevc: "HEVC",
  h264: "H.264",
  av1: "AV1",
  vp9: "VP9",
  mpeg2video: "MPEG-2",
  truehd: "TrueHD",
  eac3: "E-AC-3",
  ac3: "AC-3",
  dts: "DTS",
  aac: "AAC",
  flac: "FLAC",
  opus: "Opus",
  mp3: "MP3",
};

/** A codec as people know it: "HEVC", "TrueHD"; an unknown one in capitals. */
export function codec(name: string | null): string | null {
  const c = name?.trim().toLowerCase();
  if (!c) return null;
  return CODECS[c] ?? c.toUpperCase();
}

/** "Mono", "Stereo", "5.1", "7.1"; null for none. */
export function channels(n: number | null): string | null {
  if (!finite(n) || n <= 0) return null;
  if (n === 1) return "Mono";
  if (n === 2) return "Stereo";
  return n >= 6 ? `${n - 1}.1` : `${n} channels`;
}

/** "12 November 2026". */
export function day(unixSeconds: number): string | null {
  if (!finite(unixSeconds) || unixSeconds <= 0) return null;
  return new Date(unixSeconds * 1000).toLocaleDateString("en-GB", { day: "numeric", month: "long", year: "numeric" });
}

/** "3 March" this year, "3 March 2019" before it. */
export function shortDay(unixSeconds: number): string | null {
  if (!finite(unixSeconds) || unixSeconds <= 0) return null;
  const date = new Date(unixSeconds * 1000);
  const thisYear = date.getFullYear() === new Date().getFullYear();
  return date.toLocaleDateString("en-GB", { day: "numeric", month: "long", ...(thisYear ? {} : { year: "numeric" }) });
}

/** "4.2 GB", "700 MB"; null for nothing. */
export function bytes(n: number): string | null {
  if (!finite(n) || n <= 0) return null;
  const units = ["bytes", "KB", "MB", "GB", "TB"];
  let v = n;
  let u = 0;
  while (v >= 1000 && u < units.length - 1) {
    v /= 1000;
    u++;
  }
  return `${v >= 100 || u === 0 ? Math.round(v) : v.toFixed(1)} ${units[u]}`;
}

/** "English", "English and French", "English, French and Dutch". */
export function listed(parts: string[]): string {
  if (parts.length <= 1) return parts.join("");
  return `${parts.slice(0, -1).join(", ")} and ${parts.at(-1)}`;
}

/** How a placard and a work name the watched state and a missing drive. */
export const WATCHED = "Watched";
export const OFFLINE = "Offline";

/** How far through a started, unwatched work is (0..1), or null: nothing to show as progress. */
export function started(watched: boolean, progress: number | null): number | null {
  return !watched && progress !== null && Number.isFinite(progress) && progress > 0 ? Math.min(1, progress) : null;
}
