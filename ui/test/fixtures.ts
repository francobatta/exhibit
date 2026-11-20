// Library records for tests: a plain matched film unless told otherwise.

import { expect } from "bun:test";
import type { Activity, AudioTrack, Item, ItemDetail, ItemSummary, MediaFile, ServiceView, SubTrack } from "@/lib/api";

export function summary(over: Partial<ItemSummary> = {}): ItemSummary {
  return {
    id: 1,
    kind: "movie",
    title: "Taxi Driver",
    year: 1976,
    season: null,
    episode: null,
    show_title: null,
    poster: null,
    backdrop: null,
    runtime_min: 114,
    width: 1920,
    height: 1080,
    hdr: null,
    watched: false,
    progress: null,
    loved: false,
    available: true,
    match_state: "matched",
    genres: [],
    path: "Films/Taxi Driver.mkv",
    in_library: true,
    ...over,
  };
}

export function fullItem(over: Partial<Item> = {}): Item {
  return {
    id: 1,
    kind: "movie",
    parent_id: null,
    title: "Taxi Driver",
    year: 1976,
    season: null,
    episode: null,
    overview: null,
    runtime_min: 114,
    genres: [],
    people: [],
    poster: null,
    backdrop: null,
    tmdb_id: 103,
    match_state: "matched",
    match_source: "lookup",
    confidence: 0.9,
    candidates: [],
    loved: false,
    added_at: 0,
    ...over,
  };
}

export function detail(over: Partial<ItemDetail> = {}, item: Partial<Item> = {}): ItemDetail {
  return {
    item: fullItem(item),
    files: [],
    play: null,
    children: [],
    available: true,
    next_episode: null,
    activity: activity(),
    ...over,
  };
}

/** A file of the work, not yet probed. A film with none is one the owner does not have. */
export function mediaFile(over: Partial<MediaFile> = {}): MediaFile {
  return { id: 1, source_id: 1, rel_path: "Films/Taxi Driver.mkv", size: 4_200_000_000, mtime: 0, item_id: 1, tech: null, ...over };
}

/** `n` films with ids from 1. */
export function films(n: number): ItemSummary[] {
  return Array.from({ length: n }, (_, i) => summary({ id: i + 1, title: `Film ${i + 1}` }));
}

export const LONG =
  "The Assassination of Jesse James by the Coward Robert Ford, as Told in Ninety Characters!!";
export const CJK = "千と千尋の神隠し";

/** Fails on text a wall label must never show: "undefined", "null", "NaN", empty list slots, a dangling comma. */
export function clean(text: string): void {
  for (const bad of ["undefined", "null", "NaN", ", ,", ",,"]) expect(text).not.toContain(bad);
  const t = text.trim();
  expect(t.startsWith(",")).toBe(false);
  expect(t.endsWith(",")).toBe(false);
}

export function audioTrack(over: Partial<AudioTrack> = {}): AudioTrack {
  return {
    codec: "ac3",
    channels: 6,
    lang: "eng",
    profile: null,
    title: null,
    default: false,
    original: false,
    commentary: false,
    ...over,
  };
}

export function subTrack(over: Partial<SubTrack> = {}): SubTrack {
  return { codec: "subrip", lang: "eng", forced: false, hearing_impaired: false, ...over };
}

/** What the owner has done with a work: nothing, unless told otherwise. */
export function activity(over: Partial<Activity> = {}): Activity {
  return {
    watched: false,
    loved: false,
    watched_choice: null,
    loved_choice: null,
    ratings: [],
    reviews: [],
    watches: [],
    liked_by: [],
    watchlisted_by: [],
    ...over,
  };
}

/** Letterboxd as the app lists it, switched on and connected unless told otherwise. */
export function service(state: Partial<ServiceView["state"]> = {}, over: Partial<ServiceView> = {}): ServiceView {
  return {
    id: "letterboxd",
    name: "Letterboxd",
    markSvg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 10"><circle cx="5" cy="5" r="4"/></svg>',
    provides: ["activity"],
    needs: ["username", "export_file"],
    exportPage: "https://letterboxd.com/settings/data/",
    state: {
      service: "letterboxd",
      enabled: true,
      username: "ana",
      export_name: null,
      export_at: null,
      synced_at: null,
      error: null,
      unmatched: 0,
      ...state,
    },
    ...over,
  };
}
