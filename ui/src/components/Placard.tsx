// The wall label: the title, then "year, director", then "runtime, resolution and HDR, watched
// state", then a line or two of description. Plain text, commas between facts, every line built
// with `format.facts` so a missing fact leaves no trace. A line with nothing to say is left out.

import type { Item, ItemDetail, ItemSummary } from "@/lib/api";
import { episodeLabel, facts, resolution, runtime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { FitTitle } from "./FitTitle";

export interface PlacardLines {
  title: string;
  credit: string;
  facts: string;
  overview: string | null;
}

const isSummary = (item: ItemSummary | Item): item is ItemSummary => "available" in item;
const yearOf = (y: number | null | undefined) => (typeof y === "number" && y > 0 ? String(y) : null);

/** The placard's text, from whatever is known: a summary, an item, a detail. */
export function placardLines(item: ItemSummary | Item, detail?: ItemDetail): PlacardLines {
  const full = detail?.item ?? (isSummary(item) ? null : item);
  const summary = isSummary(item) ? item : null;
  const kind = full?.kind ?? item.kind;

  const director = full?.people.find((p) => p.role === "director")?.name ?? null;
  const year = yearOf(full?.year ?? item.year);
  const credit =
    kind === "episode"
      ? facts([summary?.show_title, episodeLabel(full?.season ?? item.season, full?.episode ?? item.episode), year])
      : facts([year, director]);

  const tech = detail?.files.find((f) => f.tech !== null)?.tech ?? null;
  const picture = tech
    ? resolution(tech.height, tech.hdr, tech.width)
    : summary
      ? resolution(summary.height, summary.hdr, summary.width)
      : null;

  const minutes = full?.runtime_min ?? item.runtime_min;
  const play = detail?.play ?? null;
  const watched = play?.watched ?? summary?.watched ?? false;
  let left: number | null = null;
  if (!watched && play !== null && play.position_s > 0 && play.duration_s > play.position_s) {
    left = (play.duration_s - play.position_s) / 60;
  } else if (!watched && summary?.progress && minutes) {
    left = minutes * (1 - summary.progress);
  }
  const leftText = left === null ? null : runtime(Math.max(1, Math.round(left)));
  const available = detail?.available ?? summary?.available ?? true;

  return {
    title: (full?.title ?? item.title).trim() || "Untitled",
    credit,
    facts: facts([
      runtime(minutes),
      picture,
      watched ? "Watched" : leftText === null ? null : `${leftText} left`,
      available ? null : "Offline",
    ]),
    overview: full?.overview?.trim() || null,
  };
}

export interface PlacardProps {
  item: ItemSummary | Item;
  detail?: ItemDetail;
  /** `hero` on Home and Details, over the backdrop; `panel` beside a wall. */
  size: "hero" | "panel";
  className?: string;
}

export function Placard({ item, detail, size, className }: PlacardProps) {
  const lines = placardLines(item, detail);
  const hero = size === "hero";
  return (
    <div className={cn("flex w-full flex-col", hero ? "max-w-measure gap-5" : "gap-3", className)}>
      <div data-line="title">
        <FitTitle
          text={lines.title}
          maxLines={2}
          className={cn(
            "font-display text-label",
            hero ? "text-hero font-title tracking-display" : "text-title font-title tracking-title",
          )}
        />
      </div>
      {(lines.credit || lines.facts) && (
        <div className={cn("flex flex-col font-display text-label-dim", hero ? "gap-1 text-lead" : "gap-0.5 text-body")}>
          {lines.credit && (
            <p data-line="credit" className="truncate">
              {lines.credit}
            </p>
          )}
          {lines.facts && (
            <p data-line="facts" className="truncate">
              {lines.facts}
            </p>
          )}
        </div>
      )}
      {lines.overview && (
        <p
          data-line="overview"
          className={cn(
            "line-clamp-2 font-reading leading-reading text-label-dim",
            hero ? "text-lead" : "text-body",
          )}
        >
          {lines.overview}
        </p>
      )}
    </div>
  );
}
