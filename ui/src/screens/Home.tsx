// Home, the Spotlight: the focused title fills the window with its backdrop and a hero placard on
// the left; shelves run along the bottom. One shelf is in view with the next one's title peeking
// below it; up and down slide the shelves inside their own region, so the page itself never
// scrolls and nothing passes under the top bar. A on a work runs its main action (play or resume;
// a show opens), X opens the item menu.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { flushSync } from "react-dom";
import { useBackdrop, useSettled } from "@/components/Backdrop";
import { Placard } from "@/components/Placard";
import { Shelf } from "@/components/Shelf";
import { Empty, ErrorState, Loading } from "@/components/states";
import { jobLine } from "@/components/TopBar";
import { useInputMode } from "@/focus/react";
import { OFFLINE, itemActions, runAction, useActionContext, type Action, type ActionItem } from "@/lib/actions";
import { plain, type ItemSummary, type Shelf as ShelfName } from "@/lib/api";
import { facts } from "@/lib/format";
import { glideScroll, remPx, seconds, stopGlide } from "@/lib/motion";
import { useItem, useJobStatus, useShelf, useStatus } from "@/lib/queries";

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

/** What A does on a work: a show (or season) opens; anything else plays or resumes. */
export function mainAction(item: ActionItem): Action {
  const actions = itemActions(item);
  const kind = "item" in item ? item.item.kind : item.kind;
  const container = kind === "show" || kind === "season";
  return (container ? actions.find((a) => a.id === "details") : undefined) ?? actions[0]!;
}

export function Home() {
  const queries = [
    useShelf(SHELVES[0]!.shelf, SHELVES[0]!.limit),
    useShelf(SHELVES[1]!.shelf, SHELVES[1]!.limit),
    useShelf(SHELVES[2]!.shelf, SHELVES[2]!.limit),
    useShelf(SHELVES[3]!.shelf, SHELVES[3]!.limit),
  ];
  const job = jobLine(useJobStatus());

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
  // Recently added holds everything in the library, so when it is empty the library is.
  const rows: Row[] = SHELVES.map((s, i) => ({ ...s, items: queries[i]!.data ?? [] })).filter(
    (r) => r.items.length > 0,
  );
  if (rows.length === 0) {
    return (
      <Empty
        title="Nothing here yet."
        body="Exhibit is still looking through your folders."
        status={job ?? undefined}
      />
    );
  }
  return <Spotlight rows={rows} />;
}

function Spotlight({ rows }: { rows: Row[] }) {
  const [focused, setFocused] = useState<ItemSummary>(rows[0]!.items[0]!);
  const [active, setActive] = useState(0);
  // The focused work as the library last reported it (marking it watched changes its facts).
  const current = rows.flatMap((r) => r.items).find((i) => i.id === focused.id) ?? focused;
  // A record seen before shows at once; a new one is asked for once focus has settled, so a held
  // direction does not fetch every work it crosses.
  const settled = useSettled(current.id, seconds("--delay-backdrop"));
  const loaded = useItem(current.id, settled === current.id).data;
  const detail = loaded?.item.id === current.id ? loaded : undefined;
  const sources = useStatus().data?.sources;
  const action = mainAction(detail ?? current);
  // Which drive a disabled play waits for, when the full record says.
  const drive = detail?.files
    .map((f) => sources?.find((s) => s.id === f.source_id))
    .find((s) => s !== undefined && !s.online)?.path;

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
    const subject: ActionItem = detail?.item.id === item.id ? detail : item;
    const action = mainAction(subject);
    if (action.disabled) ctx.fail(action.disabled);
    else void runAction(action, subject, ctx);
  }, []);
  const focus = useCallback((item: ItemSummary, row: number) => {
    setFocused(item);
    setActive(row);
  }, []);

  return (
    <div className="absolute inset-0 overflow-clip">
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

/** Why the main action cannot run, as the second half of the line:
 * "Play, the drive for D:\Movies is not connected". */
function whyNot(reason: string, drive: string | undefined): string {
  if (reason === OFFLINE && drive !== undefined) return `the drive for ${drive} is not connected`;
  return reason.charAt(0).toLowerCase() + reason.slice(1);
}

/** The placard's last line: what A does, as text with the key that does it. Not focusable. */
function ActionLine({ action, drive }: { action: Action; drive: string | undefined }) {
  const mode = useInputMode();
  if (action.disabled) {
    return (
      <p className="flex h-(--keycap) items-center font-display text-lead text-label-dim">
        {facts([action.label, whyNot(action.disabled, drive)])}
      </p>
    );
  }
  const key = mode === "pad" ? "A" : mode === "keys" ? "Enter" : null;
  return (
    <p className="flex h-(--keycap) items-center gap-3 font-display text-lead font-ui text-label">
      {key !== null && (
        <kbd className="inline-flex h-(--keycap) min-w-(--keycap) items-center justify-center rounded-work px-2 font-display text-caption font-title text-label-dim ring-1 ring-line ring-inset">
          {key}
        </kbd>
      )}
      {action.label}
    </p>
  );
}

function Shelves({
  rows,
  active,
  onOpen,
  onFocusItem,
}: {
  rows: Row[];
  active: number;
  onOpen: (item: ItemSummary) => void;
  onFocusItem: (item: ItemSummary, row: number) => void;
}) {
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
  }, [rows.length]);

  // The region scrolls itself, by code only (overflow hidden), so the page never does.
  const region = useRef<HTMLDivElement>(null);
  const top = geometry?.tops[active] ?? 0;
  useEffect(() => {
    if (region.current !== null) glideScroll(region.current, "top", top * remPx());
  }, [top]);
  useEffect(() => {
    const el = region.current;
    return () => {
      if (el !== null) stopGlide(el);
    };
  }, []);
  // Only the shelf in view is reachable, so Down from the top bar lands on it and not on a shelf
  // slid out above. Up or Down inside the region first opens its neighbour, synchronously, so the
  // focus bridge (listening on the document, after this) can cross into it.
  const [reach, setReach] = useState<number | null>(null);
  useEffect(() => setReach(null), [active]);
  const onKeyDownCapture = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const next = event.key === "ArrowUp" ? active - 1 : event.key === "ArrowDown" ? active + 1 : -1;
    if (next >= 0 && next < rows.length) flushSync(() => setReach(next));
  };
  // One stable handler per shelf, so a focus move re-renders no shelf (they are memoised).
  const handlers = useMemo(
    () => Array.from({ length: rows.length }, (_, i) => (item: ItemSummary) => onFocusItem(item, i)),
    [rows.length, onFocusItem],
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
            inert={i !== active && i !== reach}
            ref={(el) => {
              els.current[i] = el;
            }}
          >
            <Shelf
              id={`home-${row.shelf}`}
              title={row.title}
              items={row.items}
              onOpen={onOpen}
              onFocusItem={handlers[i]}
            />
          </div>
        ))}
      </div>
    </div>
  );
}

