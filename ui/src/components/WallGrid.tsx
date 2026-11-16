// A wall: every work of a kind in rows, virtualised by row with UI SDK `createVirtualizer`, and
// one focus group moved by UI SDK `createGridFocus`. Cells past the end of a short last row are
// disabled; Down into one lands on the last work. An arrow at the wall's true edge is left for the
// bridge, which carries it to the next group. The focused row stays in the middle band of the view.

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
import { remPx, rems, scrollBehavior } from "@/lib/motion";
import { cn } from "@/lib/utils";
import { openItemMenu, useMenuOpenOn } from "./ItemMenu";
import { ASPECT, Work, workLabel, type WorkShape } from "./Work";

export const cellOf = (index: number, columns: number): GridCell => ({
  row: Math.floor(index / columns),
  col: index % columns,
});
export const indexOf = (cell: GridCell, columns: number): number => cell.row * columns + cell.col;
/** A cell with no work: past the end of a short last row, or below the last row. */
export const isPastEnd = (cell: GridCell, columns: number, count: number): boolean =>
  indexOf(cell, columns) >= count;

/** Columns for a window this many CSS px wide: 7 up to 1920, 9 up to 2560, 11 wider. */
export function wallColumns(width: number): number {
  return width <= 1920 ? 7 : width <= 2560 ? 9 : 11;
}

export interface WallGridProps {
  /** Unique among live focus groups. */
  id: string;
  items: ItemSummary[];
  columns: number;
  shape: WorkShape;
  /** A, Enter or a click on a work. */
  onOpen: (item: ItemSummary) => void;
  /** Every work that takes focus, from any device. */
  onFocusItem?: (item: ItemSummary) => void;
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
  count: number;
  reveal?: (cell: GridCell) => void;
  fallback?: (index: number) => HTMLElement | undefined;
}

const wallGrid = definePrimitive({
  name: "wall-grid",
  options: { rows: "number", cols: "number", count: "number", reveal: "value", fallback: "value" },
  state: (_options: GridOptions) => ({ active: null as GridCell | null }),
  api: (context) => {
    const grid = createGridFocus(context, {
      key: "active",
      rows: () => context.options().rows,
      cols: () => context.options().cols,
      pageSize: () => PAGE_ROWS,
      disabled: (cell) => isPastEnd(cell, context.options().cols, context.options().count),
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
  count: number;
  shape: WorkShape;
  light: string;
  open: (item: ItemSummary) => void;
  focused: (index: number) => void;
}

const Context = createContext<WallContext | null>(null);


/** Rows rendered past each edge of the view. */
const OVERSCAN = 2;
/** The focused row is kept between these fractions of the view's height. */
const BAND = { top: 0.22, bottom: 0.78 };
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
  const { handle, cols, count, shape, light } = ctx;
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
          onMenu: () => openItemMenu(item, node.current),
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
    const below = { row: cell.row + 1, col: cell.col };
    const rows = Math.ceil(count / cols);
    // Down into the gap after a short last row lands on the last work (the grid would skip the gap).
    if (e.key === "ArrowDown" && below.row < rows && isPastEnd(below, cols, count)) {
      e.preventDefault();
      grid.focus(cellOf(count - 1, cols), { reason: "list-navigation", event: native });
      return;
    }
    const before = grid.active();
    grid.keydown(native);
    const after = grid.active();
    // The grid takes an arrow even where it cannot move; at a true edge the bridge may carry it on.
    if (ARROWS.has(e.key) && before !== null && after !== null && before.row === after.row && before.col === after.col) {
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
  columns,
  shape,
  onOpen,
  onFocusItem,
  selection,
  onEdge,
  light = "work-light",
  className,
  "aria-label": label,
}: WallGridProps) {
  const count = items.length;
  const rows = Math.ceil(count / columns);
  const root = useRef<HTMLDivElement>(null);
  const latest = useRef({ items, onOpen, onFocusItem, onEdge, columns });
  useLayoutEffect(() => {
    latest.current = { items, onOpen, onFocusItem, onEdge, columns };
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
      count: () => Math.ceil(latest.current.items.length / latest.current.columns),
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
    count,
    reveal: (cell: GridCell) => {
      if (!virtual.items().some((v) => v.index === cell.row)) virtual.scrollToIndex(cell.row, { align: "center" });
    },
    fallback: (index: number) => cellEl(Math.min(index, latest.current.items.length - 1)),
  });

  useEffect(() => {
    const el = root.current;
    if (el === null) return;
    const disconnect = virtual.connect(el);
    const observer = new ResizeObserver(() => virtual.reset());
    observer.observe(el);
    return () => {
      disconnect();
      observer.disconnect();
    };
  }, [virtual]);

  useEffect(() => {
    virtual.reset();
  }, [columns, virtual]);
  useEffect(() => {
    virtual.refresh();
  }, [items, virtual]);

  useEffect(() => {
    const el = root.current!;
    const cells = () => [...el.querySelectorAll<HTMLElement>("[data-cell]")];
    const enter = (from: Box, dir: Dir): boolean => {
      const n = latest.current.items.length;
      if (n === 0) return false;
      const grid = handle.api.grid;
      // grid.focus answers false for the cell that is already active, so landing is judged here.
      const land = (cell: GridCell) => {
        grid.focus(cell, { reason: "imperative" });
        const target = cellEl(indexOf(cell, latest.current.columns));
        if (target !== undefined && document.activeElement !== target) target.focus({ preventScroll: true });
        return true;
      };
      const active = grid.active();
      if (visited.current && active !== null && !isPastEnd(active, latest.current.columns, n)) {
        return land(active);
      }
      const boxes = cells().map((c) => ({ key: c.dataset.index!, rect: c.getBoundingClientRect() as Box }));
      if (boxes.length === 0) return land({ row: 0, col: 0 });
      let key = nearestInDirection(from, boxes, dir);
      if (key === undefined) {
        const axis = dir[0] === 0 ? "x" : "y";
        const mid = (b: Box) => (axis === "x" ? (b.left + b.right) / 2 : (b.top + b.bottom) / 2);
        key = boxes.reduce((best, b) => (Math.abs(mid(b.rect) - mid(from)) < Math.abs(mid(best.rect) - mid(from)) ? b : best)).key;
      }
      return land(cellOf(Number(key), latest.current.columns));
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
      const { items: list, columns: cols } = latest.current;
      const item = list[index];
      if (item === undefined) return;
      const row = Math.floor(index / cols);
      focusRow.current = row;
      latest.current.onFocusItem?.(item);
      if (useFocusState.getState().mode === "pointer") return;
      const el = root.current;
      const v = virtual.items().find((r) => r.index === row);
      if (el === null || v === undefined) return;
      const top = rems("--space-light") * remPx() + v.start;
      const bottom = top + v.size;
      const h = el.clientHeight;
      const band = { top: h * BAND.top, bottom: h * BAND.bottom };
      let target = el.scrollTop;
      if (v.size > band.bottom - band.top) target = top - (h - v.size) / 2;
      else if (top - el.scrollTop < band.top) target = top - band.top;
      else if (bottom - el.scrollTop > band.bottom) target = bottom - band.bottom;
      target = Math.max(0, Math.min(target, el.scrollHeight - h));
      if (Math.abs(target - el.scrollTop) > 1) el.scrollTo({ top: target, behavior: scrollBehavior() });
    },
    [virtual],
  );

  const open = useCallback((item: ItemSummary) => latest.current.onOpen(item), []);

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
  const ctx = useMemo<WallContext>(
    () => ({ handle, cols: columns, count, shape, light, open, focused }),
    [handle, columns, count, shape, light, open, focused],
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
        <div className="relative" style={{ height: `calc(${virtual.size()}px + var(--space-light))` }}>
          {rendered.map((v) => {
            const first = v.index * columns;
            return (
              <div
                key={v.index}
                ref={rowRef(v.index)}
                role="row"
                aria-rowindex={v.index + 1}
                className="absolute inset-x-0 grid"
                style={{
                  top: `calc(${v.start}px + var(--space-light))`,
                  gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
                  columnGap: "var(--space-work-gap)",
                  paddingBottom: "var(--space-work-gap)",
                }}
              >
                {items.slice(first, first + columns).map((item, c) => (
                  <WallCell key={item.id} item={item} index={first + c} selected={selection?.has(item.id) ?? false} />
                ))}
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
