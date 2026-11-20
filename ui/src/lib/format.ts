// How numbers, dates and lists read on a wall label, in the window's language. Each returns null
// or "" rather than a broken string.

import { currentDateFormat, language, t, type DateFormat } from "@/i18n";
import type { ItemSummary, ServiceView, Sourced, WatchFact } from "./api";

const finite = (n: number | null | undefined): n is number =>
  typeof n === "number" && Number.isFinite(n);

/** "2 h 42 min", "58 min", "2 h"; null for unknown or zero. */
export function runtime(min: number | null): string | null {
  if (!finite(min) || min <= 0) return null;
  const total = Math.round(min);
  const h = Math.floor(total / 60);
  const m = total % 60;
  if (h === 0) return t("format.minutes", { m });
  return m === 0 ? t("format.hours", { h }) : t("format.hoursMinutes", { h, m });
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

/** A person's initials, for a portrait with no picture: "Dennis Hopper" is "DH". */
export function initials(name: string): string {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .map((w) => w[0])
    .slice(0, 2)
    .join("");
}

/** "S2 E4", "E4", "S2", or "" (in Spanish "T2 E4"). */
export function episodeLabel(s: number | null, e: number | null): string {
  if (finite(s) && finite(e)) return t("format.seasonEpisode", { season: s, episode: e });
  if (finite(s)) return t("format.season", { season: s });
  if (finite(e)) return t("format.episode", { episode: e });
  return "";
}

/**
 * What a work is called on screen. Seasons, and episodes TMDB has no title for, have none of
 * their own: they are named by their numbers ("Season 2", "Specials", "Episode 3").
 */
export function titleOf(item: Pick<ItemSummary, "kind" | "title" | "season" | "episode">): string {
  const own = item.title.trim();
  if (own !== "") return own;
  if (item.kind === "season" && finite(item.season)) {
    return item.season === 0 ? t("work.specials") : t("work.season", { season: item.season });
  }
  if (item.kind === "episode" && finite(item.episode)) return t("work.episode", { episode: item.episode });
  return t("work.untitled");
}

/** "1,214" in English, "1214" or "1.214" in Spanish. */
export function number(n: number): string {
  return new Intl.NumberFormat(language()).format(n);
}

/** "English" for "eng" or "en" (in Spanish, "inglés"); null for no code or one that names no language ("und"). */
export function languageName(code: string | null): string | null {
  const c = code?.trim();
  if (!c) return null;
  try {
    const name = new Intl.DisplayNames([language()], { type: "language" }).of(c);
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

/** Profiles that name a richer format than their codec, most specific first. */
const PROFILES = ["DTS:X", "Dolby Atmos", "DTS-HD MA", "DTS-HD HRA", "DTS-ES", "DTS Express"];

/** A sound track's format as people know it: "DTS-HD MA", "Dolby Atmos", else its codec ("AC-3"). */
export function soundFormat(codecName: string | null, profile: string | null): string | null {
  return PROFILES.find((p) => profile?.includes(p)) ?? codec(codecName);
}

/** "Mono", "Stereo", "5.1", "7.1"; null for none. */
export function channels(n: number | null): string | null {
  if (!finite(n) || n <= 0) return null;
  if (n === 1) return t("format.mono");
  if (n === 2) return t("format.stereo");
  return n >= 6 ? `${n - 1}.1` : t("format.channels", { count: n });
}

/**
 * A date as the owner chose to read dates: following the language ("November 20, 2026",
 * "20 de noviembre de 2026"), day first ("20 November 2026"), month first ("November 20, 2026")
 * or numeric ("2026-11-20"). Month names are always in the window's language. Without `year`,
 * the year is left out (numeric dates always keep it). `format` writes it another way than the
 * owner's, as Settings shows each choice.
 */
export function writeDate(
  date: Date,
  { year = true, utc = false, format = currentDateFormat() }: { year?: boolean; utc?: boolean; format?: DateFormat } = {},
): string {
  const lang = language();
  const timeZone = utc ? "UTC" : undefined;
  if (format === "numeric") {
    const y = utc ? date.getUTCFullYear() : date.getFullYear();
    const m = (utc ? date.getUTCMonth() : date.getMonth()) + 1;
    const d = utc ? date.getUTCDate() : date.getDate();
    return `${y}-${String(m).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
  }
  if (format === "auto") {
    return date.toLocaleDateString(lang, { day: "numeric", month: "long", ...(year && { year: "numeric" }), timeZone });
  }
  const parts = new Intl.DateTimeFormat(lang, { day: "numeric", month: "long", year: "numeric", timeZone }).formatToParts(date);
  const part = (type: Intl.DateTimeFormatPartTypes) => parts.find((p) => p.type === type)?.value ?? "";
  const values = { day: part("day"), month: part("month"), year: part("year") };
  if (format === "day") return t(year ? "format.dayFirst" : "format.dayFirstThisYear", values);
  return t(year ? "format.monthFirst" : "format.monthFirstThisYear", values);
}

/** "12 November 2026", in the owner's date format. */
export function day(unixSeconds: number): string | null {
  if (!finite(unixSeconds) || unixSeconds <= 0) return null;
  return writeDate(new Date(unixSeconds * 1000));
}

/**
 * "3 March" this year, "3 March 2019" before it. With `utc`, the time is a day as a service gives
 * it (midnight UTC), read as that day wherever the window is.
 */
export function shortDay(unixSeconds: number, { utc = false }: { utc?: boolean } = {}): string | null {
  if (!finite(unixSeconds) || unixSeconds <= 0) return null;
  const date = new Date(unixSeconds * 1000);
  const year = utc ? date.getUTCFullYear() : date.getFullYear();
  return writeDate(date, { year: year !== new Date().getFullYear(), utc });
}

/** A rating on its own scale as stars, to the nearest half: 4.5 of 5 is "★★★★½"; "" for none. */
export function stars(value: number, scale: number): string {
  if (!finite(value) || !finite(scale) || value <= 0 || scale <= 0) return "";
  const halves = Math.round(Math.min(value, scale) * 2);
  return "★".repeat(Math.floor(halves / 2)) + (halves % 2 === 1 ? "½" : "");
}

/** "4.2 GB" ("4,2 GB" in Spanish), "700 MB"; null for nothing. */
export function bytes(n: number): string | null {
  if (!finite(n) || n <= 0) return null;
  const units = [t("format.bytes"), "KB", "MB", "GB", "TB"];
  let v = n;
  let u = 0;
  while (v >= 1000 && u < units.length - 1) {
    v /= 1000;
    u++;
  }
  const digits = v >= 100 || u === 0 ? 0 : 1;
  const shown = new Intl.NumberFormat(language(), { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(v);
  return `${shown} ${units[u]}`;
}

/** "English", "English and French", "English, French, and Dutch" ("inglés, francés y neerlandés"). */
export function listed(parts: string[]): string {
  return new Intl.ListFormat(language(), { style: "long", type: "conjunction" }).format(parts);
}

/** How far through a started, unwatched work is (0..1), or null: nothing to show as progress. */
export function started(watched: boolean, progress: number | null): number | null {
  return !watched && progress !== null && Number.isFinite(progress) && progress > 0 ? Math.min(1, progress) : null;
}

/** How sure a match candidate is, in a word: "Likely" from 0.8, "Possible" from 0.6, else "Unlikely". */
export function confidence(score: number): string {
  return score >= 0.8 ? t("match.likely") : score >= 0.6 ? t("match.possible") : t("match.unlikely");
}

/** "English or French", "inglés, francés o neerlandés". */
export function either(parts: string[]): string {
  return new Intl.ListFormat(language(), { style: "long", type: "disjunction" }).format(parts);
}

/** The service id of what Exhibit holds itself. */
export const EXHIBIT = "exhibit";

/** A service's name as its adapter gives it, or its id while the services are not known. */
export const serviceName = (id: string, services: ServiceView[] | undefined) => services?.find((s) => s.id === id)?.name ?? id;

/** A watch a service logged: "Logged on Letterboxd 3 March 2019", or only where when it has no day. */
export function loggedLine(watch: Sourced<WatchFact>, services: ServiceView[] | undefined): string {
  const service = serviceName(watch.service, services);
  // A service's day is midnight UTC: read as that day wherever the window is.
  const date = watch.on === null ? null : shortDay(watch.on, { utc: true });
  return date ? t("activity.loggedOn", { service, date }) : t("activity.loggedOnUndated", { service });
}
