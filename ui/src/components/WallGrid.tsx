// A wall: every work of a kind in rows, virtualised by row with UI SDK `createVirtualizer`, and
// one focus group moved by UI SDK `createGridFocus`. Works may come in groups, each starting a new
// row under its heading; the groups are still one grid. Cells past the end of a short row are
// disabled; Up or Down into one lands on that row's last work. An arrow at the wall's true edge is
// left for the bridge, which carries it to the next group. The focused row stays in the middle
// band of the view, gliding there on the focus spring.

import {
  createFocusRecovery,
  createGridFocus,
  createVirtualizer,
  definePrimitive,
  nearestInDirection,
  type GridCell,
} from "@ui-sdk/core";
import { usePart, usePrimitive, type PartProps, type PrimitiveHandle } from "@ui-sdk/react/primitive";
import {
  createContext,
  memo,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type KeyboardEvent,
} from "react";
import { markEdge, registerGroup, registerItem, useFocusState, type Box, type Dir } from "@/focus/bridge";
import type { Id, ItemSummary } from "@/lib/api";
import { remPx, rems, revealRow, stopGlide } from "@/lib/motion";
import { cn } from "@/lib/utils";
import { openItemMenu, useMenuOpenOn } from "./ItemMenu";
import { ASPECT, Work, workLabel, type WorkShape } from "./Work";

export const cellOf = (index: number, columns: number): GridCell => ({
  row: Math.floor(index / columns),
  col: index % columns,
});
export const indexOf = (cell: GridCell, columns: number): number => cell.row * columns + cell.col;

/** Columns for a window this many CSS px wide: 7 up to 1920, 9 up to 2560, 11 wider. */
export function wallColumns(width: number): number {
  return width <= 1920 ? 7 : width <= 2560 ? 9 : 11;
}

/** A group of works starts at item `at` under its heading. */
export interface WallHeading {
  at: number;
  title: string;
}

/**
 * Where each work hangs: `slots[cell index]` is an item index, or -1 for a gap left at the end of
 * a group's last row so the next group starts a row of its own. `titles` maps a row to its heading.
 */
export function wallLayout(count: number, columns: number, headings: readonly WallHeading[] = []) {
  const titles = new Map<number, string>();
  if (headings.length === 0) return { slots: Array.from({ length: count }, (_, i) => i), titles };
  const starts = new Map(headings.map((h) => [h.at, h.title]));
  const slots: number[] = [];
  for (let i = 0; i < count; i++) {
    const title = starts.get(i);
    if (title !== undefined) {
      while (slots.length % columns !== 0) slots.push(-1);
      titles.set(slots.length / columns, title);
    }
    slots.push(i);
  }
  return { slots, titles };
}

const isGap = (slots: readonly number[], index: number) => index >= slots.length || slots[index]! < 0;

/** The last cell holding a work on `row` (every row holds at least one). */
function lastOnRow(slots: readonly number[], row: number, columns: number): number {
  let i = Math.min(slots.length, (row + 1) * columns) - 1;
  while (i > row * columns && isGap(slots, i)) i--;
  return i;
}

export interface WallGridProps {
  /** Unique among live focus groups. */
  id: string;
  /** In order; with `headings`, grouped. */
  items: ItemSummary[];
  /** Where groups start, each under its heading. */
  headings?: readonly WallHeading[];
  columns: number;
  shape: WorkShape;
  /** A, Enter or a click on a work. */
  onOpen: (item: ItemSummary) => void;
  /** Every work that takes focus, from any device. */
  onFocusItem?: (item: ItemSummary) => void;
  /** X, `m` or right-click on a work. Default: the item menu. */
  onMenu?: (item: ItemSummary, anchor: HTMLElement | null) => void;
  /** Back (B, Escape) on a work, before the screen's. */
  onBack?: () => void;
  /** Works marked as chosen. */
  selection?: ReadonlySet<Id>;
  /** An arrow left the wall and no group lies that way. */
  onEdge?: (dir: Dir) => void;
  /** The light's shared id. Default "work-light". */
  light?: string;
  /** Size the wall: it scrolls inside its own box, so give it a height (default h-full). */
  className?: string;
  "aria-label"?: string;
}

interface GridOptions {
  rows: number;
  cols: number;
  slots: readonly number[];
  reveal?: (cell: GridCell) => void;
  fallback?: (index: number) => HTMLElement | undefined;
}

const wallGrid = definePrimitive({
  name: "wall-grid",
  options: { rows: "number", cols: "number", slots: "value", reveal: "value", fallback: "value" },
  state: (_options: GridOptions) => ({ active: null as GridCell | null }),
  api: (context) => {
    const grid = createGridFocus(context, {
      key: "active",
      rows: () => context.options().rows,
      cols: () => context.options().cols,
      pageSize: () => PAGE_ROWS,
      disabled: (cell) => isGap(context.options().slots, indexOf(cell, context.options().cols)),
      reveal: (cell) => context.options().reveal?.(cell),
    });
    const recovery = createFocusRecovery<{ index: number }>({
      fallback: (lost) => context.options().fallback?.(lost.props.index),
    });
    return { grid, recovery };
  },
  slots: (slot) => ({ root: slot({}) }),
});

type GridHandle = PrimitiveHandle<typeof wallGrid>;

interface WallContext {
  handle: GridHandle;
  cols: number;
  slots: readonly number[];
  shape: WorkShape;
  light: string;
  open: (item: ItemSummary) => void;
  menu: (item: ItemSummary, anchor: HTMLElement | null) => void;
  /** The screen's Back for works now, read on each press. */
  back: () => (() => void) | undefined;
  focused: (index: number) => void;
}

const Context = createContext<WallContext | null>(null);

/** Rows rendered past each edge of the view. */
const OVERSCAN = 2;
/** Rows Page Up and Page Down move. */
const PAGE_ROWS = 3;

const ARROWS = new Set(["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"]);

interface CellProps {
  item: ItemSummary;
  index: number;
  selected: boolean;
}

const WallCell = memo(function WallCell({ item, index, selected }: CellProps) {
  const ctx = useContext(Context)!;
  const { handle, cols, slots, shape, light } = ctx;
  const latest = useRef(ctx);
  useLayoutEffect(() => {
    latest.current = ctx;
  });
  const cell = useMemo(() => cellOf(index, cols), [index, cols]);
  const [focused, setFocused] = useState(false);
  const node = useRef<HTMLDivElement | null>(null);
  const menuOpen = useMenuOpenOn(node);

  // Only the cell gaining the tab stop and the one losing it re-render on a move.
  const tabIndex = useSyncExternalStore(
    handle.instance.subscribe,
    () => handle.api.grid.tabIndex(handle.instance.get(), cell),
    () => handle.api.grid.tabIndex(handle.instance.get(), cell),
  );

  const ref = useCallback(
    (el: HTMLDivElement | null) => {
      node.current = el;
      if (el === null) return;
      const offs = [
        handle.api.grid.bind(el, cell),
        handle.api.recovery.bind(el, { key: `item:${item.id}`, props: { index } }),
        registerItem(el, () => ({
          key: `item:${item.id}`,
          onSelect: () => latest.current.open(item),
          onMenu: () => latest.current.menu(item, node.current),
          onBack: latest.current.back(),
        })),
      ];
      return () => {
        for (const off of offs) off();
      };
    },
    [handle, cell, item, index],
  );

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.defaultPrevented || e.target !== e.currentTarget) return;
    const native = e.nativeEvent;
    if (e.key === "Enter") {
      e.preventDefault();
      latest.current.open(item);
      return;
    }
    const grid = handle.api.grid;
    const step = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    const next = { row: cell.row + step, col: cell.col };
    const rows = Math.ceil(slots.length / cols);
    // Up or Down into the gap after a short row lands on that row's last work (the grid would skip the gap).
    if (step !== 0 && next.row >= 0 && next.row < rows && isGap(slots, indexOf(next, cols))) {
      e.preventDefault();
      grid.focus(cellOf(lastOnRow(slots, next.row, cols), cols), { reason: "list-navigation", event: native });
      return;
    }
    const before = grid.active();
    grid.keydown(native);
    const after = grid.active();
    // The grid takes an arrow even where it cannot move; at a true edge the bridge may carry it on.
    // The end of a group's short row is not one: Right there stays on the wall.
    const shortEnd = e.key === "ArrowRight" && cell.col < cols - 1 && index < slots.length - 1;
    if (ARROWS.has(e.key) && !shortEnd && before !== null && after !== null && before.row === after.row && before.col === after.col) {
      markEdge(native);
    }
  };

  return (
    <div
      ref={ref}
      role="gridcell"
      tabIndex={Number(tabIndex)}
      data-cell
      data-index={index}
      data-focus-key={`item:${item.id}`}
      data-focused={focused ? "" : undefined}
      aria-label={workLabel(item)}
      aria-selected={selected || undefined}
      className="relative outline-none"
      onFocus={(e) => {
        if (e.target !== e.currentTarget) return;
        setFocused(true);
        latest.current.focused(index);
      }}
      onBlur={(e) => {
        if (e.target === e.currentTarget) setFocused(false);
      }}
      onKeyDown={onKeyDown}
      onClick={() => latest.current.open(item)}
    >
      <Work item={item} shape={shape} focused={focused || menuOpen} light={light} selected={selected} />
    </div>
  );
});

export function WallGrid({
  id,
  items,
  headings,
  columns,
  shape,
  onOpen,
  onFocusItem,
  onMenu = openItemMenu,
  onBack,
  selection,
  onEdge,
  light = "work-light",
  className,
  "aria-label": label,
}: WallGridProps) {
  const { slots, titles } = useMemo(() => wallLayout(items.length, columns, headings), [items.length, columns, headings]);
  const rows = Math.ceil(slots.length / columns);
  const root = useRef<HTMLDivElement>(null);
  const latest = useRef({ items, slots, onOpen, onFocusItem, onMenu, onBack, onEdge, columns });
  useLayoutEffect(() => {
    latest.current = { items, slots, onOpen, onFocusItem, onMenu, onBack, onEdge, columns };
  });
  const visited = useRef(false);

  const rowPx = useCallback(() => {
    const rem = remPx();
    const gap = rems("--space-work-gap") * rem;
    const el = root.current;
    // The virtualiser is asked before the wall has mounted; it is reset once it connects and has a width.
    const cell =
      el === null
        ? rems("--work-poster") * rem
        : (el.clientWidth - 2 * rems("--space-edge") * rem - (latest.current.columns - 1) * gap) / latest.current.columns;
    return cell * ASPECT[shape] + gap;
  }, [shape]);

  // The row holding focus stays rendered wherever the wall scrolls.
  const focusRow = useRef<number | null>(null);
  const [virtual] = useState(() =>
    createVirtualizer({
      count: () => Math.ceil(latest.current.slots.length / latest.current.columns),
      estimate: () => rowPx(),
      overscan: OVERSCAN,
      scrollMargin: rems("--space-light") * remPx(),
      keep: () => (focusRow.current === null ? [] : [focusRow.current]),
    }),
  );
  const rendered = useSyncExternalStore(virtual.subscribe, virtual.items, virtual.items);

  const cellEl = useCallback(
    (index: number) => root.current?.querySelector<HTMLElement>(`[data-cell][data-index="${index}"]`) ?? undefined,
    [],
  );

  const handle = usePrimitive(wallGrid, {
    rows,
    cols: columns,
    slots,
    reveal: (cell: GridCell) => {
      if (!virtual.items().some((v) => v.index === cell.row)) virtual.scrollToIndex(cell.row, { align: "center" });
    },
    // The work that took the lost one's place, else the nearest before it.
    fallback: (index: number) => {
      const s = latest.current.slots;
      for (let i = Math.min(index, s.length - 1); i >= 0; i--) if (!isGap(s, i)) return cellEl(i);
      return undefined;
    },
  });

  // One stable ref per row index, so a row is measured when it mounts, not on every virtualiser update.
  const rowRefs = useRef(new Map<number, (el: HTMLDivElement | null) => () => void>());
  const rowRef = (index: number) => {
    let ref = rowRefs.current.get(index);
    if (ref === undefined) {
      ref = (el) => virtual.measure(index, el);
      rowRefs.current.set(index, ref);
    }
    return ref;
  };
  // Forgetting the sizes (the wall's width or columns changed) also renews the refs, so the rows on
  // screen are measured again: a row whose height did not change would never report it.
  const [, setLayout] = useState(0);
  const relayout = useCallback(() => {
    virtual.reset();
    rowRefs.current.clear();
    setLayout((n) => n + 1);
  }, [virtual]);

  useEffect(() => {
    const el = root.current;
    if (el === null) return;
    const disconnect = virtual.connect(el);
    const observer = new ResizeObserver(relayout);
    observer.observe(el);
    return () => {
      disconnect();
      observer.disconnect();
      stopGlide(el);
    };
  }, [virtual, relayout]);

  useEffect(relayout, [columns, relayout]);
  useEffect(() => {
    virtual.refresh();
  }, [slots, virtual]);

  useEffect(() => {
    const el = root.current!;
    const cells = () => [...el.querySelectorAll<HTMLElement>("[data-cell]")];
    const enter = (from: Box, dir: Dir): boolean => {
      const { slots: s, columns: cols } = latest.current;
      if (s.length === 0) return false;
      const grid = handle.api.grid;
      // grid.focus answers false for the cell that is already active, so landing is judged here.
      const land = (cell: GridCell) => {
        grid.focus(cell, { reason: "imperative" });
        const target = cellEl(indexOf(cell, cols));
        if (target !== undefined && document.activeElement !== target) target.focus({ preventScroll: true });
        return true;
      };
      const active = grid.active();
      if (visited.current && active !== null && !isGap(s, indexOf(active, cols))) return land(active);
      const boxes = cells().map((c) => ({ key: c.dataset.index!, rect: c.getBoundingClientRect() as Box }));
      if (boxes.length === 0) return land({ row: 0, col: 0 });
      let key = nearestInDirection(from, boxes, dir);
      if (key === undefined) {
        const axis = dir[0] === 0 ? "x" : "y";
        const mid = (b: Box) => (axis === "x" ? (b.left + b.right) / 2 : (b.top + b.bottom) / 2);
        key = boxes.reduce((best, b) => (Math.abs(mid(b.rect) - mid(from)) < Math.abs(mid(best.rect) - mid(from)) ? b : best)).key;
      }
      return land(cellOf(Number(key), cols));
    };
    return registerGroup({
      id,
      el,
      enter,
      stop: () => {
        const active = handle.api.grid.active();
        return (active === null ? undefined : cellEl(indexOf(active, latest.current.columns))) ?? cells()[0];
      },
      items: cells,
      onEdge: (dir) => latest.current.onEdge?.(dir),
      reveals: true,
    });
  }, [id, handle, cellEl]);

  const focused = useCallback(
    (index: number) => {
      visited.current = true;
      const { items: list, slots: s, columns: cols } = latest.current;
      const item = list[s[index] ?? -1];
      if (item === undefined) return;
      const row = Math.floor(index / cols);
      focusRow.current = row;
      latest.current.onFocusItem?.(item);
      const el = root.current;
      const v = virtual.items().find((r) => r.index === row);
      if (el === null || v === undefined) return;
      const top = rems("--space-light") * remPx() + v.start;
      revealRow(el, top, top + v.size, useFocusState.getState().mode === "pointer");
    },
    [virtual],
  );

  const open = useCallback((item: ItemSummary) => latest.current.onOpen(item), []);
  const menu = useCallback((item: ItemSummary, anchor: HTMLElement | null) => latest.current.onMenu(item, anchor), []);
  const back = useCallback(() => latest.current.onBack, []);

  const ctx = useMemo<WallContext>(
    () => ({ handle, cols: columns, slots, shape, light, open, menu, back, focused }),
    [handle, columns, slots, shape, light, open, menu, back, focused],
  );

  const part = usePart(
    handle,
    "root",
    {},
    {
      ref: root,
      role: "grid",
      "aria-label": label,
      "aria-rowcount": rows,
      "aria-colcount": columns,
      "data-focus-group": id,
      className: cn("relative h-full overflow-y-auto overflow-x-hidden px-edge", className),
      children: (
        // Room for the light above the first row, and as much below the last, so it can clear an edge fade.
        <div className="relative" style={{ height: `calc(${virtual.size()}px + 2 * var(--space-light))` }}>
          {rendered.map((v) => {
            const first = v.index * columns;
            const title = titles.get(v.index);
            // Gaps only ever end a row, so a row's works are its slots before the first gap.
            const works = slots.slice(first, first + columns).filter((s) => s >= 0);
            return (
              <div
                key={v.index}
                ref={rowRef(v.index)}
                role="rowgroup"
                aria-label={title}
                className="absolute inset-x-0"
                style={{ top: `calc(${v.start}px + var(--space-light))` }}
              >
                {title !== undefined && (
                  <h3 aria-hidden className={cn("truncate pb-6 font-display text-lead font-ui text-label-dim", v.index > 0 && "pt-4")}>
                    {title}
                  </h3>
                )}
                <div
                  role="row"
                  aria-rowindex={v.index + 1}
                  className="grid"
                  style={{
                    gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
                    columnGap: "var(--space-work-gap)",
                    paddingBottom: "var(--space-work-gap)",
                  }}
                >
                  {works.map((s, c) => {
                    const item = items[s]!;
                    return <WallCell key={item.id} item={item} index={first + c} selected={selection?.has(item.id) ?? false} />;
                  })}
                </div>
              </div>
            );
          })}
        </div>
      ),
    } as PartProps<"div">,
    "div",
  );
  return <Context value={ctx}>{part}</Context>;
}
