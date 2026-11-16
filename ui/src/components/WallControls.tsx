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
import type { SearchQuery, Sort } from "@/lib/api";
import { UHD, count } from "@/lib/format";
import { cn } from "@/lib/utils";

export type WallKind = "movie" | "show" | "other";

/** What the owner has asked of a wall; kept in its navigation entry, so Back finds it as left. */
export interface WallView {
  sort: Sort;
  unwatched: boolean;
  loved: boolean;
  available: boolean;
  uhd: boolean;
  /** One genre, or any. */
  genre: string | null;
}

export const DEFAULT_VIEW: WallView = {
  sort: "title",
  unwatched: false,
  loved: false,
  available: false,
  uhd: false,
  genre: null,
};

type Filter = Exclude<keyof WallView, "sort" | "genre">;

const SORTS: Array<{ sort: Sort; label: string }> = [
  { sort: "title", label: "Title" },
  { sort: "added", label: "Recently added" },
  { sort: "year", label: "Year" },
  { sort: "runtime", label: "Runtime" },
];

const FILTERS: Array<{ filter: Filter; label: string }> = [
  { filter: "unwatched", label: "Not watched" },
  { filter: "loved", label: "Loved" },
  { filter: "available", label: "Available now" },
  { filter: "uhd", label: "4K" },
];

/** The library query a wall's view asks for: only what is set (title order is the library's own). */
export function wallQuery(kind: WallKind, view: WallView): SearchQuery {
  return {
    kind,
    ...(view.sort !== "title" && { sort: view.sort }),
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
  const [one, many] = { movie: ["movie", "movies"], show: ["show", "shows"], other: ["item", "items"] }[kind];
  return count(n, one!, many!);
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
  /** How many works the view leaves. */
  total: number;
  /** The genres the Genre control offers, sorted. */
  genres: string[];
  selection?: WallSelection;
  className?: string;
}

export function WallControls({ kind, view, onView, total, genres, selection, className }: WallControlsProps) {
  const [picking, setPicking] = useState(false);
  return (
    <>
      <FocusGroup
        id="wall-controls"
        orientation="horizontal"
        aria-label={selection ? "Chosen works" : "Sort and filter"}
        className={cn("flex h-(--wall-controls) items-center gap-1 px-edge", className)}
      >
        {selection ? (
          <>
            <p className="mr-6 font-display text-body font-ui text-label tabular-nums">{selection.count} selected</p>
            <ControlButton focusKey="wall:select:watched" onSelect={() => selection.onMark(true)} onBack={selection.onDone}>
              Mark as watched
            </ControlButton>
            <ControlButton focusKey="wall:select:unwatched" onSelect={() => selection.onMark(false)} onBack={selection.onDone}>
              Mark as not watched
            </ControlButton>
            <ControlButton focusKey="wall:select:done" onSelect={selection.onDone} onBack={selection.onDone}>
              Done
            </ControlButton>
          </>
        ) : (
          <>
            <span className={cn(caption, "mr-3")}>Sort</span>
            <div role="group" aria-label="Sort" className="contents">
              {SORTS.map(({ sort, label }) => (
                <ControlToggle
                  key={sort}
                  focusKey={`wall:sort:${sort}`}
                  pressed={view.sort === sort}
                  onPress={(on) => on && onView({ ...view, sort })}
                >
                  {label}
                </ControlToggle>
              ))}
            </div>
            <span aria-hidden className="mx-5 h-5 w-px bg-line" />
            <div role="group" aria-label="Filters" className="contents">
              {FILTERS.map(({ filter, label }) => (
                <ControlToggle
                  key={filter}
                  focusKey={`wall:filter:${filter}`}
                  pressed={view[filter]}
                  onPress={(on) => onView({ ...view, [filter]: on })}
                >
                  {label}
                </ControlToggle>
              ))}
              {(genres.length > 0 || view.genre !== null) && (
                <ControlToggle focusKey="wall:filter:genre" pressed={view.genre !== null} onPress={() => setPicking(true)}>
                  {view.genre ?? "Genre"}
                </ControlToggle>
              )}
            </div>
          </>
        )}
        <p role="status" className={cn(caption, "ml-auto tabular-nums")}>
          {countLine(kind, total)}
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
