// Home's programme: which shelves it shows at this moment, in order, and which one work it puts
// forward. The part of the day sets the shelves (short ones in the morning and after midnight; the
// watchlist and the season's films in the afternoon and evening), the weekend adds a long film,
// and the season adds one shelf of its own. The pick is the first suitable work on the first shelf
// that has one, varied by day so the same film does not stand there every evening.

import type { ItemSummary, SearchQuery, Shelf } from "./api";
import type { Daypart, Moment, Season } from "./moment";

export type RowId = "continue" | "watchlist" | "long" | "season" | "short" | "acclaimed" | "recent" | "loved";

export type RowSource = { shelf: Shelf; limit: number } | { query: SearchQuery };

export interface ProgrammeRow {
  id: RowId;
  source: RowSource;
}

/** Genre names as TMDB gives them in English and Spanish; the library holds one language's, and the match ignores case. */
const SEASON_GENRES: Record<Season, string[]> = {
  autumn: ["Horror", "Mystery", "Thriller", "Terror", "Misterio", "Suspense", "Suspenso"],
  winter: ["Drama", "History", "War", "Historia", "Bélica"],
  spring: ["Comedy", "Romance", "Animation", "Family", "Comedia", "Animación", "Familia"],
  summer: ["Adventure", "Action", "Science Fiction", "Fantasy", "Aventura", "Acción", "Ciencia ficción", "Fantasía"],
};

/** Films not seen yet, the public's favourites first. */
const unseen = (filters: SearchQuery): RowSource => ({
  query: { kind: "movie", seen: ["unwatched"], sort: "rating", descending: true, limit: 30, ...filters },
});

export function rowSource(id: RowId, moment: Moment): RowSource {
  switch (id) {
    case "continue":
      return { shelf: "continue_watching", limit: 20 };
    case "watchlist":
      return { shelf: "watchlist", limit: 30 };
    case "recent":
      return { shelf: "recently_added", limit: 30 };
    case "loved":
      return { shelf: "loved", limit: 30 };
    case "short":
      return unseen({ runtime_max: 100 });
    case "long":
      return unseen({ runtime_min: 150 });
    case "acclaimed":
      return unseen({});
    case "season":
      return unseen({ genres: SEASON_GENRES[moment.season], any_genre: true, ...(moment.season === "winter" ? { runtime_min: 120 } : {}) });
  }
}

const ROWS: Record<Daypart, RowId[]> = {
  morning: ["continue", "short", "recent"],
  afternoon: ["continue", "watchlist", "season", "recent"],
  evening: ["continue", "watchlist", "season", "acclaimed", "loved"],
  late: ["continue", "short", "loved"],
};

/** The shelves for `moment`, in order. The watchlist only while a service brings the owner's activity in. */
export function programme(moment: Moment, activity: boolean): ProgrammeRow[] {
  const ids = ROWS[moment.daypart].filter((id) => activity || id !== "watchlist");
  // A long film after the watchlist, when there is time for one.
  if (moment.weekend && (moment.daypart === "afternoon" || moment.daypart === "evening")) {
    ids.splice(ids.indexOf("season"), 0, "long");
  }
  return ids.map((id) => ({ id, source: rowSource(id, moment) }));
}

/** What Home shows: films and shows, and a show's episodes on Continue watching; never Other. */
export function onHome(item: ItemSummary, row: RowId): boolean {
  if (row === "continue") return item.kind !== "other";
  // Only the watchlist films the owner has: Home puts forward what can be watched tonight.
  return (item.kind === "movie" || item.kind === "show") && item.in_library;
}

/** Shelves that never give the pick: what is new or loved is there to browse, not to put forward. */
const NOT_PICKED: ReadonlySet<RowId> = new Set(["recent", "loved"]);

/**
 * The work Home puts forward, and the shelf it came from. Something to resume comes first, as it
 * is. Elsewhere one of a shelf's first three, chosen by the day, so the pick changes from evening
 * to evening and holds through one. A work that can play now beats one on a drive that is away.
 */
export function pick(rows: Array<{ id: RowId; items: ItemSummary[] }>, moment: Moment): { item: ItemSummary; row: RowId } | null {
  const order = DAYPARTS.indexOf(moment.daypart);
  for (const playable of [true, false]) {
    for (const row of rows) {
      if (NOT_PICKED.has(row.id) && rows.some((r) => !NOT_PICKED.has(r.id) && r.items.length > 0)) continue;
      const fit = row.items.filter((i) => !i.watched && (!playable || i.available));
      if (fit.length === 0) continue;
      const at = row.id === "continue" ? 0 : (moment.day * DAYPARTS.length + order) % Math.min(3, fit.length);
      return { item: fit[at]!, row: row.id };
    }
  }
  const first = rows.find((r) => r.items.length > 0);
  return first ? { item: first.items[0]!, row: first.id } : null;
}

const DAYPARTS: Daypart[] = ["morning", "afternoon", "evening", "late"];
