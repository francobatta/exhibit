// The navigation stack. Top-level sections replace the root; everything else stacks on it, and
// each entry remembers which element had focus so Back returns to it.

import { create } from "zustand";
import type { WallView } from "@/components/WallControls";
import type { Id } from "@/lib/api";

export type Route =
  | { screen: "home" }
  | { screen: "wall"; kind: "movie" | "show" | "other"; view?: WallView }
  | { screen: "show"; id: Id }
  | { screen: "details"; id: Id }
  | { screen: "review" }
  | { screen: "settings" }
  | { screen: "firstRun" };

/** Whether two routes are the same place, whatever state they carry (a wall's sort and filters). */
export const samePlace = (a: Route, b: Route) =>
  a.screen === b.screen && (a.screen !== "wall" || (b.screen === "wall" && a.kind === b.kind));

export interface Entry {
  /** Unique per visit, for React keys and focus memory. */
  id: number;
  route: Route;
  /** What had focus when the owner left this entry; restored on return. */
  focusKey: string | null;
}

export interface NavState {
  entries: Entry[];
  current: Route;
  /** The focus key of the current entry. */
  focusKey: string | null;
  /** How the stack last moved, for the screen transition. */
  direction: "push" | "back" | "root";
  push(route: Route): void;
  /** False at the root, where there is nothing to go back to. */
  back(): boolean;
  replaceRoot(route: Route): void;
  /** Remembers what has focus on the current entry. */
  setFocusKey(key: string | null): void;
  /** Replaces the route of entry `id`, keeping its place and focus (a wall's sort and filters). */
  setRoute(id: number, route: Route): void;
}

let nextId = 1;
const entry = (route: Route): Entry => ({ id: nextId++, route, focusKey: null });

function settle(entries: Entry[], direction: NavState["direction"]) {
  const top = entries[entries.length - 1]!;
  return { entries, current: top.route, focusKey: top.focusKey, direction };
}

export const useNav = create<NavState>()((set, get) => ({
  ...settle([entry({ screen: "home" })], "root"),

  push: (route) => set((s) => settle([...s.entries, entry(route)], "push")),

  back: () => {
    const { entries } = get();
    if (entries.length <= 1) return false;
    set(settle(entries.slice(0, -1), "back"));
    return true;
  },

  replaceRoot: (route) => set(settle([entry(route)], "root")),

  setFocusKey: (key) =>
    set((s) => {
      const entries = s.entries.slice();
      const top = entries[entries.length - 1]!;
      entries[entries.length - 1] = { ...top, focusKey: key };
      return { entries, focusKey: key };
    }),

  setRoute: (id, route) =>
    set((s) => {
      const entries = s.entries.map((e) => (e.id === id ? { ...e, route } : e));
      return { entries, current: entries[entries.length - 1]!.route };
    }),
}));
