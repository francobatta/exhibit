// TMDB's logo, as its brand rules allow: one of its approved logos (the "Alt short" wordmark,
// `assets/brands/tmdb.svg`, unmodified), its proportions kept, drawn in white, never another
// colour, never flipped or rotated. It names TMDB as the source of the library's details.

import type { CSSProperties } from "react";
import tmdb from "@/assets/brands/tmdb.svg";
import { cn } from "@/lib/utils";

/** The logo's width over its height, from its viewBox (273.42 × 35.52). */
const RATIO = 273.42 / 35.52;

export function TmdbLogo({ className }: { className?: string }) {
  return (
    <span
      role="img"
      aria-label="The Movie Database (TMDB)"
      data-brand="tmdb"
      className={cn("inline-block h-(--tmdb-logo) shrink-0 bg-(--brand-white) [mask:var(--art)_center/contain_no-repeat]", className)}
      style={{ "--art": `url("${tmdb}")`, aspectRatio: RATIO } as CSSProperties}
    />
  );
}
