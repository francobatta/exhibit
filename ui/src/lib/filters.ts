// A wall's sort and filters as the owner sees them: which choices exist, what they are called, how
// a set of filters reads in one line, and the query they make.

import { t } from "@/i18n";
import type { Filters, ItemKind, Picture, Resolution, SearchQuery, Seen, Sort, Sound } from "./api";
import { either, languageName, runtime } from "./format";

export const SORTS: Sort[] = ["title", "year", "added", "runtime", "played"];
export const SEEN: Seen[] = ["unwatched", "started", "watched"];
export const RESOLUTIONS: Resolution[] = ["uhd", "full_hd", "hd", "sd"];
export const PICTURES: Picture[] = ["dolby_vision", "hdr10", "hlg", "sdr"];
export const SOUNDS: Sound[] = ["atmos", "dts_x", "lossless", "surround"];

/** The direction a sort takes first: titles A to Z, runtimes shortest first, dates newest first. */
export const descends = (sort: Sort) => sort === "year" || sort === "added" || sort === "played" || sort === "my_rating";

const ORDER = { title: "alpha", year: "time", added: "time", played: "time", runtime: "length", my_rating: "rating" } as const;

/** "Oldest first", "Z to A": the order `sort` takes `descending` or not. */
export function direction(sort: Sort, descending: boolean): string {
  return t(`sorts.${ORDER[sort]}.${descending ? "desc" : "asc"}`);
}

export const sortName = (sort: Sort) => t(`sorts.${sort}`);
export const seenName = (seen: Seen) => t(`filters.seen.${seen}`);

const BRANDS = { uhd: "4K", full_hd: "1080p", hd: "720p", sd: "SD", dolby_vision: "Dolby Vision", hdr10: "HDR10", hlg: "HLG", sdr: "SDR", atmos: "Dolby Atmos", dts_x: "DTS:X" } as const;

/** A format's name: its brand where it has one ("Dolby Vision", "4K"), else in words ("Lossless"). */
export function formatName(value: Resolution | Picture | Sound): string {
  return value === "lossless" || value === "surround" ? t(`filters.sound.${value}`) : BRANDS[value];
}

/** A track language's name ("Spanish" for "es"), else the tag itself. */
export const langName = (tag: string) => languageName(tag) ?? tag;

/** Only what is set, so two views asking the same thing are one query. */
export function compact(f: Filters): Filters {
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(f)) {
    if (value === null || value === undefined || value === false) continue;
    if (Array.isArray(value) && value.length === 0) continue;
    out[key] = value;
  }
  if (out.genres === undefined) delete out.any_genre;
  return out as Filters;
}

/** How many kinds of filter are in force: genres count once, as does each range. */
export function filterCount(f: Filters): number {
  const c = compact(f);
  const set = (...keys: Array<keyof Filters>) => keys.some((k) => c[k] !== undefined);
  return [
    set("genres", "without_genres"),
    set("year_min", "year_max"),
    set("runtime_min", "runtime_max"),
    set("seen"),
    set("loved"),
    set("available"),
    set("resolution"),
    set("picture"),
    set("sound"),
    set("audio"),
    set("subtitles"),
  ].filter(Boolean).length;
}

/** `list` with `value` added, or taken out when it is there. */
export function toggled<T>(list: T[] | undefined, value: T): T[] {
  const now = list ?? [];
  return now.includes(value) ? now.filter((v) => v !== value) : [...now, value];
}

export type GenreState = "off" | "in" | "out";

export function genreState(f: Filters, genre: string): GenreState {
  if (f.genres?.includes(genre)) return "in";
  return f.without_genres?.includes(genre) ? "out" : "off";
}

/** A genre's next state, A after A: wanted, then left out, then neither. */
export function cycleGenre(f: Filters, genre: string): Filters {
  const genres = (f.genres ?? []).filter((g) => g !== genre);
  const without = (f.without_genres ?? []).filter((g) => g !== genre);
  const next = { off: "in", in: "out", out: "off" }[genreState(f, genre)];
  return {
    ...f,
    genres: next === "in" ? [...genres, genre] : genres,
    without_genres: next === "out" ? [...without, genre] : without,
  };
}

/** "1970–1989", "From 1970", "Up to 1989"; null for no bounds. */
export function span(min: number | null | undefined, max: number | null | undefined, name: (v: number) => string): string | null {
  const lo = min ?? null;
  const hi = max ?? null;
  if (lo === null && hi === null) return null;
  if (lo === null) return t("filters.upTo", { value: name(hi!) });
  if (hi === null) return t("filters.from", { value: name(lo) });
  return lo === hi ? name(lo) : `${name(lo)} – ${name(hi)}`;
}

export const minutes = (m: number) => runtime(m) ?? "0";

/** The filters in force, each as a few words: "Drama and Crime", "1970 – 1989", "Spanish audio". */
export function describe(f: Filters): string[] {
  const c = compact(f);
  const parts: Array<string | null> = [];
  if (c.genres) parts.push(c.any_genre ? either(c.genres) : c.genres.join(" + "));
  if (c.without_genres) parts.push(t("filters.without", { genres: either(c.without_genres) }));
  parts.push(span(c.year_min, c.year_max, String), span(c.runtime_min, c.runtime_max, minutes));
  if (c.seen) parts.push(either(c.seen.map(seenName)));
  if (c.loved) parts.push(t("filters.loved"));
  if (c.available) parts.push(t("filters.available"));
  for (const formats of [c.resolution, c.picture, c.sound]) {
    if (formats) parts.push(either(formats.map(formatName)));
  }
  if (c.audio) parts.push(t("filters.audioIn", { languages: either(c.audio.map(langName)) }));
  if (c.subtitles) parts.push(t("filters.subtitlesIn", { languages: either(c.subtitles.map(langName)) }));
  return parts.filter((p): p is string => p !== null);
}

export interface View {
  sort: Sort | null;
  descending: boolean;
  filters: Filters;
}

/** The library query for a wall: only what differs from the defaults. Other hangs by folder, unsorted. */
export function wallQuery(kind: ItemKind, view: View, sorted = true): SearchQuery {
  return {
    kind,
    ...compact(view.filters),
    ...(sorted && view.sort !== null && view.sort !== "title" && { sort: view.sort }),
    ...(sorted && view.sort !== null && view.descending !== descends(view.sort) && { descending: view.descending }),
  };
}
