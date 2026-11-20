// The row above a wall: how it is sorted, what it is narrowed to, and how many works that leaves.
// Each control maps onto one `SearchQuery` field. Up from the wall reaches it; it is one focus
// group of UI SDK toggles; Genre opens a list of the genres on the wall. While several works are
// being chosen it becomes what to do with them.

import { useState, type ReactNode } from "react";
import { GenreDialog } from "@/components/GenreDialog";
import { Lamp } from "@/components/Lamp";
import { Button } from "@/components/ui/button";
import { Toggle } from "@/components/ui/toggle";
import { FocusGroup, useFocusItem } from "@/focus/react";
import { t, useT } from "@/i18n";
import type { SearchQuery, Sort } from "@/lib/api";
import { UHD } from "@/lib/format";
import { cn } from "@/lib/utils";
import { DEFAULT_VIEW, type WallKind, type WallView } from "@/nav/stack";

type Filter = Exclude<keyof WallView, "sort" | "genre">;

const SORTS = [
  { sort: "title", label: "wall.byTitle" },
  { sort: "added", label: "wall.byAdded" },
  { sort: "year", label: "wall.byYear" },
  { sort: "runtime", label: "wall.byRuntime" },
] as const satisfies Array<{ sort: Sort; label: string }>;

const FILTERS = [
  { filter: "unwatched", label: "wall.unwatched" },
  { filter: "loved", label: "wall.loved" },
  { filter: "available", label: "wall.available" },
  { filter: "uhd", label: "wall.uhd" },
] as const satisfies Array<{ filter: Filter; label: string }>;

/** Other hangs by folder, so it has no Sort control. */
const sorts = (kind: WallKind) => kind !== "other";

/**
 * The library query a wall's view asks for: only what is set (title order is the library's own,
 * and Other's folder order needs none).
 */
export function wallQuery(kind: WallKind, view: WallView): SearchQuery {
  return {
    kind,
    ...(sorts(kind) && view.sort !== "title" && { sort: view.sort }),
    ...(view.unwatched && { watched: false }),
    ...(view.loved && { loved: true }),
    ...(view.available && { available: true }),
    ...(view.uhd && { min_width: UHD.width }),
    ...(view.genre !== null && { genre: view.genre }),
  };
}

export const hasFilters = (view: WallView) => view.genre !== null || FILTERS.some((f) => view[f.filter]);
export const clearFilters = (view: WallView): WallView => ({ ...DEFAULT_VIEW, sort: view.sort });

/** "412 movies", "1 show", "30 items". */
export function countLine(kind: WallKind, n: number): string {
  const unit = ({ movie: "units.movie", show: "units.show", other: "units.item" } as const)[kind];
  return t(unit, { count: n });
}

const control = "relative focus-visible:bg-transparent focus:text-label";

function ControlToggle({
  focusKey,
  pressed,
  onPress,
  children,
}: {
  focusKey: string;
  pressed: boolean;
  onPress: (pressed: boolean) => void;
  children: ReactNode;
}) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: focusKey });
  return (
    <Toggle ref={ref} {...props} variant="plain" pressed={pressed} onPressedChange={onPress} className={control}>
      {children}
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
  onBack: () => void;
  children: ReactNode;
}) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: focusKey, onSelect, onBack });
  return (
    <Button ref={ref} {...props} className={control}>
      {children}
      {focused && <Lamp />}
    </Button>
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
  /** The genres the Genre control offers, sorted. */
  genres: string[];
  /** The Genre list is about to open: the screen fetches every genre on the wall if it has not. */
  onGenres: () => void;
  selection?: WallSelection;
  className?: string;
}

export function WallControls({ kind, view, onView, total, genres, onGenres, selection, className }: WallControlsProps) {
  const t = useT();
  const [picking, setPicking] = useState(false);
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
            {sorts(kind) && (
              <>
                <span className={cn(caption, "mr-3")}>{t("wall.sort")}</span>
                <div role="group" aria-label={t("wall.sort")} className="contents">
                  {SORTS.map(({ sort, label }) => (
                    <ControlToggle
                      key={sort}
                      focusKey={`wall:sort:${sort}`}
                      pressed={view.sort === sort}
                      onPress={(on) => on && onView({ ...view, sort })}
                    >
                      {t(label)}
                    </ControlToggle>
                  ))}
                </div>
                <span aria-hidden className="mx-5 h-5 w-px bg-line" />
              </>
            )}
            <div role="group" aria-label={t("wall.filters")} className="contents">
              {FILTERS.map(({ filter, label }) => (
                <ControlToggle
                  key={filter}
                  focusKey={`wall:filter:${filter}`}
                  pressed={view[filter]}
                  onPress={(on) => onView({ ...view, [filter]: on })}
                >
                  {t(label)}
                </ControlToggle>
              ))}
              {(genres.length > 0 || view.genre !== null) && (
                <ControlToggle
                  focusKey="wall:filter:genre"
                  pressed={view.genre !== null}
                  onPress={() => {
                    onGenres();
                    setPicking(true);
                  }}
                >
                  {view.genre ?? t("genre.title")}
                </ControlToggle>
              )}
            </div>
          </>
        )}
        <p role="status" className={cn(caption, "ml-auto tabular-nums")}>
          {total !== null && countLine(kind, total)}
        </p>
      </FocusGroup>
      <GenreDialog
        open={picking}
        genres={genres}
        value={view.genre}
        onChoose={(genre) => {
          setPicking(false);
          onView({ ...view, genre });
        }}
        onBack={() => setPicking(false)}
      />
    </>
  );
}
