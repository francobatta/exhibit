// A show in two seasons for screen tests: show 100, season 1 (101) with episodes 11 and 12,
// season 2 (102) with episodes 21 and 22. The first episode is watched.

import { tauri } from "./setup";
import { detail, summary } from "./fixtures";
import type { Id, ItemSummary } from "@/lib/api";

export const SHOW = 100;

const episode = (id: Id, season: number, n: number, over: Partial<ItemSummary> = {}) =>
  summary({ id, kind: "episode", title: `Episode ${n}`, season, episode: n, show_title: "Severance", runtime_min: 55, path: null, ...over });
const season = (id: Id, n: number) => summary({ id, kind: "season", title: `Season ${n}`, season: n, path: null });

export const EPISODES: Record<Id, { parent: Id; summary: ItemSummary }> = {
  11: { parent: 101, summary: episode(11, 1, 1, { watched: true }) },
  12: { parent: 101, summary: episode(12, 1, 2) },
  21: { parent: 102, summary: episode(21, 2, 1) },
  22: { parent: 102, summary: episode(22, 2, 2) },
};

/** Answers `item` for the show, its seasons and its episodes; `next` is the show's next episode. */
export function fakeShow(next: Id) {
  tauri.handle("status", () => ({ movies: 0, shows: 1, other: 0, needs_review: 0, unprobed: 0, sources: [], tmdb_key: true, agent_key: false, tools: [] }));
  tauri.handle("item", (args) => {
    const id = args.id as Id;
    if (id === SHOW) {
      return detail(
        { children: [season(101, 1), season(102, 2)], next_episode: EPISODES[next]!.summary },
        { id: SHOW, kind: "show", title: "Severance", year: 2022 },
      );
    }
    if (id === 101 || id === 102) {
      const children = Object.values(EPISODES).filter((e) => e.parent === id).map((e) => e.summary);
      return detail({ children }, { id, kind: "season", parent_id: SHOW, title: `Season ${id - 100}` });
    }
    const e = EPISODES[id]!;
    return detail({}, { id, kind: "episode", parent_id: e.parent, title: e.summary.title });
  });
}
