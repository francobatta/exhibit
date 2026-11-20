// The Wall: every movie, show or other item hung in a dense grid, with the focused work's placard
// in a panel at the right. The row of controls above it sorts and narrows the wall; the grid
// scrolls inside its own region below them, so nothing passes under the top bar. A plays (a show
// opens), X opens the item menu, which can also start choosing several works at once.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { ActionLine } from "@/components/ActionLine";
import { useBackdrop, useSettled } from "@/components/Backdrop";
import { openItemMenu } from "@/components/ItemMenu";
import { Placard, placardLines } from "@/components/Placard";
import { Empty, ErrorState, Loading } from "@/components/states";
import { TechLines } from "@/components/TechLines";
import { jobLine } from "@/components/TopBar";
import { WallControls, clearFilters, sorts } from "@/components/WallControls";
import { WallGrid, wallColumns, type WallHeading } from "@/components/WallGrid";
import { focusItem } from "@/focus/bridge";
import { useLanguage, useT } from "@/i18n";
import { mainAction, mainHint, runMain, useActionContext } from "@/lib/actions";
import { filterCount, wallQuery } from "@/lib/filters";
import { plain, type Id, type ItemSummary } from "@/lib/api";
import { seconds } from "@/lib/motion";
import { useItem, useItemSources, useItems, useJobStatus } from "@/lib/queries";
import { openSettings } from "@/nav/Screens";
import type { WallKind, WallView } from "@/nav/stack";

/**
 * Columns for a window `width` CSS px wide: posters 7, 9 or 11, two fewer beside the panel; wide
 * works (Other) 4 up to 1920, 5 up to 2560, 6 wider.
 */
export function columnsFor(width: number, kind: WallKind, panel: boolean): number {
  if (kind === "other") return width <= 1920 ? 4 : width <= 2560 ? 5 : 6;
  return wallColumns(width) - (panel ? 2 : 0);
}

const byName = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true, sensitivity: "base" });

/**
 * Works grouped by the folder their file is in, the groups in path order and each group's works
 * in title order. A group's title is the folder's name.
 */
export function groupByFolder<T extends { title: string }>(
  items: T[],
  pathOf: (item: T) => string,
): Array<{ title: string; items: T[] }> {
  const folders = new Map<string, T[]>();
  for (const item of items) {
    const path = pathOf(item);
    const folder = path.slice(0, Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"), 0));
    folders.set(folder, [...(folders.get(folder) ?? []), item]);
  }
  return [...folders.entries()]
    .sort(([a], [b]) => byName(a, b))
    .map(([folder, list]) => ({
      title: folder.split(/[\\/]/).at(-1) ?? folder,
      items: list.sort((a, b) => byName(a.title, b.title)),
    }));
}

/**
 * Other items as the wall hangs them: grouped by the folder their file is in, each group under a
 * heading. Files at a source's root have no folder name; they come first, with no heading.
 */
function byFolder(items: ItemSummary[]): { items: ItemSummary[]; headings: WallHeading[] } {
  const groups = groupByFolder(items, (i) => i.path ?? "");
  const headings: WallHeading[] = [];
  let at = 0;
  for (const g of groups) {
    if (g.title !== "") headings.push({ at, title: g.title });
    at += g.items.length;
  }
  return { items: groups.flatMap((g) => g.items), headings };
}

const subscribeWidth = (onChange: () => void) => {
  window.addEventListener("resize", onChange);
  return () => window.removeEventListener("resize", onChange);
};
const windowWidth = () => window.innerWidth;

const NOUN = {
  movie: { none: "wall.noMovies", body: "wall.noMoviesBody" },
  show: { none: "wall.noShows", body: "wall.noShowsBody" },
  other: { none: "wall.noOther", body: "wall.noOtherBody" },
} as const satisfies Record<WallKind, { none: string; body: string }>;

const SECTION = { movie: "sections.movies", show: "sections.shows", other: "sections.other" } as const;

export interface WallProps {
  kind: WallKind;
  /** Sort and filters, kept in the navigation entry. */
  view: WallView;
  onView: (view: WallView) => void;
}

export function Wall({ kind, view, onView }: WallProps) {
  const t = useT();
  // Genres are named in the language they were chosen in: another language forgets them.
  const lang = useLanguage();
  const spoken = useRef(lang);
  useEffect(() => {
    if (spoken.current === lang) return;
    spoken.current = lang;
    const { genres, without_genres, ...rest } = view.filters;
    if (genres?.length || without_genres?.length) onView({ ...view, filters: rest });
  }, [lang, view, onView]);
  const query = useMemo(() => wallQuery(kind, view, sorts(kind)), [kind, view]);
  const result = useItems(query);
  const job = jobLine(useJobStatus());
  // With no work in focus the wall goes back to its own colour.
  const bare = result.items.length === 0;
  useEffect(() => {
    if (bare) useBackdrop.getState().show(null, null);
  }, [bare]);

  // A focus scope settles as it mounts, so the wall renders once its data is here.
  if (result.isPending) return <Loading label={t("wall.hanging")} />;
  if (result.isError) {
    return (
      <ErrorState
        message={plain(result.error)}
        hint={t("common.libraryReadFailed")}
        action={{ label: t("common.tryAgain"), onAction: () => result.refetch() }}
      />
    );
  }
  if (result.items.length === 0 && filterCount(view.filters) === 0) {
    return (
      <Empty
        title={t(NOUN[kind].none)}
        body={t(NOUN[kind].body)}
        status={job ?? undefined}
        action={{ label: t("common.openSettings"), onAction: openSettings }}
      />
    );
  }
  // The count waits for every page, so it never reads low while a large wall is still arriving.
  const complete = !result.hasNextPage && !result.isPlaceholderData;
  return <Hung kind={kind} view={view} onView={onView} items={result.items} complete={complete} />;
}

function Hung({ kind, view, onView, items, complete }: WallProps & { items: ItemSummary[]; complete: boolean }) {
  const t = useT();
  const columns = columnsFor(useSyncExternalStore(subscribeWidth, windowWidth), kind, true);
  const [focused, setFocused] = useState<Id | null>(null);
  // Other hangs grouped by folder, each group under its folder's name.
  const hung = useMemo(() => (kind === "other" ? byFolder(items) : { items, headings: undefined }), [kind, items]);
  // The focused work as the library last reported it; the first one before anything has focus.
  const current = hung.items.find((i) => i.id === focused) ?? hung.items[0];
  const [selected, setSelected] = useState<ReadonlySet<Id> | null>(null);
  const ctx = useActionContext();
  const latest = useRef({ ctx, selected, current });
  useLayoutEffect(() => {
    latest.current = { ctx, selected, current };
  });

  const choose = useCallback((item: ItemSummary) => {
    setSelected((s) => {
      const next = new Set(s);
      if (!next.delete(item.id)) next.add(item.id);
      return next;
    });
  }, []);
  const done = useCallback(() => {
    setSelected(null);
    // Leaving from the controls row, focus goes back to the work it was on.
    const at = latest.current.current;
    if (at !== undefined && document.activeElement?.closest("[data-focus-group='wall-controls']")) {
      focusItem(`item:${at.id}`);
    }
  }, []);
  const mark = useCallback(
    async (watched: boolean) => {
      const { ctx, selected } = latest.current;
      const results = await Promise.allSettled([...(selected ?? [])].map((id) => ctx.setWatched({ id, watched })));
      const failed = results.find((r) => r.status === "rejected");
      if (failed) ctx.fail(plain(failed.reason));
      done();
    },
    [done],
  );

  const open = useCallback(
    (item: ItemSummary) => {
      if (latest.current.selected !== null) choose(item);
      else runMain(item, latest.current.ctx);
    },
    [choose],
  );
  const menu = useCallback((item: ItemSummary, anchor: HTMLElement | null) => {
    const extra =
      latest.current.selected === null
        ? [{ id: "select", label: t("wall.selectSeveral"), run: () => setSelected(new Set([item.id])) }]
        : [];
    openItemMenu(item, anchor, extra);
  }, []);
  const focus = useCallback((item: ItemSummary) => setFocused(item.id), []);

  return (
    <div className="absolute inset-0 flex flex-col pt-bar">
      {items.length === 0 ? (
        <Empty
          title={t("wall.noMatches")}
          body={t("wall.noMatchesBody")}
          action={{ label: t("wall.clearFilters"), onAction: () => onView(clearFilters(view)) }}
        />
      ) : (
        <div className="flex min-h-0 flex-1">
          {/* The wheel moves through the wall by rows, through the arrow keys (focus/input/wheel.ts). */}
          <div data-wheel-steps className="wall-fade min-w-0 flex-1">
            <WallGrid
              id={`wall-${kind}`}
              items={hung.items}
              headings={hung.headings}
              columns={columns}
              shape={kind === "other" ? "wide" : "poster"}
              onOpen={open}
              hintOf={selected === null ? mainHint : () => t("hints.choose")}
              onFocusItem={focus}
              onMenu={menu}
              onBack={selected === null ? undefined : done}
              selection={selected ?? undefined}
              aria-label={t(SECTION[kind])}
            />
          </div>
          {current !== undefined && <Panel item={current} chosen={selected?.has(current.id)} />}
        </div>
      )}
      {/* After the wall in the page, so the screen's focus first settles on a work (or Clear filters);
          shown above it. */}
      <WallControls
        className="order-first"
        kind={kind}
        view={view}
        onView={onView}
        total={complete ? items.length : null}
        selection={selected === null ? undefined : { count: selected.size, onMark: (w) => void mark(w), onDone: done }}
      />
    </div>
  );
}

/** The focused work's placard, what A does, and what its file holds. `chosen` is set while works are being chosen: whether this one is. */
function Panel({ item, chosen }: { item: ItemSummary; chosen?: boolean }) {
  // A record seen before shows at once; a new one is asked for once focus has rested on the work.
  const settled = useSettled(item.id, seconds("--delay-panel"));
  const loaded = useItem(item.id, settled === item.id).data;
  const detail = loaded?.item.id === item.id ? loaded : undefined;
  const drive = useItemSources(detail).find((s) => !s.online)?.path;
  const ctx = useActionContext();
  const t = useT();
  const action = mainAction(detail ?? item);

  // The wall takes the work's tint; its picture stays off the wall, which is for the works.
  useEffect(() => {
    useBackdrop.getState().show(null, item.poster);
  }, [item.poster]);

  return (
    <aside aria-label={t("wall.focused")} className="flex w-(--wall-panel) shrink-0 flex-col gap-6 pt-light pr-edge">
      <Placard item={item} detail={detail} size="panel" />
      {chosen === undefined ? (
        <ActionLine
          action={action}
          drive={drive}
          progress={action.id === "resume" ? placardLines(item, detail).progress : null}
          onPress={() => runMain(detail ?? item, ctx)}
        />
      ) : (
        <ActionLine action={{ label: chosen ? t("wall.unselect") : t("wall.select") }} drive={drive} />
      )}
      {detail !== undefined && <TechLines detail={detail} className="mt-2" />}
    </aside>
  );
}
