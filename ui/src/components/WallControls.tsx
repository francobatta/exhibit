// The row above a wall: its order, the filters in force in a line, and how many works they leave.
// Up from the wall reaches it; it is one focus group. A sort is chosen with A, A again turns its
// order round, and a third A clears it (the library's own order, titles A to Z); the arrow beside
// it shows which way it runs. Filters opens the
// sheet with every choice. While several works are being chosen it becomes what to do with them.

import { useState, type ReactNode } from "react";
import { FilterSheet } from "@/components/FilterSheet";
import { Lamp } from "@/components/Lamp";
import { Button } from "@/components/ui/button";
import { Toggle } from "@/components/ui/toggle";
import { FocusGroup, useFocusItem } from "@/focus/react";
import { t, useT } from "@/i18n";
import { hasActivity } from "@/lib/actions";
import type { Sort } from "@/lib/api";
import { SORTS, describe, descends, direction, filterCount, sortName } from "@/lib/filters";
import { facts } from "@/lib/format";
import { useServices } from "@/lib/queries";
import { cn } from "@/lib/utils";
import type { WallKind, WallView } from "@/nav/stack";

/**
 * The orders a wall offers: none on Other, which hangs by folder; on Movies also the owner's own
 * rating, while a service brings it (`activity`).
 */
export function sorts(kind: WallKind, activity = false): Sort[] {
  if (kind === "other") return [];
  return kind === "movie" && activity ? [...SORTS, "my_rating"] : SORTS;
}

export const clearFilters = (view: WallView): WallView => ({ ...view, filters: {} });

/** "412 movies", "1 show", "30 items". */
export function countLine(kind: WallKind, n: number): string {
  const unit = ({ movie: "units.movie", show: "units.show", other: "units.item" } as const)[kind];
  return t(unit, { count: n });
}

/** "96 films on your watchlist, 71 not in your library": the watchlist view, the ones the owner does not have counted apart. */
export function watchlistLine(n: number, wanted: number): string {
  return facts([t("activity.watchlistCount", { count: n }), wanted > 0 ? t("activity.notInLibraryCount", { count: wanted }) : null]);
}

const control = "relative focus-visible:bg-transparent focus:text-label";

function SortToggle({ sort, view, onView }: { sort: Sort; view: WallView; onView: (view: WallView) => void }) {
  const t = useT();
  const on = view.sort === sort;
  const way = on ? view.descending : descends(sort);
  // Three presses: the sort in its own order, the other order, then none (the library's own).
  const reversed = on && view.descending !== descends(sort);
  const next: WallView = !on
    ? { ...view, sort, descending: descends(sort) }
    : reversed
      ? { ...view, sort: null, descending: false }
      : { ...view, descending: !view.descending };
  // A names what it does.
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({
    key: `wall:sort:${sort}`,
    hint: !on ? t("hints.sort") : reversed ? t("hints.unsort") : direction(sort, !view.descending),
  });
  return (
    <Toggle
      ref={ref}
      {...props}
      variant="plain"
      pressed={on}
      onPressedChange={() => onView(next)}
      aria-label={on ? `${sortName(sort)}, ${direction(sort, way)}` : sortName(sort)}
      className={cn(control, "gap-1.5")}
    >
      {sortName(sort)}
      <svg
        viewBox="0 0 12 12"
        aria-hidden
        fill="none"
        data-up={way ? undefined : ""}
        className={cn(
          "size-[0.8em] transition-[opacity,rotate] duration-(--dur-control) data-up:rotate-180",
          on ? "opacity-100" : "opacity-0",
        )}
      >
        <path d="M6 2v8M2.5 6.5 6 10l3.5-3.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
      {focused && <Lamp />}
    </Toggle>
  );
}

function ControlButton({
  focusKey,
  onSelect,
  onBack,
  children,
}: {
  focusKey: string;
  onSelect: () => void;
  onBack?: () => void;
  children: ReactNode;
}) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: focusKey, onSelect, onBack });
  return (
    <Button ref={ref} {...props} className={cn(control, "gap-2")}>
      {children}
      {focused && <Lamp />}
    </Button>
  );
}

function FilterIcon() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden fill="none" className="size-[1em]">
      <path d="M2 4h12M4.5 8h7M7 12h2" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
    </svg>
  );
}

const caption = "font-display text-body text-label-faint";

/** Several works are chosen: what can be done to them all at once. B or Done ends it. */
export interface WallSelection {
  count: number;
  onMark: (watched: boolean) => void;
  onDone: () => void;
}

export interface WallControlsProps {
  kind: WallKind;
  view: WallView;
  onView: (view: WallView) => void;
  /** How many works the view leaves; null until every page of them has arrived. */
  total: number | null;
  /** How many of them the owner does not have (the watchlist view lists those too). */
  wanted?: number;
  selection?: WallSelection;
  className?: string;
}

export function WallControls({ kind, view, onView, total, wanted = 0, selection, className }: WallControlsProps) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const offered = sorts(kind, hasActivity(useServices().data));
  // The last count holds, dimmed, while a changed view is counted, so the number never blinks out.
  const line =
    total === null ? null : kind === "movie" && view.filters.watchlist ? watchlistLine(total, wanted) : countLine(kind, total);
  const [counted, setCounted] = useState(line);
  if (line !== null && line !== counted) setCounted(line);
  const count = filterCount(view.filters);
  const summary = describe(view.filters).join(" · ");
  return (
    <>
      <FocusGroup
        id="wall-controls"
        orientation="horizontal"
        aria-label={selection ? t("wall.chosen") : t("wall.sortAndFilter")}
        className={cn("flex h-(--wall-controls) items-center gap-1 px-edge", className)}
      >
        {selection ? (
          <>
            <p className="mr-6 font-display text-body font-ui text-label tabular-nums">{t("wall.selected", { count: selection.count })}</p>
            <ControlButton focusKey="wall:select:watched" onSelect={() => selection.onMark(true)} onBack={selection.onDone}>
              {t("actions.markWatched")}
            </ControlButton>
            <ControlButton focusKey="wall:select:unwatched" onSelect={() => selection.onMark(false)} onBack={selection.onDone}>
              {t("actions.markUnwatched")}
            </ControlButton>
            <ControlButton focusKey="wall:select:done" onSelect={selection.onDone} onBack={selection.onDone}>
              {t("common.done")}
            </ControlButton>
          </>
        ) : (
          <>
            {offered.length > 0 && (
              <>
                <span className={cn(caption, "mr-3")}>{t("wall.sort")}</span>
                <div role="group" aria-label={t("wall.sort")} className="contents">
                  {offered.map((s) => (
                    <SortToggle key={s} sort={s} view={view} onView={onView} />
                  ))}
                </div>
                <span aria-hidden className="mx-5 h-5 w-px shrink-0 bg-line" />
              </>
            )}
            <ControlButton focusKey="wall:filters" onSelect={() => setOpen(true)}>
              <FilterIcon />
              {t("wall.filters")}
              {count > 0 && (
                <span className="inline-flex size-[1.4em] items-center justify-center rounded-full bg-label text-caption leading-none text-wall tabular-nums">
                  {count}
                </span>
              )}
            </ControlButton>
            {count > 0 && (
              <>
                <p className={cn(caption, "mx-3 min-w-0 truncate text-label-dim")}>{summary}</p>
                <ControlButton focusKey="wall:clear" onSelect={() => onView(clearFilters(view))}>
                  {t("wall.clearFilters")}
                </ControlButton>
              </>
            )}
          </>
        )}
        <p role="status" className={cn(caption, "ml-auto shrink-0 pl-6 tabular-nums transition-opacity", total === null && "opacity-50")}>
          {counted}
        </p>
      </FocusGroup>
      <FilterSheet
        open={open}
        kind={kind}
        filters={view.filters}
        onFilters={(filters) => onView({ ...view, filters })}
        count={counted}
        counting={total === null}
        onClose={() => setOpen(false)}
      />
    </>
  );
}
