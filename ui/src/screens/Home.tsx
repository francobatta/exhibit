// Home: any page the owner chooses (the movies' Spotlight, a wall), rendered as the router renders
// it, with its own state kept here. None is chosen yet, so Home is a quiet, empty wall.

import { useState } from "react";
import { screenFor } from "@/nav/Screens";
import type { Route } from "@/nav/stack";

/** The page Home shows; null for none. A setting will choose it. */
export const HOME_PAGE: Route | null = null;

export function Home() {
  const [page, setPage] = useState(HOME_PAGE);
  if (page === null) return <div aria-hidden className="absolute inset-0" />;
  return screenFor(page, setPage);
}
