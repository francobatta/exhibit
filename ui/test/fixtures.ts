// Library records for tests: a plain matched film unless told otherwise.

import { expect } from "bun:test";
import type { Item, ItemDetail, ItemSummary } from "@/lib/api";

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
    ...over,
  };
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
