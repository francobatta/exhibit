// The navigation stack. Top-level sections replace the root; everything else stacks on it, and
// each entry remembers which element had focus so Back returns to it.

import { create } from "zustand";
import type { Filters, Id, Sort } from "@/lib/api";

export type WallKind = "movie" | "show" | "other";

/** What the owner has asked of a wall; kept in its route, so Back finds it as left. */
export interface WallView {
  /** None chosen: the library's own order, titles A to Z. */
  sort: Sort | null;
  descending: boolean;
  /** Only what is set: an empty list or a missing field leaves the wall whole. */
  filters: Filters;
}

export const DEFAULT_VIEW: WallView = { sort: null, descending: false, filters: {} };

/** The part of Settings to land on: the tools (mpv could not start), the TMDB key (Fix match without one). */
export type SettingsPart = "tools" | "tmdb";

export type Route =
  | { screen: "home" }
  | { screen: "wall"; kind: WallKind; view?: WallView }
  | { screen: "spotlight"; kind: WallKind }
  | { screen: "show"; id: Id }
  | { screen: "details"; id: Id }
  | { screen: "review" }
  | { screen: "settings"; part?: SettingsPart }
  | { screen: "firstRun" };

/** Whether two routes are the same place, whatever state they carry (a wall's sort and filters). */
export const samePlace = (a: Route, b: Route) =>
  a.screen === b.screen && (!("kind" in a) || ("kind" in b && a.kind === b.kind));

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
  /** Back to the root as it was left, past every screen stacked on it. */
  toRoot(): void;
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

  toRoot: () => {
    const { entries } = get();
    if (entries.length > 1) set(settle(entries.slice(0, 1), "back"));
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

/** Whether first run is showing: its own flow, which sections, Settings and Ask stay out of. */
export const onFirstRun = () => useNav.getState().entries[0]!.route.screen === "firstRun";

// Ask is an overlay over whatever screen is showing, not a place in the stack.

interface AskOverlay {
  open: boolean;
  /** What the field starts with: a key typed on the top bar's field opens Ask with it. */
  seed: string;
  /** Runs once the overlay has closed and focus is back where it was. */
  then: (() => void) | null;
}

export const useAskOverlay = create<AskOverlay>()(() => ({ open: false, seed: "", then: null }));

export function openAsk(seed = ""): void {
  if (onFirstRun()) return;
  useAskOverlay.setState({ open: true, seed, then: null });
}

/** Closes Ask; `then` runs once it has closed, so a screen it opens takes focus from the page. */
export function closeAsk(then?: () => void): void {
  useAskOverlay.setState({ open: false, then: then ?? null });
}

/** Runs `go` (a move to another screen) now, or once Ask has closed if it is open. */
export function leaveAsk(go: () => void): void {
  if (useAskOverlay.getState().open) closeAsk(go);
  else go();
}
