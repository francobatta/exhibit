// Home, the Spotlight: the focused title fills the window with its backdrop and a hero placard on
// the left; shelves run along the bottom. One shelf is in view with the next one's title peeking
// below it; up and down slide the shelves inside their own region, so the page itself never
// scrolls and nothing passes under the top bar. A on a work runs its main action (play or resume;
// a show opens), X opens the item menu.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { flushSync } from "react-dom";
import { ActionLine } from "@/components/ActionLine";
import { useBackdrop, useSettled } from "@/components/Backdrop";
import { Placard } from "@/components/Placard";
import { Shelf } from "@/components/Shelf";
import { Empty, ErrorState, Loading } from "@/components/states";
import { jobLine } from "@/components/TopBar";
import { mainAction, runMain, useActionContext } from "@/lib/actions";
import { plain, type ItemSummary, type Shelf as ShelfName } from "@/lib/api";
import { glideScroll, remPx, seconds, stopGlide } from "@/lib/motion";
import { useItem, useItemSources, useJobStatus, useShelf, useStatus } from "@/lib/queries";
import { openSettings } from "@/nav/Screens";
import { useNav } from "@/nav/stack";

/** Home's shelves, in order, each shown only when it has items. */
const SHELVES: Array<{ shelf: ShelfName; limit: number; title: string }> = [
  { shelf: "continue_watching", limit: 20, title: "Continue watching" },
  { shelf: "recently_added", limit: 30, title: "Recently added" },
  { shelf: "unwatched", limit: 30, title: "Not watched yet" },
  { shelf: "loved", limit: 30, title: "Loved" },
];

interface Row {
  shelf: ShelfName;
  title: string;
  items: ItemSummary[];
}

export function Home() {
  const queries = [
    useShelf(SHELVES[0]!.shelf, SHELVES[0]!.limit),
    useShelf(SHELVES[1]!.shelf, SHELVES[1]!.limit),
    useShelf(SHELVES[2]!.shelf, SHELVES[2]!.limit),
    useShelf(SHELVES[3]!.shelf, SHELVES[3]!.limit),
  ];
  const job = useJobStatus();
  const status = useStatus().data;

  // A focus scope settles as it mounts, so the shelves render once all their data is here.
  if (queries.some((q) => q.isPending)) return <Loading />;
  const failed = queries.find((q) => q.isError);
  if (failed) {
    return (
      <ErrorState
        message={plain(failed.error)}
        hint="The library could not be read. Try again, or restart Exhibit if it keeps happening."
        action={{ label: "Try again", onAction: () => queries.forEach((q) => void q.refetch()) }}
      />
    );
  }
  // Recently added holds every film and show, so when it is empty there are none; Other has its
  // own wall and never comes to Home.
  const rows: Row[] = SHELVES.map((s, i) => ({ ...s, items: queries[i]!.data ?? [] })).filter(
    (r) => r.items.length > 0,
  );
  if (rows.length === 0) {
    // Which empty this is waits for the folders to be known.
    if (status === undefined) return <Loading />;
    const sources = status.sources.length;
    const other = sources > 0 && status.other > 0;
    return (
      <Empty
        title="Nothing here yet."
        body={emptyBody(sources, other, job === null || job.phase !== null)}
        status={jobLine(job) ?? undefined}
        action={
          other
            ? { label: "Open Other", onAction: () => useNav.getState().replaceRoot({ screen: "wall", kind: "other" }) }
            : { label: "Open Settings", onAction: openSettings }
        }
      />
    );
  }
  return <Spotlight rows={rows} />;
}

/**
 * Why Home is empty: no folders yet, only videos that belong in Other, the worker still looking
 * through the folders, or nothing in them.
 */
function emptyBody(sources: number, other: boolean, looking: boolean): string {
  if (sources === 0) return "Add the folders that hold your films and shows in Settings.";
  if (other) return "Home shows films and shows. Everything Exhibit has found so far is in Other.";
  if (looking) return "Exhibit is still looking through your folders.";
  return "Exhibit found nothing to show in your folders. Add another folder in Settings.";
}

function Spotlight({ rows }: { rows: Row[] }) {
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
  const action = mainAction(detail ?? current);
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
            <Placard item={current} detail={detail} size="hero" />
            {/* Until the full record arrives, its description's two lines are held open. */}
            {detail === undefined && <p aria-hidden className="h-[2lh] font-reading text-lead leading-reading" />}
            <ActionLine action={action} drive={drive} />
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
  useEffect(() => {
    if (region.current !== null) glideScroll(region.current, "top", top * remPx());
  }, [top]);
  useEffect(() => {
    const el = region.current;
    return () => {
      if (el !== null) stopGlide(el);
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
    if (shelf !== undefined) open(shelf);
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
