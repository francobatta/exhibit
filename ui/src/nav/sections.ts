// A section's views: Movies is a Spotlight and a wall of every film. The views are named in a row
// under the bar (components/SectionViews), never in the bar itself, and the pages know nothing of
// them: the router puts the row over any page that is one of a section's views.

import type { Route } from "./stack";
import { samePlace } from "./stack";

export interface SectionView {
  id: string;
  label: "sections.spotlight" | "sections.allMovies";
  route: Route;
}

const MOVIES: readonly SectionView[] = [
  { id: "spotlight", label: "sections.spotlight", route: { screen: "spotlight", kind: "movie" } },
  { id: "all", label: "sections.allMovies", route: { screen: "wall", kind: "movie" } },
];

const SECTION_VIEWS: ReadonlyArray<readonly SectionView[]> = [MOVIES];

/** The view after the one `route` shows, wrapping; null when `route` is not one of a section's views. */
export function nextView(route: Route): SectionView | null {
  const views = viewsOf(route);
  if (views === null) return null;
  const at = views.findIndex((v) => samePlace(v.route, route));
  return views[(at + 1) % views.length] ?? null;
}

/** The views of the section `route` is one of, or null when it stands alone. */
export function viewsOf(route: Route): readonly SectionView[] | null {
  return SECTION_VIEWS.find((views) => views.some((v) => samePlace(v.route, route))) ?? null;
}
