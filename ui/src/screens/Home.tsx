// Home, the Spotlight: the focused title fills the window with its backdrop and a hero placard on
// the left; shelves run along the bottom. One shelf is in view with the next one's title peeking
// below it; up and down slide the shelves inside their own region, so the page itself never
// scrolls and nothing passes under the top bar. A on a work runs its main action (play or resume;
// a show opens), X opens the item menu. Up from the shelves reaches the hero's cues, the same row
// as on Details: play, Details, Loved and Watched, acting on the work last focused on a shelf.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { flushSync } from "react-dom";
import { useBackdrop, useSettled } from "@/components/Backdrop";
import { Cues } from "@/components/Cues";
import { liveGroups } from "@/focus/bridge";
import { Placard, placardLines } from "@/components/Placard";
import { Shelf } from "@/components/Shelf";
import { Empty, ErrorState, Loading } from "@/components/states";
import { jobLine } from "@/components/TopBar";
import { t, useT } from "@/i18n";
import { hasActivity, mainHint, runMain, useActionContext } from "@/lib/actions";
import { plain, type ItemSummary, type Shelf as ShelfName } from "@/lib/api";
import { glideScroll, remPx, seconds, stopGlide } from "@/lib/motion";
import { useItem, useItemSources, useJobStatus, useServices, useShelf, useStatus } from "@/lib/queries";
import { openSettings } from "@/nav/Screens";
import { useNav } from "@/nav/stack";

/** Home's shelves, in order, each shown only when it has items. */
const SHELVES = [
  { shelf: "continue_watching", limit: 20, title: "home.continueWatching" },
  { shelf: "recently_added", limit: 30, title: "home.recentlyAdded" },
  { shelf: "unwatched", limit: 30, title: "home.notWatchedYet" },
  { shelf: "loved", limit: 30, title: "home.loved" },
] as const satisfies Array<{ shelf: ShelfName; limit: number; title: string }>;

interface Row {
  shelf: ShelfName;
  title: string;
  items: ItemSummary[];
}

export function Home() {
  const t = useT();
  const queries = [
    useShelf(SHELVES[0]!.shelf, SHELVES[0]!.limit),
    useShelf(SHELVES[1]!.shelf, SHELVES[1]!.limit),
    useShelf(SHELVES[2]!.shelf, SHELVES[2]!.limit),
    useShelf(SHELVES[3]!.shelf, SHELVES[3]!.limit),
  ];
  const job = useJobStatus();
  const status = useStatus().data;
  // Whether the hero keeps a line for the owner's verdict is known before it first shows, so the
  // placard never grows a line under the owner's eyes. A failure to say counts as no service.
  const services = useServices();

  // A focus scope settles as it mounts, so the shelves render once all their data is here.
  if (queries.some((q) => q.isPending) || services.isPending) return <Loading />;
  const failed = queries.find((q) => q.isError);
  if (failed) {
    return (
      <ErrorState
        message={plain(failed.error)}
        hint={t("common.libraryReadFailed")}
        action={{ label: t("common.tryAgain"), onAction: () => queries.forEach((q) => void q.refetch()) }}
      />
    );
  }
  // Recently added holds every film and show, so when it is empty there are none; Other has its
  // own wall and never comes to Home.
  const rows: Row[] = SHELVES.map((s, i) => ({ shelf: s.shelf, title: t(s.title), items: queries[i]!.data ?? [] })).filter(
    (r) => r.items.length > 0,
  );
  if (rows.length === 0) {
    // Which empty this is waits for the folders to be known.
    if (status === undefined) return <Loading />;
    const sources = status.sources.length;
    const other = sources > 0 && status.other > 0;
    return (
      <Empty
        title={t("home.empty")}
        body={emptyBody(sources, other, job === null || job.phase !== null)}
        status={jobLine(job) ?? undefined}
        action={
          other
            ? { label: t("home.openOther"), onAction: () => useNav.getState().replaceRoot({ screen: "wall", kind: "other" }) }
            : { label: t("common.openSettings"), onAction: openSettings }
        }
      />
    );
  }
  return <Spotlight rows={rows} verdicts={hasActivity(services.data)} />;
}

/**
 * Why Home is empty: no folders yet, only videos that belong in Other, the worker still looking
 * through the folders, or nothing in them.
 */
function emptyBody(sources: number, other: boolean, looking: boolean): string {
  if (sources === 0) return t("home.noFolders");
  if (other) return t("home.allInOther");
  if (looking) return t("home.looking");
  return t("home.foundNothing");
}

function Spotlight({ rows, verdicts }: { rows: Row[]; verdicts: boolean }) {
  const [focused, setFocused] = useState<ItemSummary>(rows[0]!.items[0]!);
  // The shelf in view, by its name: a refresh may add or drop a shelf above it.
  const [active, setActive] = useState<ShelfName>(rows[0]!.shelf);
  // The focused work as the library last reported it (marking it watched changes its facts).
  const current = rows.flatMap((r) => r.items).find((i) => i.id === focused.id) ?? focused;
  // A record seen before shows at once; a new one is asked for once focus has settled, so a held
  // direction does not fetch every work it crosses.
  const settled = useSettled(current.id, seconds("--delay-backdrop"));
  const loaded = useItem(current.id, settled === current.id).data;
  const detail = loaded?.item.id === current.id ? loaded : undefined;
  // Which drive a disabled play waits for, when the full record says.
  const drive = useItemSources(detail).find((s) => !s.online)?.path;

  useEffect(() => {
    useBackdrop.getState().show(current.backdrop, current.poster);
  }, [current.backdrop, current.poster]);

  const ctx = useActionContext();
  const latest = useRef({ detail, ctx });
  useLayoutEffect(() => {
    latest.current = { detail, ctx };
  });
  const open = useCallback((item: ItemSummary) => {
    const { detail, ctx } = latest.current;
    runMain(detail?.item.id === item.id ? detail : item, ctx);
  }, []);
  const focus = useCallback((item: ItemSummary, row: ShelfName) => {
    setFocused(item);
    setActive(row);
  }, []);

  return (
    // The wheel moves by shelves and works here, through the arrow keys (focus/input/wheel.ts).
    <div data-wheel-steps className="absolute inset-0 overflow-clip">
      <div className="flex h-full flex-col">
        {/* The placard stands on the shelves: a longer title grows upward, the action line stays put. */}
        <div className="flex min-h-0 flex-1 flex-col justify-end px-edge pt-bar pb-(--home-placard-foot)">
          <div data-placard className="flex max-w-measure flex-col gap-5">
            <Placard item={current} detail={detail} size="hero" holdVerdicts={verdicts} />
            <Cues
              id="home-cues"
              prefix="hero"
              item={detail ?? current}
              drive={drive}
              progress={placardLines(current, detail).progress}
              details
              aside
            />
          </div>
        </div>
        <Shelves
          rows={rows}
          active={active}
          onOpen={open}
          onFocusItem={focus}
        />
      </div>
    </div>
  );
}

function Shelves({
  rows,
  active,
  onOpen,
  onFocusItem,
}: {
  rows: Row[];
  active: ShelfName;
  onOpen: (item: ItemSummary) => void;
  onFocusItem: (item: ItemSummary, row: ShelfName) => void;
}) {
  // Which shelves are shown, in order: the layout and the handlers follow it, not the row count.
  const shape = rows.map((r) => r.shelf).join(",");
  // A shelf gone from under focus leaves the first one in view; the scope then puts focus there.
  const at = Math.max(0, rows.findIndex((r) => r.shelf === active));
  const els = useRef<Array<HTMLDivElement | null>>([]);
  // Each shelf's offset from the first, and the first shelf's height, in rem: the layout is all
  // rem, so these hold at any window size.
  const [geometry, setGeometry] = useState<{ tops: number[]; height: number } | null>(null);
  useLayoutEffect(() => {
    const rem = remPx();
    const first = els.current[0]?.offsetTop ?? 0;
    setGeometry({
      tops: els.current.slice(0, rows.length).map((el) => ((el?.offsetTop ?? 0) - first) / rem),
      height: (els.current[0]?.offsetHeight ?? 0) / rem,
    });
  }, [shape, rows.length]);

  // The region scrolls itself, by code only (overflow hidden), so the page never does.
  const region = useRef<HTMLDivElement>(null);
  const top = geometry?.tops[at] ?? 0;
  const topNow = useRef(top);
  useEffect(() => {
    topNow.current = top;
    if (region.current !== null) glideScroll(region.current, "top", top * remPx());
  }, [top]);
  // The offset is in pixels: a resized window (full screen, another monitor) sets it again at once.
  useEffect(() => {
    const el = region.current;
    if (el === null) return;
    const keep = () => {
      stopGlide(el);
      el.scrollTop = topNow.current * remPx();
    };
    addEventListener("resize", keep);
    return () => {
      removeEventListener("resize", keep);
      stopGlide(el);
    };
  }, []);
  // Only the shelf in view is reachable (its neighbours' titles stay pointer targets), so Down from
  // the top bar lands on it and not on a shelf slid out above. Up or Down inside the region first
  // opens the neighbour, synchronously, so the focus bridge (listening on the document, after this)
  // can cross into it; once the key has played out the neighbour closes again, entered or not.
  const [reach, setReach] = useState<ShelfName | null>(null);
  const open = useCallback((shelf: ShelfName) => {
    flushSync(() => setReach(shelf));
    setTimeout(() => setReach(null));
  }, []);
  const onKeyDownCapture = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const next = event.key === "ArrowUp" ? at - 1 : event.key === "ArrowDown" ? at + 1 : -1;
    const shelf = rows[next]?.shelf;
    if (shelf === undefined) return;
    open(shelf);
    // Up goes to the shelf above itself: slid out of view, it lies behind the hero's cues, which
    // the nearest-item search would take instead.
    if (event.key === "ArrowUp" && event.target instanceof HTMLElement) {
      const above = liveGroups().find((g) => g.id === `home-${shelf}`);
      if (above?.enter(event.target.getBoundingClientRect(), [0, -1])) {
        event.preventDefault();
        event.stopPropagation();
      }
    }
  };
  // Stable handlers per shelf, so a focus move re-renders no shelf (they are memoised): a pointer
  // on a shelf's title (the peeking one) opens it before the shelf focuses its work.
  const handlers = useMemo(
    () =>
      new Map(
        shape.split(",").map((shelf) => {
          const name = shelf as ShelfName;
          return [name, { focus: (item: ItemSummary) => onFocusItem(item, name), enter: () => open(name) }] as const;
        }),
      ),
    [shape, onFocusItem, open],
  );

  return (
    <div
      ref={region}
      onKeyDownCapture={onKeyDownCapture}
      className="shelf-peek relative shrink-0 overflow-hidden pt-light"
      style={{
        height: `calc(${geometry?.height ?? 0}rem + var(--space-light) + var(--home-shelf-gap) + var(--home-peek))`,
      }}
    >
      {/* Room below the last shelf, so it too can rise to the line the first one rests on. */}
      <div className="flex flex-col gap-(--home-shelf-gap) pb-[calc(var(--home-shelf-gap)+var(--home-peek))]">
        {rows.map((row, i) => (
          <div
            key={row.shelf}
            ref={(el) => {
              els.current[i] = el;
            }}
          >
            <Shelf
              id={`home-${row.shelf}`}
              title={row.title}
              items={row.items}
              onOpen={onOpen}
              hintOf={mainHint}
              onFocusItem={handlers.get(row.shelf)!.focus}
              inert={row.shelf !== active && row.shelf !== reach}
              onEnter={handlers.get(row.shelf)!.enter}
            />
          </div>
        ))}
      </div>
    </div>
  );
}
